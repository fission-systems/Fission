//! Control Flow Graph (CFG) analysis and DOT export.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::services::runtime_decode::decode_window_for_binary;
use crate::state::AppState;
use fission_core::{parse_address, MAX_HEX_READ, MAX_XREF_DECODE};
use fission_sleigh::runtime::DecodedFlowKind;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Build a Control Flow Graph for the function at `address`.
///
/// Uses the shared SLEIGH runtime to decode the function body, finds basic-block leaders from
/// branch targets and fall-through boundaries, then constructs nodes + edges.
#[tauri::command]
pub async fn get_cfg(address: String, state: State<'_, AppState>) -> CmdResult<CfgDto> {
    use std::collections::{HashMap, HashSet};

    let address = parse_address(&address)
        .ok_or_else(|| CmdError::other(format!("Invalid address: {}", address)))?;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let func = binary
        .functions
        .iter()
        .find(|f| f.address == address)
        .ok_or_else(|| CmdError::other(format!("Function at 0x{:x} not found", address)))?;

    let func_name = inner
        .renamed_functions
        .get(&address)
        .cloned()
        .unwrap_or_else(|| func.name.clone());

    let decode_size = if func.size > 0 {
        (func.size as usize).min(MAX_XREF_DECODE)
    } else {
        MAX_HEX_READ
    };
    let bytes = binary
        .get_bytes(address, decode_size)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", address)))?;

    let end_addr = address + bytes.len() as u64;

    // --- Raw instruction record ---
    #[derive(Clone)]
    struct RawInsn {
        ip: u64,
        len: usize,
        text: String,
        flow: DecodedFlowKind,
        target: Option<u64>,
    }

    let mut insns: Vec<RawInsn> = Vec::new();
    let mut leaders: HashSet<u64> = HashSet::new();
    leaders.insert(address); // entry is always a leader

    // Pass 1: decode & collect leaders
    {
        let decoded = decode_window_for_binary(binary, address, bytes.len(), MAX_XREF_DECODE)?;
        for insn in decoded {
            let flow = insn.flow_kind;
            let target = insn.direct_target;
            let in_range = target.is_some_and(|target| target >= address && target < end_addr);
            let next_ip = insn.address + insn.length as u64;

            // Branch target is a new leader (if inside the function)
            if in_range {
                leaders.insert(target.expect("checked target range"));
            }

            // The instruction after an unconditional branch / return is a leader
            match flow {
                DecodedFlowKind::Jump
                | DecodedFlowKind::Return
                | DecodedFlowKind::ConditionalJump => {
                    if next_ip < end_addr {
                        leaders.insert(next_ip);
                    }
                }
                _ => {}
            }

            insns.push(RawInsn {
                ip: insn.address,
                len: insn.length,
                text: insn.instruction_text(),
                flow,
                target,
            });
        }
    }

    // Sorted list of leader addresses
    let mut sorted_leaders: Vec<u64> = leaders.into_iter().filter(|&a| a < end_addr).collect();
    sorted_leaders.sort_unstable();

    // Leader address → block id
    let addr_to_block: HashMap<u64, usize> = sorted_leaders
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i))
        .collect();

    // Initialise nodes
    let mut nodes: Vec<CfgNode> = sorted_leaders
        .iter()
        .enumerate()
        .map(|(i, &leader)| CfgNode {
            id: i,
            start_address: format!("0x{:x}", leader),
            end_address: format!("0x{:x}", leader),
            instructions: Vec::new(),
            is_entry: leader == address,
            is_exit: false,
        })
        .collect();

    let mut edges: Vec<CfgEdge> = Vec::new();
    let mut cur_block: Option<usize> = None;

    // Pass 2: assign instructions → blocks & build edges
    for ri in &insns {
        // Switch block when we hit a new leader
        if let Some(&bid) = addr_to_block.get(&ri.ip) {
            cur_block = Some(bid);
        }
        let Some(bid) = cur_block else {
            continue;
        };

        let node = &mut nodes[bid];
        let next_ip = ri.ip + ri.len as u64;
        node.end_address = format!("0x{:x}", next_ip);
        node.instructions.push(ri.text.clone());

        match ri.flow {
            DecodedFlowKind::Return => {
                node.is_exit = true;
            }
            DecodedFlowKind::Jump => {
                if let Some(target) = ri
                    .target
                    .filter(|target| *target >= address && *target < end_addr)
                {
                    if let Some(&tid) = addr_to_block.get(&target) {
                        edges.push(CfgEdge {
                            from: bid,
                            to: tid,
                            kind: "unconditional".into(),
                        });
                    }
                }
                cur_block = None;
            }
            DecodedFlowKind::ConditionalJump => {
                // True branch (taken)
                if let Some(target) = ri
                    .target
                    .filter(|target| *target >= address && *target < end_addr)
                {
                    if let Some(&tid) = addr_to_block.get(&target) {
                        edges.push(CfgEdge {
                            from: bid,
                            to: tid,
                            kind: "true".into(),
                        });
                    }
                }
                // False branch (fall-through)
                if let Some(&nid) = addr_to_block.get(&next_ip) {
                    edges.push(CfgEdge {
                        from: bid,
                        to: nid,
                        kind: "false".into(),
                    });
                }
                cur_block = None;
            }
            _ => {
                // Fall-through: if next ip is a new leader, add implicit edge
                if addr_to_block.contains_key(&next_ip) {
                    if let Some(&nid) = addr_to_block.get(&next_ip) {
                        edges.push(CfgEdge {
                            from: bid,
                            to: nid,
                            kind: "unconditional".into(),
                        });
                    }
                    cur_block = None;
                }
            }
        }
    }

    // De-duplicate edges (same from+to+kind can appear from fall-through logic)
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);

    let block_count = nodes.len();
    let edge_count = edges.len();
    // McCabe cyclomatic complexity: V(G) = E – N + 2
    let cyclomatic_complexity = edge_count.saturating_sub(block_count) + 2;

    Ok(CfgDto {
        function_name: func_name,
        function_address: format!("0x{:x}", address),
        nodes,
        edges,
        block_count,
        edge_count,
        cyclomatic_complexity,
    })
}

