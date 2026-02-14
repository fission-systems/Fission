//! Info command - Display binary information

use crate::ui::cli::handlers::CliState;
use colored::Colorize;

pub fn cmd_info(state: &CliState) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
            return;
        }
    };

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
    println!(
        "  {} {} bytes",
        "File Size:".bold(),
        binary.data.as_slice().len()
    );
    println!("  {} {}", "Sections:".bold(), binary.sections.len());
    println!("  {} {}", "Functions:".bold(), binary.functions.len());
    println!(
        "  {} {}",
        "Inferred Types:".bold(),
        binary.inferred_types.len()
    );

    if !binary.inferred_types.is_empty() {
        println!();
        println!("{}", "Inferred Types".bold().underline());
        for ty in &binary.inferred_types {
            println!("  - {} ({})", ty.name.green(), ty.kind.cyan());
            if !ty.fields.is_empty() {
                for field in &ty.fields {
                    println!(
                        "    + 0x{:02x}: {} ({})",
                        field.offset, field.name, field.type_name
                    );
                }
            }
        }
    }

    println!();
}
