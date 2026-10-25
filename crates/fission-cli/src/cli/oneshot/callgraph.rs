//! Call graph emission (`fission-static::callgraph`).

use anyhow::{Context, Result, bail};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{CallGraph, build_xref_index};
use serde_json::json;
use std::io::Write;

use crate::cli::args::OneShotArgs;

pub(super) fn run_callgraph(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    if let Some(entry) = cli.callgraph_function {
        if binary.function_at_exact(entry).is_none() {
            bail!(
                "no discovered function starts at 0x{entry:x}; use `list` with the same discovery profile"
            );
        }
    }
    let index = build_xref_index(binary, true);
    let graph = CallGraph::build_from_xref_index(&binary.functions, &index, 0x40);
    let mut functions: Vec<_> = binary
        .functions
        .iter()
        .filter(|function| {
            cli.callgraph_function
                .is_none_or(|entry| function.address == entry)
        })
        .collect();
    functions.sort_by_key(|function| function.address);

    let mut stdout = std::io::stdout().lock();

    if cli.json {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for func in &functions {
            let callers: Vec<_> = graph
                .callers_of(func.address)
                .iter()
                .map(|e| {
                    json!({
                        "addr": format!("0x{:x}", e.addr),
                        "name": edge_label(binary, e.addr),
                        "count": e.count,
                    })
                })
                .collect();
            let callees: Vec<_> = graph
                .callees_of(func.address)
                .iter()
                .map(|e| {
                    edges.push(fission_analysis_protocol::CallGraphEdge {
                        caller_addr: func.address,
                        callee_addr: e.addr,
                        call_site: 0,
                        is_indirect: false,
                    });
                    json!({
                        "addr": format!("0x{:x}", e.addr),
                        "name": edge_label(binary, e.addr),
                        "count": e.count,
                    })
                })
                .collect();
            nodes.push(json!({
                "address": format!("0x{:x}", func.address),
                "name": func.name,
                "callers": callers,
                "callees": callees,
            }));
        }
        let payload = json!({
            "total_call_sites": graph.total_call_sites(),
            "function_count": binary.functions.len(),
            "node_count": nodes.len(),
            "selected_function": cli.callgraph_function.map(|entry| format!("0x{entry:x}")),
            "nodes": nodes,
            "edges": edges,
            "analysis": index.analysis,
        });
        let text = serde_json::to_string_pretty(&payload).context("serialize callgraph JSON")?;
        println!("{}", text);
        return Ok(());
    }

    writeln!(
        stdout,
        "callgraph: functions={} total_call_sites={}",
        binary.functions.len(),
        graph.total_call_sites()
    )
    .context("write callgraph header")?;

    writeln!(
        stdout,
        "xref_analysis: {}",
        serde_json::to_string(&index.analysis)?
    )?;

    for func in &functions {
        let callers = graph.callers_of(func.address);
        let callees = graph.callees_of(func.address);
        if callers.is_empty() && callees.is_empty() && cli.callgraph_function.is_none() {
            continue;
        }
        writeln!(
            stdout,
            "  0x{:012x}  {}  callers={}  callees={}",
            func.address,
            func.name,
            callers.len(),
            callees.len()
        )
        .context("write callgraph node")?;
        if !callers.is_empty() {
            writeln!(stdout, "    callers:")?;
            for edge in callers {
                writeln!(
                    stdout,
                    "      0x{:012x}  {:<32}  x{}",
                    edge.addr,
                    edge_label(binary, edge.addr),
                    edge.count
                )?;
            }
        }
        if !callees.is_empty() {
            writeln!(stdout, "    callees:")?;
            for edge in callees {
                writeln!(
                    stdout,
                    "      0x{:012x}  {:<32}  x{}",
                    edge.addr,
                    edge_label(binary, edge.addr),
                    edge.count
                )?;
            }
        }
    }

    Ok(())
}

/// Convert static analysis callgraph into canonical `CallGraphResponse` protocol wire type.
pub fn build_protocol_callgraph(
    binary: &LoadedBinary,
    graph: &CallGraph,
) -> fission_analysis_protocol::CallGraphResponse {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for func in &binary.functions {
        let callers = graph.callers_of(func.address);
        let callees = graph.callees_of(func.address);
        nodes.push(fission_analysis_protocol::CallGraphNode {
            address: func.address,
            name: func.name.clone(),
            caller_count: callers.len(),
            callee_count: callees.len(),
        });
        for callee in callees {
            edges.push(fission_analysis_protocol::CallGraphEdge {
                caller_addr: func.address,
                callee_addr: callee.addr,
                call_site: 0,
                is_indirect: false,
            });
        }
    }

    fission_analysis_protocol::CallGraphResponse {
        nodes,
        edges,
        total_call_sites: graph.total_call_sites(),
    }
}

/// What to call the function on the other end of an edge.
///
/// The node line has always carried a name; the edges under it were bare
/// addresses, so reading `__tmainCRTStartup calls 0x1400027d0 twice` meant
/// going back to `list` for every line. The name is in the same table the
/// node's came from.
fn edge_label(binary: &LoadedBinary, address: u64) -> String {
    match binary.function_at_exact(address) {
        Some(function) if !function.name.is_empty() => function.name.clone(),
        // An edge can land on an import thunk or a block the discovery pass
        // never made a function of; those genuinely have no name to give.
        _ => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::common::types::FunctionInfo;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    #[test]
    fn edge_label_resolves_known_function_names() {
        let binary = LoadedBinaryBuilder::new("test.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("TEST")
            .image_base(0x1000)
            .add_function(FunctionInfo {
                name: "helper_func".into(),
                address: 0x1000,
                size: 16,
                ..Default::default()
            })
            .build()
            .expect("fixture");

        assert_eq!(edge_label(&binary, 0x1000), "helper_func");
        assert_eq!(edge_label(&binary, 0x9999), "-");
    }
}
