//! Pcode Graph Generation Command
//!
//! Generates DOT graph for a function's Pcode.

use crate::analysis::loader::LoadedBinary;
use crate::analysis::pcode::graph::PcodeGraph;
use crate::analysis::pcode::optimizer::{DefUseTracker, PcodeOptimizer, PcodeOptimizerConfig};
use crate::analysis::pcode::PcodeFunction;
use crate::cli::output::OutputSilencer;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

pub fn generate_pcode_graph(
    binary: &LoadedBinary,
    addr: u64,
    output_path: Option<&PathBuf>,
    verbose: bool,
) -> io::Result<()> {
    if verbose {
        eprintln!("[*] Generating Pcode graph for function at 0x{:X}", addr);
    }

    // 1. Decompile to get Pcode
    // Initialize decompiler
    let sla_dir = std::env::current_dir()
        .unwrap()
        .join("ghidra_decompiler")
        .to_string_lossy()
        .into_owned();

    if verbose {
        eprintln!("[*] Initializing native decompiler...");
    }

    let mut decomp = {
        let _silencer = OutputSilencer::new_if(!verbose);
        match crate::analysis::decomp::ffi::DecompilerNative::new(&sla_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error: Failed to create decompiler: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Load binary
    // We need raw binary data. LoadedBinary doesn't store it?
    // LoadedBinary is parsed. We need to read file again or pass data.
    // But run_oneshot reads data.
    // We should probably pass data to this function.
    // For now, let's read it again from path.
    // Wait, LoadedBinary has `path`? No, it has `name`.
    // Let's assume we can read from `binary.name` if it's a path.
    
    let binary_path = PathBuf::from(&binary.path);
    let binary_data = match fs::read(&binary_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: Failed to read binary from {}: {}", binary.path, e);
            std::process::exit(1);
        }
    };

    {
        let _silencer = OutputSilencer::new_if(!verbose);
        if let Err(e) = decomp.load_binary(&binary_data, binary.image_base, binary.is_64bit) {
            eprintln!("Error: Failed to load binary: {}", e);
            std::process::exit(1);
        }
    }
    
    // Add memory blocks (sections)
    {
        let _silencer = OutputSilencer::new_if(!verbose);
        for section in &binary.sections {
            if let Err(e) = decomp.add_memory_block(
                &section.name,
                section.virtual_address,
                section.virtual_size,
                section.file_offset,
                section.file_size,
                section.is_executable,
                section.is_writable,
            ) {
                eprintln!("Warning: Failed to add section {}: {}", section.name, e);
            }
        }
    }
    
    // Add symbols/functions (simplified setup)
    {
        let _silencer = OutputSilencer::new_if(!verbose);
        decomp.add_symbols(&binary.iat_symbols);
        decomp.add_global_symbols(&binary.global_symbols);
        decomp.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);
        for func in &binary.functions {
            if func.address != 0 && !func.name.is_empty() {
                let _ = decomp.add_function(func.address, Some(&func.name));
            }
        }
    }

    if verbose {
        eprintln!("[*] Retrieving Pcode for function at 0x{:X}...", addr);
    }

    let pcode_json = {
        let _silencer = OutputSilencer::new_if(!verbose);
        match decomp.get_pcode(addr) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Error: Failed to get Pcode: {}", e);
                return Err(io::Error::other(e.to_string()));
            }
        }
    };
    
    // 2. Parse Pcode
    let mut func = match PcodeFunction::from_json(&pcode_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: Failed to parse Pcode JSON: {}", e);
            return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
        }
    };
    
    // 3. Optimize (Optional but recommended for clean graph)
    if verbose {
        eprintln!("[*] Optimizing Pcode...");
    }
    let config = PcodeOptimizerConfig::default();
    let mut optimizer = PcodeOptimizer::new(config);
    optimizer.optimize(&mut func);
    
    // 4. Analyze Data Flow (Def-Use)
    if verbose {
        eprintln!("[*] Analyzing data flow...");
    }
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    
    // 5. Generate DOT
    if verbose {
        eprintln!("[*] Generating DOT graph...");
    }
    let dot_content = PcodeGraph::to_dot(&func, Some(&tracker));
    
    // 6. Output and Render
    let dot_path = if let Some(path) = output_path {
        path.clone()
    } else {
        PathBuf::from(format!("function_{:X}.dot", addr))
    };

    let mut file = fs::File::create(&dot_path)?;
    file.write_all(dot_content.as_bytes())?;
    
    if verbose {
        eprintln!("[✓] DOT graph written to: {}", dot_path.display());
    } else if output_path.is_none() {
        // If user didn't specify output, print to stdout as well? 
        // No, let's prefer file output for graph command as it's usually large.
        println!("[*] DOT graph saved to: {}", dot_path.display());
    }

    // Try to render to PNG using 'dot' command
    let png_path = dot_path.with_extension("png");
    if verbose {
        eprintln!("[*] Attempting to render to PNG: {}", png_path.display());
    }

    match Command::new("dot")
        .arg("-Tpng")
        .arg(&dot_path)
        .arg("-o")
        .arg(&png_path)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                println!("[✓] Graph rendered to: {}", png_path.display());
                // Optionally open the file?
                // Command::new("open").arg(&png_path).spawn().ok(); 
            } else if verbose {
                eprintln!(
                    "Warning: 'dot' command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        },
        Err(e) => {
            if verbose {
                eprintln!("Warning: Could not run 'dot' command (is Graphviz installed?): {}", e);
            }
        }
    }
    
    Ok(())
}
