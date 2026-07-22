use super::{FunctionDiscoveryProfileArg, parse_hex_address};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(group(
    clap::ArgGroup::new("decomp_target")
        .required(true)
        .args(["addr", "all", "addresses_file"])
))]
#[command(
    long_about = "Decompile one function, an explicit address file, or all discovered functions.\n\nThis is the canonical human-facing decompilation entrypoint. Use `--addr` for focused analysis and `--addresses-file` for benchmark/operator batches that should reuse one loaded binary. Use `--all` only for bounded batch-style local runs.\n\nBy default, batch selection filters imported functions and the zero-size `register_frame_ctor` runtime wrapper. Use `--include-nonuser-functions` to restore compatibility/forensics coverage.",
    after_help = "Examples:\n  fission_cli decomp app.exe --addr 0x140001000\n  fission_cli decomp app.exe --addr 0x140001000 --ghidra-compat\n  fission_cli decomp app.exe --addresses-file addrs.txt --json\n  fission_cli decomp app.exe --all --limit 10 --json\n  fission_cli decomp app.exe --all --include-nonuser-functions --json"
)]
pub struct DecompArgs {
    /// Path to the binary file to analyze
    #[arg(required = true)]
    pub binary: PathBuf,

    /// Decompile function at specific address
    #[arg(long, value_parser = parse_hex_address)]
    pub addr: Option<u64>,

    /// Decompile all discovered functions
    #[arg(long)]
    pub all: bool,

    /// File containing one hex function address per line
    #[arg(long, value_name = "FILE")]
    pub addresses_file: Option<PathBuf>,

    /// With --all, limit to first N functions
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    pub include_nonuser_functions: bool,

    /// Decompilation profile (balanced|quality|speed|nir; mlil-preview is a deprecated alias)
    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Pseudocode layer: nir (semantic-faithful), hir (readable), both
    #[arg(long = "layer", value_name = "LAYER", value_parser = ["nir", "hir", "both"])]
    pub layer: Option<String>,

    /// Also emit DIR: the flattened, goto/label-based body structuring
    /// received as input, captured before its CFG-to-AST rewrite runs.
    /// Compares against HIR to show whether structuring changed a
    /// function's control flow (plain text: extra "DIR" section; JSON:
    /// extra "code_dir" field).
    #[arg(long)]
    pub dir: bool,

    /// Decompilation engine (auto|nir|rust-sleigh; mlil-preview is a deprecated alias, legacy is a hidden compat mode)
    #[arg(long, value_name = "ENGINE")]
    pub engine: Option<String>,

    /// Override decompiler compiler ID (auto|windows|gcc|clang|default)
    #[arg(long, value_name = "ID")]
    pub compiler_id: Option<String>,

    /// Decompilation timeout per function in milliseconds (0 = no timeout)
    #[arg(long, value_name = "MS")]
    pub timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(
        long,
        value_enum,
        value_name = "PROFILE",
        default_value = "conservative"
    )]
    pub function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,

    /// Output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Suppress the '// ===...=== Function: ...' header comment in output
    #[arg(long)]
    pub no_header: bool,

    /// Suppress WARNING/NOTICE diagnostic lines in decompilation output
    #[arg(long)]
    pub no_warnings: bool,

    /// Ghidra-compatible output mode
    #[arg(long)]
    pub ghidra_compat: bool,

    /// Benchmark mode: record per-function timing in JSON output
    #[arg(long)]
    pub benchmark: bool,

    /// Embed developer/benchmark debug bundle (`debug_decomp`) in JSON output (requires `--json` or `--benchmark`)
    #[arg(long)]
    pub debug_decomp: bool,

    /// Write debug bundle JSON to FILE (same payload as `debug_decomp`; does not require `--json`)
    #[arg(long, value_name = "FILE")]
    pub debug_decomp_bundle: Option<PathBuf>,

    /// Override binary format detection (auto|pe|elf|macho)
    #[arg(long, value_name = "FORMAT", hide = true)]
    pub format: Option<String>,
}
