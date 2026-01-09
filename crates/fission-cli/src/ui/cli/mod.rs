//! CLI Module - Headless command-line interface
//!
//! Provides a REPL (Read-Eval-Print Loop) for headless binary analysis.
//! Uses reedline for readline-style input with history and completion.

use colored::Colorize;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::path::PathBuf;

use crate::core::errors::Result;

pub mod handlers;
pub mod commands_parser;

use handlers::CliState;
pub use commands_parser::{parse_command, Command};

/// Arguments for one-shot CLI execution.
pub struct CliRunArgs {
    pub target_path: String,
    pub address: Option<String>,
    pub show_asm: bool,
    pub list_functions: bool,
    pub show_sections: bool,
    pub strings_min_len: Option<usize>,
    pub show_info: bool,
    pub instruction_count: usize,
    pub show_xrefs: Option<String>,
    pub string_xrefs: Option<String>,
    pub string_min_len: usize,
}

/// Parse address from string (supports 0x prefix and decimal)
pub fn parse_address(addr_str: &str) -> Result<u64> {
    if let Some(hex) = addr_str.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else if let Some(hex) = addr_str.strip_prefix("0X") {
        u64::from_str_radix(hex, 16)
    } else {
        addr_str.parse::<u64>()
    }
    .map_err(|_| crate::core::errors::FissionError::Other("Invalid address format".to_string()))
}

/// Print section header with title
fn print_section_header(title: &str) {
    println!();
    println!("{}", title.cyan().bold());
    println!("{}", "─".repeat(60).dimmed());
}

/// Run CLI with command-line arguments (for direct decompilation)
pub fn run_cli_with_args(args: CliRunArgs) -> Result<()> {
    print_banner();

    let CliRunArgs {
        target_path,
        address,
        show_asm,
        list_functions,
        show_sections,
        strings_min_len,
        show_info,
        instruction_count,
        show_xrefs,
        string_xrefs,
        string_min_len,
    } = args;

    let mut state = CliState::default();
    let target = PathBuf::from(target_path);
    let target_str = target.to_string_lossy();

    // Only show loading message if not just listing
    if !list_functions || address.is_some() {
        println!("{} {}", "Loading:".cyan(), target.display());
    }

    handlers::cmd_load(&mut state, &target_str);

    if state.binary.is_none() {
        eprintln!("{}", "Failed to load binary".red());
        return Ok(());
    }

    // Handle information display commands (no address required)
    if address.is_none() {
        if list_functions {
            print_section_header("Function List");
            handlers::cmd_functions(&state);
            return Ok(());
        }

        if show_sections {
            print_section_header("Section Information");
            handlers::cmd_sections(&state);
            return Ok(());
        }

        if let Some(min_len) = strings_min_len {
            print_section_header("Strings");
            handlers::cmd_strings(&state, min_len);
            return Ok(());
        }

        if show_info {
            println!();
            handlers::cmd_info(&state);
            return Ok(());
        }

        if let Some(ref addr_str) = show_xrefs {
            let address = parse_address(addr_str)?;
            print_section_header(&format!("Cross-References for: 0x{:x}", address));
            handlers::cmd_xrefs(&state, address);
            return Ok(());
        }

        if let Some(ref search_term) = string_xrefs {
            print_section_header(&format!("String Cross-References: \"{}\"", search_term));
            handlers::cmd_string_xrefs(&state, search_term, string_min_len);
            return Ok(());
        }

        // No flags, enter interactive mode
        return run_cli();
    }

    // If address is provided, do decompilation/disassembly
    if let Some(addr_str) = address {
        let address = parse_address(&addr_str)?;
        println!();

        // Show assembly if requested
        if show_asm {
            println!("{} 0x{:x}", "Disassembling:".cyan(), address);
            println!("{}", "─".repeat(60).dimmed());
            handlers::cmd_disasm(&mut state, Some(address), Some(instruction_count));
            println!();
        }

        // Show decompilation (default behavior, unless only --asm)
        if !show_asm {
            println!("{} 0x{:x}", "Decompiling:".cyan(), address);
            println!("{}", "─".repeat(60).dimmed());
            handlers::cmd_decompile(&state, Some(address));
        }

        return Ok(());
    }

    // Otherwise, run interactive REPL
    run_cli()
}

/// Run the CLI REPL
pub fn run_cli() -> Result<()> {
    print_banner();

    let mut line_editor = Reedline::create();
    let prompt = create_prompt();
    let mut state = CliState::default();

    println!("{}", "Type 'help' for available commands.".dimmed());
    println!();

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match parse_command(line) {
                    Command::Load(path) => handlers::cmd_load(&mut state, &path),
                    Command::Info => handlers::cmd_info(&state),
                    Command::Functions => handlers::cmd_functions(&state),
                    Command::Disasm { address, count } => {
                        handlers::cmd_disasm(&mut state, address, count)
                    }
                    Command::Decompile { address } => handlers::cmd_decompile(&state, address),
                    Command::Graph { address } => handlers::cmd_graph(&state, address),
                    Command::Strings => handlers::cmd_strings(&state, 4),
                    Command::Sections => handlers::cmd_sections(&state),
                    Command::Analyze => handlers::cmd_analyze(&mut state),
                    Command::Xrefs { address } => handlers::cmd_xrefs(&state, address),
                    Command::StringXrefs { search_term, min_length } => {
                        handlers::cmd_string_xrefs(&state, &search_term, min_length)
                    }
                    Command::Help => handlers::cmd_help(),
                    Command::Clear => handlers::cmd_clear(),
                    Command::Exit => {
                        println!("{}", "Goodbye!".cyan());
                        break;
                    }
                    Command::Unknown(cmd) => {
                        println!(
                            "{} Unknown command: '{}'. Type 'help' for available commands.",
                            "Error:".red(),
                            cmd
                        );
                    }
                }
            }
            Ok(Signal::CtrlC) => {
                println!("{}", "Use 'quit' or Ctrl-D to exit.".dimmed());
            }
            Ok(Signal::CtrlD) => {
                println!("{}", "\nGoodbye!".cyan());
                break;
            }
            Err(e) => {
                println!("{} {}", "Error:".red(), e);
            }
        }
    }

    Ok(())
}

fn print_banner() {
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════════════════════════".cyan()
    );
    println!(
        "  {} v{}",
        "Fission".bold().cyan(),
        env!("CARGO_PKG_VERSION")
    );
    println!("  {}", "Next-Gen Dynamic Instrumentation Platform".dimmed());
    println!(
        "  {}",
        "\"Split the Binary, Fuse the Power.\"".italic().dimmed()
    );
    println!(
        "{}",
        "═══════════════════════════════════════════════════════════".cyan()
    );
    println!();
}

fn create_prompt() -> DefaultPrompt {
    DefaultPrompt::new(
        DefaultPromptSegment::Basic("fission".to_string()),
        DefaultPromptSegment::Empty,
    )
}
