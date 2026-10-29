//! Mutable session state owned by [`super::FactStore`].
//!
//! The program snapshot and loader/debug metadata are immutable inputs to a
//! decompilation session. Learned facts have a different lifecycle: a cloned
//! store starts with the same learned values but can evolve independently.
//! Completed runtime decodes are different again, because all clones should
//! share them as a process-local performance cache.

use fission_core::CallingConvention;
use fission_loader::loader::types::InferredTypeInfo;
use fission_sleigh::runtime::{
    DecodeStopReason, DecodedPcodeFunction, PackedContextOverride, PackedDecodedPcodeFunction,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use super::{FactProvenance, NameFact};

#[derive(Debug, Clone, Default)]
pub(super) struct LearnedFactOverlay {
    pub(super) name_facts: HashMap<u64, Vec<NameFact>>,
    pub(super) native_type_facts: HashMap<u64, Vec<InferredTypeInfo>>,
    pub(super) structuring_hints: HashMap<u64, fission_midend_core::NirFunctionHints>,
    pub(super) calling_conventions: HashMap<u64, CallingConvention>,
}

impl LearnedFactOverlay {
    pub(super) fn ingest_name_fact(
        &mut self,
        address: u64,
        name: String,
        provenance: FactProvenance,
    ) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }

        let fact = NameFact {
            name: trimmed.to_string(),
            provenance,
        };
        let facts = self.name_facts.entry(address).or_default();
        if !facts.iter().any(|current| current == &fact) {
            facts.push(fact);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct DecodeCacheKey {
    pub(super) address: u64,
    pub(super) context_override: Option<PackedContextOverride>,
}

/// Shared completed raw-p-code decodes.
///
/// This is deliberately not a fact overlay: it is derived runtime state whose
/// only ownership rule is that `FactStore` clones share it. Keeping it separate
/// makes that performance contract explicit instead of hiding it in a mixed
/// facts struct.
#[derive(Debug, Clone, Default)]
pub(super) struct DecodeCache {
    entries: Arc<Mutex<HashMap<DecodeCacheKey, Arc<CachedDecode>>>>,
}

#[derive(Debug)]
enum DecodeStorage {
    Cold {
        packed: PackedDecodedPcodeFunction,
        original: Weak<DecodedPcodeFunction>,
    },
    Hot(Arc<DecodedPcodeFunction>),
}

#[derive(Debug)]
struct CachedDecode {
    storage: Mutex<DecodeStorage>,
}

impl CachedDecode {
    fn new(decoded: &Arc<DecodedPcodeFunction>) -> Self {
        Self::from_packed(decoded, PackedDecodedPcodeFunction::pack(decoded))
    }

    fn from_packed(
        decoded: &Arc<DecodedPcodeFunction>,
        packed: anyhow::Result<PackedDecodedPcodeFunction>,
    ) -> Self {
        let storage = match packed {
            Ok(packed) => DecodeStorage::Cold {
                packed,
                original: Arc::downgrade(decoded),
            },
            // Preserve the previous cache contract if encoding is unavailable.
            Err(_) => DecodeStorage::Hot(Arc::clone(decoded)),
        };
        Self {
            storage: Mutex::new(storage),
        }
    }

    fn get(&self) -> Option<Arc<DecodedPcodeFunction>> {
        let mut storage = self.storage.lock().ok()?;
        match &*storage {
            DecodeStorage::Hot(decoded) => Some(Arc::clone(decoded)),
            DecodeStorage::Cold { packed, original } => {
                let decoded = original.upgrade().unwrap_or_else(|| {
                    Arc::new(
                        packed
                            .unpack()
                            .expect("internally packed decode must round-trip"),
                    )
                });
                // Discard the packed representation once requested. Holding
                // both for every function would undo the memory saving for
                // all-function callers. Materialization never calls Rayon,
                // I/O, inference or another cache.
                *storage = DecodeStorage::Hot(Arc::clone(&decoded));
                Some(decoded)
            }
        }
    }
}

impl DecodeCache {
    pub(super) fn get(&self, address: u64) -> Option<Arc<DecodedPcodeFunction>> {
        let entry = {
            let entries = self.entries.lock().ok()?;
            let none_key = DecodeCacheKey {
                address,
                context_override: None,
            };
            if let Some(entry) = entries.get(&none_key) {
                Arc::clone(entry)
            } else {
                let mut matched = entries
                    .iter()
                    .filter(|(k, _)| k.address == address)
                    .map(|(_, v)| v);
                let first = matched.next()?;
                if matched.next().is_some() {
                    return None;
                }
                Arc::clone(first)
            }
        };
        entry.get()
    }

    pub(super) fn get_with_context(
        &self,
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Option<Arc<DecodedPcodeFunction>> {
        // Release the shared map before materializing this particular entry.
        let key = DecodeCacheKey {
            address,
            context_override,
        };
        let entry = self.entries.lock().ok()?.get(&key).cloned()?;
        entry.get()
    }

    pub(super) fn insert_if_complete(&self, address: u64, decoded: &Arc<DecodedPcodeFunction>) {
        self.insert_if_complete_with_context(address, None, decoded);
    }

    pub(super) fn insert_if_complete_with_context(
        &self,
        address: u64,
        context_override: Option<PackedContextOverride>,
        decoded: &Arc<DecodedPcodeFunction>,
    ) {
        if decoded.stop_reason != DecodeStopReason::TerminalControlFlow {
            return;
        }
        let key = DecodeCacheKey {
            address,
            context_override,
        };
        if self
            .entries
            .lock()
            .is_ok_and(|entries| entries.contains_key(&key))
        {
            return;
        }
        // Encoding can be substantial; do it outside the global cache lock.
        let entry = Arc::new(CachedDecode::new(decoded));
        if let Ok(mut entries) = self.entries.lock() {
            entries.entry(key).or_insert_with(|| Arc::clone(&entry));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_pcode_ir::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use std::collections::{BTreeMap, BTreeSet};

    fn decoded(reason: DecodeStopReason) -> Arc<DecodedPcodeFunction> {
        Arc::new(DecodedPcodeFunction {
            function: PcodeFunction {
                blocks: vec![PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: Vec::new(),
                    ops: vec![PcodeOp {
                        seq_num: 9,
                        opcode: PcodeOpcode::Return,
                        address: 0x1000,
                        output: None,
                        inputs: vec![Varnode::constant(i64::MIN, 8)],
                        asm_mnemonic: Some("ret".to_string()),
                    }],
                }],
            },
            instructions: Vec::new(),
            decoded_instructions: 1,
            stop_reason: reason,
            template_source_counts: BTreeMap::from([("SpecDerived".to_string(), 1)]),
            reachable_instruction_addresses: vec![0x1000],
            instruction_lengths: BTreeMap::from([(0x1000, 1)]),
            inferred_indirect_edges: BTreeMap::new(),
            indirect_targets: BTreeSet::new(),
        })
    }

    #[test]
    fn completed_decode_storage_releases_cold_graph_and_packed_bytes_on_lookup() {
        let cache = DecodeCache::default();
        let original = decoded(DecodeStopReason::TerminalControlFlow);
        let expected = (*original).clone();
        let weak = Arc::downgrade(&original);
        cache.insert_if_complete(0x1000, &original);
        let key = DecodeCacheKey {
            address: 0x1000,
            context_override: None,
        };
        let entry = cache.entries.lock().unwrap().get(&key).cloned().unwrap();
        assert!(matches!(
            *entry.storage.lock().unwrap(),
            DecodeStorage::Cold { .. }
        ));
        drop(original);
        assert!(
            weak.upgrade().is_none(),
            "cold cache must not retain the original graph"
        );
        let restored = cache.get(0x1000).unwrap();
        assert_eq!(*restored, expected);
        assert!(matches!(
            *entry.storage.lock().unwrap(),
            DecodeStorage::Hot(_)
        ));
        assert!(Arc::ptr_eq(&restored, &cache.get(0x1000).unwrap()));
        assert!(Arc::ptr_eq(&restored, &cache.clone().get(0x1000).unwrap()));
    }

    #[test]
    fn completed_decode_storage_retains_live_identity_and_first_insertion() {
        let cache = DecodeCache::default();
        let original = decoded(DecodeStopReason::TerminalControlFlow);
        cache.insert_if_complete(0x1000, &original);
        let mut different = decoded(DecodeStopReason::TerminalControlFlow);
        Arc::make_mut(&mut different).decoded_instructions = 17;
        cache.insert_if_complete(0x1000, &different);
        assert!(Arc::ptr_eq(&original, &cache.get(0x1000).unwrap()));
    }

    #[test]
    fn completed_decode_storage_preserves_snapshot_when_caller_uses_copy_on_write() {
        let cache = DecodeCache::default();
        let mut original = decoded(DecodeStopReason::TerminalControlFlow);
        let expected = (*original).clone();
        cache.insert_if_complete(0x1000, &original);
        assert!(Arc::get_mut(&mut original).is_none());
        Arc::make_mut(&mut original).decoded_instructions = 17;
        assert_eq!(*cache.get(0x1000).unwrap(), expected);
    }

    #[test]
    fn completed_decode_storage_ignores_incomplete_and_keeps_encoding_fallback() {
        let cache = DecodeCache::default();
        for reason in [
            DecodeStopReason::IndirectBranchHalt,
            DecodeStopReason::InputExhausted,
            DecodeStopReason::InstructionLimit,
        ] {
            cache.insert_if_complete(0x1000, &decoded(reason));
            assert!(cache.get(0x1000).is_none());
        }
        let original = decoded(DecodeStopReason::TerminalControlFlow);
        let fallback =
            CachedDecode::from_packed(&original, Err(anyhow::anyhow!("codec unavailable")));
        assert!(matches!(
            *fallback.storage.lock().unwrap(),
            DecodeStorage::Hot(_)
        ));
        assert!(Arc::ptr_eq(&original, &fallback.get().unwrap()));
    }

    #[test]
    fn decode_cache_isolates_different_context_overrides_at_same_address() {
        let cache = DecodeCache::default();
        let arm_ctx = Some(PackedContextOverride::new(0, 1));
        let thumb_ctx = Some(PackedContextOverride::new(1, 1));
        let mut arm_decoded = decoded(DecodeStopReason::TerminalControlFlow);
        Arc::make_mut(&mut arm_decoded).decoded_instructions = 4;
        let mut thumb_decoded = decoded(DecodeStopReason::TerminalControlFlow);
        Arc::make_mut(&mut thumb_decoded).decoded_instructions = 8;

        cache.insert_if_complete_with_context(0x1000, arm_ctx, &arm_decoded);
        cache.insert_if_complete_with_context(0x1000, thumb_ctx, &thumb_decoded);

        assert_eq!(
            cache
                .get_with_context(0x1000, arm_ctx)
                .unwrap()
                .decoded_instructions,
            4
        );
        assert_eq!(
            cache
                .get_with_context(0x1000, thumb_ctx)
                .unwrap()
                .decoded_instructions,
            8
        );
        assert!(cache.get(0x1000).is_none());
    }

    #[test]
    fn completed_decode_storage_concurrent_readers_share_one_materialization() {
        let cache = DecodeCache::default();
        let original = decoded(DecodeStopReason::TerminalControlFlow);
        let expected = (*original).clone();
        cache.insert_if_complete(0x1000, &original);
        drop(original);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let cache = cache.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    cache.get(0x1000).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let values = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>();
        for value in &values {
            assert_eq!(**value, expected);
            assert!(Arc::ptr_eq(value, &values[0]));
        }
    }
}
