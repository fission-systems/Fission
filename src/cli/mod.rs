//! CLI - reedline-based REPL interface
//!
//! Provides a powerful command-line interface with autocomplete,
//! syntax highlighting, and history support.

use anyhow::Result;
use colored::Colorize;
use reedline::{
    Prompt, PromptHistorySearch, PromptHistorySearchStatus,
    Reedline, Signal,
};
use std::borrow::Cow;

/// Custom prompt for Fission CLI
pub struct FissionPrompt {
    /// Current working address (for navigation commands)
    current_address: u64,
    /// Whether a debugging session is active
    is_debugging: bool,
}

impl FissionPrompt {
    pub fn new() -> Self {
        Self {
            current_address: 0,
            is_debugging: false,
        }
    }

    pub fn set_address(&mut self, addr: u64) {
        self.current_address = addr;
    }

    pub fn set_debugging(&mut self, debugging: bool) {
        self.is_debugging = debugging;
    }
}

impl Default for FissionPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompt for FissionPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        let status = if self.is_debugging { "dbg" } else { "---" };
        Cow::Owned(format!("[{}:{:#x}]", status, self.current_address))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _prompt_mode: reedline::PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("... ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "(failed) ",
        };
        Cow::Owned(format!("(search: {}{}) ", prefix, history_search.term))
    }
}

/// Command parsing result
#[derive(Debug)]
pub enum ParsedCommand {
    /// Navigate to address: s <addr>
    Seek(u64),
    /// Print disassembly at function: pdf @ <name>
    PrintDisasmFunction(String),
    /// Print N instructions: pd <n>
    PrintDisasm(usize),
    /// Show memory map: dm
    MemoryMap,
    /// Show registers: dr
    Registers,
    /// Set breakpoint: db <addr>
    BreakpointSet(u64),
    /// Delete breakpoint: db- <addr>
    BreakpointDelete(u64),
    /// Continue execution: dc
    Continue,
    /// Step instruction: ds
    StepInto,
    /// Step over: dso
    StepOver,
    /// Load binary: o <path>
    Open(String),
    /// Python passthrough: .<code>
    Python(String),
    /// Help: ? or help
    Help,
    /// Quit: q or exit
    Quit,
    /// Unknown command
    Unknown(String),
}

/// Parse a command string into a structured command
fn parse_command(input: &str) -> ParsedCommand {
    let input = input.trim();

    // Python passthrough (starts with .)
    if let Some(code) = input.strip_prefix('.') {
        return ParsedCommand::Python(code.to_string());
    }

    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts.first().unwrap_or(&"");
    let arg = parts.get(1).map(|s| s.trim());

    match *cmd {
        // Navigation
        "s" | "seek" => {
            if let Some(addr_str) = arg {
                if let Ok(addr) = parse_address(addr_str) {
                    return ParsedCommand::Seek(addr);
                }
            }
            ParsedCommand::Unknown(input.to_string())
        }

        // Disassembly
        "pd" => {
            let count = arg.and_then(|s| s.parse().ok()).unwrap_or(10);
            ParsedCommand::PrintDisasm(count)
        }
        "pdf" => {
            // pdf @ main -> function name is after @
            if let Some(func_spec) = arg {
                let func_name = func_spec.strip_prefix("@ ").unwrap_or(func_spec);
                return ParsedCommand::PrintDisasmFunction(func_name.to_string());
            }
            ParsedCommand::PrintDisasmFunction("main".to_string())
        }

        // Memory
        "dm" => ParsedCommand::MemoryMap,

        // Debug
        "dr" | "regs" => ParsedCommand::Registers,
        "db" => {
            if let Some(addr_str) = arg {
                if let Some(addr_str) = addr_str.strip_prefix('-') {
                    if let Ok(addr) = parse_address(addr_str.trim()) {
                        return ParsedCommand::BreakpointDelete(addr);
                    }
                } else if let Ok(addr) = parse_address(addr_str) {
                    return ParsedCommand::BreakpointSet(addr);
                }
            }
            ParsedCommand::Unknown(input.to_string())
        }
        "dc" | "continue" | "c" => ParsedCommand::Continue,
        "ds" | "step" => ParsedCommand::StepInto,
        "dso" | "next" | "n" => ParsedCommand::StepOver,

        // File operations
        "o" | "open" | "load" => {
            if let Some(path) = arg {
                return ParsedCommand::Open(path.to_string());
            }
            ParsedCommand::Unknown(input.to_string())
        }

        // Help
        "?" | "help" => ParsedCommand::Help,

        // Quit
        "q" | "quit" | "exit" => ParsedCommand::Quit,

        _ => ParsedCommand::Unknown(input.to_string()),
    }
}

