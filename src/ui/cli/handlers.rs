//! CLI Command Handlers
//!
//! Implementation of CLI commands.

use colored::Colorize;
use std::sync::Arc;

use crate::analysis::disasm::DisasmEngine;
use crate::analysis::loader::LoadedBinary;

/// CLI session state
pub struct CliState {
    /// Currently loaded binary
    pub binary: Option<Arc<LoadedBinary>>,
    /// Disassembler engine (lazy initialized)
    pub disasm: Option<DisasmEngine>,
}

impl Default for CliState {
    fn default() -> Self {
        Self {
            binary: None,
            disasm: None,
        }
    }
}

impl CliState {
    /// Get or create disassembler for the current binary
    pub fn get_disasm(&mut self) -> Option<&DisasmEngine> {
        if self.disasm.is_none() {
            if let Some(ref binary) = self.binary {
                self.disasm = DisasmEngine::new(binary.is_64bit).ok();
            }
        }
        self.disasm.as_ref()
    }
}

pub fn cmd_load(state: &mut CliState, path: &str) {
    println!("{} Loading '{}'...", "[*]".blue(), path);

    match LoadedBinary::from_file(path) {
        Ok(binary) => {
            println!("{} {}", "[✓]".green(), "Binary loaded successfully".green());
            println!();
            println!("  {} {}", "Format:".bold(), binary.format);
            println!(
                "  {} {}",
                "Architecture:".bold(),
                if binary.is_64bit { "64-bit" } else { "32-bit" }
            );
            println!("  {} 0x{:X}", "Entry Point:".bold(), binary.entry_point);
            println!("  {} 0x{:X}", "Image Base:".bold(), binary.image_base);
            println!("  {} {}", "Sections:".bold(), binary.sections.len());
            println!("  {} {}", "Functions:".bold(), binary.functions.len());

            if binary.is_dotnet {
                println!(
                    "  {} {}",
                    ".NET:".bold(),
                    binary.dotnet_runtime_version.as_deref().unwrap_or("yes")
                );
            }

            state.binary = Some(Arc::new(binary));
            state.disasm = None; // Reset disassembler for new binary
        }
        Err(e) => {
            println!("{} Failed to load binary: {}", "[!]".red(), e);
        }
    }
}

