use std::collections::{BTreeMap, BTreeSet};
use std::env;

use anyhow::{anyhow, Result};
use fission_sleigh::compiler::{
    build_ghidra_language_manifest, compile_frontend_for_entry_spec, discover_all_entry_specs,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct RuntimeReadyAudit {
    entries: Vec<EntryRuntimeReadyAudit>,
}

#[derive(Debug, Serialize)]
struct EntryRuntimeReadyAudit {
    arch: String,
    entry_id: String,
    entry_spec: String,
    constructor_template_count: usize,
    sla_identity_count: usize,
    no_sla_identity_count: usize,
    runtime_ready_count: usize,
    unsupported_template_count: usize,
    unsupported_by_reason: BTreeMap<String, usize>,
    unsupported_sla_identity_count: usize,
    unsupported_sla_by_reason: BTreeMap<String, usize>,
    unsupported_no_sla_identity_count: usize,
    unsupported_no_sla_identity_by_reason: BTreeMap<String, usize>,
}

fn main() -> Result<()> {
    let mut entry_filter = Some("x86-64".to_string());
    let mut all = false;
    let mut executable_candidates = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--entry" => {
                entry_filter = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("--entry requires an entry id"))?,
                );
            }
            "--all" => {
                all = true;
                entry_filter = None;
            }
            "--executable-candidates" => {
                executable_candidates = true;
                entry_filter = None;
            }
            "--help" | "-h" => {
                println!(
                    "usage: sla_runtime_ready_audit [--entry <entry-id>] [--all] [--executable-candidates]"
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument {other}")),
        }
    }

    let executable_candidate_ids = if executable_candidates {
        Some(
            build_ghidra_language_manifest()?
                .entries
                .into_iter()
                .filter(|entry| entry.runtime_status == "executable_candidate")
                .map(|entry| entry.entry_id)
                .collect::<BTreeSet<_>>(),
        )
    } else {
        None
    };

    let mut audits = Vec::new();
    for entry in discover_all_entry_specs()? {
        if let Some(filter) = entry_filter.as_deref() {
            if filter != entry.entry_id {
                continue;
            }
        } else if !all && executable_candidate_ids.is_none() {
            continue;
        }
        if let Some(ids) = &executable_candidate_ids {
            if !ids.contains(&entry.entry_id) {
                continue;
            }
        }
        let compiled = compile_frontend_for_entry_spec(&entry.path)?;
        let mut constructor_template_count = 0usize;
        let mut sla_identity_count = 0usize;
        let mut runtime_ready_count = 0usize;
        let mut unsupported_by_reason = BTreeMap::new();
        let mut unsupported_sla_by_reason = BTreeMap::new();
        let mut unsupported_no_sla_identity_by_reason = BTreeMap::new();
        for constructor in compiled
            .subtables
            .values()
            .flat_map(|subtable| subtable.constructors.iter())
        {
            constructor_template_count += 1;
            if constructor.sla_identity.is_some() {
                sla_identity_count += 1;
            }
            if constructor.runtime_ready {
                runtime_ready_count += 1;
            } else {
                let reason = constructor
                    .unsupported_template_kind
                    .as_deref()
                    .unwrap_or("unknown_unsupported_template")
                    .to_string();
                *unsupported_by_reason.entry(reason).or_insert(0) += 1;
                let target = if constructor.sla_identity.is_some() {
                    &mut unsupported_sla_by_reason
                } else {
                    &mut unsupported_no_sla_identity_by_reason
                };
                *target
                    .entry(
                        constructor
                            .unsupported_template_kind
                            .as_deref()
                            .unwrap_or("unknown_unsupported_template")
                            .to_string(),
                    )
                    .or_insert(0) += 1;
            }
        }
        let no_sla_identity_count = constructor_template_count - sla_identity_count;
        let unsupported_sla_identity_count = unsupported_sla_by_reason.values().sum();
        let unsupported_no_sla_identity_count =
            unsupported_no_sla_identity_by_reason.values().sum();
        audits.push(EntryRuntimeReadyAudit {
            arch: entry.arch,
            entry_id: entry.entry_id,
            entry_spec: entry.entry_spec,
            constructor_template_count,
            sla_identity_count,
            no_sla_identity_count,
            runtime_ready_count,
            unsupported_template_count: constructor_template_count - runtime_ready_count,
            unsupported_by_reason,
            unsupported_sla_identity_count,
            unsupported_sla_by_reason,
            unsupported_no_sla_identity_count,
            unsupported_no_sla_identity_by_reason,
        });
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&RuntimeReadyAudit { entries: audits })?
    );
    Ok(())
}
