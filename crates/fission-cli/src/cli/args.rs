//! Common CLI argument parsing utilities

use clap::Parser;
use std::path::PathBuf;

/// Parse hexadecimal address string (supports 0x prefix)
pub fn parse_hex_address(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address: {}", e))
}

/// One-shot CLI arguments (for fission_cli binary)
#[derive(Parser, Debug)]
#[command(name = "fission_cli")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Next-Gen Binary Analysis & Decompilation")]
#[command(
    long_about = "Fission - A powerful binary analysis tool with native Ghidra decompilation support.\n\nQuick Start:\n  fission_cli binary.exe -i              # Show info\n  fission_cli binary.exe -l              # List functions\n  fission_cli binary.exe --decomp 0x1400 # Decompile function\n  fission_cli binary.exe --asm 0x1400    # Disassemble\n"
)]
pub struct OneShotArgs {
    /// Path to the binary file to analyze
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

    /// Decompilation profile (balanced|quality|speed|mlil-preview)
    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Decompilation engine (auto|mlil-preview; legacy is accepted as a hidden compat mode)
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
    /// (useful for clean output or benchmark comparison against Ghidra)
    #[arg(long)]
    pub no_header: bool,

    /// Ghidra-compatible output mode: implies --no-header, strips WARNING lines
    /// and inferred struct definitions for cleaner benchmark comparison
    #[arg(long)]
    pub ghidra_compat: bool,

    /// Suppress WARNING/NOTICE diagnostic lines in decompilation output
    #[arg(long)]
    pub no_warnings: bool,

    /// Benchmark mode: record per-function decomp_sec timing in JSON output.
    /// Implies --json. Adds initialization timing metadata.
    #[arg(long)]
    pub benchmark: bool,

    /// Decompile all discovered functions (batch mode)
    #[arg(long, alias = "all")]
    pub decomp_all: bool,

    /// With --decomp-all, limit to first N functions (for faster benchmark/testing)
    #[arg(long, value_name = "N")]
    pub decomp_limit: Option<usize>,

    /// Decompilation timeout per function in milliseconds (0 = no timeout)
    #[arg(long, value_name = "MS")]
    pub timeout_ms: Option<u64>,

    /// Override binary format detection (auto|pe|elf|macho)
    /// Affects sleigh ID selection for architecture-specific analysis
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,
}
