//! Bounded reuse of bindings that provably did not consume the instruction PC.
//! FID alone binds isolated bytes/default context. This never reuses a function
//! lift's remaining-byte window or its propagated context overrides.
use super::*;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Weak};

const SLOTS: usize = 256;
const KEY_LIMIT: usize = 64;
const ENTRY_LIMIT: usize = 64 * 1024;
const PAYLOAD_LIMIT: usize = 2 * 1024 * 1024;

struct Entry {
    bytes: Box<[u8]>,
    state: Arc<RuntimeConstructState>,
    max_checked_offset: u64,
    payload: usize,
}
struct Cache {
    owner: Weak<CompiledFrontend>,
    entries: Vec<Option<Entry>>,
    payload: usize,
    #[cfg(test)]
    hits: usize,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            owner: Weak::new(),
            entries: (0..SLOTS).map(|_| None).collect(),
            payload: 0,
            #[cfg(test)]
            hits: 0,
        }
    }
}
impl Cache {
    fn clear(&mut self) {
        for entry in &mut self.entries {
            *entry = None;
        }
        self.payload = 0;
    }
    fn ensure_owner(&mut self, owner: &Arc<CompiledFrontend>) {
        // A retained Weak prevents address reuse; the active Arc proves the
        // current allocation is live and immutable (COW changes its identity).
        if self.owner.as_ptr() != Arc::as_ptr(owner) {
            self.clear();
            self.owner = Arc::downgrade(owner);
        }
    }
    fn get(
        &mut self,
        slot: usize,
        bytes: &[u8],
        address: u64,
    ) -> Option<Arc<RuntimeConstructState>> {
        let entry = self.entries[slot].as_ref()?;
        if entry.bytes.as_ref() != bytes || address.checked_add(entry.max_checked_offset).is_none()
        {
            return None;
        }
        #[cfg(test)]
        {
            self.hits += 1;
        }
        Some(Arc::clone(&entry.state))
    }
    fn insert(&mut self, slot: usize, entry: Entry) {
        if let Some(previous) = self.entries[slot].take() {
            self.payload -= previous.payload;
        }
        if self.payload + entry.payload > PAYLOAD_LIMIT {
            self.clear();
        }
        self.payload += entry.payload;
        self.entries[slot] = Some(entry);
    }
}
thread_local! {
    static ACTIVE: RefCell<Option<Arc<CompiledFrontend>>> = const { RefCell::new(None) };
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(crate) struct FrontendScope {
    previous: Option<Arc<CompiledFrontend>>,
    _local: PhantomData<Rc<()>>,
}
impl FrontendScope {
    pub(crate) fn enter(frontend: &Arc<CompiledFrontend>) -> Self {
        Self {
            previous: ACTIVE.with(|a| a.replace(Some(Arc::clone(frontend)))),
            _local: PhantomData,
        }
    }
}
impl Drop for FrontendScope {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.replace(self.previous.take()));
    }
}

// Keep uncached states on their existing stack path; boxing would add an
// allocation to every PC-sensitive or otherwise unretained instruction.
#[allow(clippy::large_enum_variant)]
pub(super) enum Binding {
    Owned(RuntimeConstructState),
    Shared(Arc<RuntimeConstructState>),
}
impl std::ops::Deref for Binding {
    type Target = RuntimeConstructState;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(state) => state,
            Self::Shared(state) => state,
        }
    }
}
fn slot_for(bytes: &[u8]) -> usize {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    (hash.finish() as usize) & (SLOTS - 1)
}

