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
            let mut filtered = rendered.code.clone();
            if effective_no_warnings {
                filtered = strip_warnings(&filtered);
            }
            if cli.ghidra_compat {
                filtered = strip_inferred_structs(&filtered);
            }
            if cli.json {
                let json_output = serde_json::to_string_pretty(&serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": filtered,
                    "engine_used": rendered.engine_used,
                    "fell_back": rendered.fell_back,
                    "fallback_reason": rendered.fallback_reason,
                    "preview_build_stats": rendered.preview_build_stats,
                    "preview_hint_stats": rendered.preview_hint_stats,
                }))
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
                return Ok(());
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
