use std::path::PathBuf;
use super::FunctionDiscoveryProfileArg;

/// Internal normalized one-shot execution arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneShotArgs {
    pub binary: PathBuf,
    pub address: Option<u64>,
    pub list: bool,
    pub info: bool,
    pub sections: bool,
    pub imports: bool,
    pub exports: bool,
    /// Run packer/compiler/language detection for `info`.
    pub info_detections: bool,
    /// Emit structured loader identity report (entropy, overlay, import/section hints with evidence).
    pub info_identity: bool,
    pub strings: Option<usize>,
    pub disasm: Option<u64>,
    pub disasm_function: Option<u64>,
    pub raw_pcode: Option<u64>,
    pub raw_pcode_max_bytes: usize,
    pub raw_pcode_instruction_limit: usize,
    pub raw_pcode_continue_past_indirect: bool,
    pub pcode_stages: Option<u64>,
    pub pcode_stages_max_bytes: usize,
    pub pcode_stages_instruction_limit: usize,
    pub pcode_stages_strict_indirect_stop: bool,
    pub nir_stats: Option<u64>,
    pub nir_stats_max_bytes: usize,
    pub nir_stats_instruction_limit: usize,
    pub nir_stats_strict_indirect_stop: bool,
    pub pcode_topology: Option<u64>,
    pub pcode_topology_max_bytes: usize,
    pub pcode_topology_instruction_limit: usize,
    pub pcode_topology_strict_indirect_stop: bool,
    pub count: usize,
    pub compiler_id: Option<String>,
    pub profile: Option<String>,
    pub engine: Option<String>,
    pub output: Option<PathBuf>,
    pub json: bool,
    pub verbose: bool,
    pub no_header: bool,
    pub ghidra_compat: bool,
    pub no_warnings: bool,
    pub benchmark: bool,
    pub decomp_all: bool,
    pub decomp_limit: Option<usize>,
    pub include_nonuser_functions: bool,
    pub timeout_ms: Option<u64>,
    pub format: Option<String>,
    pub function_discovery_profile: Option<FunctionDiscoveryProfileArg>,
    pub preview_candidate_inventory: bool,
    pub preview_candidate_limit: Option<usize>,
    pub preview_candidate_scan_batch: bool,
    pub addresses_file: Option<PathBuf>,
    pub functions_limit: Option<usize>,
    pub chunk_size: Option<usize>,
    pub output_jsonl: Option<PathBuf>,
    pub summary_json: Option<PathBuf>,
    pub resume_from: Option<PathBuf>,
    pub quiet_batch_errors: bool,
    pub emit_function_facts_inventory: bool,
    /// Canonical `xrefs` subcommand (merged loader + optional disassembly layer).
    pub xrefs_cmd: bool,
    /// Loader seeds only (`xrefs --no-disassembly`).
    pub xref_no_disassembly: bool,
    /// Optional function VA for JSON slice (`xrefs --function`).
    pub xref_function: Option<u64>,
    /// Embed xref summary into `info --json` (`info --xrefs`).
    pub info_xrefs: bool,
    /// Embed stage metric/evidence bundle in JSON (`decomp`); requires `--json` or `--benchmark`.
    pub debug_decomp: bool,
    /// Write the same debug bundle to a JSON file (works without embedding).
    pub debug_decomp_bundle: Option<PathBuf>,
    /// Canonical `callgraph` subcommand.
    pub callgraph_cmd: bool,
}

impl Default for OneShotArgs {
    fn default() -> Self {
        Self {
            binary: PathBuf::default(),
            address: None,
            list: false,
            info: false,
            sections: false,
            imports: false,
            exports: false,
            info_detections: false,
            info_identity: false,
            strings: None,
            disasm: None,
            disasm_function: None,
            raw_pcode: None,
            raw_pcode_max_bytes: 4096,
            raw_pcode_instruction_limit: 512,
            raw_pcode_continue_past_indirect: false,
            pcode_stages: None,
            pcode_stages_max_bytes: 0x4000,
            pcode_stages_instruction_limit: 512,
            pcode_stages_strict_indirect_stop: false,
            nir_stats: None,
            nir_stats_max_bytes: 0x4000,
            nir_stats_instruction_limit: 512,
            nir_stats_strict_indirect_stop: false,
            pcode_topology: None,
            pcode_topology_max_bytes: 0x4000,
            pcode_topology_instruction_limit: 512,
            pcode_topology_strict_indirect_stop: false,
            count: 20,
            compiler_id: None,
            profile: None,
            engine: None,
            output: None,
            json: false,
            verbose: false,
            no_header: false,
            ghidra_compat: false,
            no_warnings: false,
            benchmark: false,
            decomp_all: false,
            decomp_limit: None,
            include_nonuser_functions: false,
            timeout_ms: None,
            format: None,
            function_discovery_profile: None,
            preview_candidate_inventory: false,
            preview_candidate_limit: None,
            preview_candidate_scan_batch: false,
            addresses_file: None,
            functions_limit: None,
            chunk_size: None,
            output_jsonl: None,
            summary_json: None,
            resume_from: None,
            quiet_batch_errors: false,
            emit_function_facts_inventory: false,
            xrefs_cmd: false,
            xref_no_disassembly: false,
            xref_function: None,
            info_xrefs: false,
            debug_decomp: false,
            debug_decomp_bundle: None,
            callgraph_cmd: false,
        }
    }
}

impl OneShotArgs {
    pub(super) fn with_binary(binary: PathBuf) -> Self {
        Self {
            binary,
            ..Self::default()
        }
    }
}
