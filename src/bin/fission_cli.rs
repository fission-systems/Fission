//! Fission CLI - Enhanced command-line interface for binary analysis
//!
//! Usage:
//!   fission-cli <binary> [OPTIONS]
//!
//! Commands:
//!   --info           Show binary information
//!   --list           List all functions
//!   --sections       Show section information
//!   --imports        List imported functions
//!   --exports        List exported functions
//!   --strings        Extract strings from binary
//!   --disasm <ADDR>  Disassemble at address
//!   -a <ADDR>        Decompile function at address
//!   --all            Decompile all functions

use clap::Parser;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[cfg(feature = "native_decomp")]
use fission::analysis::decomp::ffi::DecompilerNative;

use fission::analysis::loader::{FunctionInfo, LoadedBinary};

#[derive(Parser, Debug)]
#[command(name = "fission-cli")]
#[command(author = "Fission Dev Team")]
#[command(version = "0.2.0")]
#[command(about = "Next-Gen Binary Analysis CLI", long_about = None)]
struct Cli {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Decompile function at specific address (hex, e.g., 0x140001400)
    #[arg(short, long, value_parser = parse_hex_address)]
    address: Option<u64>,

    /// Decompile all discovered functions
    #[arg(short = 'A', long)]
    all: bool,

    /// List all discovered functions
    #[arg(short, long)]
    list: bool,

    /// Show binary information
    #[arg(short, long)]
    info: bool,

    /// Show section information
    #[arg(short = 'S', long)]
    sections: bool,

    /// List imported functions
    #[arg(short = 'I', long)]
    imports: bool,

    /// List exported functions
    #[arg(short = 'E', long)]
    exports: bool,

    /// Extract strings from binary (min length)
    #[arg(long, value_name = "MIN_LEN", num_args = 0..=1, default_missing_value = "4")]
    strings: Option<usize>,

    /// Disassemble at address (with optional count)
    #[arg(short = 'd', long, value_parser = parse_hex_address)]
    disasm: Option<u64>,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value = "20")]
    count: usize,

    /// Output to file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn parse_hex_address(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address: {}", e))
}

fn main() {
    if let Err(e) = run() {
        if e.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();

    // Load binary
    if cli.verbose {
        eprintln!("[*] Loading binary: {}", cli.binary.display());
    }

    let binary_data = match fs::read(&cli.binary) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: Failed to read binary: {}", e);
            std::process::exit(1);
        }
    };

    let binary = match LoadedBinary::from_bytes(
        binary_data.clone(),
        cli.binary.to_string_lossy().to_string(),
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: Failed to parse binary: {}", e);
            std::process::exit(1);
        }
    };

    if cli.verbose {
        eprintln!(
            "[✓] Loaded: {} ({}-bit, {} functions)",
            cli.binary.display(),
            if binary.is_64bit { 64 } else { 32 },
            binary.functions.len()
        );
    }

    // Handle commands (in priority order)
    if cli.info {
        return print_binary_info(&binary, cli.json);
    }

    if cli.sections {
        return print_sections(&binary, cli.json);
    }

    if cli.imports {
        return print_imports(&binary, cli.json);
    }

    if cli.exports {
        return print_exports(&binary, cli.json);
    }

    if cli.list {
        return print_function_list(&binary, cli.json);
    }

    if let Some(min_len) = cli.strings {
        return print_strings(&binary_data, min_len.max(4), cli.json);
    }

    if let Some(addr) = cli.disasm {
        return disassemble(&binary, &binary_data, addr, cli.count, cli.json);
    }

    // Handle decompilation
    if cli.address.is_some() || cli.all {
        #[cfg(feature = "native_decomp")]
        {
            if let Err(e) = run_decompilation(&cli, &binary, &binary_data) {
                // Return IO errors (like BrokenPipe), suppress others handled internally if needed
                return Err(e);
            }
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            eprintln!("Error: Decompilation requires native_decomp feature");
            eprintln!("Run with: cargo run --bin fission_cli --features native_decomp -- ...");
            std::process::exit(1);
        }
        return Ok(());
    }

    // Default: show help
    print_help();
    Ok(())
}

