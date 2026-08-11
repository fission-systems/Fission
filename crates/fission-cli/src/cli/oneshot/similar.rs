//! Fuzzy function similarity (`fission_decompiler::similarity`).

use anyhow::{Context, Result, bail};
use fission_decompiler::similarity::{SimilarityCorpus, extract_function_features};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;
use serde_json::json;
use std::io::Write;

use crate::cli::args::OneShotArgs;

const MAX_BYTES: usize = 1 << 16;
const INSTRUCTION_LIMIT: usize = 4000;

pub(super) fn run_similar(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let load_spec = binary
        .load_spec()
        .context("similarity requires a resolved SLEIGH load spec for this binary")?;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec)
        .context("failed to build a SLEIGH frontend for this binary")?;

    let mut corpus = SimilarityCorpus::new();
    let mut keys: Vec<(String, u64)> = Vec::new();
    for func in &binary.functions {
        if func.is_import {
            continue;
        }
        let Some((decode_addr, lifted)) = lift_for_similarity(binary, &frontend, func.address)
        else {
            continue;
        };
        let features = extract_function_features(&lifted.function);
        let key = format!("{}@{:#x}", func.name, decode_addr);
        corpus.add(key.clone(), features);
        keys.push((key, decode_addr));
    }

    if corpus.is_empty() {
        bail!(
            "no functions could be lifted for similarity comparison (try --function-discovery-profile balanced)"
        );
    }

    let mut stdout = std::io::stdout().lock();

    if let Some(address) = cli.similar_function {
        let Some((key, _)) = keys.iter().find(|(_, addr)| *addr == address) else {
            bail!("0x{address:x} is not a known (non-import) function in this binary");
        };
        let matches = corpus.most_similar_to(key, cli.similar_top_k);
        if cli.json {
            print_json(&mut stdout, &[(key.clone(), matches)])?;
        } else {
            writeln!(stdout, "{key}")?;
            print_matches_text(&mut stdout, &matches)?;
        }
        return Ok(());
    }

    let mut all_results: Vec<(String, Vec<(String, f64)>)> = Vec::with_capacity(keys.len());
    for (key, _) in &keys {
        all_results.push((key.clone(), corpus.most_similar_to(key, cli.similar_top_k)));
    }

    if cli.json {
        print_json(&mut stdout, &all_results)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "similar: functions_indexed={} function_discovery_profile_applied={}",
        corpus.len(),
        cli.function_discovery_profile.is_some()
    )?;
    for (key, matches) in &all_results {
        if matches.is_empty() {
            continue;
        }
        writeln!(stdout, "{key}")?;
        print_matches_text(&mut stdout, matches)?;
    }
    Ok(())
}

fn lift_for_similarity(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
) -> Option<(u64, fission_sleigh::runtime::DecodedPcodeFunction)> {
    let address_state = frontend.normalize_low_bit_code_address(addr);
    let decode_addr = address_state.address;
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| MAX_BYTES.min(available).max(1))
        .unwrap_or(MAX_BYTES);
    let bytes = binary.view_bytes(decode_addr, max_bytes)?;
    let memory_context = decode_memory_context_for(binary, decode_addr, bytes.len());
    let contract = DecodeContract::decomp_function(INSTRUCTION_LIMIT);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            contract,
            &memory_context,
            address_state.context_override,
        )
        .ok()?;
    Some((decode_addr, lifted))
}

fn print_matches_text(stdout: &mut impl Write, matches: &[(String, f64)]) -> Result<()> {
    for (name, score) in matches {
        writeln!(stdout, "  {score:.4}  {name}").context("write similar match")?;
    }
    Ok(())
}

fn print_json(stdout: &mut impl Write, results: &[(String, Vec<(String, f64)>)]) -> Result<()> {
    let nodes: Vec<_> = results
        .iter()
        .map(|(name, matches)| {
            json!({
                "function": name,
                "matches": matches.iter().map(|(m, score)| json!({
                    "name": m,
                    "score": score,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let payload = json!({ "results": nodes });
    let text = serde_json::to_string_pretty(&payload).context("serialize similar JSON")?;
    writeln!(stdout, "{text}").context("write similar JSON")?;
    Ok(())
}
