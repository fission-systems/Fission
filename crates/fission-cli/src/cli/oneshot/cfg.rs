//! CFG (Control Flow Graph) Analysis Command
//!
//! Generates control flow analysis for a function.

use crate::analysis::cfg::{CfgAnalysis, CfgVisualizer, DotOptions};
use crate::analysis::pcode::PcodeFunction;
use crate::cli::output::OutputSilencer;
use fission_analysis::analysis::cfg::CfgSummary;
use fission_core::find_sla_dir;
use fission_loader::loader::LoadedBinary;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

fn apply_profile(decomp: &mut fission_ffi::DecompilerNative, profile: Option<&str>) {
    let selected = profile.unwrap_or("balanced").to_ascii_lowercase();
    match selected.as_str() {
        "quality" => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", true);
            decomp.set_feature("readonly_propagate", true);
        }
        "speed" => {
            decomp.set_feature("infer_pointers", false);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", false);
        }
        _ => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", true);
        }
    }
}

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

    // Initialize decompiler
    let sla_dir = find_sla_dir();

    if verbose {
        eprintln!("[*] Initializing native decompiler...");
    }

    let mut decomp = {
        let _silencer = OutputSilencer::new_if(!verbose);
        match fission_ffi::DecompilerNative::new(&sla_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error: Failed to create decompiler: {}", e);
                std::process::exit(1);
            }
        }
    };
    apply_profile(&mut decomp, profile_override);

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
        let compiler_id = if let Some(user_compiler) = compiler_id_override {
            Some(match user_compiler.to_ascii_lowercase().as_str() {
                "windows" => "windows",
                "gcc" => "gcc",
                "clang" => "clang",
                "default" => "default",
                _ => "default",
            })
        } else {
            let detection = fission_loader::detect(binary);
            let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
            detection
                .compiler()
                .map(|d| match d.name.to_lowercase().as_str() {
                    "microsoft visual c++" | "msvc" => "windows",
                    "gcc" | "mingw" => {
                        if is_pe {
                            "windows"
                        } else {
                            "gcc"
                        }
                    }
                    "clang" => "clang",
                    _ => "default",
                })
        };

        if let Err(e) = decomp.load_binary(
            &binary_data,
            binary.image_base,
            binary.is_64bit,
            Some(&binary.arch_spec),
            compiler_id,
        ) {
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