fn print_help() {
    eprintln!("Fission CLI - Binary Analysis Tool");
    eprintln!();
    eprintln!("Usage: fission_cli <binary> [OPTIONS]");
    eprintln!();
    eprintln!("Information:");
    eprintln!("  -i, --info       Show binary information");
    eprintln!("  -S, --sections   Show section details");
    eprintln!("  -l, --list       List all functions");
    eprintln!("  -I, --imports    List imported functions");
    eprintln!("  -E, --exports    List exported functions");
    eprintln!();
    eprintln!("Analysis:");
    eprintln!("  -d, --disasm <ADDR>  Disassemble at address");
    eprintln!("  -n, --count <N>      Number of instructions (default: 20)");
    eprintln!("  --strings [MIN]      Extract strings (min length, default: 4)");
    eprintln!();
    eprintln!("Decompilation:");
    eprintln!("  -a, --address <ADDR>  Decompile function at address");
    eprintln!("  -A, --all             Decompile all functions");
    eprintln!();
    eprintln!("Output:");
    eprintln!("  -o, --output <FILE>   Write to file");
    eprintln!("  -j, --json            JSON output format");
    eprintln!("  -v, --verbose         Verbose output");
}

fn print_binary_info(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": binary.path,
                "format": binary.format,
                "arch": if binary.is_64bit { "x86_64" } else { "x86" },
                "bits": if binary.is_64bit { 64 } else { 32 },
                "entry": format!("0x{:x}", binary.entry_point),
                "image_base": format!("0x{:x}", binary.image_base),
                "sections": binary.sections.len(),
                "functions": binary.functions.len(),
                "imports": binary.functions.iter().filter(|f| f.is_import).count(),
                "exports": binary.functions.iter().filter(|f| f.is_export).count(),
            }))
            .unwrap()
        )?;
    } else {
        writeln!(
            stdout,
            "╔══════════════════════════════════════════════════════════╗"
        )?;
        writeln!(
            stdout,
            "║                    BINARY INFORMATION                    ║"
        )?;
        writeln!(
            stdout,
            "╠══════════════════════════════════════════════════════════╣"
        )?;
        writeln!(stdout, "║ Path:       {:<46} ║", truncate(&binary.path, 46))?;
        writeln!(stdout, "║ Format:     {:<46} ║", &binary.format)?;
        writeln!(
            stdout,
            "║ Arch:       {:<46} ║",
            if binary.is_64bit {
                "x86_64 (64-bit)"
            } else {
                "x86 (32-bit)"
            }
        )?;
        writeln!(
            stdout,
            "║ Entry:      {:<46} ║",
            format!("0x{:x}", binary.entry_point)
        )?;
        writeln!(
            stdout,
            "║ Image Base: {:<46} ║",
            format!("0x{:x}", binary.image_base)
        )?;
        writeln!(
            stdout,
            "╠══════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            stdout,
            "║ Sections:   {:<10} Functions: {:<10} IAT: {:<7} ║",
            binary.sections.len(),
            binary.functions.len(),
            binary.iat_symbols.len()
        )?;
        writeln!(
            stdout,
            "║ Imports:    {:<10} Exports:   {:<24} ║",
            binary.functions.iter().filter(|f| f.is_import).count(),
            binary.functions.iter().filter(|f| f.is_export).count()
        )?;
        writeln!(
            stdout,
            "╚══════════════════════════════════════════════════════════╝"
        )?;
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - max + 3..])
    }
}

fn print_sections(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        let sections: Vec<serde_json::Value> = binary
            .sections
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "virtual_address": format!("0x{:x}", s.virtual_address),
                    "virtual_size": s.virtual_size,
                    "file_offset": format!("0x{:x}", s.file_offset),
                    "file_size": s.file_size,
                    "executable": s.is_executable,
                    "readable": s.is_readable,
                    "writable": s.is_writable,
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&sections).unwrap()
        )?;
    } else {
        writeln!(stdout, "Sections ({}):", binary.sections.len())?;
        writeln!(
            stdout,
            "{:<12} {:>16} {:>10} {:>16} {:>10} {:>5}",
            "Name", "VirtAddr", "VirtSize", "FileOffset", "FileSize", "Flags"
        )?;
        writeln!(stdout, "{:─<75}", "")?;
        for sec in &binary.sections {
            let flags = format!(
                "{}{}{}",
                if sec.is_readable { "R" } else { "-" },
                if sec.is_writable { "W" } else { "-" },
                if sec.is_executable { "X" } else { "-" }
            );
            writeln!(
                stdout,
                "{:<12} {:>16} {:>10} {:>16} {:>10} {:>5}",
                truncate(&sec.name, 12),
                format!("0x{:x}", sec.virtual_address),
                sec.virtual_size,
                format!("0x{:x}", sec.file_offset),
                sec.file_size,
                flags
            )?;
        }
    }
    Ok(())
}

fn print_imports(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let imports: Vec<&FunctionInfo> = binary.functions.iter().filter(|f| f.is_import).collect();

    if json {
        let funcs: Vec<serde_json::Value> = imports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                })
            })
            .collect();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&funcs).unwrap())?;
    } else {
        writeln!(stdout, "Imported Functions ({}):", imports.len())?;
        writeln!(stdout, "{:>18}  {}", "Address", "Name")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in imports {
            writeln!(stdout, "  0x{:012x}  {}", func.address, func.name)?;
        }
    }
    Ok(())
}

