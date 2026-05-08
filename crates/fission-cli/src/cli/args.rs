//! Common CLI argument parsing utilities

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FunctionDiscoveryProfileArg {
    Conservative,
    Balanced,
    Aggressive,
}

/// Parse hexadecimal address string (supports 0x prefix)
pub fn parse_hex_address(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address: {}", e))
}

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
        }
    }
}

impl OneShotArgs {
    fn with_binary(binary: PathBuf) -> Self {
        Self {
            binary,
            ..Self::default()
        }
    }
}

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

/// Parsed top-level CLI invocation (legacy one-shot pipeline vs Rhai script runner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedInvocation {
    OneShot(ParsedOneShotArgs),
    Script(ScriptInvocation),
    ResourcesStatus { json: bool, verbose: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptInvocation {
    pub verbose: bool,
    pub cmd: ScriptCmd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptCmd {
    Check {
        script: PathBuf,
    },
    Run {
        binary: PathBuf,
        script: PathBuf,
        json: bool,
    },
}

#[derive(Args, Clone, Debug, Default)]
struct CommonBinaryOutputArgs {
    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Parser, Debug)]
#[command(name = "fission_cli")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Rust-native binary analysis and decompilation")]
#[command(
    long_about = "Fission is a headless-first binary analysis and decompilation tool with explicit one-shot subcommands.\n\nCanonical human-facing entrypoints:\n  fission_cli info binary.exe\n  fission_cli list binary.exe --json\n  fission_cli disasm binary.exe --addr 0x1400\n  fission_cli decomp binary.exe --addr 0x1400\n  fission_cli strings binary.exe --min-len 6\n  fission_cli xrefs binary.exe --json\n\nOperator-oriented inventory lives under:\n  fission_cli inventory <SUBCOMMAND> ...\n"
)]
#[command(arg_required_else_help = true)]
struct CliArgs {
    /// Override resource bundle root (signatures, detectors). Also read from FISSION_RESOURCE_ROOT.
    #[arg(long = "resource-root", global = true, value_name = "DIR")]
    resource_root: Option<PathBuf>,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Show binary metadata and inventory views
    Info(InfoArgs),
    /// List discovered functions
    List(ListArgs),
    /// Disassemble instructions or a full function
    Disasm(DisasmArgs),
    /// Decompile one function or all discovered functions
    Decomp(DecompArgs),
    /// Extract binary strings
    Strings(StringsArgs),
    /// Canonical cross-reference index (loader seeds + optional disassembly layer)
    Xrefs(XrefsArgs),
    /// Operator-oriented inventory and batch emitters
    Inventory(InventoryArgs),
    /// Inspect resolved resource paths and bundle-root candidates
    Resources(ResourcesArgs),
    /// Rhai scripts over read-only binary inventory (`binary.*`, `emit`)
    Script(ScriptArgs),
}

