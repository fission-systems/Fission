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
#[command(name = "fission")]
#[command(author = "Fission Dev Team")]
#[command(version = "0.2.0")]
#[command(about = "🔬 Next-Gen Binary Analysis & Decompilation")]
#[command(long_about = "Fission - A powerful binary analysis tool with native Ghidra decompilation support.\n\nQuick Start:\n  fission binary.exe -i              # Show info\n  fission binary.exe -l              # List functions\n  fission binary.exe --decomp 0x1400 # Decompile function\n  fission binary.exe --asm 0x1400    # Disassemble\n")]
pub struct OneShotArgs {
    /// Path to the binary file to analyze
    pub binary: PathBuf,

    /// Decompile function at specific address (hex, e.g., 0x140001400)
    #[arg(short, long, alias = "decomp", value_parser = parse_hex_address)]
    pub address: Option<u64>,

    /// Decompile all discovered functions
    #[arg(short = 'A', long, alias = "decomp-all")]
    pub all: bool,

    /// List all discovered functions
    #[arg(short, long)]
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

    /// Generate Pcode DOT graph for function at address
    #[arg(short = 'g', long, alias = "graph", value_parser = parse_hex_address)]
    pub graph: Option<u64>,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value = "20")]
    pub count: usize,

    /// Output to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

/// TUI CLI arguments (for fission_tui binary)
#[derive(Parser, Debug)]
#[command(name = "fission-tui")]
#[command(author = "Fission Dev Team")]
#[command(version = "0.1.0")]
#[command(about = "Terminal UI for Binary Analysis")]
pub struct TuiArgs {
    /// Path to the binary file to analyze
    pub binary: PathBuf,
}
