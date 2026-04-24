use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{PcodeOp, Varnode};
use fission_sleigh::runtime::{DecodedInstruction, RuntimeSleighFrontend};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ProbeReport {
    binary: String,
    language_id: Option<String>,
    compiler_spec_id: Option<String>,
    entry_id: String,
    execution_mode: String,
    compat_emitter_used: bool,
    start_address: u64,
    requested_count: usize,
    instructions: Vec<InstructionReport>,
}

#[derive(Debug, Serialize)]
struct InstructionReport {
    address: u64,
    status: String,
    error: Option<String>,
    decoded: Option<DecodedInstruction>,
    length: Option<u64>,
    compat_emitter_used: bool,
    template_source: Option<String>,
    pcode: Vec<SerializablePcodeOp>,
}

#[derive(Debug, Serialize)]
struct SerializablePcodeOp {
    seq_num: u32,
    opcode: String,
    address: u64,
    output: Option<Varnode>,
    inputs: Vec<Varnode>,
}

impl From<&PcodeOp> for SerializablePcodeOp {
    fn from(op: &PcodeOp) -> Self {
        Self {
            seq_num: op.seq_num,
            opcode: format!("{:?}", op.opcode),
            address: op.address,
            output: op.output.clone(),
            inputs: op.inputs.clone(),
        }
    }
}

#[derive(Debug)]
struct Args {
    binary: PathBuf,
    address: u64,
    count: usize,
    window_bytes: usize,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to load {}", args.binary.display()))?;
    let load_spec = binary
        .load_spec()
        .ok_or_else(|| anyhow!("loader did not select a load spec"))?;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec)?;

    let mut instructions = Vec::new();
    let mut current = args.address;
    for _ in 0..args.count {
        let Some(bytes) = binary.view_bytes(current, args.window_bytes) else {
            instructions.push(InstructionReport {
                address: current,
                status: "error".to_string(),
                error: Some(format!("unable to read bytes at 0x{current:x}")),
                decoded: None,
                length: None,
                compat_emitter_used: false,
                template_source: None,
                pcode: Vec::new(),
            });
            break;
        };

        let decoded = frontend.decode_window(bytes, current, 1);
        match frontend.decode_and_lift_with_details(bytes, current) {
            Ok((ops, len, details)) => {
                instructions.push(InstructionReport {
                    address: current,
                    status: "ok".to_string(),
                    error: None,
                    decoded: decoded.ok().and_then(|mut window| window.pop()),
                    length: Some(len),
                    compat_emitter_used: details.compat_emitter_used,
                    template_source: details
                        .template_source
                        .map(|source| format!("{:?}", source)),
                    pcode: ops.iter().map(SerializablePcodeOp::from).collect(),
                });
                if len == 0 {
                    break;
                }
                current = current.saturating_add(len);
            }
            Err(err) => {
                instructions.push(InstructionReport {
                    address: current,
                    status: "error".to_string(),
                    error: Some(err.to_string()),
                    decoded: decoded.ok().and_then(|mut window| window.pop()),
                    length: None,
                    compat_emitter_used: false,
                    template_source: None,
                    pcode: Vec::new(),
                });
                break;
            }
        }
    }

    let report = ProbeReport {
        binary: args.binary.display().to_string(),
        language_id: Some(load_spec.pair.language_id.as_str().to_string()),
        compiler_spec_id: Some(load_spec.pair.compiler_spec_id.as_str().to_string()),
        entry_id: frontend.entry().entry_id.clone(),
        execution_mode: "compiled_table_mixed".to_string(),
        compat_emitter_used: instructions
            .iter()
            .any(|instruction| instruction.compat_emitter_used),
        start_address: args.address,
        requested_count: args.count,
        instructions,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut binary = None;
    let mut address = None;
    let mut count = 8usize;
    let mut window_bytes = 32usize;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--binary" => binary = iter.next().map(PathBuf::from),
            "--addr" | "--address" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                address = Some(parse_u64(&raw)?);
            }
            "--count" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                count = raw
                    .parse()
                    .with_context(|| format!("invalid count: {raw}"))?;
            }
            "--window-bytes" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for {arg}"))?;
                window_bytes = raw
                    .parse()
                    .with_context(|| format!("invalid window byte count: {raw}"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(Args {
        binary: binary.ok_or_else(|| anyhow!("missing --binary"))?,
        address: address.ok_or_else(|| anyhow!("missing --addr"))?,
        count,
        window_bytes,
    })
}

fn parse_u64(raw: &str) -> Result<u64> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).with_context(|| format!("invalid hex address: {raw}"))
    } else {
        raw.parse()
            .with_context(|| format!("invalid decimal address: {raw}"))
    }
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p fission-sleigh --example raw_pcode_probe -- \\
  --binary <path> --addr <hex-or-decimal> [--count N] [--window-bytes N]"
    );
}