#[derive(Args, Debug)]
#[command(
    long_about = "Show binary metadata plus optional section/import/export inventories.\n\nUse this command for quick facts about the loaded binary without entering the decompilation path.",
    after_help = "Examples:\n  fission_cli info app.exe\n  fission_cli info app.exe --sections\n  fission_cli info app.exe --imports --json\n  fission_cli info app.exe --detections --json\n  fission_cli info app.exe --identity --json\n  fission_cli info app.exe --xrefs --json"
)]
struct InfoArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Run integrated detection (section/import/string rules plus Detect It Easy signatures)
    #[arg(long)]
    detections: bool,

    /// Attach structured loader identity report (entropy, overlay, PE hints with evidence)
    #[arg(long)]
    identity: bool,

    /// Show section information
    #[arg(short = 'S', long)]
    sections: bool,

    /// List imported functions
    #[arg(short = 'I', long)]
    imports: bool,

    /// List exported functions
    #[arg(short = 'E', long)]
    exports: bool,

    /// Attach canonical xref index summary (JSON adds `xrefs`; text mode prints a short section)
    #[arg(long)]
    xrefs: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "List discovered functions for a binary.\n\nThis is the canonical function inventory surface for humans and automation.",
    after_help = "Examples:\n  fission_cli list app.exe\n  fission_cli list app.exe --json"
)]
struct ListArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Disassemble instructions at a specific address or decode the full containing function.\n\nUse `--function` when you want function boundaries rather than a fixed instruction window.",
    after_help = "Examples:\n  fission_cli disasm app.exe --addr 0x140001000\n  fission_cli disasm app.exe --addr 0x140001000 --count 64\n  fission_cli disasm app.exe --addr 0x140001000 --function --json"
)]
struct DisasmArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Address to disassemble
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Disassemble the full function instead of a fixed instruction count
    #[arg(long)]
    function: bool,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value_t = 20)]
    count: usize,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(group(
    clap::ArgGroup::new("decomp_target")
        .required(true)
        .args(["addr", "all"])
))]
#[command(
    long_about = "Decompile one function or all discovered functions.\n\nThis is the canonical human-facing decompilation entrypoint. Use `--addr` for focused analysis and `--all` only for bounded batch-style local runs.\n\nBy default, `--all` filters imported functions and the zero-size `register_frame_ctor` runtime wrapper. Use `--include-nonuser-functions` to restore compatibility/forensics coverage.",
    after_help = "Examples:\n  fission_cli decomp app.exe --addr 0x140001000\n  fission_cli decomp app.exe --addr 0x140001000 --ghidra-compat\n  fission_cli decomp app.exe --all --limit 10 --json\n  fission_cli decomp app.exe --all --include-nonuser-functions --json"
)]
struct DecompArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Decompile function at specific address
    #[arg(long, value_parser = parse_hex_address)]
    addr: Option<u64>,

    /// Decompile all discovered functions
    #[arg(long)]
    all: bool,

    /// With --all, limit to first N functions
    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    include_nonuser_functions: bool,

    /// Decompilation profile (balanced|quality|speed|nir; mlil-preview is a deprecated alias)
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,

    /// Decompilation engine (auto|nir|rust-sleigh; mlil-preview is a deprecated alias, legacy is a hidden compat mode)
    #[arg(long, value_name = "ENGINE")]
    engine: Option<String>,

    /// Override decompiler compiler ID (auto|windows|gcc|clang|default)
    #[arg(long, value_name = "ID")]
    compiler_id: Option<String>,

    /// Decompilation timeout per function in milliseconds (0 = no timeout)
    #[arg(long, value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Output to file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Suppress the '// ===...=== Function: ...' header comment in output
    #[arg(long)]
    no_header: bool,

    /// Suppress WARNING/NOTICE diagnostic lines in decompilation output
    #[arg(long)]
    no_warnings: bool,

    /// Ghidra-compatible output mode
    #[arg(long)]
    ghidra_compat: bool,

    /// Benchmark mode: record per-function timing in JSON output
    #[arg(long)]
    benchmark: bool,

    /// Embed developer/benchmark debug bundle (`debug_decomp`) in JSON output (requires `--json` or `--benchmark`)
    #[arg(long)]
    debug_decomp: bool,

    /// Write debug bundle JSON to FILE (same payload as `debug_decomp`; does not require `--json`)
    #[arg(long, value_name = "FILE")]
    debug_decomp_bundle: Option<PathBuf>,

    /// Override binary format detection (auto|pe|elf|macho)
    #[arg(long, value_name = "FORMAT", hide = true)]
    format: Option<String>,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Extract printable strings from the binary image.",
    after_help = "Examples:\n  fission_cli strings app.exe\n  fission_cli strings app.exe --min-len 8 --json"
)]
struct StringsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Minimum string length
    #[arg(long = "min-len", default_value_t = 4)]
    min_len: usize,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit canonical cross-reference records (`fission-static::xref_index`): loader-derived seeds plus optional Sleigh disassembly layer.\n\nDisassembly requires a usable load spec on `LoadedBinary`. Use `--no-disassembly` for import/export/string/global anchors only.",
    after_help = "Examples:\n  fission_cli xrefs app.exe --json\n  fission_cli xrefs app.exe --no-disassembly --json\n  fission_cli xrefs app.exe --function 0x140001000 --json"
)]
struct XrefsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Skip Sleigh xref extraction (loader seeds only)
    #[arg(long)]
    no_disassembly: bool,

    /// Include per-function xref slice for this function entry VA in JSON output
    #[arg(long, value_parser = parse_hex_address)]
    function: Option<u64>,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Inspect bundled runtime resources (signatures, type corpora, detectors).\n\nUse `resources status` to see candidate bundle roots and paths resolved via `PATHS`.",
    after_help = "Examples:\n  fission_cli resources status\n  fission_cli resources status --json\n  fission_cli --resource-root /opt/fission-data resources status --json"
)]
struct ResourcesArgs {
    #[command(subcommand)]
    command: ResourcesCommand,
}

#[derive(Subcommand, Debug)]
enum ResourcesCommand {
    /// Show candidate bundle roots and resolved resource paths
    Status(ResourcesStatusArgs),
}

