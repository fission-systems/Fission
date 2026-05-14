use super::pcode_diagnostics::{json_scalar, run_pcode_diagnostic, write_json};
use fission_loader::loader::LoadedBinary;
use serde_json::Value;
use std::io::{self, Write};

const STAGE_ORDER: &[&str] = &[
    "load",
    "decode",
    "raw_pcode",
    "nir_build",
    "normalize",
    "structuring",
    "render",
];

pub(super) fn emit_pcode_stages(
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
    if json {
        write_json(&run.bundle)?;
    } else {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", render_stage_text(&run.bundle))?;
    }
    Ok(())
}

fn render_stage_text(bundle: &Value) -> String {
    let function = &bundle["function"];
    let name = function["name"].as_str().unwrap_or("<unknown>");
    let requested = function["requested_address"]
        .as_str()
        .unwrap_or("<unknown>");
    let resolved = function["resolved_address"].as_str().unwrap_or("<unknown>");

    let mut out = String::new();
    out.push_str(&format!(
        "p-code stages for {name} requested={requested} resolved={resolved}\n"
    ));
    out.push_str("stage status:\n");
    for stage in STAGE_ORDER {
        let status = bundle["stage_status"][stage].as_str().unwrap_or("unknown");
        out.push_str(&format!("  {stage}: {status}\n"));
    }

    if let Some(pipeline) = bundle.get("rust_sleigh_pipeline") {
        out.push_str("rust-sleigh pipeline:\n");
        push_optional_str(&mut out, "decode_stop_reason", pipeline);
        push_optional_usize(&mut out, "decode_attempt_count", pipeline);
        push_optional_usize(&mut out, "raw_pcode_op_count", pipeline);
        push_optional_usize(&mut out, "raw_pcode_block_count", pipeline);
        push_optional_usize(&mut out, "raw_pcode_edge_count", pipeline);
        if let Some(counts) = pipeline["template_source_counts"].as_object() {
            out.push_str("  template_source_counts:");
            if counts.is_empty() {
                out.push_str(" {}\n");
            } else {
                out.push('\n');
                for (source, count) in counts {
                    out.push_str(&format!("    {source}: {}\n", format_json_scalar(count)));
                }
            }
        }
    }

    if let Some(metrics) = bundle.get("stage_metrics") {
        out.push_str("stage metrics:\n");
        for key in [
            "validated_pcode_op_count",
            "invalid_pcode_shape_count",
            "build_duration_ms",
            "normalize_duration_ms",
            "structuring_duration_ms",
            "render_duration_ms",
        ] {
            if !metrics[key].is_null() {
                out.push_str(&format!("  {key}: {}\n", format_json_scalar(&metrics[key])));
            }
        }
    }

    if let Some(buckets) = bundle["owner_buckets"].as_array() {
        out.push_str("owner buckets:");
        if buckets.is_empty() {
            out.push_str(" none\n");
        } else {
            out.push(' ');
            out.push_str(
                &buckets
                    .iter()
                    .map(format_json_scalar)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('\n');
        }
    }

    out
}

fn push_optional_str(out: &mut String, key: &str, value: &Value) {
    if let Some(s) = value[key].as_str() {
        if !s.is_empty() {
            out.push_str(&format!("  {key}: {s}\n"));
        }
    }
}

fn push_optional_usize(out: &mut String, key: &str, value: &Value) {
    if !value[key].is_null() {
        out.push_str(&format!("  {key}: {}\n", format_json_scalar(&value[key])));
    }
}

fn format_json_scalar(value: &Value) -> String {
    json_scalar(value)
}