fn print_exports(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let exports: Vec<&FunctionInfo> = binary.functions.iter().filter(|f| f.is_export).collect();

    if json {
        let funcs: Vec<serde_json::Value> = exports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                })
            })
            .collect();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&funcs).unwrap())?;
    } else {
        writeln!(stdout, "Exported Functions ({}):", exports.len())?;
        writeln!(stdout, "{:>18}  {:>8}  {}", "Address", "Size", "Name")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in exports {
            writeln!(
                stdout,
                "  0x{:012x}  {:>6}  {}",
                func.address, func.size, func.name
            )?;
        }
    }
    Ok(())
}

fn print_function_list(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        let funcs: Vec<serde_json::Value> = binary
            .functions
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                    "is_import": f.is_import,
                    "is_export": f.is_export,
                })
            })
            .collect();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&funcs).unwrap())?;
    } else {
        writeln!(stdout, "Functions ({}):", binary.functions.len())?;
        writeln!(stdout, "{:>18}  {:>8}  {}", "Address", "Size", "Name")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in &binary.functions {
            let marker = if func.is_import {
                " [import]"
            } else if func.is_export {
                " [export]"
            } else {
                ""
            };
            writeln!(
                stdout,
                "  0x{:012x}  {:>6}  {}{}",
                func.address, func.size, func.name, marker
            )?;
        }
    }
    Ok(())
}

