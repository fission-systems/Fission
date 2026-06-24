//! Lift a function and dump address-keyed CFG snapshots for parity harnesses.

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

#[derive(Debug)]
struct Args {
    binary: PathBuf,
    address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    model: String,
}

#[derive(Debug, Serialize)]
struct CfgProbeReport {
    tool: &'static str,
    binary: String,
    function_address: u64,
    function_name: Option<String>,
    model: String,
    block_count: usize,
    edge_count: usize,
    snapshot: AddressCfgSnapshot,
    timing: ProbeTiming,
}

#[derive(Debug, Serialize)]
struct ProbeTiming {
    wall_clock_sec: f64,
    binary_load_sec: f64,
    frontend_load_sec: f64,
    decode_lift_sec: f64,
    rust_probe_sec: f64,
}

fn parse_args() -> Result<Args> {
    let mut binary = None;
    let mut address = None;
    let mut max_bytes = 256 * 1024;
    let mut instruction_limit = 512;
    let mut model = "pcode_cfg_builder".to_string();
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--binary" => binary = Some(PathBuf::from(iter.next().context("--binary value")?)),
            "--addr" => {
                address = Some(parse_int(&iter.next().context("--addr value")?)?);
            }
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
            "--model" => model = iter.next().context("--model value")?,
            "--help" | "-h" => {
                bail!(
                    "usage: cfg_probe --binary PATH --addr HEX [--model pcode_cfg_builder|pcode_structuring|pcode_instruction_cfg] [--max-bytes N] [--instruction-limit N]"
                );
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(Args {
        binary: binary.context("--binary is required")?,
        address: address.context("--addr is required")?,
        max_bytes,
        instruction_limit,
        model,
    })
}

fn parse_int(value: &str) -> Result<u64> {
    u64::from_str_radix(
        value.trim_start_matches("0x"),
        if value.starts_with("0x") { 16 } else { 10 },
    )
    .map_err(|err| anyhow!("invalid address {value:?}: {err}"))
}

fn main() -> Result<()> {
    let probe_started = Instant::now();
    let args = parse_args()?;

    let binary_load_started = Instant::now();
    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to load {}", args.binary.display()))?;
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

    let address_state = frontend.normalize_low_bit_code_address(args.address);
    let decode_entry_address = address_state.address;
    let max_bytes = function_max_bytes(&binary, decode_entry_address, args.max_bytes);
    let bytes = binary
        .view_bytes(decode_entry_address, max_bytes)
        .ok_or_else(|| anyhow!("unable to read bytes at 0x{decode_entry_address:x}"))?;

    let decode_lift_started = Instant::now();
    let memory_context = decode_memory_context_for(&binary, decode_entry_address, max_bytes);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_entry_address,
            DecodeContract::decomp_function(args.instruction_limit),
            &memory_context,
            address_state.context_override,
        )
        .with_context(|| format!("lift failed for function at 0x{decode_entry_address:x}"))?;
    let decode_lift_sec = decode_lift_started.elapsed().as_secs_f64();

    let snapshot = match args.model.as_str() {
        "pcode_cfg_builder" => AddressCfgSnapshot::from_pcode_cfg_builder(&lifted.function)
            .with_context(|| format!("cfg export failed for model {}", args.model))?,
        "pcode_structuring" => AddressCfgSnapshot::from_pcode_structuring(&lifted.function),
        "pcode_instruction_cfg" => {
            let cfg_hints = InstructionCfgHints::from_memory_context(&memory_context);
            build_instruction_cfg_snapshot(
                decode_entry_address,
                &lifted.reachable_instruction_addresses,
                &lifted.instruction_lengths,
                &lifted
                    .function
                    .blocks
                    .iter()
                    .flat_map(|block| block.ops.iter().cloned())
                    .collect::<Vec<_>>(),
                &lifted.indirect_targets,
                &lifted.inferred_indirect_edges,
                &cfg_hints,
                false,
            )
        }
        other => bail!("unsupported --model {other}"),
    };

    let report = CfgProbeReport {
        tool: "fission",
        binary: args.binary.display().to_string(),
        function_address: decode_entry_address,
        function_name: None,
        model: args.model,
        block_count: snapshot.block_starts.len(),
        edge_count: snapshot.edges.len(),
        snapshot,
        timing: ProbeTiming {
            wall_clock_sec: probe_started.elapsed().as_secs_f64(),
            binary_load_sec,
            frontend_load_sec,
            decode_lift_sec,
            rust_probe_sec: probe_started.elapsed().as_secs_f64(),
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
