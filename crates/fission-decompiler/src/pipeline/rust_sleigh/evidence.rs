//! Structured telemetry for the Rust-Sleigh decode → NIR pipeline (not serialized into CLI/Tauri JSON).

use serde::Serialize;
use std::collections::BTreeMap;

const MAX_PCODE_BLOCK_EVIDENCE: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct PcodeBlockEvidence {
    pub index: u32,
    pub start_address: u64,
    pub successors: Vec<u32>,
    pub op_count: usize,
    pub terminal_seq_num: Option<u32>,
    pub terminal_address: Option<u64>,
    pub terminal_opcode: Option<String>,
    pub terminal_target: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RustSleighPipelineEvidence {
    pub entry_address: u64,
    pub max_bytes: usize,
    pub instruction_limit: usize,
    pub wrapper_probe_attempted: bool,
    pub wrapper_probe_matched: bool,
    pub decode_attempt_count: usize,
    pub decode_stop_reason: String,
    pub template_source_counts: BTreeMap<String, usize>,
    pub raw_pcode_op_count: Option<usize>,
    pub raw_pcode_block_count: Option<usize>,
    pub raw_pcode_edge_count: Option<usize>,
    pub raw_pcode_terminal_opcode_counts: BTreeMap<String, usize>,
    pub raw_pcode_block_evidence_truncated: bool,
    pub raw_pcode_blocks: Vec<PcodeBlockEvidence>,
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
            template_source_counts: BTreeMap::new(),
            raw_pcode_op_count: None,
            raw_pcode_block_count: None,
            raw_pcode_edge_count: None,
            raw_pcode_terminal_opcode_counts: BTreeMap::new(),
            raw_pcode_block_evidence_truncated: false,
            raw_pcode_blocks: Vec::new(),
            strict_indirect_retry_attempted: false,
            nir_fallback_kind: None,
            nir_fallback_kind_refined: None,
            nir_fallback_reason_summary: None,
            pipeline_stage_status: BTreeMap::new(),
        }
    }
}

impl RustSleighPipelineEvidence {
    pub(crate) fn observe_pcode(&mut self, pcode: &crate::PcodeFunction) {
        self.raw_pcode_block_count = Some(pcode.blocks.len());
        self.raw_pcode_edge_count = Some(
            pcode
                .blocks
                .iter()
                .map(|block| block.successors.len())
                .sum(),
        );
        self.raw_pcode_terminal_opcode_counts.clear();
        self.raw_pcode_blocks.clear();
        self.raw_pcode_block_evidence_truncated = pcode.blocks.len() > MAX_PCODE_BLOCK_EVIDENCE;

        for block in pcode.blocks.iter().take(MAX_PCODE_BLOCK_EVIDENCE) {
            let terminal = block.ops.last();
            if let Some(op) = terminal {
                *self
                    .raw_pcode_terminal_opcode_counts
                    .entry(format!("{:?}", op.opcode))
                    .or_insert(0) += 1;
            }
            self.raw_pcode_blocks.push(PcodeBlockEvidence {
                index: block.index,
                start_address: block.start_address,
                successors: block.successors.clone(),
                op_count: block.ops.len(),
                terminal_seq_num: terminal.map(|op| op.seq_num),
                terminal_address: terminal.map(|op| op.address),
                terminal_opcode: terminal.map(|op| format!("{:?}", op.opcode)),
                terminal_target: terminal.and_then(|op| {
                    op.inputs
                        .first()
                        .filter(|target| target.is_constant)
                        .map(|target| target.constant_val as u64)
                }),
            });
        }
    }
}

impl Default for RustSleighPipelineEvidence {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}
