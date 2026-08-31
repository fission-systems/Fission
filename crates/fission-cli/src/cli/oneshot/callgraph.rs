//! Call graph emission (`fission-static::callgraph`).

use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{CallGraph, XrefDatabase};
use serde_json::json;
use std::io::Write;

use crate::cli::args::OneShotArgs;

pub(super) fn run_callgraph(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let xref_db = XrefDatabase::build_from_binary(binary);
    let graph = CallGraph::build_from_xrefs(&binary.functions, &xref_db, 0x40);

    let mut stdout = std::io::stdout().lock();

    if cli.json {
        let mut nodes = Vec::new();
        for func in &binary.functions {
            let callers: Vec<_> = graph
                .callers_of(func.address)
                .iter()
                .map(|e| {
                    json!({
                        "addr": format!("0x{:x}", e.addr),
                        "count": e.count,
                    })
                })
                .collect();
            let callees: Vec<_> = graph
                .callees_of(func.address)
                .iter()
                .map(|e| {
                    json!({
                        "addr": format!("0x{:x}", e.addr),
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
            "nodes": nodes,
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

    for func in &binary.functions {
        let callers = graph.callers_of(func.address);
        let callees = graph.callees_of(func.address);
        if callers.is_empty() && callees.is_empty() {
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
