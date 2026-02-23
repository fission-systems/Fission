//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! Entry point that handles CLI argument parsing and mode switching
//! between headless CLI and full GUI modes.

use clap::Parser;
use fission_cli::cli;
use std::path::PathBuf;

/// Fission: Hybrid Dynamic Analysis Platform
///
/// Usage examples:
///   fission                                         # Launch GUI
///   fission binary.exe                              # Analyze binary in GUI
///   fission --cli binary.exe                        # CLI interactive mode
///   fission --cli binary.exe --list                 # List all functions
///   fission --cli binary.exe --sections             # Show sections
///   fission --cli binary.exe --strings              # Show strings
///   fission --cli binary.exe --info                 # Show binary info
///   fission --cli binary.exe --xrefs 0x140001537    # Show cross-references
///   fission --cli binary.exe 0x140001537            # Decompile address
///   fission --cli binary.exe 0x140001537 --asm      # Show assembly
///   fission --cli binary.exe 0x140001537 --count 100 # 100 instructions
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target binary path to analyze
    #[arg(value_name = "BINARY")]
    target: Option<PathBuf>,

    /// Run in headless mode (CLI only, no GUI)
    #[arg(long, conflicts_with = "cli")]
    headless: bool,

    /// Run in CLI mode (alias for --headless)
    #[arg(long, conflicts_with = "headless")]
    cli: bool,

    /// Address to decompile (hex format: 0x140001537 or decimal)
    #[arg(value_name = "ADDRESS")]
    address: Option<String>,

    /// Show assembly disassembly instead of (or with) decompilation
    #[arg(long)]
    asm: bool,

    /// List all functions in the binary and exit
    #[arg(short, long)]
    list: bool,

    /// Show section information
    #[arg(long)]
    sections: bool,

    /// Show strings in the binary (optional min length)
    #[arg(long, value_name = "MIN_LEN", num_args = 0..=1, default_missing_value = "4")]
    strings: Option<usize>,

    /// Show detailed binary information
    #[arg(long)]
    info: bool,

    /// Number of instructions to disassemble (default: 50)
    #[arg(long, default_value = "50")]
    count: usize,

    /// Show cross-references for an address
    #[arg(long)]
    xrefs: Option<String>,

    /// Find string cross-references (search term)
    #[arg(long = "string-xrefs", value_name = "SEARCH")]
    string_xrefs: Option<String>,

    /// Minimum string length for string-xrefs (default: 4)
    #[arg(long = "string-min-len", default_value = "4")]
    string_min_len: usize,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> fission_core::Result<()> {
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();

    // 1. Parse command line arguments
    let args = Args::parse();

    // 2. Initialize logger from global config (fission.toml)
    // If CLI verbosity flags are provided (-v), they override the config level.
    let log_level = if args.verbose > 0 {
        match args.verbose {
            1 => fission_core::logging::LogLevel::INFO,
            2 => fission_core::logging::LogLevel::DEBUG,
            _ => fission_core::logging::LogLevel::TRACE,
        }
    } else {
        fission_core::CONFIG.logging.level.to_tracing_level()
    };
    fission_core::logging::init(log_level);

    fission_core::logging::info("Fission Core Initialized");
    fission_core::logging::debug(&format!("Target: {:?}", args.target));

    let is_cli_mode = args.headless || args.cli;
    fission_core::logging::debug(&format!(
        "Mode: {}",
        if is_cli_mode { "CLI" } else { "GUI" }
    ));

    // 3. Branch based on execution mode
    if is_cli_mode {
        let is_one_shot = args.address.is_some()
            || args.list
            || args.sections
            || args.strings.is_some()
            || args.info
            || args.xrefs.is_some()
            || args.string_xrefs.is_some();

        if is_one_shot && args.verbose == 0 {
            // Safe: we only set a process-local env var before spawning threads.
            unsafe { std::env::set_var("FISSION_SUPPRESS_NATIVE_LOGS", "1") };
        }

        // CLI mode: Run REPL in main thread
        println!("╔══════════════════════════════════════════════════════════╗");
        println!(
            "║  🔬 Fission v{} - CLI Mode               ║",
            env!("CARGO_PKG_VERSION")
        );
        println!("╚══════════════════════════════════════════════════════════╝");
        println!();

        let target_path = if let Some(ref target) = args.target {
            println!("📂 Target: {}", target.display());
            println!();
            target.to_string_lossy().to_string()
        } else {
            eprintln!("Error: Target binary path required for CLI mode");
            return Err(fission_core::errors::FissionError::Other(
                "No target binary specified".to_string(),
            ));
        };

        cli::interactive::run_cli_with_args(cli::interactive::CliRunArgs {
            target_path,
            address: args.address,
            show_asm: args.asm,
            list_functions: args.list,
            show_sections: args.sections,
            strings_min_len: args.strings,
            show_info: args.info,
            instruction_count: args.count,
            show_xrefs: args.xrefs,
            string_xrefs: args.string_xrefs,
            string_min_len: args.string_min_len,
        })
        .map_err(|e| fission_core::errors::FissionError::Other(e.to_string()))?;
    } else {
        // GUI has moved to the Tauri app (fission-tauri).
        eprintln!("GUI mode has been moved to the Fission Tauri desktop app.");
        eprintln!("Use --cli or --headless flag to run in CLI mode.");
        std::process::exit(1);
    }

    Ok(())
}