#[derive(Args, Debug)]
struct ResourcesStatusArgs {
    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Compile-check or run embedded Rhai scripts with read-only access to loaded binary inventory.\n\nScripts receive `binary` (path, format, functions, imports, …) and may call `emit` to record structured findings.",
    after_help = "Examples:\n  fission_cli script check --script scan.rhai\n  fission_cli script run app.exe --script scan.rhai --json"
)]
struct ScriptArgs {
    #[command(subcommand)]
    command: ScriptCommand,
}

#[derive(Subcommand, Debug)]
enum ScriptCommand {
    /// Syntax-check a Rhai script (no binary load)
    Check(ScriptCheckArgs),
    /// Load a binary, run Balanced discovery, then evaluate the script
    Run(ScriptRunArgs),
}

#[derive(Args, Debug)]
struct ScriptCheckArgs {
    /// Path to the `.rhai` script
    #[arg(long, value_name = "FILE")]
    script: PathBuf,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args, Debug)]
struct ScriptRunArgs {
    /// Binary to analyze
    binary: PathBuf,

    /// Path to the `.rhai` script
    #[arg(long, value_name = "FILE")]
    script: PathBuf,

    #[arg(short, long)]
    json: bool,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Operator-oriented inventory and batch emitters.\n\nThese commands are intended for automation, corpus curation, and offline report generation rather than the primary one-shot decompilation flow.",
    after_help = "Examples:\n  fission_cli inventory function-facts app.exe --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory preview-candidates app.exe --inventory"
)]
struct InventoryArgs {
    #[command(subcommand)]
    command: InventoryCommand,
}

