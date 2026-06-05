//! Dump `ControlFlowFacts` slices as JSON for the CFG fact coverage benchmark.

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::control_flow_facts::{control_flow_facts_for, FunctionControlFlowFacts};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FactsProbeReport {
    tool: &'static str,
    binary: String,
    function_address: u64,
    function_name: Option<String>,
    facts: FunctionControlFlowFacts,
    timing: ProbeTiming,
}

#[derive(Debug, Serialize)]
struct FactsSweepReport {
    tool: &'static str,
    binary: String,
    function_count: usize,
    functions: std::collections::BTreeMap<String, FactsSweepEntry>,
    timing: SweepTiming,
}

#[derive(Debug, Serialize)]
struct FactsSweepEntry {
    function_address: u64,
    function_name: Option<String>,
    label_count: usize,
    flow_edge_count: usize,
    indirect_target_count: usize,
    noreturn_callsite_count: usize,
    facts: FunctionControlFlowFacts,
}

#[derive(Debug, Serialize)]
struct ProbeTiming {
    wall_clock_sec: f64,
    binary_load_sec: f64,
    facts_build_sec: f64,
}

#[derive(Debug, Serialize)]
struct SweepTiming {
    wall_clock_sec: f64,
    binary_load_sec: f64,
    facts_build_sec: f64,
}

fn parse_addr(value: &str) -> Result<u64> {
    u64::from_str_radix(value.trim_start_matches("0x"), 16)
        .with_context(|| format!("invalid address {value}"))
}

fn function_max_bytes(binary: &LoadedBinary, entry: u64) -> usize {
    if let Some(func) = binary.function_at_exact(entry) {
        if func.size > 0 {
            return func.size as usize;
        }
    }
    let mut next = entry.saturating_add(256 * 1024);
    for info in &binary.functions {
        if info.address > entry && info.address < next {
            next = info.address;
        }
    }
    next.saturating_sub(entry) as usize
}

fn facts_for_entry(
    binary: &LoadedBinary,
    facts: &fission_static::analysis::control_flow_facts::ControlFlowFacts,
    entry: u64,
) -> FunctionControlFlowFacts {
    facts.facts_for_function(binary, entry, function_max_bytes(binary, entry))
}

fn main() -> Result<()> {
    let probe_started = Instant::now();
    let mut binary_path = None;
    let mut addr = None;
    let mut all_functions = false;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--binary" => binary_path = Some(PathBuf::from(iter.next().context("--binary value")?)),
            "--addr" => addr = Some(parse_addr(&iter.next().context("--addr value")?)?),
            "--all-functions" => all_functions = true,
            other => bail!("unknown argument {other}"),
        }
    }

    let binary_path = binary_path.context("--binary is required")?;
    if !all_functions && addr.is_none() {
        bail!("one of --addr or --all-functions is required");
    }

    let load_started = Instant::now();
    let binary = LoadedBinary::from_file(&binary_path).context("load binary")?;
    let binary_load_sec = load_started.elapsed().as_secs_f64();

    let facts_started = Instant::now();
    let facts = control_flow_facts_for(&binary);
    let facts_build_sec = facts_started.elapsed().as_secs_f64();

    if all_functions {
        let mut functions = std::collections::BTreeMap::new();
        for func in &binary.functions {
            if func.is_import {
                continue;
            }
            let entry = func.address;
            let slice = facts_for_entry(&binary, &facts, entry);
            functions.insert(
                format!("0x{entry:x}"),
                FactsSweepEntry {
                    function_address: entry,
                    function_name: Some(func.name.clone()),
                    label_count: slice.labels.len(),
                    flow_edge_count: slice.flow_edges.len(),
                    indirect_target_count: slice.indirect_targets.len(),
                    noreturn_callsite_count: slice.noreturn_callsites.len(),
                    facts: slice,
                },
            );
        }

        let report = FactsSweepReport {
            tool: "fission",
            binary: binary_path.display().to_string(),
            function_count: functions.len(),
            functions,
            timing: SweepTiming {
                wall_clock_sec: probe_started.elapsed().as_secs_f64(),
                binary_load_sec,
                facts_build_sec,
            },
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let entry = addr.context("--addr is required unless --all-functions")?;
    let slice = facts_for_entry(&binary, &facts, entry);
    let report = FactsProbeReport {
        tool: "fission",
        binary: binary_path.display().to_string(),
        function_address: entry,
        function_name: binary
            .function_at_exact(entry)
            .map(|func| func.name.clone()),
        facts: slice,
        timing: ProbeTiming {
            wall_clock_sec: probe_started.elapsed().as_secs_f64(),
            binary_load_sec,
            facts_build_sec,
        },
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