/// Export the CFG of `address` as a Graphviz DOT string (copied to clipboard on the frontend).
#[tauri::command]
pub async fn export_cfg_dot(address: String, state: State<'_, AppState>) -> CmdResult<String> {
    let cfg = get_cfg(address, state).await?;

    let mut dot = format!(
        "digraph \"{}\" {{\n  rankdir=TB;\n  node [shape=box fontname=\"Courier\" fontsize=10];\n",
        cfg.function_name.replace('"', "'")
    );

    for node in &cfg.nodes {
        let lines: Vec<String> = node
            .instructions
            .iter()
            .map(|i| {
                i.replace('\\', "\\\\")
                    .replace('"', "'")
                    .replace('<', "\\<")
                    .replace('>', "\\>")
            })
            .collect();
        let label = lines.join("\\l");
        let header = node.start_address.replace('"', "'");
        let color = if node.is_entry {
            "lightblue"
        } else if node.is_exit {
            "lightyellow"
        } else {
            "white"
        };
        dot.push_str(&format!(
            "  B{id} [label=\"{h}:\\l{lbl}\\l\" style=filled fillcolor={c}];\n",
            id = node.id,
            h = header,
            lbl = label,
            c = color
        ));
    }

    for edge in &cfg.edges {
        let color = match edge.kind.as_str() {
            "true" => "green",
            "false" => "red",
            _ => "black",
        };
        dot.push_str(&format!(
            "  B{f} -> B{t} [color={c} label=\"{k}\"];\n",
            f = edge.from,
            t = edge.to,
            c = color,
            k = edge.kind
        ));
    }

    dot.push_str("}\n");
    Ok(dot)
}
