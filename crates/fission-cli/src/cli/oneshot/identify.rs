//! FID (Function ID) signature matching (`fission_decompiler::fid`).

use anyhow::{Context, Result, bail};
use fission_decompiler::fid::{FidIdentifier, load_fid_databases};
use fission_loader::loader::LoadedBinary;
use serde_json::json;
use std::io::Write;

use crate::cli::args::OneShotArgs;

pub(super) fn run_identify(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let databases = load_fid_databases(binary);
    let Some(identifier) = FidIdentifier::new(binary, &databases) else {
        bail!(
            "FID identification is not available for this binary (no register model or SLEIGH frontend for its language)"
        );
    };

    let mut stdout = std::io::stdout().lock();

    if let Some(address) = cli.identify_function {
        let result = identifier.identify(address);
        if cli.json {
            let payload = match &result {
                Some(m) => json!({
                    "address": format!("0x{:x}", address),
                    "matched": true,
                    "name": m.name,
                    "library_family": m.library_family,
                    "score": m.score,
                    "full_hash": format!("0x{:016x}", m.full_hash),
                    "code_unit_count": m.code_unit_count,
                    "specific_matched": m.specific_matched,
                }),
                // No match says nothing about why; the hashes a match was
                // looked up by do -- they are what to search a FID database
                // for, or what a missing signature would have to carry.
                None => match identifier.hashes(address) {
                    Some((code_unit_count, full_hash, specific_hash)) => json!({
                        "address": format!("0x{:x}", address),
                        "matched": false,
                        "code_unit_count": code_unit_count,
                        "full_hash": format!("0x{:016x}", full_hash),
                        "specific_hash": format!("0x{:016x}", specific_hash),
                    }),
                    None => json!({
                        "address": format!("0x{:x}", address),
                        "matched": false,
                        "hashed": false,
                        "reason": "no FID hash: the function is too short, uses an operand shape the hasher does not support, or failed to decode",
                    }),
                },
            };
            let text = serde_json::to_string_pretty(&payload).context("serialize identify JSON")?;
            println!("{}", text);
            return Ok(());
        }
        match result {
            Some(m) => writeln!(
                stdout,
                "0x{:012x}  {}  ({})  score={:.1}  full_hash=0x{:016x}  code_units={}  specific_matched={}",
                address,
                m.name,
                m.library_family,
                m.score,
                m.full_hash,
                m.code_unit_count,
                m.specific_matched
            )
            .context("write identify match")?,
            None => match identifier.hashes(address) {
                Some((code_units, full_hash, specific_hash)) => writeln!(
                    stdout,
                    "0x{:012x}  no match  full_hash=0x{:016x}  specific_hash=0x{:016x}  code_units={}",
                    address, full_hash, specific_hash, code_units
                )
                .context("write identify no-match")?,
                None => writeln!(stdout, "0x{:012x}  no match (could not be hashed)", address)
                    .context("write identify no-match")?,
            },
        }
        return Ok(());
    }

    let mut matches = Vec::new();
    let mut attempted = 0usize;
    for func in &binary.functions {
        if func.is_import {
            continue;
        }
        attempted += 1;
        if let Some(m) = identifier.identify(func.address) {
            matches.push((func.address, func.name.clone(), m));
        }
    }

    if cli.json {
        let nodes: Vec<_> = matches
            .iter()
            .map(|(addr, fission_name, m)| {
                json!({
                    "address": format!("0x{:x}", addr),
                    "fission_name": fission_name,
                    "name": m.name,
                    "library_family": m.library_family,
                    "score": m.score,
                    "full_hash": format!("0x{:016x}", m.full_hash),
                    "code_unit_count": m.code_unit_count,
                    "specific_matched": m.specific_matched,
                })
            })
            .collect();
        let payload = json!({
            "functions_considered": attempted,
            "functions_matched": matches.len(),
            "matches": nodes,
        });
        let text = serde_json::to_string_pretty(&payload).context("serialize identify JSON")?;
        println!("{}", text);
        return Ok(());
    }

    writeln!(
        stdout,
        "identify: functions_considered={} functions_matched={}",
        attempted,
        matches.len()
    )
    .context("write identify header")?;
    for (addr, fission_name, m) in &matches {
        writeln!(
            stdout,
            "  0x{:012x}  {}  ->  {} ({})  score={:.1}  full_hash=0x{:016x}",
            addr, fission_name, m.name, m.library_family, m.score, m.full_hash
        )
        .context("write identify match")?;
    }

    Ok(())
}
