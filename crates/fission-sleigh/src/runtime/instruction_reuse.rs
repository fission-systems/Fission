//! Instructions one analysis lifted, offered to a later linear sweep over the
//! same image. Owned by the analysis transaction that built it; never a
//! program cache.
//!
//! A lift is a function of the compiled frontend, the address, the context it
//! starts in, and the bytes it read. A record therefore keeps the bytes the lift
//! read (see [`LiftReads`](super::spine::compiled_table::LiftReads)) and is
//! served only to a decode at the same address and context whose window starts
//! with those same bytes. Anything else decodes as usual.
use super::*;
use bincode::Options;
use std::sync::Arc;

fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_varint_encoding()
        .reject_trailing_bytes()
}

type Context = Option<(
    crate::packed_context::ContextWords,
    crate::packed_context::ContextWords,
)>;
type Key = (u64, Context);

fn key(address: u64, context: Option<PackedContextOverride>) -> Key {
    (address, context.map(|c| (c.context(), c.mask())))
}

/// Instructions recorded by one lift, in decode order.
#[derive(Debug, Default)]
pub struct InstructionRecords {
    rows: Vec<(Key, Box<[u8]>)>,
}

/// One record's bytes: the length of the read prefix (one byte), the prefix,
/// then the instruction encoded. A sweep over a large image holds hundreds of
/// thousands of these at once, so each is a single small allocation.
fn pack(read: &[u8], instruction: &DecodedInstruction) -> Option<Box<[u8]>> {
    let read_len = u8::try_from(read.len()).ok()?;
    let mut packed = Vec::with_capacity(1 + read.len() + 64);
    packed.push(read_len);
    packed.extend_from_slice(read);
    codec().serialize_into(&mut packed, instruction).ok()?;
    Some(packed.into_boxed_slice())
}

fn unpack(packed: &[u8]) -> (&[u8], &[u8]) {
    let read_len = usize::from(packed[0]);
    packed[1..].split_at(read_len)
}

