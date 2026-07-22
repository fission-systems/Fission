use super::record::{CliRustDecompileRecord, CliRustOutcome};
use fission_decompiler::{LayeredPseudocode, PseudocodeLayer};

fn primary_json_code(
    code: &str,
    code_hir: Option<&str>,
    layer: PseudocodeLayer,
) -> String {
    match layer {
        PseudocodeLayer::Hir => code_hir.unwrap_or(code).to_string(),
        _ => code.to_string(),
    }
}

fn layered_text(
    code: &str,
    code_nir: Option<&str>,
    code_hir: Option<&str>,
    layer: PseudocodeLayer,
) -> String {
    let nir = code_nir.unwrap_or(code).to_string();
    let hir = code_hir.unwrap_or(code).to_string();
    LayeredPseudocode { nir, hir }.format_text(layer, true)
}

pub(crate) fn record_to_json(entry: &CliRustDecompileRecord, benchmark: bool) -> serde_json::Value {
    let addr_str = format!("0x{:x}", entry.func.address);
    match &entry.outcome {
        CliRustOutcome::Success {
            code,
            code_nir,
            code_hir,
            code_dir,
            fell_back,
            fallback_reason,
            build_stats,
            hint_stats,
            decomp_sec,
        } => {
            let mut obj = serde_json::json!({
                "address": addr_str,
                "name": entry.func.name,
                "size": entry.func.size,
                "code": primary_json_code(code, code_hir.as_deref(), entry.layer),
                "code_nir": code_nir,
                "code_hir": code_hir,
                "layer": entry.layer.as_str(),
                "engine_used": "rust_sleigh",
                "fell_back": fell_back,
                "fallback_reason": fallback_reason,
            });
            // Only present when `decomp --dir` was passed -- absence means
            // "not requested", not "structuring produced nothing".
            if let Some(dir) = code_dir {
                obj["code_dir"] = serde_json::json!(dir);
            }
            if let Some(stats) = build_stats {
                obj["preview_build_stats"] = serde_json::json!(stats);
            }
            if let Some(stats) = hint_stats {
                obj["preview_hint_stats"] = serde_json::json!(stats);
            }
            if benchmark {
                obj["decomp_sec"] =
                    serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                obj["postprocess_sec"] = serde_json::json!(0.0);
            }
            obj
        }
        CliRustOutcome::AssemblyFallback {
            fallback_code,
            original_error,
            decomp_sec,
        } => {
            let mut obj = serde_json::json!({
                "address": addr_str,
                "name": entry.func.name,
                "size": entry.func.size,
                "code": fallback_code,
                "code_nir": serde_json::Value::Null,
                "code_hir": serde_json::Value::Null,
                "layer": entry.layer.as_str(),
                "engine_used": "rust_sleigh",
                "fell_back": true,
                "fallback": "assembly",
                "fallback_reason": format!("assembly_fallback: {}", original_error),
            });
            if benchmark {
                obj["decomp_sec"] =
                    serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                obj["postprocess_sec"] = serde_json::json!(0.0);
            }
            obj
        }
        CliRustOutcome::HardError {
            error_text,
            decomp_sec,
        } => {
            let mut obj = serde_json::json!({
                "address": addr_str,
                "name": entry.func.name,
                "size": entry.func.size,
                "engine_used": "rust_sleigh",
                "fell_back": true,
                "fallback_reason": format!("rust_sleigh: {}", error_text),
                "error": error_text,
                "layer": entry.layer.as_str(),
            });
            if benchmark {
                obj["decomp_sec"] =
                    serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                obj["postprocess_sec"] = serde_json::json!(0.0);
            }
            obj
        }
        CliRustOutcome::WorkerInternalError {
            message,
            assembly_fallback_code,
        } => {
            let plain_fallback = assembly_fallback_code.clone().unwrap_or_else(|| {
                format!(
                    "// Error decompiling {} (0x{:x}): {}",
                    entry.func.name, entry.func.address, message
                )
            });
            let mut obj = serde_json::json!({
                "address": addr_str,
                "name": entry.func.name,
                "size": entry.func.size,
                "engine_used": "rust_sleigh",
                "fell_back": true,
                "fallback_reason": "rust_sleigh:worker_internal_error",
                "error": message,
                "layer": entry.layer.as_str(),
            });
            if let Some(code) = assembly_fallback_code {
                obj["code"] = serde_json::json!(code);
                obj["fallback"] = serde_json::json!("assembly");
            } else {
                obj["code"] = serde_json::json!(plain_fallback.clone());
            }
            if benchmark {
                obj["decomp_sec"] = serde_json::json!(0.0);
                obj["postprocess_sec"] = serde_json::json!(0.0);
            }
            obj
        }
    }
}

pub(crate) fn record_plain_output(entry: &CliRustDecompileRecord) -> String {
    match &entry.outcome {
        CliRustOutcome::Success {
            code,
            code_nir,
            code_hir,
            code_dir,
            ..
        } => {
            let mut out = layered_text(code, code_nir.as_deref(), code_hir.as_deref(), entry.layer);
            // Only present when `decomp --dir` was passed. Appended as its
            // own section (not merged into the NIR/HIR layer switch above)
            // since DIR is a diagnostic view, not a selectable final
            // pseudocode surface.
            if let Some(dir) = code_dir {
                out.push_str("\n\n// ===== DIR (pre-structuring) =====\n");
                out.push_str(dir);
            }
            out
        }
        CliRustOutcome::AssemblyFallback { fallback_code, .. } => fallback_code.clone(),
        CliRustOutcome::HardError { error_text, .. } => format!(
            "// Error decompiling {} (0x{:x}): {}",
            entry.func.name, entry.func.address, error_text
        ),
        CliRustOutcome::WorkerInternalError {
            message,
            assembly_fallback_code,
        } => assembly_fallback_code.clone().unwrap_or_else(|| {
            format!(
                "// Error decompiling {} (0x{:x}): {}",
                entry.func.name, entry.func.address, message
            )
        }),
    }
}
