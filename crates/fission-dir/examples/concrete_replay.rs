// SPDX-License-Identifier: Apache-2.0
//! Explicit concrete inputs for the repository replay fixture, plus bounded HIR export.

#[path = "replay/python.rs"]
mod python;

use anyhow::{Context, Result, bail, ensure};
use fission_dir::eval::{World, normalize, run_hir};
use fission_dir::{EmulatorHarness, PreHirHirPair, StubbedEnd, call_sites, decompile_one};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use serde_json::json;
use std::path::Path;

const OBSERVED_BUFFER_LEN: usize = 3;
const INSTRUCTION_LIMIT: u64 = 10_000;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [mode, path, name, rest @ ..] = args.as_slice() else {
        bail!("usage: concrete_replay <observe|python> <binary> <function> [input byte byte byte]");
    };
    let path = Path::new(path);
    let binary = LoadedBinary::from_file(path).context("load binary")?;
    let matches = binary
        .inner()
        .functions
        .iter()
        .filter(|function| function.name == *name)
        .collect::<Vec<_>>();
    let [function] = matches.as_slice() else {
        bail!("expected one exact symbol named {name}");
    };
    let facts = FactStore::from_binary(&binary);
    let pair = decompile_one(&binary, &facts, function).context("decompile function")?;
    match (mode.as_str(), rest) {
        ("python", []) => print!("{}", python::export(&pair.hir)?),
        ("observe", [input, a, b, c]) => {
            let initial = [a.parse()?, b.parse()?, c.parse()?];
            observe(path, &binary, function, &pair, input.parse()?, initial)?;
        }
        _ => bail!("invalid arguments for {mode}"),
    }
    Ok(())
}

fn observe(
    path: &Path,
    binary: &LoadedBinary,
    function: &FunctionInfo,
    pair: &PreHirHirPair,
    input: u32,
    initial: [u8; OBSERVED_BUFFER_LEN],
) -> Result<()> {
    let flow = call_sites(binary, function.address)?;
    ensure!(
        flow.calls.is_empty() && flow.tail_jumps.is_empty(),
        "fixture functions must be leaf functions"
    );
    let mut harness = EmulatorHarness::build(path, Some(INSTRUCTION_LIMIT))?;
    let scratch = harness.install_scratch(&initial)?;
    let args = match function.name.as_str() {
        "mix_word" => vec![u64::from(input)],
        "write_byte" | "narrowing_store" => vec![scratch, u64::from(input)],
        _ => bail!("no native fixture ABI for {}", function.name),
    };
    ensure!(
        pair.hir.params.len() == args.len(),
        "HIR parameter count differs from the fixture ABI"
    );
    let hir_args = args
        .iter()
        .zip(&pair.hir.params)
        .map(|(&arg, parameter)| normalize(arg as i64, &parameter.ty))
        .collect::<Vec<_>>();
    let image = initial
        .iter()
        .enumerate()
        .map(|(offset, &byte)| (scratch + offset as u64, byte))
        .collect::<Vec<_>>();
    let tree = run_hir(
        &pair.hir.body,
        &pair.hir.params,
        &pair.hir.locals,
        &hir_args,
        &World {
            image: &image,
            ..World::default()
        },
    )
    .context("evaluate printed HIR")?;
    ensure!(tree.calls.is_empty(), "unexpected call from HIR");
    let machine = harness.call_stubbed(function.address, &args, &flow, &|_| 0)?;
    ensure!(machine.calls.is_empty(), "unexpected emulated call");
    ensure!(
        machine.undefined_frame_read.is_none(),
        "emulator read an uninitialized frame byte"
    );
    let StubbedEnd::Returned(returned) = machine.end else {
        bail!("emulator ended without a return: {:?}", machine.end);
    };
    let hir_return = tree.ret.context("HIR ended without a return")?;
    println!(
        "{}",
        json!({
            "address": format!("{:#x}", function.address),
            "instruction_limit": INSTRUCTION_LIMIT,
            "emulator": observation(returned as u32, initial, scratch, machine.writes),
            "hir": observation(hir_return as u32, initial, scratch, tree.writes),
        })
    );
    Ok(())
}

fn observation(
    result: u32,
    mut initial: [u8; OBSERVED_BUFFER_LEN],
    scratch: u64,
    writes: std::collections::BTreeMap<u64, u8>,
) -> serde_json::Value {
    let mut outside = Vec::new();
    for (address, value) in writes {
        let byte = address
            .checked_sub(scratch)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| initial.get_mut(offset));
        if let Some(byte) = byte {
            *byte = value;
        } else {
            outside.push((format!("{address:#x}"), value));
        }
    }
    json!({"return_value": result, "buffer": initial, "outside_writes": outside})
}
