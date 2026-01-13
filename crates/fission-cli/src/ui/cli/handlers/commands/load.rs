//! Load command - Load binary files

use colored::Colorize;
use std::sync::Arc;

use crate::ui::cli::handlers::CliState;
use fission_loader::loader::LoadedBinary;

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

            state.binary = Some(Arc::new(binary));
            state.disasm = None; // Reset disassembler for new binary
        }
        Err(e) => {
            println!("{} Failed to load binary: {}", "[!]".red(), e);
        }
    }
}
