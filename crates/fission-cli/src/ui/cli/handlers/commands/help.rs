//! Help and utility commands

use colored::Colorize;

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
        "  {} {}  Show cross-references for address",
        "xrefs".cyan(),
        "<addr>".dimmed()
    );
    println!(
        "  {} {}  Find string cross-references",
        "string-xrefs".cyan(),
        "<term> [min_len]".dimmed()
    );
    println!(
        "                        {}",
        "Use /regex/ for regex search".dimmed()
    );
    println!(
        "                        {}",
        "Use \"exact\" for exact match".dimmed()
    );
    println!(
        "                        {}",
        "Or just term for partial match".dimmed()
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