pub(super) fn get_or_decode(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    reuse: bool,
) -> Result<Binding> {
    // The old raw-state entry clears this before every instruction, including
    // those whose later p-code completion uses CROSSBUILD/delay resolution.
    clear_bind_cache();
    let eligible = reuse
        && bytes.len() <= KEY_LIMIT
        && ACTIVE.with(|active| {
            active
                .borrow()
                .as_ref()
                .is_some_and(|a| std::ptr::eq(a.as_ref(), compiled))
        });
    if !eligible {
        return decode_instruction_raw_state(compiled, bytes, address).map(Binding::Owned);
    }
    let slot = slot_for(bytes);
    let hit = ACTIVE.with(|active| {
        CACHE.with(|cache| {
            let active = active.borrow();
            let mut cache = cache.borrow_mut();
            cache.ensure_owner(active.as_ref().expect("checked active owner"));
            cache.get(slot, bytes, address)
        })
    });
    if let Some(state) = hit {
        return Ok(Binding::Shared(state));
    }

    // Capture the complete selection/binding attempt, including paths discarded
    // before the winner. A failed/PC-sensitive parse is never retained.
    let (parsed, witness) =
        binding_address_use::capture(|| decode_instruction_raw_state(compiled, bytes, address));
    let state = parsed?;
    if !witness.allows_reuse_at(address) {
        return Ok(Binding::Owned(state));
    }
    let Some(payload) = retained_payload(&state)
        .and_then(|n| n.checked_add(bytes.len()))
        .filter(|n| *n <= ENTRY_LIMIT)
    else {
        return Ok(Binding::Owned(state));
    };
    let state = Arc::new(state);
    ACTIVE.with(|active| {
        CACHE.with(|cache| {
            let active = active.borrow();
            let mut cache = cache.borrow_mut();
            cache.ensure_owner(active.as_ref().expect("active owner during native bind"));
            cache.insert(
                slot,
                Entry {
                    bytes: bytes.into(),
                    state: Arc::clone(&state),
                    max_checked_offset: witness.max_checked_offset,
                    payload,
                },
            );
        })
    });
    Ok(Binding::Shared(state))
}

/// Payload estimate bounds owned binding data. Shared immutable compiler
/// metadata, allocator overhead and the fixed slot table are measured via RSS.
fn retained_payload(state: &RuntimeConstructState) -> Option<usize> {
    struct Budget {
        bytes: usize,
        states: usize,
    }
    impl Budget {
        fn add(&mut self, bytes: usize) -> Option<()> {
            self.bytes = self.bytes.checked_add(bytes)?;
            (self.bytes <= ENTRY_LIMIT).then_some(())
        }
        fn operand(&mut self, value: &BoundOperand) -> Option<()> {
            if let BoundOperand::NamedVarnode { name, .. } = value {
                self.add(name.len().checked_add(2 * std::mem::size_of::<usize>())?)?;
            }
            Some(())
        }
        fn handle(&mut self, handle: &RuntimeHandle) -> Option<()> {
            for space in [
                &handle.fixed.space,
                &handle.fixed.offset_space,
                &handle.fixed.temp_space,
            ]
            .into_iter()
            .flatten()
            {
                self.add(space.name.capacity())?;
            }
            if let Some(value) = &handle.debug_value {
                self.operand(value)?;
            }
            if let Some(state) = &handle.subtable_state {
                self.state(state)?;
            }
            Some(())
        }
        fn state(&mut self, state: &RuntimeConstructState) -> Option<()> {
            self.states += 1;
            if self.states > 128 {
                return None;
            }
            self.add(std::mem::size_of::<RuntimeConstructState>())?;
            self.add(
                state
                    .construct_nodes
                    .capacity()
                    .checked_mul(std::mem::size_of::<spine::RuntimeConstructNode>())?,
            )?;
            self.add(
                state
                    .handles
                    .capacity()
                    .checked_mul(std::mem::size_of::<RuntimeHandle>())?,
            )?;
            self.add(
                state
                    .operands
                    .capacity()
                    .checked_mul(std::mem::size_of::<BoundOperand>())?,
            )?;
            self.add(
                state
                    .replaced_wrapper_patterns
                    .capacity()
                    .checked_mul(std::mem::size_of::<(usize, Arc<CompiledDisjointPattern>)>())?,
            )?;
            self.add(
                state
                    .match_trace
                    .leaf_constructor_indexes
                    .capacity()
                    .checked_mul(std::mem::size_of::<usize>())?,
            )?;
            self.add(
                state
                    .match_trace
                    .root_bucket
                    .len()
                    .checked_add(2 * std::mem::size_of::<usize>())?,
            )?;
            self.add(
                state
                    .match_trace
                    .probes
                    .retained_payload(ENTRY_LIMIT - self.bytes)?,
            )?;
            for handle in &state.handles {
                self.handle(handle)?;
            }
            if let Some(handle) = &state.exported_handle {
                self.handle(handle)?;
            }
            for value in &state.operands {
                self.operand(value)?;
            }
            Some(())
        }
    }
    let mut budget = Budget {
        bytes: std::mem::size_of::<Entry>() + 2 * std::mem::size_of::<usize>(),
        states: 0,
    };
    budget.state(state)?;
    Some(budget.bytes)
}

#[cfg(test)]
#[path = "fid_binding_tests.rs"]
mod tests;
