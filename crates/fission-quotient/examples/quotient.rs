//! Compute the behavioural quotient of one step function and print the
//! `quotient.v1` JSON.
//!
//! The function is taken as `result step(ctx *, input)`: the context pointer
//! in the first argument slot, one input port in the second.
//!
//! Usage:
//!   cargo run --release -p fission-quotient --example quotient -- \
//!     <binary> <address> [--input-bits N] [--output-bits N] \
//!     [--context-bytes N] [--horizon H] [--max-states K]

use std::path::PathBuf;

use fission_quotient::{
    ContextRegion, InputPort, QuotientBoundary, QuotientBudget, QuotientEngine, Wbcq,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: quotient <binary> <address> [--input-bits N] [--output-bits N] \
                 [--context-bytes N] [--horizon H] [--max-states K]";
    let path = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!(usage))?);
    let address = args.next().ok_or_else(|| anyhow::anyhow!(usage))?;
    let entry = u64::from_str_radix(address.trim_start_matches("0x"), 16)?;
    let (mut input_bits, mut output_bits, mut context_bytes) = (32, 32, 8usize);
    let mut budget = QuotientBudget::default();
    while let Some(flag) = args.next() {
        let value: u64 = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))?
            .parse()?;
        match flag.as_str() {
            "--input-bits" => input_bits = value as u32,
            "--output-bits" => output_bits = value as u32,
            "--context-bytes" => context_bytes = value as usize,
            "--horizon" => budget.horizon = value as u32,
            "--max-states" => budget.max_states = value as usize,
            other => anyhow::bail!("unknown flag {other}\n{usage}"),
        }
    }
    let boundary = QuotientBoundary {
        entry,
        inputs: vec![InputPort {
            name: "input".to_string(),
            arg_index: 1,
            bits: input_bits,
        }],
        context: ContextRegion {
            arg_index: 0,
            initial: vec![0; context_bytes],
        },
        output_bits,
    };
    let (mut engine, foundation) = Wbcq::for_binary(&path, boundary, 1_000_000)?;
    let report = engine.synthesize(&foundation, budget)?;
    println!("{}", report.to_json()?);
    Ok(())
}
