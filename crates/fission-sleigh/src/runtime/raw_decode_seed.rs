//! Consuming, transaction-local raw instruction evidence. Never a program cache.
use super::*;
use bincode::Options;
use std::collections::BTreeMap;
use std::sync::Arc;

type Key = (
    u64,
    Option<(
        crate::packed_context::ContextWords,
        crate::packed_context::ContextWords,
    )>,
);
type Raw = (
    DecodedInstruction,
    Vec<PcodeOp>,
    u64,
    RuntimeExecutionDetails,
);
type Stored = (
    DecodedInstruction,
    Vec<PcodeOp>,
    u64,
    Option<crate::compiler::CompiledTemplateSource>,
    Vec<(u64, u32, u32, u32)>,
);

/// Opaque successful raw results tied to their complete input window and frontend.
/// Owned by one analysis transaction, consumed on replay, never globally retained.
pub struct RawDecodeSeed {
    frontend: RuntimeSleighFrontend,
    bytes: Box<[u8]>,
    entry: u64,
    records: BTreeMap<Key, Box<[u8]>>,
}

impl std::fmt::Debug for RawDecodeSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawDecodeSeed")
            .field("entry", &self.entry)
            .field("window_bytes", &self.bytes.len())
            .field("records", &self.records.len())
            .field(
                "packed_bytes",
                &self.records.values().map(|r| r.len()).sum::<usize>(),
            )
            .finish_non_exhaustive()
    }
}

fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_varint_encoding()
        .reject_trailing_bytes()
}
fn key(address: u64, context: Option<PackedContextOverride>) -> Key {
    (address, context.map(|c| (c.context(), c.mask())))
}
impl RawDecodeSeed {
    #[cfg(test)]
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }

    pub(super) fn new(frontend: &RuntimeSleighFrontend, bytes: &[u8], entry: u64) -> Self {
        Self {
            frontend: frontend.clone(),
            bytes: bytes.into(),
            entry,
            records: BTreeMap::new(),
        }
    }
    pub(super) fn matches(
        &self,
        frontend: &RuntimeSleighFrontend,
        bytes: &[u8],
        entry: u64,
    ) -> bool {
        self.entry == entry
            && self.bytes.as_ref() == bytes
            && self.frontend.language == frontend.language
            && self.frontend.entry == frontend.entry
            && self.frontend.status == frontend.status
            && match (&self.frontend.compiled, &frontend.compiled) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
    pub(super) fn record(
        &mut self,
        address: u64,
        context: Option<PackedContextOverride>,
        raw: &Raw,
    ) {
        let (
            instruction,
            ops,
            len,
            RuntimeExecutionDetails {
                template_source,
                pending_context_commits,
                userops,
            },
        ) = raw;
        let Some(compiled) = &self.frontend.compiled else {
            return;
        };
        if !Arc::ptr_eq(userops, &compiled.userops) {
            return;
        }
        if let Ok(bytes) = codec().serialize(&(
            instruction,
            ops,
            len,
            template_source,
            pending_context_commits,
        )) {
            self.records
                .insert(key(address, context), bytes.into_boxed_slice());
        }
    }
    pub(super) fn take(
        &mut self,
        address: u64,
        context: Option<PackedContextOverride>,
    ) -> Option<Raw> {
        let bytes = self.records.remove(&key(address, context))?;
        let (instruction, ops, len, template_source, pending_context_commits): Stored = codec()
            .deserialize(&bytes)
            .expect("internally captured raw decode must round-trip");
        Some((
            instruction,
            ops,
            len,
            RuntimeExecutionDetails {
                template_source,
                pending_context_commits,
                userops: Arc::clone(
                    &self
                        .frontend
                        .compiled
                        .as_ref()
                        .expect("captured compiled frontend")
                        .userops,
                ),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_decode_seed_round_trips_complete_success_and_is_consumed() {
        let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
        let bytes = [0x48, 0x8b, 0x05, 0x10, 0, 0, 0, 0xc3];
        let context = None;
        let raw = frontend
            .decode_instruction_and_lift_with_context_override(&bytes, 0x1000, context)
            .unwrap();
        let mut seed = RawDecodeSeed::new(&frontend, &bytes, 0x1000);
        seed.record(0x1000, context, &raw);
        assert_eq!(seed.records.len(), 1);
        assert_eq!(
            seed.take(0x1000, Some(PackedContextOverride::default())),
            None
        );
        let restored = seed.take(0x1000, context).unwrap();
        assert_eq!(restored, raw);
        assert!(Arc::ptr_eq(&restored.3.userops, &raw.3.userops));
        assert!(seed.take(0x1000, context).is_none());
        // A downstream consumer mutating its owned op sequence cannot change evidence.
        let mut consumer = raw.clone();
        consumer.1[0].seq_num = 777;
        assert_ne!(consumer, restored);
    }

    #[test]
    fn raw_decode_seed_rejects_window_content_entry_and_frontend_changes() {
        let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
        let seed = RawDecodeSeed::new(&frontend, &[0xc3, 0x90], 0x1000);
        assert!(seed.matches(&frontend.clone(), &[0xc3, 0x90], 0x1000));
        assert!(!seed.matches(&frontend, &[0xc3], 0x1000));
        assert!(!seed.matches(&frontend, &[0xc3, 0x91], 0x1000));
        assert!(!seed.matches(&frontend, &[0xc3, 0x90], 0x2000));
        let mut changed = frontend.clone();
        changed.language.push_str("other");
        assert!(!seed.matches(&changed, &[0xc3, 0x90], 0x1000));
        changed = frontend.clone();
        changed.entry.entry_id.push_str("other");
        assert!(!seed.matches(&changed, &[0xc3, 0x90], 0x1000));
        changed = frontend.clone();
        changed.compiled = None;
        assert!(!seed.matches(&changed, &[0xc3, 0x90], 0x1000));
        changed = frontend.clone();
        changed.compiled = Some(Arc::new((**frontend.compiled.as_ref().unwrap()).clone()));
        assert!(!seed.matches(&changed, &[0xc3, 0x90], 0x1000));
    }

    #[test]
    fn raw_decode_seed_context_key_preserves_all_bits_and_catalog_identity() {
        let frontend = RuntimeSleighFrontend::new_for_language("ARM8_le").unwrap();
        let context = frontend
            .normalize_low_bit_code_address(0x1001)
            .context_override;
        let bytes = [0x4c, 0xf6, 0xcd, 0x41];
        let mut raw = frontend
            .decode_instruction_and_lift_with_context_override(&bytes, 0x1000, context)
            .unwrap();
        let mut seed = RawDecodeSeed::new(&frontend, &bytes, 0x1000);
        seed.record(0x1000, context, &raw);
        assert!(seed.take(0x1000, None).is_none());
        assert!(seed
            .take(0x1000, Some(PackedContextOverride::new(u64::MAX, u64::MAX)))
            .is_none());
        assert_eq!(seed.take(0x1000, context).unwrap(), raw);
        raw.3.userops = Arc::new((*raw.3.userops).clone());
        seed.record(0x1000, context, &raw);
        assert!(seed.records.is_empty());
    }

    #[test]
    fn raw_decode_seed_preserves_commits_high_bits_and_private_archive_errors() {
        let frontend = RuntimeSleighFrontend::new_for_language("x86-64").unwrap();
        let bytes = [0xc3];
        let mut raw = frontend
            .decode_instruction_and_lift_with_context_override(&bytes, 0xffff_ffff_0000_1000, None)
            .unwrap();
        raw.3
            .pending_context_commits
            .push((u64::MAX - 9, u32::MAX, 0x8000_0001, u32::MAX));
        raw.0.pending_context_commits = raw.3.pending_context_commits.clone();
        let mut seed = RawDecodeSeed::new(&frontend, &bytes, 0xffff_ffff_0000_1000);
        seed.record(raw.0.address, None, &raw);
        assert_eq!(seed.take(raw.0.address, None).unwrap(), raw);
        seed.records.insert(key(0, None), Box::new([]));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| seed.take(0, None))).is_err()
        );
    }
}
