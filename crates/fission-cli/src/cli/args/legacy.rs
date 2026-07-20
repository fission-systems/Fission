use super::{FunctionDiscoveryProfileArg, OneShotArgs, parse_hex_address};
use clap::Parser;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyInvocationKind {
    Info,
    List,
    Disasm,
    Decomp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedOneShotArgs {
    pub args: OneShotArgs,
    pub legacy_warning: Option<LegacyInvocationKind>,
}

/// Legacy flat one-shot CLI arguments retained as a compatibility shim.
#[derive(Parser, Debug)]
#[command(name = "fission_cli")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Rust-native binary analysis and decompilation")]
#[command(
    long_about = "Legacy flat CLI compatibility surface.\n\nCanonical commands:\n  fission_cli info binary.exe\n  fission_cli list binary.exe --json\n  fission_cli disasm binary.exe --addr 0x1400\n  fission_cli decomp binary.exe --addr 0x1400\n\nLegacy flat invocations remain available as deprecated compatibility shims and normalize into the canonical subcommand execution path."
)]
pub struct LegacyCliArgs {
    /// Path to the binary file to analyze
    #[arg(required = true)]
    pub binary: PathBuf,

    /// Decompile function at specific address (hex, e.g., 0x140001400)
    #[arg(short, long, alias = "decomp", value_parser = parse_hex_address)]
    pub address: Option<u64>,

    /// List all discovered functions
    #[arg(short, long, alias = "funcs")]
    pub list: bool,

    /// Show binary information
    #[arg(short, long)]
    pub info: bool,

    /// Show section information
    #[arg(short = 'S', long)]
    pub sections: bool,

    /// List imported functions
    #[arg(short = 'I', long)]
    pub imports: bool,

    /// List exported functions
    #[arg(short = 'E', long)]
    pub exports: bool,

    /// Extract strings from binary (min length)
    #[arg(long, value_name = "MIN_LEN", num_args = 0..=1, default_missing_value = "4")]
    pub strings: Option<usize>,

    /// Disassemble at address (with optional count)
    #[arg(short = 'd', long, alias = "asm", value_parser = parse_hex_address)]
    pub disasm: Option<u64>,

    /// Disassemble entire function at address (function boundaries)
    #[arg(long, alias = "asm-func", value_parser = parse_hex_address)]
    pub disasm_function: Option<u64>,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value = "20")]
    pub count: usize,

    /// Override decompiler compiler ID (auto|windows|gcc|clang|default)
    #[arg(long, value_name = "ID")]
    pub compiler_id: Option<String>,

    /// Decompilation profile (balanced|quality|speed|nir; mlil-preview is a deprecated alias)
    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Decompilation engine (auto|nir|rust-sleigh; mlil-preview is a deprecated alias, legacy is a hidden compat mode)
    #[arg(long, value_name = "ENGINE")]
    pub engine: Option<String>,

    /// Output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Suppress the '// ===...=== Function: ...' header comment in decompilation output
    #[arg(long)]
    pub no_header: bool,

    /// Ghidra-compatible output mode
    #[arg(long)]
    pub ghidra_compat: bool,

    /// Suppress WARNING/NOTICE diagnostic lines in decompilation output
    #[arg(long)]
    pub no_warnings: bool,

    /// Benchmark mode: record per-function decomp_sec timing in JSON output
    #[arg(long)]
    pub benchmark: bool,

    /// Embed developer/benchmark debug bundle in JSON output (requires `--json` or `--benchmark`)
    #[arg(long)]
    pub debug_decomp: bool,

    /// Write debug bundle JSON to FILE
    #[arg(long, value_name = "FILE")]
    pub debug_decomp_bundle: Option<PathBuf>,

    /// Decompile all discovered functions (batch mode)
    #[arg(long, alias = "all")]
    pub decomp_all: bool,

    /// With --decomp-all, limit to first N functions
    #[arg(long, value_name = "N")]
    pub decomp_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    pub include_nonuser_functions: bool,

    /// Decompilation timeout per function in milliseconds (0 = no timeout)
    #[arg(long, value_name = "MS")]
    pub timeout_ms: Option<u64>,

    /// Override binary format detection (auto|pe|elf|macho)
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    pub function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Emit preview candidate inventory JSON for quality-corpus curation
    #[arg(long, hide = true)]
    pub preview_candidate_inventory: bool,

    /// Limit functions included in preview candidate inventory
    #[arg(long, hide = true, value_name = "N")]
    pub preview_candidate_limit: Option<usize>,

    /// Run preview candidate scan in batch mode inside one process
    #[arg(long, hide = true)]
    pub preview_candidate_scan_batch: bool,

    /// File containing one hex function address per line for batch preview scan
    #[arg(long, hide = true, value_name = "FILE")]
    pub addresses_file: Option<PathBuf>,

    /// Limit number of functions selected for batch preview scan
    #[arg(long, hide = true, value_name = "N")]
    pub functions_limit: Option<usize>,

    /// Batch chunk size for preview candidate scan
    #[arg(long, hide = true, value_name = "N")]
    pub chunk_size: Option<usize>,

    /// Output JSONL path for batch preview candidate rows
    #[arg(long, hide = true, value_name = "FILE")]
    pub output_jsonl: Option<PathBuf>,

    /// Output summary JSON path for batch preview candidate scan
    #[arg(long, hide = true, value_name = "FILE")]
    pub summary_json: Option<PathBuf>,

    /// Resume batch preview candidate scan from an existing JSONL file
    #[arg(long, hide = true, value_name = "FILE")]
    pub resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output during batch preview candidate scans
    #[arg(long, hide = true)]
    pub quiet_batch_errors: bool,

    /// Emit whole-binary function facts inventory as JSONL plus summary JSON
    #[arg(long, hide = true)]
    pub emit_function_facts_inventory: bool,
}

