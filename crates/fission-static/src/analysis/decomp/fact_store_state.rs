//! Mutable session state owned by [`super::FactStore`].
//!
//! The program snapshot and loader/debug metadata are immutable inputs to a
//! decompilation session. Learned facts have a different lifecycle: a cloned
//! store starts with the same learned values but can evolve independently.
//! Completed runtime decodes are different again, because all clones should
//! share them as a process-local performance cache.

use fission_core::CallingConvention;
use fission_loader::loader::types::InferredTypeInfo;
use fission_sleigh::runtime::{DecodeStopReason, DecodedPcodeFunction};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

/// Shared completed raw-p-code decodes.
///
/// This is deliberately not a fact overlay: it is derived runtime state whose
/// only ownership rule is that `FactStore` clones share it. Keeping it separate
/// makes that performance contract explicit instead of hiding it in a mixed
/// facts struct.
#[derive(Debug, Clone, Default)]
pub(super) struct DecodeCache {
    entries: Arc<Mutex<HashMap<u64, Arc<DecodedPcodeFunction>>>>,
}

impl DecodeCache {
    pub(super) fn get(&self, address: u64) -> Option<Arc<DecodedPcodeFunction>> {
        self.entries.lock().ok()?.get(&address).cloned()
    }

    pub(super) fn insert_if_complete(&self, address: u64, decoded: &Arc<DecodedPcodeFunction>) {
        if decoded.stop_reason != DecodeStopReason::TerminalControlFlow {
            return;
        }
        if let Ok(mut entries) = self.entries.lock() {
            entries
                .entry(address)
                .or_insert_with(|| Arc::clone(decoded));
        }
    }
}