fn print_strings(data: &[u8], min_len: usize, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    // Pre-allocate with estimated capacity (heuristic: ~1 string per 1KB of data)
    let estimated_strings = data.len() / 1024;
    let mut strings: Vec<(usize, String)> = Vec::with_capacity(estimated_strings.max(100));
    
    // Pre-allocate buffer with reasonable capacity to reduce reallocations
    let mut current_bytes: Vec<u8> = Vec::with_capacity(256);
    let mut start_offset = 0;

    for (i, &byte) in data.iter().enumerate() {
        if byte >= 0x20 && byte < 0x7f {
            if current_bytes.is_empty() {
                start_offset = i;
            }
            current_bytes.push(byte);
        } else {
            if current_bytes.len() >= min_len {
                // SAFETY: We only pushed bytes in 0x20-0x7E range, which are valid ASCII/UTF-8
                let value = unsafe { String::from_utf8_unchecked(std::mem::take(&mut current_bytes)) };
                strings.push((start_offset, value));
            }
            current_bytes.clear();
        }
    }
    // Don't forget last string
    if current_bytes.len() >= min_len {
        let value = unsafe { String::from_utf8_unchecked(current_bytes) };
        strings.push((start_offset, value));
    }

    if json {
        let str_json: Vec<serde_json::Value> = strings
            .iter()
            .map(|(off, s)| {
                serde_json::json!({
                    "offset": format!("0x{:x}", off),
                    "string": s,
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&str_json).unwrap()
        )?;
    } else {
        writeln!(
            stdout,
            "Strings ({} found, min length {}):",
            strings.len(),
            min_len
        )?;
        writeln!(stdout, "{:>12}  {}", "Offset", "String")?;
        writeln!(stdout, "{:─<60}", "")?;
        for (off, s) in &strings {
            // Truncate long strings for display
            if s.len() > 60 {
                writeln!(stdout, "  0x{:08x}  {}...", off, &s[..57])?;
            } else {
                writeln!(stdout, "  0x{:08x}  {}", off, s)?;
            }
        }
    }
    Ok(())
}

fn disassemble(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
    count: usize,
    json: bool,
) -> io::Result<()> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
    let mut stdout = io::stdout().lock();

    // Find the section containing this address
    let section = binary
        .sections
        .iter()
        .find(|s| addr >= s.virtual_address && addr < s.virtual_address + s.virtual_size);

    let (bytes, base) = if let Some(sec) = section {
        // Calculate offset within section
        let offset = (addr - sec.virtual_address) as usize;
        let file_offset = sec.file_offset as usize + offset;
        let remaining = (sec.virtual_size as usize).saturating_sub(offset);
        let len = remaining
            .min(1024)
            .min(data.len().saturating_sub(file_offset));

        if file_offset + len <= data.len() {
            (&data[file_offset..file_offset + len], addr)
        } else {
            eprintln!("Error: Address 0x{:x} is outside file bounds", addr);
            std::process::exit(1);
        }
    } else {
        eprintln!("Error: Address 0x{:x} not in any section", addr);
        std::process::exit(1);
    };

    let decoder_options = if binary.is_64bit { 64 } else { 32 };

    let mut decoder = Decoder::with_ip(decoder_options, bytes, base, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    // Pre-allocate output string buffer to reduce reallocations
    let mut output = String::with_capacity(64);
    // Pre-allocate results vector with requested count
    let mut instructions = Vec::with_capacity(count);

    for _ in 0..count {
        if !decoder.can_decode() {
            break;
        }
        let instr = decoder.decode();
        output.clear();
        formatter.format(&instr, &mut output);

        let bytes_str: String = bytes[instr.ip() as usize - base as usize
            ..instr.ip() as usize - base as usize + instr.len()]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        instructions.push((instr.ip(), bytes_str, output.clone()));
    }

    if json {
        let instr_json: Vec<serde_json::Value> = instructions
            .iter()
            .map(|(ip, bytes, mnemonic)| {
                serde_json::json!({
                    "address": format!("0x{:x}", ip),
                    "bytes": bytes,
                    "instruction": mnemonic,
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&instr_json).unwrap()
        )?;
    } else {
        writeln!(stdout, "Disassembly at 0x{:x}:", addr)?;
        writeln!(
            stdout,
            "{:>18}  {:24}  {}",
            "Address", "Bytes", "Instruction"
        )?;
        writeln!(stdout, "{:─<70}", "")?;
        for (ip, bytes, mnemonic) in &instructions {
            writeln!(stdout, "  0x{:012x}  {:24}  {}", ip, bytes, mnemonic)?;
        }
    }
    Ok(())
}

#[cfg(feature = "native_decomp")]
fn run_decompilation(cli: &Cli, binary: &LoadedBinary, binary_data: &[u8]) -> io::Result<()> {
    // Initialize decompiler
    let sla_dir = std::env::current_dir()
        .unwrap()
        .join("ghidra_decompiler")
        .to_string_lossy()
        .into_owned();

    if cli.verbose {
        eprintln!("[*] Initializing native decompiler...");
    }

    let mut decomp = match DecompilerNative::new(&sla_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: Failed to create decompiler: {}", e);
            std::process::exit(1);
        }
    };

    // Load binary
    if let Err(e) = decomp.load_binary(binary_data, binary.image_base, binary.is_64bit) {
        eprintln!("Error: Failed to load binary: {}", e);
        std::process::exit(1);
    }

    // Add IAT symbols
    decomp.add_symbols(&binary.iat_symbols);

    if cli.verbose {
        eprintln!("[✓] Decompiler ready");
    }

    // Collect functions to decompile
    let functions: Vec<&FunctionInfo> = if let Some(addr) = cli.address {
        binary
            .functions
            .iter()
            .filter(|f| f.address == addr)
            .collect()
    } else if cli.all {
        binary.functions.iter().filter(|f| !f.is_import).collect()
    } else {
        vec![]
    };

    if functions.is_empty() && cli.address.is_some() {
        eprintln!(
            "Warning: No function found at address 0x{:x}",
            cli.address.unwrap()
        );
        // Try to decompile anyway
        let addr = cli.address.unwrap();
        decompile_and_output(cli, &decomp, addr, &format!("sub_{:x}", addr))?;
        return Ok(());
    }

    // Decompile each function
    let mut all_output = String::new();
    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for func in &functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        match decomp.decompile(func.address) {
            Ok(code) => {
                if cli.json {
                    json_results.push(serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": code
                    }));
                } else {
                    all_output.push_str(&format!(
                        "// ============================================\n"
                    ));
                    all_output.push_str(&format!(
                        "// Function: {} @ 0x{:x}\n",
                        func.name, func.address
                    ));
                    all_output.push_str(&format!(
                        "// ============================================\n\n"
                    ));
                    all_output.push_str(&code);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                if cli.json {
                    json_results.push(serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "error": e.to_string()
                    }));
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, e
                    ));
                }
            }
        }
    }

    // Output results
    let final_output = if cli.json {
        serde_json::to_string_pretty(&json_results).unwrap()
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).expect("Failed to create output file");
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}

#[cfg(feature = "native_decomp")]
fn decompile_and_output(
    cli: &Cli,
    decomp: &DecompilerNative,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    match decomp.decompile(addr) {
        Ok(code) => {
            let mut stdout = io::stdout().lock();
            if cli.json {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "address": format!("0x{:x}", addr),
                        "name": name,
                        "code": code
                    }))
                    .unwrap()
                )?;
            } else {
                writeln!(stdout, "// Function: {} @ 0x{:x}\n", name, addr)?;
                writeln!(stdout, "{}", code)?;
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}
