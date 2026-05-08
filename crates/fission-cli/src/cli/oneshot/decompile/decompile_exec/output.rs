use crate::cli::oneshot::debug_decomp::{
    attach_pcode_topology, debug_decomp_bundle_json, write_debug_decomp_bundle_file,
};
use super::super::decompile_render::{
    attach_native_timing, decompile_code_with_profile, make_assembly_fallback,
    strip_inferred_structs, strip_warnings,
};
use super::super::*;

pub(crate) fn decompile_and_output(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    selected_profile: &str,
    engine_mode: EngineMode,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decompile_code_with_profile(
        selected_profile,
        engine_mode,
        decomp,
        binary,
        addr,
        name,
        cli.timeout_ms,
        cli.verbose,
    ) {
        Ok(rendered) => {
            let func_meta = binary.function_at_exact(addr).cloned().unwrap_or_else(|| {
                FunctionInfo {
                    name: name.to_string(),
                    address: addr,
                    size: 0,
                    is_export: false,
                    is_import: false,
                    ..Default::default()
                }
            });

            let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                let t = s.trim();
                if t.is_empty() || t == "{}" {
                    None
                } else {
                    serde_json::from_str(&s).ok()
                }
            });

            let debug_bundle = (cli.debug_decomp || cli.debug_decomp_bundle.is_some()).then(|| {
                let mut bundle = debug_decomp_bundle_json(
                    binary,
                    cli.address,
                    &func_meta,
                    rendered.preview_build_stats.as_ref(),
                    rendered.preview_hint_stats.as_ref(),
                    rendered.rust_sleigh_evidence.as_ref(),
                    native_timing.as_ref(),
                    false,
                    rendered.preview_build_stats.is_none() && rendered.fell_back,
                );
                if let Ok(pcode_json) = decomp.get_pcode(addr)
                    && let Ok(pcode) = PcodeFunction::from_json(&pcode_json)
                {
                    attach_pcode_topology(&mut bundle, &pcode);
                }
                bundle
            });

            if let Some(ref path) = cli.debug_decomp_bundle {
                if let Some(ref bundle) = debug_bundle {
                    write_debug_decomp_bundle_file(path, std::slice::from_ref(bundle))?;
                }
            }

            let mut filtered = rendered.code.clone();
            if effective_no_warnings {
                filtered = strip_warnings(&filtered);
            }
            if cli.ghidra_compat {
                filtered = strip_inferred_structs(&filtered);
            }
            if cli.json {
                let mut obj = serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": filtered,
                    "engine_used": rendered.engine_used,
                    "fell_back": rendered.fell_back,
                    "fallback_reason": rendered.fallback_reason,
                    "preview_build_stats": rendered.preview_build_stats,
                    "preview_hint_stats": rendered.preview_hint_stats,
                });
                if cli.debug_decomp {
                    if let Some(bundle) = debug_bundle {
                        obj["debug_decomp"] = bundle;
                    }
                }
                let json_output = serde_json::to_string_pretty(&obj)
                    .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?;
                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, json_output.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", json_output)?;
                }
            } else {
                let mut out_buf = String::new();
                if !effective_no_header {
                    out_buf.push_str(&format!("// Function: {} @ 0x{:x}\n\n", name, addr));
                }
                out_buf.push_str(&filtered);
                out_buf.push('\n');

                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, out_buf.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", out_buf)?;
                }
            }
        }
        Err(e) => {
            let error_text = e.to_string();
            if let Some(func) = binary.function_at_exact(addr)
                && let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
            {
                let mut stdout = io::stdout().lock();
                writeln!(stdout, "{}", fallback)?;

                if let Some(ref path) = cli.debug_decomp_bundle {
                    let bundle = debug_decomp_bundle_json(
                        binary,
                        cli.address,
                        func,
                        None,
                        None,
                        None,
                        None,
                        false,
                        true,
                    );
                    write_debug_decomp_bundle_file(path, &[bundle])?;
                }
                return Ok(());
            }

            if cli.debug_decomp_bundle.is_some() {
                let func_meta = binary.function_at_exact(addr).cloned().unwrap_or_else(|| {
                    FunctionInfo {
                        name: name.to_string(),
                        address: addr,
                        size: 0,
                        is_export: false,
                        is_import: false,
                        ..Default::default()
                    }
                });
                let bundle = debug_decomp_bundle_json(
                    binary,
                    cli.address,
                    &func_meta,
                    None,
                    None,
                    None,
                    None,
                    true,
                    false,
                );
                if let Some(ref path) = cli.debug_decomp_bundle {
                    write_debug_decomp_bundle_file(path, &[bundle])?;
                }
            }

            eprintln!("Error: {}", error_text);
            std::process::exit(1);
        }
    }
    Ok(())
}

pub(super) fn attach_native_timing_if_present(
    entry: &mut serde_json::Value,
    decomp: &mut DecompilerNative,
) {
    attach_native_timing(entry, decomp);
}
