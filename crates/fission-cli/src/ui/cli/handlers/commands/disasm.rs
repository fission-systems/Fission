//! Disassembly command

use crate::ui::cli::handlers::CliState;
use colored::Colorize;
use std::sync::Arc;

pub fn cmd_disasm(state: &mut CliState, addr: Option<u64>, count: Option<usize>) {
    let binary: Arc<fission_loader::loader::LoadedBinary> = match &state.binary {
        Some(b) => b.clone(),
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
            return;
        }
    };

    let addr = match addr {
        Some(a) => a,
        None => {
            println!(
                "{} Please specify an address: disasm <address> [count]",
                "[!]".yellow()
            );
            return;
        }
    };

    let count = count.unwrap_or(10);

    // Get bytes at address
    let max_bytes = count * 15; // max instruction size is ~15 bytes
    let bytes: Vec<u8> = match binary.get_bytes(addr, max_bytes) {
        Some(b) => b.to_vec(),
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
