//! `fission_cli evidence` and the sandbox's runtime trace output.
//!
//! Parsing and printing only: capture, import and the view belong to
//! `fission-analysis-kernel`.

use crate::cli::args::{EvidenceArgs, EvidenceCommand};
use anyhow::{Context, Result};
use fission_analysis_kernel::{ImageView, import_trace, indirect_targets_view, record, store};
use fission_analysis_protocol::{
    EvidenceImport, EvidenceImportedTrace, IndirectSiteKind, IndirectTargetsView, TargetAttribution,
};
use fission_emulator::core::Emulator;
use fission_evidence::{ContentDigest, RuntimeTrace};
use fission_loader::loader::LoadedBinary;
use std::fmt::Write as _;
use std::path::Path;

pub(super) fn write_indirect_trace(
    emulator: &Emulator,
    path: &Path,
    inputs: ContentDigest,
    error: Option<&str>,
) -> Result<()> {
    let recorder = emulator
        .observers()
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<record::IndirectRecorder>())
        .context("the indirect recorder is no longer registered")?;
    let image = ImageView::new(&emulator.binary)?;
    let trace = recorder.trace(&image, inputs, record::completeness(emulator, error));
    store::write_atomically(path, trace.to_json()?.as_bytes())?;
    tracing::info!(
        "Wrote {} indirect transfers to {}",
        trace.transfers.len(),
        path.display()
    );
    Ok(())
}

fn load_binary(path: &Path) -> Result<LoadedBinary> {
    LoadedBinary::from_file(path)
        .with_context(|| format!("failed to read binary at {}", path.display()))
}

pub(super) fn run(args: &EvidenceArgs) -> Result<()> {
    match &args.command {
        EvidenceCommand::Import {
            binary,
            traces,
            store: store_path,
        } => {
            let binary = load_binary(binary)?;
            let image = ImageView::new(&binary)?;
            let mut ledger = store::load_or_empty(store_path)?;
            let mut imported = Vec::with_capacity(traces.len());
            for path in traces {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read trace at {}", path.display()))?;
                let trace = RuntimeTrace::from_json(&text)
                    .with_context(|| format!("failed to read trace at {}", path.display()))?;
                let report = import_trace(&image, &trace, &mut ledger)
                    .with_context(|| format!("cannot import {}", path.display()))?;
                imported.push(EvidenceImportedTrace {
                    trace: path.display().to_string(),
                    run: report.run.0.to_string(),
                    added: report.added,
                    duplicate: report.duplicate,
                    unattributed_sites: report.unattributed_sites,
                    complete: report.complete,
                });
            }
            // Only now: one refused trace leaves the file as it was.
            store::save(store_path, &ledger)?;
            let response = EvidenceImport {
                claims: ledger.len(),
                traces: imported,
            };
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                for trace in &response.traces {
                    println!(
                        "{}: {} added, {} already held, {} unattributed sites{}",
                        trace.trace,
                        trace.added,
                        trace.duplicate,
                        trace.unattributed_sites,
                        if trace.complete {
                            ""
                        } else {
                            " (capture incomplete)"
                        }
                    );
                }
                println!("{}: {} claims", store_path.display(), response.claims);
            }
        }
        EvidenceCommand::Explain {
            binary,
            addr,
            store: store_path,
        } => {
            let binary = load_binary(binary)?;
            let image = ImageView::new(&binary)?;
            let ledger = store::load(store_path)?;
            let view = indirect_targets_view(&image, *addr, &ledger)?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&view)?);
            } else {
                print!("{}", render(&view));
            }
        }
    }
    Ok(())
}

fn render(view: &IndirectTargetsView) -> String {
    let mut out = String::new();
    let named = |name: &Option<String>| name.as_deref().map_or(String::new(), |n| format!(" {n}"));
    let _ = writeln!(
        out,
        "indirect targets of{} at 0x{:x}",
        named(&view.function_name),
        view.function_address
    );
    let _ = writeln!(
        out,
        "target sets are open: a target not listed here remains possible"
    );
    if view.sites.is_empty() {
        let _ = writeln!(out, "no indirect call or jump");
    }
    for site in &view.sites {
        let kind = match site.kind {
            IndirectSiteKind::Call => "call",
            IndirectSiteKind::Jump => "jump",
        };
        let _ = writeln!(
            out,
            "\n0x{:x} indirect {kind}{}",
            site.address,
            if site.in_static_lift {
                ""
            } else {
                " (not in the static lift)"
            }
        );
        for candidate in &site.static_candidates {
            let _ = writeln!(
                out,
                "  static   0x{:x}{}",
                candidate.address,
                named(&candidate.name)
            );
        }
        if site.observed_candidates.is_empty() {
            let _ = writeln!(out, "  no observed target");
        }
        for candidate in &site.observed_candidates {
            let attribution = match candidate.attribution {
                TargetAttribution::Image => "",
                TargetAttribution::RuntimeCode => " [run-time code]",
                TargetAttribution::OtherImage => " [another image]",
                TargetAttribution::Unattributed => " [unattributed]",
            };
            let _ = writeln!(
                out,
                "  observed 0x{:x}{}{attribution}",
                candidate.address,
                named(&candidate.name)
            );
            if let Some(detail) = &candidate.detail {
                let _ = writeln!(out, "    {detail}");
            }
            for observation in &candidate.observations {
                let _ = writeln!(
                    out,
                    "    {} by {} {}, {}, run {}",
                    observation.source,
                    observation.producer,
                    observation.producer_version,
                    observation.verification,
                    observation.run.as_deref().unwrap_or("-")
                );
                let _ = writeln!(out, "      evidence {}", observation.evidence_id);
                for assumption in &observation.assumptions {
                    let _ = writeln!(out, "      assumes {assumption}");
                }
            }
        }
    }
    for excluded in &view.excluded {
        let _ = writeln!(
            out,
            "\nnot applied at 0x{:x}: {}\n  evidence {}",
            excluded.site_address, excluded.reason, excluded.evidence_id
        );
    }
    if view.other_image_claims > 0 {
        let _ = writeln!(
            out,
            "\n{} claims in the file are about other images",
            view.other_image_claims
        );
    }
    out
}
