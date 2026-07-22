//! Guarded-tail promotion entry free functions.
//!
//! Deep canonicalize/alias residual remains host-side via [`GuardedTailHost`]
//! hooks on [`crate::host::StructuringHost`].

use super::types::*;
use crate::cleanup::{collect_referenced_label_counts, normalize_guarded_tail_layout};
use crate::host::StructuringHost;
use crate::regions::RegionKind;
use fission_midend_core::ir::{DirExpr, DirStmt};
use crate::HashMap;

/// Promote all single-entry guarded-tail shapes in `body` (one pass).
pub fn promote_single_entry_guarded_tail_regions(
    host: &mut impl StructuringHost,
    body: &mut Vec<DirStmt>,
) -> bool {
    let protected = host.lsda_landing_pad_labels();
    let (normalized, alias_rewrites) =
        normalize_guarded_tail_layout(std::mem::take(body), &protected);
    *body = normalized;
    let referenced = collect_referenced_label_counts(body);
    let mut changed = alias_rewrites > 0;
    let mut idx = 0usize;
    while idx < body.len() {
        let DirStmt::If { cond, .. } = &body[idx] else {
            idx += 1;
            continue;
        };
        let Some(trial) = host.try_build_guarded_tail_trial(body, idx, &referenced) else {
            idx += 1;
            continue;
        };
        let trial = match trial {
            Ok(trial) => trial,
            Err(reason) => {
                host.mark_guarded_tail_execution_rejection(
                    GuardedTailExecutionRejection::Witness(reason),
                );
                match reason {
                    GuardedTailWitnessRejection::MissingTerminalJoin => {
                        host.mark_promotion_shape_rejection(
                            PromotionShapeRejection::MissingTerminalJoinTarget,
                        );
                    }
                    GuardedTailWitnessRejection::AmbiguousFollow => {
                        host.mark_promotion_shape_rejection(
                            PromotionShapeRejection::EmptyNonterminalTail,
                        );
                    }
                    GuardedTailWitnessRejection::AliasInterleaveConflict
                    | GuardedTailWitnessRejection::NonCanonicalLayout
                    | GuardedTailWitnessRejection::SideEntryConflict => {}
                }
                idx += 1;
                continue;
            }
        };
        let legality = trial.witness.region_legality();
        host.bump_region_proof_candidate();
        if legality.is_complete_for(RegionKind::GuardedTail) {
            host.bump_region_proof_completed();
        }
        let verification = host.verify_guarded_tail_trial(body, idx, &trial);
        if let Some(reason) = verification.rejection_reason {
            host.mark_guarded_tail_execution_rejection(reason);
            idx += 1;
            continue;
        }

        host.bump_guarded_tail_candidate();
        host.bump_promotion_candidate();
        let plan = match host.build_guarded_tail_execution_plan(body, idx, &trial, &verification) {
            Ok(plan) => plan,
            Err(reason) => {
                host.mark_guarded_tail_execution_rejection(reason);
                idx += 1;
                continue;
            }
        };
        let cond = cond.clone();
        host.execute_guarded_tail_plan(body, idx, trial, plan, cond);
        changed = true;
        idx += 1;
    }
    changed
}

/// Discover guarded-tail candidates for telemetry (no mutation of structure).
pub fn discover_guarded_tail_candidates(host: &mut impl StructuringHost, body: &[DirStmt]) {
    let protected = host.lsda_landing_pad_labels();
    let (normalized, _) = normalize_guarded_tail_layout(body.to_vec(), &protected);
    host.discover_guarded_tail_candidates_in_body(&normalized);
}

/// Iterate promotion until fixed point or iteration budget.
pub fn promote_guarded_tail_regions_until_stable(
    host: &mut impl StructuringHost,
    body: &mut Vec<DirStmt>,
) {
    let mut iterations = 0;
    while promote_single_entry_guarded_tail_regions(host, body) {
        iterations += 1;
        if iterations >= 30 {
            if crate::linear_types::structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] promote_single_entry_guarded_tail_regions: budget tripped at {} iterations",
                    iterations
                );
            }
            break;
        }
    }
}