/// Parse an address string (supports 0x prefix and decimal)
fn parse_address(s: &str) -> Result<u64, std::num::ParseIntError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse()
    }
}

/// Print the help message
fn print_help() {
    println!("{}", "Fission CLI Commands".bold().cyan());
    println!("{}", "═".repeat(50).cyan());

    println!("\n{}", "Navigation:".bold().yellow());
    println!("  {}         Seek to address", "s <addr>".green());

    println!("\n{}", "Disassembly:".bold().yellow());
    println!("  {}          Print N disassembly lines", "pd <n>".green());
    println!(
        "  {}  Print disassembly of function",
        "pdf @ <func>".green()
    );

    println!("\n{}", "Debugging:".bold().yellow());
    println!("  {}             Show registers", "dr".green());
    println!("  {}      Set breakpoint", "db <addr>".green());
    println!("  {}     Delete breakpoint", "db- <addr>".green());
    println!("  {}             Continue execution", "dc".green());
    println!("  {}             Step into", "ds".green());
    println!("  {}            Step over", "dso".green());

    println!("\n{}", "Memory:".bold().yellow());
    println!("  {}             Show memory map", "dm".green());

    println!("\n{}", "Files:".bold().yellow());
    println!("  {}      Open/load binary", "o <path>".green());

    println!("\n{}", "Scripting:".bold().yellow());
    println!("  {}     Execute Python code", ".<code>".green());

    println!("\n{}", "Other:".bold().yellow());
    println!("  {}            Show this help", "?".green());
    println!("  {}            Quit Fission", "q".green());
}

/// Execute a parsed command
fn execute_command(cmd: ParsedCommand) {
    match cmd {
        ParsedCommand::Seek(addr) => {
            println!("[*] Seeking to {:#x}", addr);
            // TODO: Update disassembly view
        }
        ParsedCommand::PrintDisasm(count) => {
            println!("[*] Disassembling {} instructions...", count);
            // TODO: Use DisassemblyEngine
            println!("    (not implemented yet)");
        }
        ParsedCommand::PrintDisasmFunction(name) => {
            println!("[*] Disassembling function: {}", name);
            // TODO: Look up function and disassemble
            println!("    (not implemented yet)");
        }
        ParsedCommand::MemoryMap => {
            println!("[*] Memory Map:");
            println!("    {}", "(not implemented yet)".dimmed());
        }
        ParsedCommand::Registers => {
            println!("[*] Registers:");
            println!("    RAX = {:#018x}", 0u64);
            println!("    RBX = {:#018x}", 0u64);
            println!("    RCX = {:#018x}", 0u64);
            println!("    RDX = {:#018x}", 0u64);
            println!("    RSI = {:#018x}", 0u64);
            println!("    RDI = {:#018x}", 0u64);
            println!("    RBP = {:#018x}", 0u64);
            println!("    RSP = {:#018x}", 0u64);
            println!("    RIP = {:#018x}", 0u64);
        }
        ParsedCommand::BreakpointSet(addr) => {
            println!("[*] Breakpoint set at {:#x}", addr);
        }
        ParsedCommand::BreakpointDelete(addr) => {
            println!("[*] Breakpoint deleted at {:#x}", addr);
        }
        ParsedCommand::Continue => {
            println!("[*] Continuing execution...");
        }
        ParsedCommand::StepInto => {
            println!("[*] Stepping into...");
        }
        ParsedCommand::StepOver => {
            println!("[*] Stepping over...");
        }
        ParsedCommand::Open(path) => {
            println!("[*] Loading binary: {}", path);
            // TODO: Use goblin to parse
        }
        ParsedCommand::Python(code) => {
            println!("[*] Executing Python: {}", code);
            // TODO: Use PythonBridge
        }
        ParsedCommand::Help => {
            print_help();
        }
        ParsedCommand::Quit => {
            println!("[*] Shutting down...");
            std::process::exit(0);
        }
        ParsedCommand::Unknown(input) => {
            println!("{} Unknown command: '{}'", "[!]".red(), input);
            println!("    Type '?' for help");
        }
    }
}

/// Run the CLI REPL
pub fn run_cli() -> Result<()> {
    let mut line_editor = Reedline::create();
    let prompt = FissionPrompt::new();

    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗".cyan()
    );
    println!(
        "{}",
        "║  Fission CLI - Type '?' for help, 'q' to quit                ║".cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝".cyan()
    );

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                let input = buffer.trim();
                if input.is_empty() {
                    continue;
                }

                let cmd = parse_command(input);
                execute_command(cmd);
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\n[*] Interrupted");
                break;
            }
        }
    }

    Ok(())
}
