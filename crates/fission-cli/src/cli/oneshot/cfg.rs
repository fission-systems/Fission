//! CFG (Control Flow Graph) Analysis Command
//!
//! Generates control flow analysis for a function.

use crate::analysis::cfg::{CfgAnalysis, CfgVisualizer, DotOptions};
use crate::analysis::loader::LoadedBinary;
use crate::analysis::pcode::PcodeFunction;
use crate::cli::output::OutputSilencer;
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CfgOutputFormat {
    Summary,
    Dot,
    Ascii,
    Json,
}

#[derive(Serialize)]
struct CfgJsonOutput {
    function_address: String,
    block_count: usize,
    edge_count: usize,
    cyclomatic_complexity: usize,
    max_nesting_depth: usize,
    loop_count: usize,
    loops: Vec<LoopInfo>,
    blocks: Vec<BlockInfo>,
}

#[derive(Serialize)]
struct LoopInfo {
    header: usize,
    kind: String,
    body: Vec<usize>,
    back_edges: Vec<(usize, usize)>,
}

#[derive(Serialize)]
struct BlockInfo {
    index: usize,
    address: String,
    is_entry: bool,
    is_exit: bool,
    successors: Vec<usize>,
    predecessors: Vec<usize>,
    instruction_count: usize,
}

pub fn analyze_cfg(
    binary: &LoadedBinary,
    addr: u64,
    format: CfgOutputFormat,
    output_path: Option<&PathBuf>,
    verbose: bool,
) -> io::Result<()> {
    if verbose {
        eprintln!("[*] Analyzing CFG for function at 0x{:X}", addr);
    }

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

    // Load binary data
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
                if verbose {
                    eprintln!("Warning: Failed to add section {}: {}", section.name, e);
                }
            }
        }
    }

    // Add symbols/functions
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

    // Get Pcode
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

    // Parse Pcode
    let func = match PcodeFunction::from_json(&pcode_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: Failed to parse Pcode JSON: {}", e);
            return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
        }
    };

    if verbose {
        eprintln!("[*] Building CFG...");
    }

    // Build CFG Analysis
    let analysis = match CfgAnalysis::from_pcode(&func) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: Failed to build CFG: {}", e);
            return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
        }
    };

    if verbose {
        eprintln!(
            "[✓] CFG built: {} blocks, {} edges, cyclomatic complexity: {}",
            analysis.cfg.block_count(),
            analysis.cfg.edge_count(),
            analysis.metrics.cyclomatic_complexity
        );
    }

    // Output based on format
    let output = match format {
        CfgOutputFormat::Summary => analysis.summary(),
        CfgOutputFormat::Dot => {
            let options = DotOptions {
                show_instructions: true,
                show_addresses: true,
                highlight_loops: true,
                show_edge_labels: true,
                title: Some(format!("CFG @ 0x{:X}", addr)),
                ..Default::default()
            };
            CfgVisualizer::to_dot(&analysis.cfg, &analysis.loops, &options)
        }
        CfgOutputFormat::Ascii => CfgVisualizer::to_ascii(&analysis.cfg),
        CfgOutputFormat::Json => {
            let json_output = build_json_output(&analysis, addr);
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        }
    };

    // Write output
    if let Some(path) = output_path {
        let mut file = fs::File::create(path)?;
        file.write_all(output.as_bytes())?;
        println!("[✓] CFG analysis saved to: {}", path.display());

        // For DOT format, try to render to PNG
        if format == CfgOutputFormat::Dot {
            render_dot_to_png(path, verbose);
        }
    } else {
        println!("{}", output);
    }

    Ok(())
}

fn build_json_output(analysis: &CfgAnalysis, addr: u64) -> CfgJsonOutput {
    let loops: Vec<LoopInfo> = analysis
        .loops
        .iter()
        .map(|l| LoopInfo {
            header: l.header,
            kind: format!("{:?}", l.kind),
            body: l.body.iter().copied().collect(),
            back_edges: l.back_edges.clone(),
        })
        .collect();

    let blocks: Vec<BlockInfo> = analysis
        .cfg
        .blocks
        .iter()
        .map(|b| BlockInfo {
            index: b.index,
            address: format!("0x{:x}", b.start_address),
            is_entry: b.is_entry,
            is_exit: b.is_exit,
            successors: b.successors.iter().map(|e| e.target).collect(),
            predecessors: b.predecessors.clone(),
            instruction_count: b.operations.len(),
        })
        .collect();

    CfgJsonOutput {
        function_address: format!("0x{:x}", addr),
        block_count: analysis.cfg.block_count(),
        edge_count: analysis.cfg.edge_count(),
        cyclomatic_complexity: analysis.metrics.cyclomatic_complexity,
        max_nesting_depth: analysis.metrics.max_nesting_depth,
        loop_count: analysis.loops.len(),
        loops,
        blocks,
    }
}

fn render_dot_to_png(dot_path: &PathBuf, verbose: bool) {
    let png_path = dot_path.with_extension("png");
    if verbose {
        eprintln!("[*] Attempting to render to PNG: {}", png_path.display());
    }

    match Command::new("dot")
        .arg("-Tpng")
        .arg(dot_path)
        .arg("-o")
        .arg(&png_path)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                println!("[✓] Graph rendered to: {}", png_path.display());
            } else if verbose {
                eprintln!(
                    "Warning: 'dot' command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Err(e) => {
            if verbose {
                eprintln!(
                    "Warning: Could not run 'dot' command (is Graphviz installed?): {}",
                    e
                );
            }
        }
    }
}
