//! `fission_cli call`: run one function in the emulator with chosen
//! arguments. The machinery is `fission_dir::EmulatorHarness`, the same
//! harness `verify`'s ground-truth tier calls functions through; this command
//! hands it the caller's arguments instead of generated ones.

use crate::cli::args::CallArgs;
use anyhow::{Context, Result};
use serde_json::{Value, json};

fn hex(value: u64) -> String {
    format!("0x{value:x}")
}

/// How a plain call ended, as fields.
fn outcome_json(outcome: &fission_dir::CallOutcome) -> Value {
    match outcome {
        fission_dir::CallOutcome::Returned(value) => {
            json!({ "outcome": "returned", "return_value": hex(*value) })
        }
        fission_dir::CallOutcome::HitBudget => json!({ "outcome": "instruction_budget" }),
        fission_dir::CallOutcome::ProcessExited => json!({ "outcome": "process_exited" }),
        fission_dir::CallOutcome::Other(reason) => {
            json!({ "outcome": "stopped", "reason": format!("{reason:?}") })
        }
    }
}

/// How a stubbed call ended, with every call it made and every byte it
/// stored outside its own frame.
fn stubbed_json(run: &fission_dir::StubbedRun) -> Value {
    let mut out = match &run.end {
        fission_dir::StubbedEnd::Returned(value) => {
            json!({ "outcome": "returned", "return_value": hex(*value) })
        }
        fission_dir::StubbedEnd::NoReturnCall(site) => {
            json!({ "outcome": "no_return_call", "at": hex(*site) })
        }
        fission_dir::StubbedEnd::LeftDecodedCode(at) => {
            json!({ "outcome": "left_decoded_code", "at": hex(*at) })
        }
        fission_dir::StubbedEnd::Other(outcome) => outcome_json(outcome),
    };
    out["calls"] = run
        .calls
        .iter()
        .map(|call| {
            json!({
                "site": hex(call.site),
                "args": call.args.iter().map(|a| hex(*a)).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>()
        .into();
    out["writes"] = run
        .writes
        .iter()
        .map(|(address, byte)| json!({ "address": hex(*address), "byte": format!("{byte:02x}") }))
        .collect::<Vec<_>>()
        .into();
    out
}

pub(crate) fn run(args: &CallArgs) -> Result<()> {
    let binary = fission_loader::loader::LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
    let address = match (args.addr, args.name.as_deref()) {
        (Some(address), _) => address,
        (None, Some(name)) => {
            let mut binary = binary.clone();
            fission_static::analysis::discover_functions_with_runtime(&mut binary);
            super::function_select::select_function_address_by_name(&binary, name)?
        }
        (None, None) => anyhow::bail!("give --addr or --name"),
    };

    let mut harness = fission_dir::EmulatorHarness::build(&args.binary, Some(args.max_inst))
        .context("failed to set up the emulator for this binary")?;
    let mut report = json!({
        "address": hex(address),
        "args": args.args.iter().map(|a| hex(*a)).collect::<Vec<_>>(),
    });
    let result = match args.stub_calls {
        None => outcome_json(&harness.call(address, &args.args)?),
        Some(value) => {
            let flow = fission_dir::call_sites(&binary, address)
                .context("failed to decode the function's calls")?;
            let stubbed = harness.call_stubbed(address, &args.args, &flow, &|_| value)?;
            let mut result = stubbed_json(&stubbed);
            result["stub_return"] = hex(value).into();
            result
        }
    };
    if let (Value::Object(report), Value::Object(result)) = (&mut report, result) {
        report.extend(result);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "call 0x{address:x}({}): {}",
            args.args
                .iter()
                .map(|a| hex(*a))
                .collect::<Vec<_>>()
                .join(", "),
            match report["outcome"].as_str() {
                Some("returned") => format!(
                    "returned {}",
                    report["return_value"].as_str().unwrap_or("?")
                ),
                Some(other) => other.replace('_', " "),
                None => "?".into(),
            }
        );
        if let Some(calls) = report["calls"].as_array() {
            for call in calls {
                println!("  call at {}: {}", call["site"], call["args"]);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::args::parse_call_argument;

    #[test]
    fn arguments_take_decimal_hex_and_negative_values() {
        assert_eq!(parse_call_argument("42"), Ok(42));
        assert_eq!(parse_call_argument("0x2a"), Ok(42));
        assert_eq!(parse_call_argument("-1"), Ok(u64::MAX));
        assert!(parse_call_argument("forty-two").is_err());
    }
}
