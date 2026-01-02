//! CLI Module - Headless command-line interface
//!
//! Provides a REPL (Read-Eval-Print Loop) for headless binary analysis.
//! Uses reedline for readline-style input with history and completion.

use colored::Colorize;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};

use crate::core::errors::Result;

pub mod commands;
pub mod handlers;

use commands::{parse_command, Command};
use handlers::CliState;

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
                    Command::Disasm { addr, count } => {
                        handlers::cmd_disasm(&mut state, addr, count)
                    }
                    Command::Decompile(addr) => handlers::cmd_decompile(&state, addr),
                    Command::Strings => handlers::cmd_strings(&state),
                    Command::Sections => handlers::cmd_sections(&state),
                    Command::Analyze => handlers::cmd_analyze(&mut state),
                    Command::Help => handlers::cmd_help(),
                    Command::Clear => handlers::cmd_clear(),
                    Command::Quit => {
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