pub fn normalize_legacy(cli: LegacyCliArgs) -> ParsedOneShotArgs {
    let legacy_warning = legacy_warning_kind(&cli);
    let args = OneShotArgs {
        binary: cli.binary,
        address: cli.address,
        list: cli.list,
        info: cli.info,
        sections: cli.sections,
        imports: cli.imports,
        exports: cli.exports,
        info_detections: false,
        info_identity: false,
        strings: cli.strings,
        disasm: cli.disasm,
        disasm_function: cli.disasm_function,
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
        count: cli.count,
        compiler_id: cli.compiler_id,
        profile: cli.profile,
        layer: None,
        engine: cli.engine,
        output: cli.output,
        json: cli.json,
        verbose: cli.verbose,
        no_header: cli.no_header,
        ghidra_compat: cli.ghidra_compat,
        no_warnings: cli.no_warnings,
        benchmark: cli.benchmark,
        debug_decomp: cli.debug_decomp,
        debug_decomp_bundle: cli.debug_decomp_bundle.clone(),
        decomp_all: cli.decomp_all,
        decomp_limit: cli.decomp_limit,
        include_nonuser_functions: cli.include_nonuser_functions,
        timeout_ms: cli.timeout_ms,
        format: cli.format,
        function_discovery_profile: cli.function_discovery_profile,
        preview_candidate_inventory: cli.preview_candidate_inventory,
        preview_candidate_limit: cli.preview_candidate_limit,
        preview_candidate_scan_batch: cli.preview_candidate_scan_batch,
        addresses_file: cli.addresses_file,
        functions_limit: cli.functions_limit,
        chunk_size: cli.chunk_size,
        output_jsonl: cli.output_jsonl,
        summary_json: cli.summary_json,
        resume_from: cli.resume_from,
        quiet_batch_errors: cli.quiet_batch_errors,
        emit_function_facts_inventory: cli.emit_function_facts_inventory,
        emit_program_metadata: false,
        xrefs_cmd: false,
        xref_no_disassembly: false,
        xref_function: None,
        info_xrefs: false,
        callgraph_cmd: false,
        identify_cmd: false,
        identify_function: None,
    };

    ParsedOneShotArgs {
        args,
        legacy_warning,
    }
}

pub fn legacy_warning_kind(cli: &LegacyCliArgs) -> Option<LegacyInvocationKind> {
    if cli.info || cli.sections || cli.imports || cli.exports {
        return Some(LegacyInvocationKind::Info);
    }
    if cli.list {
        return Some(LegacyInvocationKind::List);
    }
    if cli.disasm.is_some() || cli.disasm_function.is_some() {
        return Some(LegacyInvocationKind::Disasm);
    }
    if cli.address.is_some() || cli.decomp_all {
        return Some(LegacyInvocationKind::Decomp);
    }
    None
}
