//! A function's indirect sites, joined with the evidence about them.

use crate::KernelError;
use crate::image::{ImageView, decode_context};
use fission_analysis_db::ProgramSnapshot;
use fission_analysis_protocol::{
    ExcludedEvidence, INDIRECT_TARGETS_SCHEMA, IndirectSite, IndirectSiteKind, IndirectTargetsView,
    Observation, ObservedCandidate, StaticCandidate, TargetAttribution, TargetSetCompleteness,
};
use fission_decompiler::{PcodeFunction, PcodeOpcode, lift_function_pcode};
use fission_evidence::{
    CodeLocation, Dependency, Entity, Evidence, EvidenceId, EvidenceLedger, ImageBytes, Lifecycle,
    Predicate, Scope, SourceKind, Value, VerificationStatus,
};
use std::collections::BTreeMap;

type Candidates = BTreeMap<(TargetAttribution, u64, Option<String>), Vec<Observation>>;

struct Site {
    kind: IndirectSiteKind,
    in_static_lift: bool,
    static_targets: Vec<u64>,
    observed: Candidates,
}

impl Site {
    fn evidence_only(kind: IndirectSiteKind) -> Self {
        Self {
            kind,
            in_static_lift: false,
            static_targets: Vec::new(),
            observed: BTreeMap::new(),
        }
    }
}

/// The indirect sites of the lift, with the targets the lift connected.
fn static_sites(pcode: &PcodeFunction) -> BTreeMap<u64, Site> {
    let mut sites = BTreeMap::new();
    for block in &pcode.blocks {
        for (index, op) in block.ops.iter().enumerate() {
            let kind = match op.opcode {
                PcodeOpcode::CallInd => IndirectSiteKind::Call,
                PcodeOpcode::BranchInd => IndirectSiteKind::Jump,
                _ => continue,
            };
            let site = sites
                .entry(op.address)
                .or_insert_with(|| Site::evidence_only(kind));
            site.in_static_lift = true;
            // A computed jump ending its block flows to the block's successors.
            if kind == IndirectSiteKind::Jump && index + 1 == block.ops.len() {
                site.static_targets.extend(
                    block
                        .successors
                        .iter()
                        .filter_map(|next| pcode.blocks.iter().find(|b| b.index == *next))
                        .map(|next| next.start_address),
                );
            }
        }
    }
    sites
}

