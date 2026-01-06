//! Analysis commands - Functions, sections, strings, analyze

use colored::Colorize;
use std::sync::Arc;
use crate::ui::cli::handlers::CliState;

/// List all discovered functions
pub fn cmd_functions(state: &CliState) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
            return;
        }
    };

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

/// Display section table
pub fn cmd_sections(state: &CliState) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
            return;
        }
    };

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

/// Extract and display ASCII strings
pub fn cmd_strings(state: &CliState, min_len: usize) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
            return;
        }
    };

    let min_len = min_len.max(1);
    // Pre-allocate with estimated capacity (heuristic: ~1 string per 1KB of data)
    let estimated_strings = binary.data.len() / 1024;
    let mut strings: Vec<(usize, String)> = Vec::with_capacity(estimated_strings.max(100));

    // Simple ASCII string extraction
    // Pre-allocate buffer with reasonable capacity to reduce reallocations
    let mut current_bytes: Vec<u8> = Vec::with_capacity(256);
    let mut start_offset = 0usize;

    for (i, &byte) in binary.data.iter().enumerate() {
        if byte >= 0x20 && byte <= 0x7E {
            if current_bytes.is_empty() {
                start_offset = i;
            }
            current_bytes.push(byte);
        } else {
            if current_bytes.len() >= min_len {
                // SAFETY: We only pushed bytes in 0x20-0x7E range, which are valid ASCII/UTF-8
                let value =
                    unsafe { String::from_utf8_unchecked(std::mem::take(&mut current_bytes)) };
                strings.push((start_offset, value));
            }
            current_bytes.clear();
        }
    }

    // Handle any remaining string at end of data
    if current_bytes.len() >= min_len {
        let value = unsafe { String::from_utf8_unchecked(current_bytes) };
        strings.push((start_offset, value));
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
        if s.len() > 60 {
            println!("  {:08X}  {}...", offset, s[..57].green());
        } else {
            println!("  {:08X}  {}", offset, s.green());
        }
    }

    if strings.len() > 100 {
        println!(
            "  {}",
            format!("... and {} more", strings.len() - 100).dimmed()
        );
    }
    println!();
}

/// Analyze binary and discover internal functions
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
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
        }
    }
}