impl InstructionRecords {
    pub(super) fn push(
        &mut self,
        address: u64,
        context: Option<PackedContextOverride>,
        read: &[u8],
        instruction: DecodedInstruction,
    ) {
        if let Some(packed) = pack(read, &instruction) {
            self.rows.push((key(address, context), packed));
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Lifted instructions keyed by address and starting context, tied to the
/// frontend that lifted them.
pub struct InstructionReuse {
    frontend: RuntimeSleighFrontend,
    /// The distinct starting contexts seen; a handful per image.
    contexts: Vec<Context>,
    /// Sorted by (address, index into `contexts`), one entry per key.
    entries: Vec<(u64, u32, Box<[u8]>)>,
}

impl std::fmt::Debug for InstructionReuse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstructionReuse")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl InstructionReuse {
    pub fn new(frontend: &RuntimeSleighFrontend) -> Self {
        Self {
            frontend: frontend.clone(),
            contexts: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// Take in lifts' records, in order. The first record for an address and
    /// context is kept: the image is the same, so later ones match it.
    pub fn absorb(&mut self, records: impl IntoIterator<Item = InstructionRecords>) {
        let records: Vec<InstructionRecords> = records.into_iter().collect();
        self.entries
            .reserve(records.iter().map(InstructionRecords::len).sum());
        for records in records {
            for ((address, context), packed) in records.rows {
                let index = match self.contexts.iter().position(|c| *c == context) {
                    Some(index) => index,
                    None => {
                        self.contexts.push(context);
                        self.contexts.len() - 1
                    }
                };
                let Ok(index) = u32::try_from(index) else {
                    continue;
                };
                self.entries.push((address, index, packed));
            }
        }
        // Stable, so `dedup` keeps the earliest record per key.
        self.entries.sort_by_key(|entry| (entry.0, entry.1));
        self.entries
            .dedup_by(|later, earlier| (later.0, later.1) == (earlier.0, earlier.1));
        self.entries.shrink_to_fit();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `frontend` decodes exactly as the one that made these records.
    pub fn serves(&self, frontend: &RuntimeSleighFrontend) -> bool {
        self.frontend.language == frontend.language
            && self.frontend.entry == frontend.entry
            && self.frontend.status == frontend.status
            && match (&self.frontend.compiled, &frontend.compiled) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }

    /// The recorded instruction at `address` in `context`, if `window` (the
    /// bytes from `address` on) starts with the bytes that lift read.
    pub(super) fn get(
        &self,
        address: u64,
        context: Option<PackedContextOverride>,
        window: &[u8],
    ) -> Option<DecodedInstruction> {
        let (address, context) = key(address, context);
        let context = u32::try_from(self.contexts.iter().position(|c| *c == context)?).ok()?;
        let index = self
            .entries
            .binary_search_by_key(&(address, context), |entry| (entry.0, entry.1))
            .ok()?;
        let (read, encoded) = unpack(&self.entries[index].2);
        if window.get(..read.len()) != Some(read) {
            return None;
        }
        Some(
            codec()
                .deserialize(encoded)
                .expect("an instruction encoded in this process must decode"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x86_64() -> RuntimeSleighFrontend {
        RuntimeSleighFrontend::new_for_language("x86-64").expect("x86-64 frontend")
    }

    fn reuse_of(frontend: &RuntimeSleighFrontend, records: InstructionRecords) -> InstructionReuse {
        let mut reuse = InstructionReuse::new(frontend);
        reuse.absorb([records]);
        reuse
    }

    #[test]
    fn a_record_is_served_only_where_the_window_starts_with_the_bytes_it_read() {
        let frontend = x86_64();
        let mut sentinel = frontend
            .decode_window(&[0xc3], 0x1000, 1)
            .expect("ret")
            .remove(0);
        sentinel.mnemonic = "SENTINEL".to_string();
        let mut records = InstructionRecords::default();
        records.push(0x1000, None, &[0xc3], sentinel);
        let reuse = reuse_of(&frontend, records);
        let first = |bytes: &[u8], address: u64| {
            frontend
                .decode_window_reusing(bytes, address, 1, None, Some(&reuse))
                .expect("decode")
                .remove(0)
                .mnemonic
        };

        // Same address, context and bytes: the record is what comes back.
        assert_eq!(first(&[0xc3, 0x90], 0x1000), "SENTINEL");
        // A different byte where the record read, another address, or another
        // starting context: decoded as usual.
        assert_ne!(first(&[0x90, 0xc3], 0x1000), "SENTINEL");
        assert_ne!(first(&[0xc3], 0x2000), "SENTINEL");
        let other_context = PackedContextOverride::default();
        assert_ne!(
            frontend
                .decode_window_reusing(&[0xc3], 0x1000, 1, Some(other_context), Some(&reuse))
                .expect("decode")[0]
                .mnemonic,
            "SENTINEL"
        );
    }

    #[test]
    fn a_function_lift_records_exactly_what_a_sweep_over_the_same_bytes_decodes() {
        let frontend = x86_64();
        // push rbp; mov rbp,rsp; mov eax,1; pop rbp; ret
        let bytes = [
            0x55, 0x48, 0x89, 0xe5, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x5d, 0xc3,
        ];
        let (_, seed, records) = frontend
            .lift_raw_pcode_function_recording(
                &bytes,
                0x1000,
                DecodeContract::decomp_function(100),
                &DecodeMemoryContext::default(),
                None,
                false,
            )
            .expect("lift");
        assert!(seed.is_none());
        assert_eq!(records.len(), 5, "one record per instruction");
        let reuse = reuse_of(&frontend, records);

        let sweep = |bytes: &[u8], reuse: Option<&InstructionReuse>| {
            frontend
                .decode_window_reusing(bytes, 0x1000, usize::MAX, None, reuse)
                .expect("sweep")
        };
        assert_eq!(sweep(&bytes, Some(&reuse)), sweep(&bytes, None));

        // The image changed under the record (mov eax,1 -> mov eax,2): the
        // sweep sees the new instruction, not the recorded one.
        let mut patched = bytes;
        patched[5] = 0x02;
        assert_eq!(sweep(&patched, Some(&reuse)), sweep(&patched, None));
        assert_ne!(sweep(&patched, None), sweep(&bytes, None));
    }

    #[test]
    fn records_are_not_served_to_a_different_frontend() {
        let x64 = x86_64();
        let x86 = RuntimeSleighFrontend::new_for_language("x86").expect("x86 frontend");
        let reuse = InstructionReuse::new(&x64);
        assert!(reuse.serves(&x64));
        assert!(!reuse.serves(&x86));
    }
}
