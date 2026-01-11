//! Cross-reference analysis command

use crate::ui::cli::handlers::CliState;
use colored::Colorize;
use fission_analysis::analysis::xrefs::{XrefDatabase, XrefType};

pub fn cmd_xrefs(state: &CliState, addr: u64) {
    match &state.binary {
        Some(binary) => {
            println!();
            println!("{}", "Cross-Reference Analysis".cyan().bold());
            println!(
                "{} {}",
                "Target Address:".dimmed(),
                format!("0x{:x}", addr).cyan()
            );
            println!();

            // Find function containing this address
            let mut found_function = None;
            for func in &binary.functions {
                if addr >= func.address && addr < func.address + func.size {
                    found_function = Some(func);
                    break;
                }
            }

            if let Some(func) = found_function {
                println!("{}", "Function Information:".yellow());
                println!("  Name:    {}", func.name.as_str());
                println!("  Address: 0x{:x}", func.address);
                println!("  Size:    {} bytes", func.size);
                if func.is_import {
                    println!("  Type:    {}", "IMPORT".cyan());
                } else if func.is_export {
                    println!("  Type:    {}", "EXPORT".cyan());
                } else {
                    println!("  Type:    {}", "INTERNAL".dimmed());
                }
                println!();
            } else {
                println!("{}", "Address is not within any known function".dimmed());
                println!();
            }

            // Build xref database
            println!("{}", "Building cross-reference database...".dimmed());
            let xref_db = XrefDatabase::build_from_binary(binary);
            let total_xrefs = xref_db.total_refs();
            println!(
                "{} {}",
                "Total cross-references:".dimmed(),
                total_xrefs.to_string().cyan()
            );
            println!();

            // Get references TO this address (callers)
            let refs_to = xref_db.get_refs_to(addr);
            if !refs_to.is_empty() {
                println!(
                    "{} {} {}",
                    "References TO".yellow(),
                    format!("0x{:x}", addr).cyan(),
                    format!("({} found)", refs_to.len()).dimmed()
                );
                for xref in refs_to {
                    let type_str = match xref.xref_type {
                        XrefType::Call => "CALL".green(),
                        XrefType::Jump => "JUMP".yellow(),
                        XrefType::Data => "DATA".blue(),
                    };

                    // Find function name for the caller
                    let caller_name = binary
                        .functions
                        .iter()
                        .find(|f| {
                            xref.from_addr >= f.address && xref.from_addr < f.address + f.size
                        })
                        .map(|f| f.name.as_str())
                        .unwrap_or("unknown");

                    println!(
                        "  {} 0x{:08x} → 0x{:08x}  {}",
                        type_str,
                        xref.from_addr,
                        xref.to_addr,
                        caller_name.dimmed()
                    );
                }
                println!();
            } else {
                println!("{}", "No references TO this address found".dimmed());
                println!();
            }

            // Get references FROM this address (callees)
            let refs_from = xref_db.get_refs_from(addr);
            if !refs_from.is_empty() {
                println!(
                    "{} {} {}",
                    "References FROM".yellow(),
                    format!("0x{:x}", addr).cyan(),
                    format!("({} found)", refs_from.len()).dimmed()
                );
                for xref in refs_from {
                    let type_str = match xref.xref_type {
                        XrefType::Call => "CALL".green(),
                        XrefType::Jump => "JUMP".yellow(),
                        XrefType::Data => "DATA".blue(),
                    };

                    // Find function name for the callee
                    let callee_name = binary
                        .functions
                        .iter()
                        .find(|f| xref.to_addr >= f.address && xref.to_addr < f.address + f.size)
                        .map(|f| f.name.as_str())
                        .unwrap_or("unknown");

                    println!(
                        "  {} 0x{:08x} → 0x{:08x}  {}",
                        type_str,
                        xref.from_addr,
                        xref.to_addr,
                        callee_name.dimmed()
                    );
                }
            } else {
                println!("{}", "No references FROM this address found".dimmed());
            }
        }
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
        }
    }
}
