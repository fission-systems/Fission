use super::pcode_diagnostics::{json_scalar, run_pcode_diagnostic, write_json};
use fission_loader::loader::LoadedBinary;
use serde_json::Value;
use std::io::{self, Write};

const SUMMARY_KEYS: &[&str] = &[
    "validated_pcode_op_count",
    "invalid_pcode_shape_count",
    "build_duration_ms",
    "normalize_duration_ms",
    "structuring_duration_ms",
    "render_duration_ms",
    "replacement_plan_rejected_alias_unsafe_count",
    "replacement_plan_rejected_missing_merge_count",
    "region_emit_ready_failed_count",
    "call_target_unresolved_sub_fallback_count",
    "structuring_irreducible_scc_count",
];

pub(super) fn emit_nir_stats(
    binary: &LoadedBinary,
    addr: u64,
    max_bytes: usize,
    instruction_limit: usize,
    strict_indirect_stop: bool,
    json: bool,
) -> io::Result<()> {
    let run = run_pcode_diagnostic(
        binary,
        addr,
        max_bytes,
        instruction_limit,
        strict_indirect_stop,
    )?;
    let payload = serde_json::json!({
        "schema_version": 1u32,
        "binary": run.bundle["binary"].clone(),
        "function": run.bundle["function"].clone(),
        "stage_status": run.bundle["stage_status"].clone(),
        "owner_buckets": run.bundle["owner_buckets"].clone(),
        "nir_build_stats": run.build_stats,
        "preview_hint_stats": run.hint_stats,
        "rust_sleigh_pipeline": run.pipeline,
    });

    if json {
        write_json(&payload)?;
    } else {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", render_stats_text(&payload))?;
    }
    Ok(())
}

fn render_stats_text(payload: &Value) -> String {
    let function = &payload["function"];
    let name = function["name"].as_str().unwrap_or("<unknown>");
    let requested = function["requested_address"]
        .as_str()
        .unwrap_or("<unknown>");
    let resolved = function["resolved_address"].as_str().unwrap_or("<unknown>");

    let mut out = String::new();
    out.push_str(&format!(
        "NIR stats for {name} requested={requested} resolved={resolved}\n"
    ));

    if let Some(stats) = payload
        .get("nir_build_stats")
        .filter(|value| !value.is_null())
    {
        out.push_str("summary counters:\n");
        for key in SUMMARY_KEYS {
            out.push_str(&format!("  {key}: {}\n", json_scalar(&stats[key])));
        }
        if let Some(object) = stats.as_object() {
            out.push_str(&format!("total NirBuildStats fields: {}\n", object.len()));
        }
    } else {
        out.push_str("NirBuildStats: unavailable\n");
    }

    if let Some(buckets) = payload["owner_buckets"].as_array() {
        out.push_str("owner buckets:");
        if buckets.is_empty() {
            out.push_str(" none\n");
        } else {
            out.push(' ');
            out.push_str(
                &buckets
                    .iter()
                    .map(json_scalar)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('\n');
        }
    }

    if let Some(stop_reason) = payload["rust_sleigh_pipeline"]["decode_stop_reason"].as_str() {
        if !stop_reason.is_empty() {
            out.push_str(&format!("decode_stop_reason: {stop_reason}\n"));
        }
    }

    out
}