#[derive(Subcommand, Debug)]
enum InventoryCommand {
    /// Emit whole-binary function facts inventory as JSONL plus summary JSON
    FunctionFacts(InventoryFunctionFactsArgs),
    /// Emit preview candidate inventory and batch scans
    PreviewCandidates(InventoryPreviewCandidatesArgs),
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit whole-binary function facts inventory as JSONL plus summary JSON.\n\nThis is an operator/batch surface used by automation and reporting lanes.\n\nWhole-binary selection filters imported functions and the zero-size `register_frame_ctor` runtime wrapper by default. Use `--include-nonuser-functions` to restore compatibility/forensics coverage.",
    after_help = "Examples:\n  fission_cli inventory function-facts app.exe --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory function-facts app.exe --include-nonuser-functions --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory function-facts app.exe --addr 0x140001000 --summary-json summary.json"
)]
struct InventoryFunctionFactsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Restrict inventory emit to one function address
    #[arg(long, value_parser = parse_hex_address)]
    addr: Option<u64>,

    /// File containing one hex function address per line
    #[arg(long, value_name = "FILE")]
    addresses_file: Option<PathBuf>,

    /// Limit number of functions selected
    #[arg(long, value_name = "N")]
    functions_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    include_nonuser_functions: bool,

    /// Batch chunk size
    #[arg(long, value_name = "N")]
    chunk_size: Option<usize>,

    /// Output JSONL path
    #[arg(long, value_name = "FILE")]
    output_jsonl: Option<PathBuf>,

    /// Output summary JSON path
    #[arg(long, value_name = "FILE")]
    summary_json: Option<PathBuf>,

    /// Resume from an existing JSONL file
    #[arg(long, value_name = "FILE")]
    resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output
    #[arg(long)]
    quiet_batch_errors: bool,

    /// Override decompiler compiler ID
    #[arg(long, value_name = "ID")]
    compiler_id: Option<String>,

    /// Decompilation profile
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,

    /// Decompilation timeout per function in milliseconds
    #[arg(long, value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit preview candidate inventory rows or run preview candidate batch scans.\n\nThis surface exists for operator-grade corpus curation and candidate analysis, not normal interactive decompilation.\n\nWhole-binary selection filters imported functions and the zero-size `register_frame_ctor` runtime wrapper by default. Use `--include-nonuser-functions` to restore compatibility/forensics coverage.",
    after_help = "Examples:\n  fission_cli inventory preview-candidates app.exe --inventory\n  fission_cli inventory preview-candidates app.exe --batch --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory preview-candidates app.exe --batch --include-nonuser-functions --output-jsonl rows.jsonl --summary-json summary.json"
)]
struct InventoryPreviewCandidatesArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Emit preview candidate inventory JSON
    #[arg(long)]
    inventory: bool,

    /// Run preview candidate scan in batch mode inside one process
    #[arg(long)]
    batch: bool,

    /// Restrict inventory emit to one function address
    #[arg(long, value_parser = parse_hex_address)]
    addr: Option<u64>,

    /// Limit functions included in preview candidate inventory
    #[arg(long, value_name = "N")]
    preview_candidate_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    include_nonuser_functions: bool,

    /// File containing one hex function address per line
    #[arg(long, value_name = "FILE")]
    addresses_file: Option<PathBuf>,

    /// Limit number of functions selected for batch preview scan
    #[arg(long, value_name = "N")]
    functions_limit: Option<usize>,

    /// Batch chunk size for preview candidate scan
    #[arg(long, value_name = "N")]
    chunk_size: Option<usize>,

    /// Output JSONL path for batch preview candidate rows
    #[arg(long, value_name = "FILE")]
    output_jsonl: Option<PathBuf>,

    /// Output summary JSON path for batch preview candidate scan
    #[arg(long, value_name = "FILE")]
    summary_json: Option<PathBuf>,

    /// Resume batch preview candidate scan from an existing JSONL file
    #[arg(long, value_name = "FILE")]
    resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output
    #[arg(long)]
    quiet_batch_errors: bool,

    /// Override decompiler compiler ID
    #[arg(long, value_name = "ID")]
    compiler_id: Option<String>,

    /// Decompilation profile
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,

    /// Decompilation timeout per function in milliseconds
    #[arg(long, value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
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
struct LegacyCliArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Decompile function at specific address (hex, e.g., 0x140001400)
    #[arg(short, long, alias = "decomp", value_parser = parse_hex_address)]
    address: Option<u64>,

    /// List all discovered functions
    #[arg(short, long, alias = "funcs")]
    list: bool,

    /// Show binary information
    #[arg(short, long)]
    info: bool,

    /// Show section information
    #[arg(short = 'S', long)]
    sections: bool,

    /// List imported functions
    #[arg(short = 'I', long)]
    imports: bool,

    /// List exported functions
    #[arg(short = 'E', long)]
    exports: bool,

    /// Extract strings from binary (min length)
    #[arg(long, value_name = "MIN_LEN", num_args = 0..=1, default_missing_value = "4")]
    strings: Option<usize>,

    /// Disassemble at address (with optional count)
    #[arg(short = 'd', long, alias = "asm", value_parser = parse_hex_address)]
    disasm: Option<u64>,

    /// Disassemble entire function at address (function boundaries)
    #[arg(long, alias = "asm-func", value_parser = parse_hex_address)]
    disasm_function: Option<u64>,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value = "20")]
    count: usize,

    /// Override decompiler compiler ID (auto|windows|gcc|clang|default)
    #[arg(long, value_name = "ID")]
    compiler_id: Option<String>,

    /// Decompilation profile (balanced|quality|speed|nir; mlil-preview is a deprecated alias)
    #[arg(long, value_name = "PROFILE")]
    profile: Option<String>,

    /// Decompilation engine (auto|nir|rust-sleigh; mlil-preview is a deprecated alias, legacy is a hidden compat mode)
    #[arg(long, value_name = "ENGINE")]
    engine: Option<String>,

    /// Output to file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Suppress the '// ===...=== Function: ...' header comment in decompilation output
    #[arg(long)]
    no_header: bool,

    /// Ghidra-compatible output mode
    #[arg(long)]
    ghidra_compat: bool,

    /// Suppress WARNING/NOTICE diagnostic lines in decompilation output
    #[arg(long)]
    no_warnings: bool,

    /// Benchmark mode: record per-function decomp_sec timing in JSON output
    #[arg(long)]
    benchmark: bool,

    /// Embed developer/benchmark debug bundle in JSON output (requires `--json` or `--benchmark`)
    #[arg(long)]
    debug_decomp: bool,

    /// Write debug bundle JSON to FILE
    #[arg(long, value_name = "FILE")]
    debug_decomp_bundle: Option<PathBuf>,

    /// Decompile all discovered functions (batch mode)
    #[arg(long, alias = "all")]
    decomp_all: bool,

    /// With --decomp-all, limit to first N functions
    #[arg(long, value_name = "N")]
    decomp_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    include_nonuser_functions: bool,

    /// Decompilation timeout per function in milliseconds (0 = no timeout)
    #[arg(long, value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Override binary format detection (auto|pe|elf|macho)
    #[arg(long, value_name = "FORMAT")]
    format: Option<String>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Emit preview candidate inventory JSON for quality-corpus curation
    #[arg(long, hide = true)]
    preview_candidate_inventory: bool,

    /// Limit functions included in preview candidate inventory
    #[arg(long, hide = true, value_name = "N")]
    preview_candidate_limit: Option<usize>,

    /// Run preview candidate scan in batch mode inside one process
    #[arg(long, hide = true)]
    preview_candidate_scan_batch: bool,

    /// File containing one hex function address per line for batch preview scan
    #[arg(long, hide = true, value_name = "FILE")]
    addresses_file: Option<PathBuf>,

    /// Limit number of functions selected for batch preview scan
    #[arg(long, hide = true, value_name = "N")]
    functions_limit: Option<usize>,

    /// Batch chunk size for preview candidate scan
    #[arg(long, hide = true, value_name = "N")]
    chunk_size: Option<usize>,

    /// Output JSONL path for batch preview candidate rows
    #[arg(long, hide = true, value_name = "FILE")]
    output_jsonl: Option<PathBuf>,

    /// Output summary JSON path for batch preview candidate scan
    #[arg(long, hide = true, value_name = "FILE")]
    summary_json: Option<PathBuf>,

    /// Resume batch preview candidate scan from an existing JSONL file
    #[arg(long, hide = true, value_name = "FILE")]
    resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output during batch preview candidate scans
    #[arg(long, hide = true)]
    quiet_batch_errors: bool,

    /// Emit whole-binary function facts inventory as JSONL plus summary JSON
    #[arg(long, hide = true)]
    emit_function_facts_inventory: bool,
}

