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
                writeln!(stdout, "      0x{:012x}  x{}", edge.addr, edge.count)?;
            }
        }
        if !callees.is_empty() {
            writeln!(stdout, "    callees:")?;
            for edge in callees {
                writeln!(stdout, "      0x{:012x}  x{}", edge.addr, edge.count)?;
            }
        }
    }

    Ok(())
}
