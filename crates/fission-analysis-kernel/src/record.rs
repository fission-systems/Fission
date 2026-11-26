//! Watch an emulator run and write its runtime trace.

use crate::image::{ImageView, decode_context};
use fission_emulator::core::Emulator;
use fission_emulator::observe::{IndirectKind, IndirectTransfer, ObserveMask, Observer};
use fission_evidence::{
    Completeness, ContentDigest, HexBytes, ImageBytes, MappingBacking, ObservedTransfer, Producer,
    RUNTIME_TRACE_SCHEMA, RuntimeTrace, TransferKind,
};
use std::collections::BTreeMap;

/// Producer name written into traces and the claims imported from them.
pub const PRODUCER: &str = "fission-emulator";

/// Distinct transfers kept by default; others are counted as dropped.
pub const DEFAULT_TRANSFER_LIMIT: usize = 65_536;

/// The emulator runs one guest process, whose mappings are recorded once.
const PROCESS: u32 = 1;
const EPOCH: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    kind: IndirectKind,
    thread: u64,
    site: u64,
    site_bytes: Vec<u8>,
    target: u64,
    target_bytes: Vec<u8>,
    decode: Option<String>,
}

/// Collects distinct indirect transfers, in order of first occurrence.
#[derive(Debug)]
pub struct IndirectRecorder {
    seen: BTreeMap<Key, usize>,
    transfers: Vec<(Key, u64)>,
    /// Transfers that ran after the limit was reached and were not distinct
    /// ones already held. A count, so recording stays bounded.
    dropped: u64,
    limit: usize,
}

impl Default for IndirectRecorder {
    fn default() -> Self {
        Self::with_limit(DEFAULT_TRANSFER_LIMIT)
    }
}

impl IndirectRecorder {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            seen: BTreeMap::new(),
            transfers: Vec::new(),
            dropped: 0,
            limit,
        }
    }

    /// The trace of this recording.
    ///
    /// `inputs` is the digest from [`inputs_digest`]; `completeness` is how
    /// the run ended, to which the recorder adds what it dropped.
    pub fn trace(
        &self,
        image: &ImageView<'_>,
        inputs: ContentDigest,
        mut completeness: Completeness,
    ) -> RuntimeTrace {
        completeness.dropped_transfers = self.dropped;
        let mappings = image.code_mappings();
        let transfers = self
            .transfers
            .iter()
            .map(|(key, count)| {
                // The read window may run past the code the target is in.
                let mut target_bytes = key.target_bytes.clone();
                if let Some(mapping) = mappings.iter().find(|m| {
                    matches!(m.backing, MappingBacking::Image { .. })
                        && key.target >= m.start
                        && key.target - m.start < m.size
                }) {
                    let offset = key.target - mapping.start;
                    let mut len = target_bytes.len().min((mapping.size - offset) as usize);
                    if let MappingBacking::Image { image_offset, .. } = mapping.backing {
                        // ...or past the bytes the file holds for it.
                        while len > 1 && image.bytes(image_offset + offset, len).is_none() {
                            len -= 1;
                        }
                    }
                    target_bytes.truncate(len);
                }
                ObservedTransfer {
                    kind: match key.kind {
                        IndirectKind::Call => TransferKind::IndirectCall,
                        IndirectKind::Jump => TransferKind::IndirectJump,
                    },
                    process: PROCESS,
                    // Guest thread ids are 32-bit.
                    thread: key.thread as u32,
                    epoch: EPOCH,
                    site: key.site,
                    site_bytes: HexBytes(key.site_bytes.clone()),
                    target: key.target,
                    target_bytes: HexBytes(target_bytes),
                    decode: fission_evidence::DecodeContext {
                        context_override: key.decode.clone(),
                    },
                    count: *count,
                }
            })
            .collect();
        RuntimeTrace {
            schema: RUNTIME_TRACE_SCHEMA.to_string(),
            producer: Producer {
                name: PRODUCER.to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            image: image.identity().clone(),
            inputs,
            mappings,
            transfers,
            completeness,
        }
    }
}

impl Observer for IndirectRecorder {
    fn interest(&self) -> ObserveMask {
        ObserveMask::indirect()
    }

    fn on_indirect_transfer(&mut self, transfer: &IndirectTransfer) {
        let key = Key {
            kind: transfer.kind,
            thread: transfer.thread,
            site: transfer.site,
            site_bytes: transfer.site_bytes.clone(),
            target: transfer.target,
            target_bytes: transfer.target_bytes.clone(),
            decode: decode_context(transfer.decode_context).context_override,
        };
        if let Some(&index) = self.seen.get(&key) {
            self.transfers[index].1 += 1;
        } else if self.transfers.len() < self.limit {
            self.seen.insert(key.clone(), self.transfers.len());
            self.transfers.push((key, 1));
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Digest of what a run was given. The arguments exclude the program path,
/// which is not part of what the program is asked to do.
pub fn inputs_digest(
    args: &[String],
    stdin: Option<&[u8]>,
    max_inst: Option<u64>,
) -> ContentDigest {
    let mut buffer = Vec::new();
    let mut field = |bytes: &[u8]| {
        buffer.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buffer.extend_from_slice(bytes);
    };
    field(&(args.len() as u64).to_le_bytes());
    for arg in args {
        field(arg.as_bytes());
    }
    if let Some(stdin) = stdin {
        field(b"stdin");
        field(stdin);
    }
    if let Some(max_inst) = max_inst {
        field(b"max_inst");
        field(&max_inst.to_le_bytes());
    }
    ContentDigest::of(&buffer)
}

/// How `emulator`'s run ended. `error` is the run loop's failure, if it had one.
pub fn completeness(emulator: &Emulator, error: Option<&str>) -> Completeness {
    let stop_reason = match (error, emulator.exit_code, emulator.exit_signal) {
        (Some(error), ..) => format!("emulator error: {error}"),
        (None, Some(_), _) => "guest_exit".to_string(),
        (None, None, Some(signal)) => format!("guest killed by signal {signal}"),
        (None, None, None) => emulator
            .metrics
            .exit_reason
            .clone()
            .unwrap_or_else(|| "stopped before the guest exited".to_string()),
    };
    Completeness {
        guest_finished: error.is_none() && emulator.exit_code.is_some(),
        stop_reason,
        dropped_transfers: 0,
    }
}