const CANONICAL_SUBCOMMANDS: &[&str] = &[
    "info",
    "list",
    "disasm",
    "decomp",
    "strings",
    "xrefs",
    "inventory",
    "resources",
    "script",
];

fn should_use_canonical_parser(argv: &[OsString]) -> bool {
    if argv.len() <= 1 {
        return true;
    }

    // Global flags may precede the subcommand (`fission_cli --resource-root DIR resources status`).
    if argv.iter().any(|arg| {
        arg.to_str()
            .is_some_and(|s| CANONICAL_SUBCOMMANDS.contains(&s))
    }) {
        return true;
    }

    match argv[1].to_str() {
        Some("help" | "--help" | "-h" | "--version" | "-V") => true,
        _ => false,
    }
}

pub fn parse_oneshot_args() -> ParsedInvocation {
    parse_oneshot_args_from(std::env::args_os())
}

pub fn parse_oneshot_args_from<I, T>(iter: I) -> ParsedInvocation
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let argv: Vec<OsString> = iter.into_iter().map(Into::into).collect();
    if should_use_canonical_parser(&argv) {
        let cli = CliArgs::parse_from(argv);
        fission_core::resource_roots::set_cli_resource_bundle_root(cli.resource_root.clone());
        normalize_canonical(cli)
    } else {
        ParsedInvocation::OneShot(normalize_legacy(LegacyCliArgs::parse_from(argv)))
    }
}

