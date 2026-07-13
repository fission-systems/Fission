use super::pcode_diagnostics::{json_scalar, write_json};
use super::raw_pcode::{decode_stop_reason_label, lift_raw_pcode};
use fission_decompiler::PcodeFunction;
use fission_loader::loader::LoadedBinary;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Write};

pub(super) fn emit_pcode_topology(
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
    let (decode_addr, lifted) = lift_raw_pcode(
        binary,
        addr,
        max_bytes,
        instruction_limit,
        !strict_indirect_stop,
    )?;
    let topology = topology_json(&lifted.function);
    let payload = serde_json::json!({
        "schema_version": 1u32,
        "binary": {
            "path": binary.path.as_str(),
            "format": binary.format.as_str(),
            "image_base": format!("0x{:x}", binary.image_base),
            "language_id": binary.sleigh_language_id().unwrap_or(binary.arch_spec.as_str()),
            "compiler_id": binary
                .get_ghidra_compiler_id()
                .unwrap_or_else(|| "unknown".to_string()),
        },
        "function": {
            "name": func.name,
            "requested_address": format!("0x{addr:x}"),
            "resolved_address": format!("0x{decode_addr:x}"),
            "size": func.size,
            "is_export": func.is_export,
            "is_import": func.is_import,
            "function_origin": func.origin,
            "is_thunk_like": func.is_thunk_like,
        },
        "stage_status": {
            "load": "ok",
            "decode": "ok",
            "raw_pcode": "ok",
            "nir_build": "not_run",
            "normalize": "not_run",
            "structuring": "not_run",
            "render": "not_run",
        },
        "decode_stop_reason": decode_stop_reason_label(lifted.stop_reason),
        "template_source_counts": lifted.template_source_counts,
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

fn topology_json(pcode: &PcodeFunction) -> Value {
    let mut terminal_opcode_counts = BTreeMap::<String, usize>::new();
    let mut op_count = 0usize;
    let mut edge_count = 0usize;
    let blocks = pcode
        .blocks
        .iter()
        .map(|block| {
            op_count += block.ops.len();
            edge_count += block.successors.len();
            let terminal = block.ops.last();
            if let Some(op) = terminal {
                *terminal_opcode_counts
                    .entry(format!("{:?}", op.opcode))
                    .or_default() += 1;
            }
            serde_json::json!({
                "index": block.index,
                "start_address": block.start_address,
                "op_count": block.ops.len(),
                "successors": block.successors,
                "terminal_address": terminal.map(|op| op.address).unwrap_or(block.start_address),
                "terminal_opcode": terminal.map(|op| format!("{:?}", op.opcode)),
                "terminal_seq_num": terminal.map(|op| op.seq_num),
                "terminal_target": Value::Null,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "block_count": pcode.blocks.len(),
        "op_count": op_count,
        "edge_count": edge_count,
        "terminal_opcode_counts": terminal_opcode_counts,
        "block_evidence_truncated": false,
        "blocks": blocks,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use fission_decompiler::{PcodeBasicBlock, PcodeOp, PcodeOpcode};

    #[test]
    fn topology_json_is_derived_from_raw_pcode_blocks() {
        let pcode = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![0, 1],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Call,
                        address: 0x1004,
                        output: None,
                        inputs: vec![],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1009,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x1009,
                        output: None,
                        inputs: vec![],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let topology = topology_json(&pcode);

        assert_eq!(topology["block_count"], 2);
        assert_eq!(topology["edge_count"], 2);
        assert_eq!(topology["op_count"], 2);
        assert_eq!(topology["blocks"][0]["terminal_address"], 0x1004);
        assert_eq!(topology["terminal_opcode_counts"]["Call"], 1);
    }
}