pub fn cmd_info(state: &CliState) {
    match &state.binary {
        Some(binary) => {
            println!();
            println!("{}", "Binary Information".bold().underline());
            println!();
            println!("  {} {}", "Path:".bold(), binary.path);
            println!("  {} {}", "Format:".bold(), binary.format);
            println!("  {} {}", "Architecture:".bold(), binary.arch_spec);
            println!(
                "  {} {}",
                "Bitness:".bold(),
                if binary.is_64bit { "64-bit" } else { "32-bit" }
            );
            println!("  {} 0x{:016X}", "Entry Point:".bold(), binary.entry_point);
            println!("  {} 0x{:016X}", "Image Base:".bold(), binary.image_base);
            println!("  {} {} bytes", "File Size:".bold(), binary.data.len());
            println!("  {} {}", "Sections:".bold(), binary.sections.len());
            println!("  {} {}", "Functions:".bold(), binary.functions.len());

            if binary.is_dotnet {
                println!(
                    "  {} {}",
                    ".NET Runtime:".bold(),
                    binary
                        .dotnet_runtime_version
                        .as_deref()
                        .unwrap_or("unknown")
                );
            }
            println!();
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_functions(state: &CliState) {
    match &state.binary {
        Some(binary) => {
            let funcs = binary.functions_sorted();

            if funcs.is_empty() {
                println!("{} No functions found.", "[!]".yellow());
                return;
            }

            println!();
            println!("{} ({} total)", "Functions".bold().underline(), funcs.len());
            println!();
            println!(
                "  {:<18} {:<8} {:<6} {}",
                "Address".bold(),
                "Size".bold(),
                "Type".bold(),
                "Name".bold()
            );
            println!("  {}", "─".repeat(60));

            for func in funcs.iter().take(50) {
                let type_str = if func.is_import {
                    "IMP".yellow()
                } else if func.is_export {
                    "EXP".green()
                } else {
                    "INT".dimmed()
                };

                println!(
                    "  0x{:016X} {:>8} {:^6} {}",
                    func.address,
                    if func.size > 0 {
                        format!("{}", func.size)
                    } else {
                        "-".to_string()
                    },
                    type_str,
                    func.name
                );
            }

            if funcs.len() > 50 {
                println!(
                    "  {}",
                    format!("... and {} more", funcs.len() - 50).dimmed()
                );
            }
            println!();
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_disasm(state: &mut CliState, addr: u64, count: usize) {
    let binary = match &state.binary {
        Some(b) => b.clone(),
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
            return;
        }
    };

    // Get bytes at address
    let max_bytes = count * 15; // max instruction size is ~15 bytes
    let bytes = match binary.get_bytes(addr, max_bytes) {
        Some(b) => b,
        None => {
            println!("{} Cannot read memory at 0x{:X}", "[!]".red(), addr);
            return;
        }
    };

    // Create or reuse disassembler
    let disasm = match state.get_disasm() {
        Some(d) => d,
        None => {
            println!("{} Failed to initialize disassembler", "[!]".red());
            return;
        }
    };

    match disasm.disassemble(&bytes, addr) {
        Ok(instructions) => {
            println!();
            println!("{} @ 0x{:X}", "Disassembly".bold().underline(), addr);
            println!();

            for insn in instructions.iter().take(count) {
                let bytes_str: String = insn
                    .bytes
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");

                let mnemonic = if insn.is_flow_control {
                    insn.mnemonic.cyan().to_string()
                } else {
                    insn.mnemonic.clone()
                };

                println!(
                    "  {:016X}  {:<24} {} {}",
                    insn.address,
                    bytes_str.dimmed(),
                    mnemonic,
                    insn.operands
                );
            }
            println!();
        }
        Err(e) => {
            println!("{} Disassembly failed: {}", "[!]".red(), e);
        }
    }
}

pub fn cmd_decompile(state: &CliState, addr: u64) {
    match &state.binary {
        Some(binary) => {
            // Find function at address
            let func = binary.functions.iter().find(|f| {
                f.address == addr || (f.size > 0 && addr >= f.address && addr < f.address + f.size)
            });

            let func_name = func.map(|f| f.name.as_str()).unwrap_or("unknown");

            println!();
            println!(
                "{} {} @ 0x{:X}",
                "Decompile".bold().underline(),
                func_name,
                addr
            );
            println!();

            // Try to use the decompiler
            use crate::analysis::decomp::native::{find_cli, DecompilerServer};

            match find_cli() {
                Some(cli_path) => {
                    println!("{} Using decompiler at {:?}", "[*]".blue(), cli_path);

                    // Get SLA directory
                    let sla_dir = cli_path
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.join("ghidra_decompiler/processors"))
                        .unwrap_or_default();

                    match DecompilerServer::new(&cli_path, sla_dir.to_str().unwrap_or("")) {
                        Ok(mut server) => {
                            // Load binary into decompiler
                            let mapped_data = binary.get_memory_mapped_data();
                            if let Err(e) = server.load_binary(
                                &mapped_data,
                                binary.arch_spec.as_str(),
                                binary.image_base,
                                &binary.iat_symbols,
                                None,
                            ) {
                                println!(
                                    "{} Failed to load binary into decompiler: {}",
                                    "[!]".red(),
                                    e
                                );
                                return;
                            }

                            // Decompile
                            match server.decompile(&[], addr, binary.is_64bit) {
                                Ok(code) => {
                                    println!("{}", code);
                                }
                                Err(e) => {
                                    println!("{} Decompilation failed: {}", "[!]".red(), e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("{} Failed to start decompiler: {}", "[!]".red(), e);
                        }
                    }
                }
                None => {
                    println!(
                        "{} Decompiler CLI not found. Build the decompiler first.",
                        "[!]".yellow()
                    );
                    println!("  Run: cd ghidra_decompiler && mkdir build && cd build && cmake .. && make");
                }
            }
            println!();
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_strings(state: &CliState) {
    match &state.binary {
        Some(binary) => {
            let min_len = 4;
            let mut strings = Vec::new();

            // Simple ASCII string extraction
            let mut current_string = String::new();
            let mut start_offset = 0usize;

            for (i, &byte) in binary.data.iter().enumerate() {
                if byte >= 0x20 && byte <= 0x7E {
                    if current_string.is_empty() {
                        start_offset = i;
                    }
                    current_string.push(byte as char);
                } else {
                    if current_string.len() >= min_len {
                        strings.push((start_offset, current_string.clone()));
                    }
                    current_string.clear();
                }
            }

            println!();
            println!(
                "{} ({} found, min length: {})",
                "Strings".bold().underline(),
                strings.len(),
                min_len
            );
            println!();

            for (offset, s) in strings.iter().take(100) {
                let display = if s.len() > 60 {
                    format!("{}...", &s[..57])
                } else {
                    s.clone()
                };
                println!("  {:08X}  {}", offset, display.green());
            }

            if strings.len() > 100 {
                println!(
                    "  {}",
                    format!("... and {} more", strings.len() - 100).dimmed()
                );
            }
            println!();
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_sections(state: &CliState) {
    match &state.binary {
        Some(binary) => {
            println!();
            println!("{}", "Sections".bold().underline());
            println!();
            println!(
                "  {:<12} {:<18} {:<12} {}",
                "Name".bold(),
                "Virtual Addr".bold(),
                "Size".bold(),
                "Flags".bold()
            );
            println!("  {}", "─".repeat(60));

            for section in &binary.sections {
                let flags = format!(
                    "{}{}{}",
                    if section.is_readable { "R" } else { "-" },
                    if section.is_writable { "W" } else { "-" },
                    if section.is_executable { "X" } else { "-" }
                );

                let flags_colored = if section.is_executable {
                    flags.red()
                } else if section.is_writable {
                    flags.yellow()
                } else {
                    flags.normal()
                };

                println!(
                    "  {:<12} 0x{:016X} {:>10} {}",
                    section.name, section.virtual_address, section.virtual_size, flags_colored
                );
            }
            println!();
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_analyze(state: &mut CliState) {
    // Clone and modify
    let binary_opt = state.binary.clone();

    match binary_opt {
        Some(binary_arc) => {
            println!(
                "{} Analyzing binary for internal functions...",
                "[*]".blue()
            );

            let mut binary = (*binary_arc).clone();
            let before = binary.functions.len();

            binary.discover_internal_functions();

            let after = binary.functions.len();
            let discovered = after - before;

            state.binary = Some(Arc::new(binary));
            state.disasm = None; // Reset disassembler

            println!(
                "{} Found {} new internal functions ({} total)",
                "[✓]".green(),
                discovered,
                after
            );
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}

pub fn cmd_help() {
    println!();
    println!("{}", "Available Commands".bold().underline());
    println!();
    println!(
        "  {}         {}  Load a binary file for analysis",
        "load <path>".cyan(),
        "".dimmed()
    );
    println!(
        "  {}               {}  Show binary information",
        "info".cyan(),
        "".dimmed()
    );
    println!(
        "  {}              {}  List discovered functions",
        "funcs".cyan(),
        "".dimmed()
    );
    println!(
        "  {}           {}  Show section table",
        "sections".cyan(),
        "".dimmed()
    );
    println!(
        "  {}            {}  Extract ASCII strings",
        "strings".cyan(),
        "".dimmed()
    );
    println!(
        "  {}            {}  Analyze and discover functions",
        "analyze".cyan(),
        "".dimmed()
    );
    println!();
    println!(
        "  {} {}  Disassemble at address",
        "disasm".cyan(),
        "<addr> [count]".dimmed()
    );
    println!(
        "  {}      {}  Decompile function at address",
        "decompile".cyan(),
        "<addr>".dimmed()
    );
    println!();
    println!(
        "  {}              {}  Clear the screen",
        "clear".cyan(),
        "".dimmed()
    );
    println!(
        "  {}               {}  Show this help message",
        "help".cyan(),
        "".dimmed()
    );
    println!(
        "  {}               {}  Exit the program",
        "quit".cyan(),
        "".dimmed()
    );
    println!();
    println!(
        "{}",
        "Address formats: 0x1234, 1234 (hex if >4 digits)".dimmed()
    );
    println!();
}

pub fn cmd_clear() {
    // ANSI escape to clear screen and move cursor to top-left
    print!("\x1B[2J\x1B[1;1H");
}