fn normalize_canonical(cli: CliArgs) -> ParsedInvocation {
    match cli.command {
        CliCommand::Script(script) => {
            let invocation = match script.command {
                ScriptCommand::Check(c) => ScriptInvocation {
                    verbose: c.verbose,
                    cmd: ScriptCmd::Check { script: c.script },
                },
                ScriptCommand::Run(r) => ScriptInvocation {
                    verbose: r.verbose,
                    cmd: ScriptCmd::Run {
                        binary: r.binary,
                        script: r.script,
                        json: r.json,
                    },
                },
            };
            ParsedInvocation::Script(invocation)
        }
        CliCommand::Resources(resources) => match resources.command {
            ResourcesCommand::Status(s) => ParsedInvocation::ResourcesStatus {
                json: s.common.json,
                verbose: s.common.verbose,
            },
        },
        cmd => {
            let args = match cmd {
                CliCommand::Info(info) => {
                    let mut args = OneShotArgs::with_binary(info.binary);
                    args.json = info.common.json;
                    args.verbose = info.common.verbose;
                    args.sections = info.sections;
                    args.imports = info.imports;
                    args.exports = info.exports;
                    args.info_detections = info.detections;
                    args.info_identity = info.identity;
                    args.info_xrefs = info.xrefs;
                    args.info = !args.sections && !args.imports && !args.exports;
                    args
                }
                CliCommand::List(list) => {
                    let mut args = OneShotArgs::with_binary(list.binary);
                    args.list = true;
                    args.json = list.common.json;
                    args.verbose = list.common.verbose;
                    args
                }
                CliCommand::Disasm(disasm) => {
                    let mut args = OneShotArgs::with_binary(disasm.binary);
                    args.count = disasm.count;
                    args.json = disasm.common.json;
                    args.verbose = disasm.common.verbose;
                    if disasm.function {
                        args.disasm_function = Some(disasm.addr);
                    } else {
                        args.disasm = Some(disasm.addr);
                    }
                    args
                }
                CliCommand::Decomp(decomp) => {
                    let mut args = OneShotArgs::with_binary(decomp.binary);
                    args.address = decomp.addr;
                    args.decomp_all = decomp.all;
                    args.decomp_limit = decomp.limit;
                    args.include_nonuser_functions = decomp.include_nonuser_functions;
                    args.profile = decomp.profile;
                    args.engine = decomp.engine;
                    args.compiler_id = decomp.compiler_id;
                    args.timeout_ms = decomp.timeout_ms;
                    args.function_discovery_profile = decomp.function_discovery_profile;
                    args.json = decomp.json;
                    args.output = decomp.output;
                    args.verbose = decomp.verbose;
                    args.no_header = decomp.no_header;
                    args.no_warnings = decomp.no_warnings;
                    args.ghidra_compat = decomp.ghidra_compat;
                    args.benchmark = decomp.benchmark;
                    args.debug_decomp = decomp.debug_decomp;
                    args.debug_decomp_bundle = decomp.debug_decomp_bundle;
                    args.format = decomp.format;
                    args
                }
                CliCommand::Strings(strings) => {
                    let mut args = OneShotArgs::with_binary(strings.binary);
                    args.strings = Some(strings.min_len);
                    args.json = strings.common.json;
                    args.verbose = strings.common.verbose;
                    args
                }
                CliCommand::Xrefs(xrefs) => {
                    let mut args = OneShotArgs::with_binary(xrefs.binary);
                    args.xrefs_cmd = true;
                    args.xref_no_disassembly = xrefs.no_disassembly;
                    args.xref_function = xrefs.function;
                    args.json = xrefs.common.json;
                    args.verbose = xrefs.common.verbose;
                    args
                }
                CliCommand::Inventory(inventory) => match inventory.command {
                    InventoryCommand::FunctionFacts(facts) => {
                        let mut args = OneShotArgs::with_binary(facts.binary);
                        args.emit_function_facts_inventory = true;
                        args.address = facts.addr;
                        args.addresses_file = facts.addresses_file;
                        args.functions_limit = facts.functions_limit;
                        args.include_nonuser_functions = facts.include_nonuser_functions;
                        args.chunk_size = facts.chunk_size;
                        args.output_jsonl = facts.output_jsonl;
                        args.summary_json = facts.summary_json;
                        args.resume_from = facts.resume_from;
                        args.quiet_batch_errors = facts.quiet_batch_errors;
                        args.compiler_id = facts.compiler_id;
                        args.profile = facts.profile;
                        args.timeout_ms = facts.timeout_ms;
                        args.function_discovery_profile = facts.function_discovery_profile;
                        args.verbose = facts.verbose;
                        args
                    }
                    InventoryCommand::PreviewCandidates(preview) => {
                        let mut args = OneShotArgs::with_binary(preview.binary);
                        args.preview_candidate_inventory = preview.inventory || !preview.batch;
                        args.preview_candidate_scan_batch = preview.batch;
                        args.address = preview.addr;
                        args.preview_candidate_limit = preview.preview_candidate_limit;
                        args.include_nonuser_functions = preview.include_nonuser_functions;
                        args.addresses_file = preview.addresses_file;
                        args.functions_limit = preview.functions_limit;
                        args.chunk_size = preview.chunk_size;
                        args.output_jsonl = preview.output_jsonl;
                        args.summary_json = preview.summary_json;
                        args.resume_from = preview.resume_from;
                        args.quiet_batch_errors = preview.quiet_batch_errors;
                        args.compiler_id = preview.compiler_id;
                        args.profile = preview.profile;
                        args.timeout_ms = preview.timeout_ms;
                        args.function_discovery_profile = preview.function_discovery_profile;
                        args.verbose = preview.verbose;
                        args
                    }
                },
                CliCommand::Script(_) => unreachable!("script branch handled above"),
                CliCommand::Resources(_) => unreachable!("resources branch handled above"),
            };
            ParsedInvocation::OneShot(ParsedOneShotArgs {
                args,
                legacy_warning: None,
            })
        }
    }
}

fn normalize_legacy(cli: LegacyCliArgs) -> ParsedOneShotArgs {
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
        count: cli.count,
        compiler_id: cli.compiler_id,
        profile: cli.profile,
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
        xrefs_cmd: false,
        xref_no_disassembly: false,
        xref_function: None,
        info_xrefs: false,
    };

    ParsedOneShotArgs {
        args,
        legacy_warning,
    }
}

