//! Cross-reference analysis command

use colored::Colorize;
use crate::ui::cli::handlers::CliState;

pub fn cmd_xrefs(state: &CliState, addr: u64) {
    match &state.binary {
        Some(binary) => {
            println!();
            println!("{}", "Cross-Reference Analysis".cyan().bold());
            println!("{} {}", "Target Address:".dimmed(), format!("0x{:x}", addr).cyan());
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
            
            // Show potential callers (functions that might call near this address)
            println!("{}", "Potential Callers:".yellow());
            let mut call_count = 0;
            
            for func in &binary.functions {
                // Simple heuristic: show functions in reasonable range
                let distance = if func.address > addr {
                    func.address - addr
                } else {
                    addr - func.address
                };
                
                if distance < 0x100000 && func.address != addr {
                    if call_count < 10 {  // Limit to first 10
                        println!(
                            "  {} 0x{:08x}  {}",
                            if distance < 0x1000 { "▸".green() } else { "·".dimmed() },
                            func.address,
                            func.name.as_str().dimmed()
                        );
                        call_count += 1;
                    }
                }
            }
            
            if call_count == 0 {
                println!("  {}", "No nearby functions found".dimmed());
            } else if call_count == 10 {
                println!("  {}", "... (showing first 10)".dimmed());
            }
        }
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
        }
    }
}
