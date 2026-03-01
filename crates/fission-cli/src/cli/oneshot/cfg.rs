//! CFG (Control Flow Graph) Analysis Command
//!
//! Generates control flow analysis for a function.

use crate::analysis::cfg::{CfgAnalysis, CfgVisualizer, DotOptions};
use crate::analysis::pcode::PcodeFunction;
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, load_binary_into_decompiler, read_binary_data,
    resolve_compiler_id, resolve_profile,
};
use crate::cli::output::OutputSilencer;
use fission_analysis::analysis::cfg::CfgSummary;
use fission_loader::loader::LoadedBinary;
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

pub fn analyze_cfg(
    binary: &LoadedBinary,
    addr: u64,
    format: CfgOutputFormat,
    output_path: Option<&PathBuf>,
    verbose: bool,
    compiler_id_override: Option<&str>,
    profile_override: Option<&str>,
) -> io::Result<()> {
    if verbose {
        eprintln!("[*] Analyzing CFG for function at 0x{:X}", addr);
    }

    let mut decomp = init_decompiler(verbose);
    let (selected_profile, _) = resolve_profile(profile_override);
    apply_profile(&mut decomp, selected_profile);

    let binary_data = read_binary_data(binary);
    let (compiler_id, _) = resolve_compiler_id(binary, compiler_id_override);
    load_binary_into_decompiler(&mut decomp, binary, &binary_data, compiler_id, verbose);

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
            let summary = CfgSummary::from_analysis(&analysis, Some(addr), false);
            serde_json::to_string_pretty(&summary).unwrap_or_default()
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
