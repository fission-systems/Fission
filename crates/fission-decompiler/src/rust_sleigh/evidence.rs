//! Structured telemetry for the Rust-Sleigh decode → NIR pipeline (not serialized into CLI/Tauri JSON).

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct RustSleighPipelineEvidence {
    pub entry_address: u64,
    pub max_bytes: usize,
    pub instruction_limit: usize,
    pub wrapper_probe_attempted: bool,
    pub wrapper_probe_matched: bool,
    pub decode_attempt_count: usize,
    pub decode_stop_reason: String,
    pub raw_pcode_op_count: Option<usize>,
    pub strict_indirect_retry_attempted: bool,
    pub nir_fallback_kind: Option<String>,
    pub nir_fallback_kind_refined: Option<String>,
    pub nir_fallback_reason_summary: Option<String>,
    pub pipeline_stage_status: BTreeMap<String, String>,
}

impl RustSleighPipelineEvidence {
    pub fn new(entry_address: u64, max_bytes: usize, instruction_limit: usize) -> Self {
        Self {
            entry_address,
            max_bytes,
            instruction_limit,
            wrapper_probe_attempted: false,
            wrapper_probe_matched: false,
            decode_attempt_count: 0,
            decode_stop_reason: String::new(),
            raw_pcode_op_count: None,
            strict_indirect_retry_attempted: false,
            nir_fallback_kind: None,
            nir_fallback_kind_refined: None,
            nir_fallback_reason_summary: None,
            pipeline_stage_status: BTreeMap::new(),
        }
    }
}

impl Default for RustSleighPipelineEvidence {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}