fn label<T: std::fmt::Debug>(value: T) -> String {
    // The contract enums serialize in snake_case; keep the view's text equal.
    let name = format!("{value:?}");
    let mut out = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn observation(id: &EvidenceId, claim: &Evidence, verification: VerificationStatus) -> Observation {
    let (run, process, thread, epoch) = match &claim.scope {
        Scope::Image => (None, None, None, None),
        Scope::Run {
            run,
            process,
            thread,
            epoch,
        } => (
            Some(run.0.to_string()),
            Some(*process),
            Some(*thread),
            Some(*epoch),
        ),
    };
    Observation {
        evidence_id: id.to_string(),
        source: label::<SourceKind>(claim.source),
        verification: label(verification),
        producer: claim.producer.name.clone(),
        producer_version: claim.producer.version.clone(),
        run,
        process,
        thread,
        epoch,
        artifacts: claim
            .dependencies
            .iter()
            .filter_map(|dependency| match dependency {
                Dependency::Artifact(artifact) => Some(match artifact.record {
                    Some(record) => {
                        format!("{} {}#{record}", artifact.schema, artifact.digest)
                    }
                    None => format!("{} {}", artifact.schema, artifact.digest),
                }),
                Dependency::Evidence { .. } => None,
            })
            .collect(),
        assumptions: claim.assumptions.iter().cloned().collect(),
    }
}

/// Where a claim's target stands relative to the analysed image.
fn candidate(image: &ImageView<'_>, value: &Value) -> (TargetAttribution, u64, Option<String>) {
    match value {
        Value::Code(target) if &target.image == image.identity() => {
            match (image.address_of(target.offset), current(image, target)) {
                (Some(address), true) => (TargetAttribution::Image, address, None),
                (address, _) => (
                    TargetAttribution::Unattributed,
                    address.unwrap_or(target.offset),
                    Some("the image's bytes are not the ones observed".to_string()),
                ),
            }
        }
        Value::Code(target) => (
            TargetAttribution::OtherImage,
            target.offset,
            Some(format!("offset in image {}", target.image.content)),
        ),
        Value::RuntimeCode(code) => (
            TargetAttribution::RuntimeCode,
            code.at.address,
            Some(format!("code version {}", code.version.digest)),
        ),
        Value::Address { at, reason } => (
            TargetAttribution::Unattributed,
            at.address,
            Some(label(*reason)),
        ),
    }
}

/// Whether the image still holds the version of code the location names.
fn current(image: &ImageView<'_>, location: &CodeLocation) -> bool {
    usize::try_from(location.version.len)
        .ok()
        .and_then(|len| image.bytes(location.offset, len))
        .is_some_and(|bytes| location.version.matches(bytes))
}

/// The indirect calls and jumps of the function containing `address`, each with
/// the targets the static lift connected and the targets evidence reports.
///
/// Read-only: it neither decompiles nor feeds anything back, and every target
/// set it reports is open.
pub fn indirect_targets_view(
    image: &ImageView<'_>,
    address: u64,
    ledger: &EvidenceLedger,
) -> Result<IndirectTargetsView, KernelError> {
    // One malformed loader row costs that row, as in the other lookup views.
    let snapshot = ProgramSnapshot::publish_loaded_binary_quarantining_rows(image.binary())
        .publication
        .into_result()
        .map_err(|report| KernelError::Snapshot(format!("{report:?}")))?;
    let record = snapshot.function_containing(address);
    let entry = record.map_or(address, |function| function.entry);
    let end = record
        .filter(|function| function.size > 0)
        .and_then(|function| function.entry.checked_add(function.size));
    // Several rows can share an entry (an unnamed unwind record beside the
    // symbol): take the first that has a name.
    let name_at = |address| {
        snapshot
            .functions_at(address)
            .find(|function| !function.name.is_empty())
            .map(|function| function.name.clone())
    };

    let lifted = lift_function_pcode(image.binary(), entry).map_err(KernelError::Lift)?;
    let context = decode_context(lifted.context_override);
    let mut sites = static_sites(&lifted.pcode);

    let mut excluded = Vec::new();
    let mut other_image_claims = 0;
    for (id, claim) in ledger.claims() {
        let kind = match claim.predicate {
            Predicate::IndirectCallTarget => IndirectSiteKind::Call,
            Predicate::IndirectJumpTarget => IndirectSiteKind::Jump,
        };
        // The site as an offset in some image, and what that image is.
        let (site_image, site_offset, runtime_site) = match &claim.entity {
            Entity::Code(location) => (&location.image, location.offset, false),
            Entity::RuntimeCode(code) => match &code.origin {
                fission_evidence::RuntimeCodeOrigin::ModifiedImage { image, offset } => {
                    (image, *offset, true)
                }
                fission_evidence::RuntimeCodeOrigin::Anonymous => continue,
            },
            Entity::Function(_) => continue,
        };
        if site_image.content != image.identity().content {
            other_image_claims += 1;
            continue;
        }
        let Some(site_address) = image.address_of(site_offset) else {
            continue;
        };
        let in_function = sites.contains_key(&site_address)
            || end.is_some_and(|end| site_address >= entry && site_address < end);
        if !in_function {
            continue;
        }
        let reason = if let Lifecycle::Withdrawn { reason } = ledger.lifecycle(id) {
            Some(format!("withdrawn: {reason}"))
        } else if ledger.verification(id) == VerificationStatus::Refuted {
            Some("refuted".to_string())
        } else if site_image != image.identity() {
            Some("decoded under another language or compiler spec".to_string())
        } else {
            match &claim.entity {
                Entity::Code(location) if location.decode != context => {
                    Some("decoded in another decoder context".to_string())
                }
                Entity::Code(location) if !current(image, location) => {
                    Some("the image's code at this site has another version".to_string())
                }
                _ if runtime_site => {
                    Some("the code at this site was modified at run time".to_string())
                }
                _ => None,
            }
        };
        if let Some(reason) = reason {
            excluded.push(ExcludedEvidence {
                evidence_id: id.to_string(),
                site_address,
                reason,
            });
            continue;
        }
        sites
            .entry(site_address)
            .or_insert_with(|| Site::evidence_only(kind))
            .observed
            .entry(candidate(image, &claim.value))
            .or_default()
            .push(observation(id, claim, ledger.verification(id)));
    }
    excluded
        .sort_by(|a, b| (a.site_address, &a.evidence_id).cmp(&(b.site_address, &b.evidence_id)));

    Ok(IndirectTargetsView {
        schema: INDIRECT_TARGETS_SCHEMA.to_string(),
        image: image.identity().content.to_string(),
        function_address: entry,
        function_name: name_at(entry),
        sites: sites
            .into_iter()
            .map(|(address, mut site)| {
                site.static_targets.sort_unstable();
                site.static_targets.dedup();
                IndirectSite {
                    address,
                    kind: site.kind,
                    in_static_lift: site.in_static_lift,
                    target_set: TargetSetCompleteness::Open,
                    static_candidates: site
                        .static_targets
                        .into_iter()
                        .map(|target| StaticCandidate {
                            address: target,
                            name: name_at(target),
                        })
                        .collect(),
                    observed_candidates: site
                        .observed
                        .into_iter()
                        .map(
                            |((attribution, address, detail), observations)| ObservedCandidate {
                                attribution,
                                address,
                                name: (attribution == TargetAttribution::Image)
                                    .then(|| name_at(address))
                                    .flatten(),
                                detail,
                                observations,
                            },
                        )
                        .collect(),
                }
            })
            .collect(),
        excluded,
        other_image_claims,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_decompiler::{PcodeBasicBlock, PcodeOp, Varnode};

    fn op(opcode: PcodeOpcode, address: u64) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address,
            output: None,
            inputs: vec![Varnode {
                space_id: 2,
                offset: 0,
                size: 8,
                is_constant: false,
                constant_val: 0,
            }],
            asm_mnemonic: None,
        }
    }

    fn block(index: u32, start: u64, successors: &[u32], ops: Vec<PcodeOp>) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index,
            start_address: start,
            successors: successors.to_vec(),
            ops,
        }
    }

    /// What the lift connected is a static candidate. A call's fall-through is
    /// not one of its targets.
    #[test]
    fn static_candidates_are_the_successors_of_a_computed_jump() {
        let pcode = PcodeFunction {
            blocks: vec![
                block(0, 0x100, &[1], vec![op(PcodeOpcode::CallInd, 0x104)]),
                block(1, 0x108, &[2, 3], vec![op(PcodeOpcode::BranchInd, 0x10c)]),
                block(2, 0x110, &[], vec![op(PcodeOpcode::Return, 0x110)]),
                block(3, 0x118, &[], vec![op(PcodeOpcode::Return, 0x118)]),
            ],
        };
        let sites = static_sites(&pcode);
        assert_eq!(sites.keys().copied().collect::<Vec<_>>(), [0x104, 0x10c]);
        assert!(sites[&0x104].static_targets.is_empty());
        assert_eq!(sites[&0x10c].static_targets, [0x110, 0x118]);
        assert!(sites.values().all(|site| site.in_static_lift));
    }
}
