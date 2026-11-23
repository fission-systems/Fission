//! Bounded offline analysis of emulator execution traces.
//!
//! Trace evidence is an observation, not a proof of semantic equivalence.
//! In particular, showing which instructions executed never feeds speculative
//! facts into the normal decompiler pipeline.

use anyhow::{Context, Result, bail};
use fission_loader::loader::LoadedBinary;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::cli::args::{DebugTraceDecompArgs, DebugTraceDiffArgs, DebugTraceInspectArgs};

const MAX_EVENTS: usize = 1_000_000;
const MAX_DISPLAY_CALLS: usize = 1000;

fn limit(requested: usize) -> Result<usize> {
    if requested == 0 || requested > MAX_EVENTS {
        bail!("--max-events must be between 1 and {MAX_EVENTS}");
    }
    Ok(requested)
}

fn parse_hex_or_integer(value: &Value) -> Option<u64> {
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    let text = value.as_str()?;
    if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<u64>().ok()
    }
}

#[derive(Default)]
struct TraceData {
    instructions: BTreeMap<u64, u64>,
    calls: Vec<Value>,
    call_counts: BTreeMap<String, u64>,
    errors: u64,
    total: usize,
    truncated: bool,
}

fn inspect<R: BufRead>(reader: R, max: usize) -> Result<TraceData> {
    let mut trace = TraceData::default();
    for (line_number, line) in reader.lines().enumerate() {
        if trace.total >= max {
            trace.truncated = true;
            break;
        }
        let line = line.with_context(|| format!("read trace line {}", line_number + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse trace line {}", line_number + 1))?;
        trace.total += 1;
        match event["kind"].as_str() {
            Some("Instruction") => {
                if let Some(pc) = parse_hex_or_integer(&event["pc"]) {
                    *trace.instructions.entry(pc).or_default() += 1;
                }
            }
            Some("HleDispatch") | Some("UseropeDispatch") => {
                let name = event["func_name"]
                    .as_str()
                    .or_else(|| event["userop_name"].as_str())
                    .unwrap_or("<unknown>");
                *trace.call_counts.entry(name.to_owned()).or_default() += 1;
                if trace.calls.len() < MAX_DISPLAY_CALLS {
                    trace.calls.push(json!({
                        "kind": event["kind"],
                        "pc": event["pc"],
                        "name": name,
                        "source": "emulator_trace",
                    }));
                }
            }
            Some("DecodeError") => trace.errors += 1,
            _ => {
                // Unknown trace events are retained in the total but are
                // never guessed to be instructions or runtime API calls.
            }
        }
    }
    Ok(trace)
}

fn open_trace(path: &str, max: usize) -> Result<TraceData> {
    let file = File::open(path).with_context(|| format!("open trace {path}"))?;
    inspect(BufReader::new(file), limit(max)?)
}

fn evidence(trace: &TraceData) -> Value {
    let mut instructions = Vec::new();
    for (pc, count) in &trace.instructions {
        instructions.push(json!({"pc": format!("0x{pc:x}"), "hits": count}));
    }
    json!({
        "total_events": trace.total,
        "truncated": trace.truncated,
        "instruction_addresses": trace.instructions.len(),
        "instruction_hits": instructions,
        "call_counts": trace.call_counts,
        "call_events": trace.calls,
        "decode_errors": trace.errors,
        "calls_truncated": trace.calls.len() >= MAX_DISPLAY_CALLS,
        "source": "emulator_trace_observation",
    })
}

pub(super) fn trace_inspect(args: &DebugTraceInspectArgs) -> Result<Value> {
    let trace = open_trace(&args.trace, args.max_events)?;
    Ok(json!({
        "schema_version": 1,
        "action": "trace_inspect",
        "trace": args.trace,
        "evidence": evidence(&trace),
    }))
}

fn next_nonblank<R: BufRead>(lines: &mut std::io::Lines<R>) -> Result<Option<Value>> {
    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        return Ok(Some(serde_json::from_str(&line)?));
    }
    Ok(None)
}

fn diff<R: BufRead, S: BufRead>(left: R, right: S, max: usize) -> Result<Value> {
    let mut left = left.lines();
    let mut right = right.lines();
    for index in 0..max {
        let a = next_nonblank(&mut left)?;
        let b = next_nonblank(&mut right)?;
        if a != b {
            return Ok(json!({
                "status": "diverged",
                "first_difference_index": index,
                "left": a,
                "right": b,
                "meaning": "recorded_event_difference",
            }));
        }
        if a.is_none() {
            return Ok(json!({
                "status": "matched_recorded_events",
                "compared_events": index,
                "semantic_equivalence_proved": false,
            }));
        }
    }
    Ok(json!({
        "status": "inconclusive",
        "compared_events": max,
        "reason": "event comparison budget exhausted",
    }))
}

pub(super) fn trace_diff(args: &DebugTraceDiffArgs) -> Result<Value> {
    let max = limit(args.max_events)?;
    let left = File::open(&args.left).with_context(|| format!("open {}", args.left))?;
    let right = File::open(&args.right).with_context(|| format!("open {}", args.right))?;
    Ok(json!({
        "schema_version": 1,
        "action": "trace_diff",
        "left": args.left,
        "right": args.right,
        "result": diff(BufReader::new(left), BufReader::new(right), max)?,
    }))
}

pub(super) fn trace_decomp(args: &DebugTraceDecompArgs) -> Result<Value> {
    let mut binary = LoadedBinary::from_file(Path::new(&args.binary))
        .with_context(|| format!("open binary {}", args.binary))?;
    let _ = fission_static::analysis::discover_functions_with_runtime(&mut binary);
    let func = binary
        .function_at(args.addr)
        .with_context(|| format!("no discovered function contains 0x{:x}", args.addr))?;

    let trace = open_trace(&args.trace, args.max_events)?;
    let within =
        |pc: &u64| func.size != 0 && *pc >= func.address && (*pc - func.address) < func.size;
    let observed: Vec<Value> = trace
        .instructions
        .iter()
        .filter(|(pc, _)| within(pc))
        .map(|(pc, count)| json!({"pc": format!("0x{pc:x}"), "hits": count}))
        .collect();
    let config = fission_decompiler::RustSleighDecompileConfig::cli_defaults();
    let decompiled = fission_decompiler::decompile_with_rust_sleigh(
        &binary,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(|err| anyhow::anyhow!("static decompile failed: {err}"))?;
    Ok(json!({
        "schema_version": 1,
        "action": "trace_decomp",
        "binary": args.binary,
        "trace": args.trace,
        "function": {
            "entry": format!("0x{:x}", func.address),
            "name": func.name,
            "size": func.size,
        },
        "decompilation": {
            "code": decompiled.code,
            "fell_back": decompiled.fell_back,
            "source": "on_disk_static_analysis",
        },
        "dynamic_evidence": {
            "status": "observed_not_promoted",
            "instruction_hits_in_function": observed,
            "trace_truncated": trace.truncated,
            "notes": "Only dynamically observed instruction PCs are shown. They do not alter static analysis or prove path completeness.",
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_separate_hle_calls_from_instruction_hits() {
        let data = br#"{"kind":"Instruction","pc":4096}
{"kind":"Instruction","pc":4096}
{"kind":"HleDispatch","pc":4100,"func_name":"VirtualAlloc"}
{"kind":"UseropeDispatch","pc":4102,"userop_name":"syscall"}
{"kind":"DecodeError","pc":4104,"reason":"bad"}
"#;
        let result = inspect(&data[..], 64).unwrap();
        assert_eq!(result.total, 5);
        assert_eq!(result.instructions.get(&4096), Some(&2));
        assert_eq!(result.call_counts.get("VirtualAlloc"), Some(&1));
        assert_eq!(result.call_counts.get("syscall"), Some(&1));
        assert_eq!(result.errors, 1);
    }

    #[test]
    fn diff_reports_first_mismatch_and_bounds() {
        let left = b"{\"a\":1}\n{\"a\":2}\n";
        let right = b"{\"a\":1}\n{\"a\":3}\n";
        let result = diff(&left[..], &right[..], 10).unwrap();
        assert_eq!(result["status"], "diverged");
        assert_eq!(result["first_difference_index"], 1);
        let result = diff(&left[..], &left[..], 1).unwrap();
        assert_eq!(result["status"], "inconclusive");
    }

    #[test]
    fn invalid_budget_and_invalid_json_fail() {
        assert!(limit(0).is_err());
        assert!(limit(MAX_EVENTS + 1).is_err());
        assert!(inspect(&b"garbage\n"[..], 5).is_err());
    }
}
