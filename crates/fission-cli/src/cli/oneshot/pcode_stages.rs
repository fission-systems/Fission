use super::debug_decomp::debug_decomp_bundle_json;
use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh};
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
    let func = binary.function_at(addr).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no function found at or containing 0x{addr:x}"),
        )
    })?;

    let mut config = RustSleighDecompileConfig::cli_defaults();
    config.continue_past_indirect_branch = !strict_indirect_stop;

    let result = decompile_with_rust_sleigh(
        binary,
        func.address,
        &func.name,
        &config,
        Some(clamp_usize_to_u32(max_bytes)),
        Some(clamp_usize_to_u32(instruction_limit)),
    )
    .map_err(|err| io::Error::other(format!("Rust-Sleigh decompile failed: {err}")))?;

    let bundle = debug_decomp_bundle_json(
        binary,
        Some(addr),
        func,
        result.build_stats.as_ref(),
        result.hint_stats.as_ref(),
        Some(&result.evidence),
        None,
        false,
        result.build_stats.is_none(),
    );

    let mut stdout = io::stdout().lock();
    if json {
        let output = serde_json::to_string_pretty(&bundle)
            .map_err(|err| io::Error::other(format!("stage JSON serialization failed: {err}")))?;
        writeln!(stdout, "{output}")?;
    } else {
        write!(stdout, "{}", render_stage_text(&bundle))?;
    }
    Ok(())
}

fn clamp_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX).max(1)
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
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}
