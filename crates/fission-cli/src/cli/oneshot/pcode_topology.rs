use super::pcode_diagnostics::{json_scalar, run_pcode_diagnostic, write_json};
use fission_loader::loader::LoadedBinary;
use serde_json::Value;
use std::io::{self, Write};

pub(super) fn emit_pcode_topology(
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
    let topology = serde_json::json!({
        "block_count": run.pipeline["raw_pcode_block_count"].clone(),
        "op_count": run.pipeline["raw_pcode_op_count"].clone(),
        "edge_count": run.pipeline["raw_pcode_edge_count"].clone(),
        "terminal_opcode_counts": run.pipeline["raw_pcode_terminal_opcode_counts"].clone(),
        "block_evidence_truncated": run.pipeline["raw_pcode_block_evidence_truncated"].clone(),
        "blocks": run.pipeline["raw_pcode_blocks"].clone(),
    });
    let payload = serde_json::json!({
        "schema_version": 1u32,
        "binary": run.bundle["binary"].clone(),
        "function": run.bundle["function"].clone(),
        "stage_status": run.bundle["stage_status"].clone(),
        "decode_stop_reason": run.pipeline["decode_stop_reason"].clone(),
        "template_source_counts": run.pipeline["template_source_counts"].clone(),
        "raw_pcode_topology": topology,
    });

    if json {
        write_json(&payload)?;
    } else {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{}", render_topology_text(&payload))?;
    }
    Ok(())
}

fn render_topology_text(payload: &Value) -> String {
    let function = &payload["function"];
    let name = function["name"].as_str().unwrap_or("<unknown>");
    let requested = function["requested_address"]
        .as_str()
        .unwrap_or("<unknown>");
    let resolved = function["resolved_address"].as_str().unwrap_or("<unknown>");
    let topology = &payload["raw_pcode_topology"];

    let mut out = String::new();
    out.push_str(&format!(
        "raw p-code topology for {name} requested={requested} resolved={resolved}\n"
    ));
    out.push_str(&format!(
        "blocks={} edges={} ops={} stop={}\n",
        json_scalar(&topology["block_count"]),
        json_scalar(&topology["edge_count"]),
        json_scalar(&topology["op_count"]),
        json_scalar(&payload["decode_stop_reason"])
    ));

    if let Some(counts) = topology["terminal_opcode_counts"].as_object() {
        out.push_str("terminal opcode counts:");
        if counts.is_empty() {
            out.push_str(" {}\n");
        } else {
            out.push('\n');
            for (opcode, count) in counts {
                out.push_str(&format!("  {opcode}: {}\n", json_scalar(count)));
            }
        }
    }

    if let Some(blocks) = topology["blocks"].as_array() {
        out.push_str("blocks:\n");
        for block in blocks {
            out.push_str(&format!(
                "  block_{} @ 0x{:x} ops={} succ={}\n",
                block["index"].as_u64().unwrap_or_default(),
                block["start_address"].as_u64().unwrap_or_default(),
                json_scalar(&block["op_count"]),
                json_scalar(&block["successors"])
            ));
        }
    }

    out
}
