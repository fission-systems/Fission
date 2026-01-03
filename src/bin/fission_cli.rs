//! Fission CLI - Enhanced command-line interface for binary analysis
//!
//! Usage:
//!   fission-cli <binary> [OPTIONS]
//!
//! Options:
//!   -a, --address <ADDR>   Decompile function at specific address
//!   -A, --all              Decompile all discovered functions
//!   -o, --output <FILE>    Output to file instead of stdout
//!   -j, --json             Output in JSON format
//!   -l, --list             List all functions
//!   -i, --info             Show binary info
//!   -v, --verbose          Verbose output

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[cfg(feature = "native_decomp")]
use fission::analysis::decomp::ffi::DecompilerNative;

use fission::analysis::loader::{FunctionInfo, LoadedBinary};

#[derive(Parser, Debug)]
#[command(name = "fission-cli")]
#[command(author = "Fission Dev Team")]
#[command(version = "0.1.0")]
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

    // Handle info flag
    if cli.info {
        print_binary_info(&binary, cli.json);
        return;
    }

    // Handle list flag
    if cli.list {
        print_function_list(&binary, cli.json);
        return;
    }

    // Handle decompilation
    if cli.address.is_some() || cli.all {
        #[cfg(feature = "native_decomp")]
        {
            run_decompilation(&cli, &binary, &binary_data);
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            eprintln!("Error: Decompilation requires native_decomp feature");
            eprintln!("Run with: cargo run --bin fission-cli --features native_decomp -- ...");
            std::process::exit(1);
        }
        return;
    }

    // Default: show help
    eprintln!("No action specified. Use --help for usage.");
    eprintln!();
    eprintln!("Quick start:");
    eprintln!("  fission-cli <binary> --info          # Show binary info");
    eprintln!("  fission-cli <binary> --list          # List functions");
    eprintln!("  fission-cli <binary> -a 0x140001400  # Decompile at address");
    eprintln!("  fission-cli <binary> --all           # Decompile all functions");
}

fn print_binary_info(binary: &LoadedBinary, json: bool) {
    if json {
        println!(
            r#"{{"path":"{}","arch":"{}","bits":{},"entry":"0x{:x}","sections":{},"functions":{}}}"#,
            binary.path,
            if binary.is_64bit { "x86_64" } else { "x86" },
            if binary.is_64bit { 64 } else { 32 },
            binary.entry_point,
            binary.sections.len(),
            binary.functions.len()
        );
    } else {
        println!("Binary Information:");
        println!("  Path:       {}", binary.path);
        println!(
            "  Arch:       {}",
            if binary.is_64bit { "x86_64" } else { "x86" }
        );
        println!("  Entry:      0x{:x}", binary.entry_point);
        println!("  Sections:   {}", binary.sections.len());
        println!("  Functions:  {}", binary.functions.len());
        println!("  IAT Symbols: {}", binary.iat_symbols.len());
    }
}

fn print_function_list(binary: &LoadedBinary, json: bool) {
    if json {
        let funcs: Vec<serde_json::Value> = binary
            .functions
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                    "is_import": f.is_import
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&funcs).unwrap());
    } else {
        println!("Functions ({}):", binary.functions.len());
        println!("{:>16}  {:>8}  {}", "Address", "Size", "Name");
        println!("{:-<50}", "");
        for func in &binary.functions {
            let import_marker = if func.is_import { " [import]" } else { "" };
            println!(
                "  0x{:012x}  {:>6}  {}{}",
                func.address, func.size, func.name, import_marker
            );
        }
    }
}

#[cfg(feature = "native_decomp")]
fn run_decompilation(cli: &Cli, binary: &LoadedBinary, binary_data: &[u8]) {
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
        decompile_and_output(cli, &decomp, addr, &format!("sub_{:x}", addr));
        return;
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
        file.write_all(final_output.as_bytes())
            .expect("Failed to write output");
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        println!("{}", final_output);
    }
}

#[cfg(feature = "native_decomp")]
fn decompile_and_output(cli: &Cli, decomp: &DecompilerNative, addr: u64, name: &str) {
    match decomp.decompile(addr) {
        Ok(code) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "address": format!("0x{:x}", addr),
                        "name": name,
                        "code": code
                    }))
                    .unwrap()
                );
            } else {
                println!("// Function: {} @ 0x{:x}\n", name, addr);
                println!("{}", code);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