fn legacy_warning_kind(cli: &LegacyCliArgs) -> Option<LegacyInvocationKind> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse_canonical(args: &[&str]) -> ParsedOneShotArgs {
        match parse_oneshot_args_from(args.iter().copied()) {
            ParsedInvocation::OneShot(p) => p,
            ParsedInvocation::Script(_) => panic!("expected one-shot canonical parse"),
            ParsedInvocation::ResourcesStatus { .. } => {
                panic!("expected one-shot canonical parse")
            }
        }
    }

    fn parse_legacy(args: &[&str]) -> ParsedOneShotArgs {
        match parse_oneshot_args_from(args.iter().copied()) {
            ParsedInvocation::OneShot(p) => p,
            ParsedInvocation::Script(_) => panic!("legacy parser cannot emit script"),
            ParsedInvocation::ResourcesStatus { .. } => {
                panic!("legacy parser cannot emit resources status")
            }
        }
    }

    #[test]
    fn canonical_script_check_parsing() {
        let inv =
            parse_oneshot_args_from(["fission_cli", "script", "check", "--script", "rules.rhai"]);
        match inv {
            ParsedInvocation::Script(s) => {
                assert!(!s.verbose);
                match s.cmd {
                    ScriptCmd::Check { script } => {
                        assert_eq!(script, PathBuf::from("rules.rhai"));
                    }
                    _ => panic!("expected check"),
                }
            }
            _ => panic!("expected script invocation"),
        }
    }

    #[test]
    fn canonical_script_run_parsing() {
        let inv = parse_oneshot_args_from([
            "fission_cli",
            "script",
            "run",
            "app.exe",
            "--script",
            "scan.rhai",
            "--json",
            "--verbose",
        ]);
        match inv {
            ParsedInvocation::Script(s) => {
                assert!(s.verbose);
                match s.cmd {
                    ScriptCmd::Run {
                        binary,
                        script,
                        json,
                    } => {
                        assert_eq!(binary, PathBuf::from("app.exe"));
                        assert_eq!(script, PathBuf::from("scan.rhai"));
                        assert!(json);
                    }
                    _ => panic!("expected run"),
                }
            }
            _ => panic!("expected script invocation"),
        }
    }

    #[test]
    fn canonical_resources_status_parsing() {
        let inv = parse_oneshot_args_from(["fission_cli", "resources", "status", "--json"]);
        match inv {
            ParsedInvocation::ResourcesStatus { json, verbose } => {
                assert!(json);
                assert!(!verbose);
            }
            _ => panic!("expected ResourcesStatus"),
        }
    }

    #[test]
    fn global_resource_root_sets_override() {
        let tmp = "/tmp/fission-res-root-test";
        let inv =
            parse_oneshot_args_from(["fission_cli", "--resource-root", tmp, "resources", "status"]);
        match &inv {
            ParsedInvocation::ResourcesStatus { json, verbose } => {
                assert!(!*json);
                assert!(!*verbose);
            }
            _ => panic!("expected ResourcesStatus, got {inv:?}"),
        }
        assert_eq!(
            fission_core::resource_roots::cli_resource_bundle_root(),
            Some(PathBuf::from(tmp))
        );

        let _cleanup = parse_oneshot_args_from(["fission_cli", "info", "app.exe"]);
        assert_eq!(
            fission_core::resource_roots::cli_resource_bundle_root(),
            None
        );
    }

    #[test]
    fn canonical_info_parsing_maps_to_info_command() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe"]);
        assert_eq!(parsed.legacy_warning, None);
        assert!(parsed.args.info);
        assert_eq!(parsed.args.binary, PathBuf::from("bin.exe"));
        assert!(!parsed.args.info_detections);
        assert!(!parsed.args.info_identity);
    }

    #[test]
    fn canonical_info_identity_flag_sets_info_identity() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--identity", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.json);
        assert!(parsed.args.info_identity);
    }

    #[test]
    fn canonical_info_detections_flag_sets_info_detections() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--detections", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.json);
        assert!(parsed.args.info_detections);
    }

    #[test]
    fn canonical_info_xrefs_sets_flag() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--xrefs", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.info_xrefs);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_xrefs_subcommand_maps_fields() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "xrefs",
            "bin.exe",
            "--no-disassembly",
            "--function",
            "0x140001000",
            "--json",
        ]);
        assert!(parsed.args.xrefs_cmd);
        assert!(parsed.args.xref_no_disassembly);
        assert_eq!(parsed.args.xref_function, Some(0x140001000));
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_list_parsing_maps_to_list_command() {
        let parsed = parse_canonical(&["fission_cli", "list", "bin.exe", "--json"]);
        assert!(parsed.args.list);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_disasm_parsing_maps_to_instruction_mode() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "disasm",
            "bin.exe",
            "--addr",
            "0x1400",
            "--count",
            "32",
        ]);
        assert_eq!(parsed.args.disasm, Some(0x1400));
        assert_eq!(parsed.args.disasm_function, None);
        assert_eq!(parsed.args.count, 32);
    }

    #[test]
    fn canonical_decomp_parsing_maps_to_decomp_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x1400",
            "--ghidra-compat",
            "--benchmark",
            "--json",
        ]);
        assert_eq!(parsed.args.address, Some(0x1400));
        assert!(parsed.args.ghidra_compat);
        assert!(parsed.args.benchmark);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_decomp_debug_flags_parse() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x100",
            "--json",
            "--debug-decomp",
            "--debug-decomp-bundle",
            "/tmp/dbg.json",
        ]);
        assert!(parsed.args.debug_decomp);
        assert!(parsed.args.json);
        assert_eq!(
            parsed.args.debug_decomp_bundle,
            Some(PathBuf::from("/tmp/dbg.json"))
        );
    }

    #[test]
    fn canonical_decomp_all_limit_maps_to_batch_decomp() {
        let parsed =
            parse_canonical(&["fission_cli", "decomp", "bin.exe", "--all", "--limit", "10"]);
        assert!(parsed.args.decomp_all);
        assert_eq!(parsed.args.decomp_limit, Some(10));
    }

    #[test]
    fn canonical_decomp_all_can_include_nonuser_functions() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--all",
            "--include-nonuser-functions",
        ]);
        assert!(parsed.args.decomp_all);
        assert!(parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_inventory_function_facts_maps_to_inventory_surface() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "inventory",
            "function-facts",
            "bin.exe",
            "--output-jsonl",
            "rows.jsonl",
            "--summary-json",
            "summary.json",
        ]);
        assert!(parsed.args.emit_function_facts_inventory);
        assert_eq!(parsed.args.output_jsonl, Some(PathBuf::from("rows.jsonl")));
        assert_eq!(
            parsed.args.summary_json,
            Some(PathBuf::from("summary.json"))
        );
        assert!(!parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_inventory_preview_candidates_maps_to_inventory_surface() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "inventory",
            "preview-candidates",
            "bin.exe",
            "--batch",
            "--output-jsonl",
            "rows.jsonl",
        ]);
        assert!(parsed.args.preview_candidate_scan_batch);
        assert!(!parsed.args.preview_candidate_inventory);
        assert_eq!(parsed.args.output_jsonl, Some(PathBuf::from("rows.jsonl")));
    }

    #[test]
    fn canonical_inventory_surfaces_accept_include_nonuser_functions() {
        let facts = parse_canonical(&[
            "fission_cli",
            "inventory",
            "function-facts",
            "bin.exe",
            "--include-nonuser-functions",
            "--output-jsonl",
            "rows.jsonl",
            "--summary-json",
            "summary.json",
        ]);
        assert!(facts.args.emit_function_facts_inventory);
        assert!(facts.args.include_nonuser_functions);

        let preview = parse_canonical(&[
            "fission_cli",
            "inventory",
            "preview-candidates",
            "bin.exe",
            "--batch",
            "--include-nonuser-functions",
            "--output-jsonl",
            "rows.jsonl",
        ]);
        assert!(preview.args.preview_candidate_scan_batch);
        assert!(preview.args.include_nonuser_functions);
    }

    #[test]
    fn legacy_info_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--info"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Info));
        assert!(parsed.args.info);
    }

    #[test]
    fn legacy_list_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--funcs"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::List));
        assert!(parsed.args.list);
    }

    #[test]
    fn legacy_disasm_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--asm", "0x1400"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Disasm));
        assert_eq!(parsed.args.disasm, Some(0x1400));
    }

    #[test]
    fn legacy_decomp_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--decomp", "0x1400"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Decomp));
        assert_eq!(parsed.args.address, Some(0x1400));
    }

    #[test]
    fn legacy_batch_invocation_can_include_nonuser_functions() {
        let parsed = parse_legacy(&[
            "fission_cli",
            "bin.exe",
            "--decomp-all",
            "--include-nonuser-functions",
        ]);
        assert!(parsed.args.decomp_all);
        assert!(parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_non_decomp_subcommands_reject_decomp_only_options() {
        let err = CliArgs::try_parse_from(["fission_cli", "info", "bin.exe", "--benchmark"])
            .expect_err("expected unknown flag on info subcommand");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn canonical_non_inventory_subcommands_reject_inventory_only_options() {
        let err = CliArgs::try_parse_from([
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x1400",
            "--output-jsonl",
            "rows.jsonl",
        ])
        .expect_err("expected inventory-only flag rejection");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }
}
