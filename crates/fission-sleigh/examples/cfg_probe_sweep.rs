//! Sweep all loader-known functions and dump instruction-level CFG snapshots.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_pcode::cfg::AddressCfgSnapshot;
use fission_sleigh::runtime::{
    build_instruction_cfg_snapshot, DecodeContract, InstructionCfgHints, RuntimeSleighFrontend,
};
use fission_static::analysis::control_flow_facts::{decode_memory_context_for, function_max_bytes};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FunctionSweepEntry {
    function_address: u64,
    function_name: String,
    block_count: usize,
    edge_count: usize,
    lift_error: Option<String>,
    snapshot: Option<AddressCfgSnapshot>,
}

#[derive(Debug, Serialize)]
struct CfgSweepReport {
    tool: &'static str,
    binary: String,
    model: String,
    function_count: usize,
    lifted_count: usize,
    failed_count: usize,
    functions: BTreeMap<String, FunctionSweepEntry>,
    timing: SweepTiming,
}

#[derive(Debug, Serialize)]
struct SweepTiming {
    wall_clock_sec: f64,
    binary_load_sec: f64,
    frontend_load_sec: f64,
    sweep_lift_sec: f64,
}

fn parse_int(value: &str) -> Result<u64> {
    u64::from_str_radix(
        value.trim_start_matches("0x"),
        if value.starts_with("0x") { 16 } else { 10 },
    )
    .map_err(|err| anyhow!("invalid address {value:?}: {err}"))
}

fn lift_function_snapshot(
    frontend: &RuntimeSleighFrontend,
    binary: &LoadedBinary,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
) -> Result<AddressCfgSnapshot> {
    let address_state = frontend.normalize_low_bit_code_address(entry_address);
    let decode_entry_address = address_state.address;
    let func_max_bytes = function_max_bytes(&binary, decode_entry_address, max_bytes);
    let bytes = binary
        .view_bytes(decode_entry_address, func_max_bytes)
        .ok_or_else(|| anyhow!("unable to read bytes at 0x{decode_entry_address:x}"))?;
    let memory_context = decode_memory_context_for(&binary, decode_entry_address, func_max_bytes);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_entry_address,
            DecodeContract::decomp_function(instruction_limit),
            &memory_context,
            address_state.context_override,
        )
        .with_context(|| format!("lift failed for function at 0x{decode_entry_address:x}"))?;

    let ops = lifted
        .function
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter().cloned())
        .collect::<Vec<_>>();

    let cfg_hints = InstructionCfgHints::from_memory_context(&memory_context);

    Ok(build_instruction_cfg_snapshot(
        decode_entry_address,
        &lifted.reachable_instruction_addresses,
        &lifted.instruction_lengths,
        &ops,
        &lifted.indirect_targets,
        &lifted.inferred_indirect_edges,
        &cfg_hints,
        false,
    ))
}

fn main() -> Result<()> {
    let probe_started = Instant::now();
    let mut binary_path = None;
    let mut model = "pcode_instruction_cfg".to_string();
    let mut max_bytes = 256 * 1024;
    let mut instruction_limit = 512;
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--binary" => binary_path = Some(PathBuf::from(iter.next().context("--binary value")?)),
            "--model" => model = iter.next().context("--model value")?,
            "--max-bytes" => {
                max_bytes = iter
                    .next()
                    .context("--max-bytes value")?
                    .parse()
                    .context("invalid --max-bytes")?;
            }
            "--instruction-limit" => {
                instruction_limit = iter
                    .next()
                    .context("--instruction-limit value")?
                    .parse()
                    .context("invalid --instruction-limit")?;
            }
            "--help" | "-h" => {
                bail!("usage: cfg_probe_sweep --binary PATH [--model pcode_instruction_cfg] [--max-bytes N] [--instruction-limit N]")
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let binary_path = binary_path.context("--binary is required")?;
    if model != "pcode_instruction_cfg" {
        bail!("cfg_probe_sweep currently supports only --model pcode_instruction_cfg");
    }

    let binary_load_started = Instant::now();
    let binary = LoadedBinary::from_file(&binary_path)
        .with_context(|| format!("failed to load {}", binary_path.display()))?;
    let binary_load_sec = binary_load_started.elapsed().as_secs_f64();

    let load_spec = binary
        .load_spec()
        .ok_or_else(|| anyhow!("loader did not select a load spec"))?;

    let frontend_load_started = Instant::now();
    let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)?;
    let frontend = frontends
        .first()
        .ok_or_else(|| anyhow!("no executable SLEIGH frontend candidates"))?;
    let frontend_load_sec = frontend_load_started.elapsed().as_secs_f64();

    let sweep_started = Instant::now();
    let mut functions = BTreeMap::new();
    let mut lifted_count = 0usize;
    let mut failed_count = 0usize;

    let mut entries: Vec<_> = binary.inner().functions.iter().collect();
    entries.sort_by_key(|func| func.address);

    for func in entries {
        if func.is_import || func.is_thunk_like {
            continue;
        }
        let entry = func.address;
        match lift_function_snapshot(frontend, &binary, entry, max_bytes, instruction_limit) {
            Ok(snapshot) => {
                lifted_count += 1;
                functions.insert(
                    format!("0x{entry:x}"),
                    FunctionSweepEntry {
                        function_address: entry,
                        function_name: func.name.clone(),
                        block_count: snapshot.block_starts.len(),
                        edge_count: snapshot.edges.len(),
                        lift_error: None,
                        snapshot: Some(snapshot),
                    },
                );
            }
            Err(err) => {
                failed_count += 1;
                functions.insert(
                    format!("0x{entry:x}"),
                    FunctionSweepEntry {
                        function_address: entry,
                        function_name: func.name.clone(),
                        block_count: 0,
                        edge_count: 0,
                        lift_error: Some(err.to_string()),
                        snapshot: None,
                    },
                );
            }
        }
    }

    let report = CfgSweepReport {
        tool: "fission",
        binary: binary_path.display().to_string(),
        model,
        function_count: functions.len(),
        lifted_count,
        failed_count,
        functions,
        timing: SweepTiming {
            wall_clock_sec: probe_started.elapsed().as_secs_f64(),
            binary_load_sec,
            frontend_load_sec,
            sweep_lift_sec: sweep_started.elapsed().as_secs_f64(),
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
