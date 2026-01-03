//! Info command - Display binary information

use colored::Colorize;
use crate::ui::cli::handlers::CliState;

pub fn cmd_info(state: &CliState) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
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
