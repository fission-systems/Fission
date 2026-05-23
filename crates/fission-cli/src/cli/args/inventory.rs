use clap::{Args, Subcommand};
use std::path::PathBuf;
use super::{FunctionDiscoveryProfileArg, parse_hex_address};

#[derive(Args, Debug)]
#[command(
    long_about = "Operator-oriented inventory and batch emitters.\n\nThese commands are intended for automation, corpus curation, and offline report generation rather than the primary one-shot decompilation flow.",
    after_help = "Examples:\n  fission_cli inventory function-facts app.exe --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory preview-candidates app.exe --inventory"
)]
pub struct InventoryArgs {
    #[command(subcommand)]
    pub command: InventoryCommand,
}

#[derive(Subcommand, Debug)]
pub enum InventoryCommand {
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
pub struct InventoryFunctionFactsArgs {
    /// Path to the binary file to analyze
    #[arg(required = true)]
    pub binary: PathBuf,

    /// Restrict inventory emit to one function address
    #[arg(long, value_parser = parse_hex_address)]
    pub addr: Option<u64>,

    /// File containing one hex function address per line
    #[arg(long, value_name = "FILE")]
    pub addresses_file: Option<PathBuf>,

    /// Limit number of functions selected
    #[arg(long, value_name = "N")]
    pub functions_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    pub include_nonuser_functions: bool,

    /// Batch chunk size
    #[arg(long, value_name = "N")]
    pub chunk_size: Option<usize>,

    /// Output JSONL path
    #[arg(long, value_name = "FILE")]
    pub output_jsonl: Option<PathBuf>,

    /// Output summary JSON path
    #[arg(long, value_name = "FILE")]
    pub summary_json: Option<PathBuf>,

    /// Resume from an existing JSONL file
    #[arg(long, value_name = "FILE")]
    pub resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output
    #[arg(long)]
    pub quiet_batch_errors: bool,

    /// Override decompiler compiler ID
    #[arg(long, value_name = "ID")]
    pub compiler_id: Option<String>,

    /// Decompilation profile
    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Decompilation timeout per function in milliseconds
    #[arg(long, value_name = "MS")]
    pub timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    pub function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit preview candidate inventory rows or run preview candidate batch scans.\n\nThis surface exists for operator-grade corpus curation and candidate analysis, not normal interactive decompilation.\n\nWhole-binary selection filters imported functions and the zero-size `register_frame_ctor` runtime wrapper by default. Use `--include-nonuser-functions` to restore compatibility/forensics coverage.",
    after_help = "Examples:\n  fission_cli inventory preview-candidates app.exe --inventory\n  fission_cli inventory preview-candidates app.exe --batch --output-jsonl rows.jsonl --summary-json summary.json\n  fission_cli inventory preview-candidates app.exe --batch --include-nonuser-functions --output-jsonl rows.jsonl --summary-json summary.json"
)]
pub struct InventoryPreviewCandidatesArgs {
    /// Path to the binary file to analyze
    #[arg(required = true)]
    pub binary: PathBuf,

    /// Emit preview candidate inventory JSON
    #[arg(long)]
    pub inventory: bool,

    /// Run preview candidate scan in batch mode inside one process
    #[arg(long)]
    pub batch: bool,

    /// Restrict inventory emit to one function address
    #[arg(long, value_parser = parse_hex_address)]
    pub addr: Option<u64>,

    /// Limit functions included in preview candidate inventory
    #[arg(long, hide = true, value_name = "N")]
    pub preview_candidate_limit: Option<usize>,

    /// Include imported/runtime wrapper functions in batch selection
    #[arg(long)]
    pub include_nonuser_functions: bool,

    /// File containing one hex function address per line for batch preview scan
    #[arg(long, value_name = "FILE")]
    pub addresses_file: Option<PathBuf>,

    /// Limit number of functions selected for batch preview scan
    #[arg(long, value_name = "N")]
    pub functions_limit: Option<usize>,

    /// Batch chunk size for preview candidate scan
    #[arg(long, value_name = "N")]
    pub chunk_size: Option<usize>,

    /// Output JSONL path for batch preview candidate rows
    #[arg(long, value_name = "FILE")]
    pub output_jsonl: Option<PathBuf>,

    /// Output summary JSON path for batch preview candidate scan
    #[arg(long, value_name = "FILE")]
    pub summary_json: Option<PathBuf>,

    /// Resume batch preview candidate scan from an existing JSONL file
    #[arg(long, value_name = "FILE")]
    pub resume_from: Option<PathBuf>,

    /// Suppress noisy per-address panic/log output during batch preview candidate scans
    #[arg(long)]
    pub quiet_batch_errors: bool,

    /// Override decompiler compiler ID
    #[arg(long, value_name = "ID")]
    pub compiler_id: Option<String>,

    /// Decompilation profile
    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Decompilation timeout per function in milliseconds
    #[arg(long, value_name = "MS")]
    pub timeout_ms: Option<u64>,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long, value_enum, value_name = "PROFILE")]
    pub function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}
