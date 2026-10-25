//! Region linearization recovery free functions.
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`. Core `lower_linear_body` still lives
//! on the host until the full linear pipeline is extracted.

use crate::HashSet;
use crate::cleanup::cleanup_redundant_labels;
use crate::helpers::block_label;
use crate::host::StructuringHost;
use crate::linear_types::{LinearBodyLoweringOutcome, LinearExit, structuring_diag_enabled};
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::PreHirStmt;

/// Soft SESE region proof budget, in `sese_region_proof_budget_exceeded()`
/// calls since the structuring attempt began. Deliberately a call count, not
/// a wall-clock duration: this budget gates how many candidate-region proof
/// attempts a single function's structuring may make before falling back,
/// and a wall-clock version made that fallback point -- and therefore the
/// decompiled output -- depend on machine speed / load (see PROJECT.md).
/// Calibrated generously above what any corpus function needs; only meant
/// to bound genuinely pathological (e.g. exponential node-splitting) cases.
pub const SESE_REGION_PROOF_BUDGET_CALLS: u64 = 20_000;

fn push_unique_region_exit(candidates: &mut Vec<LinearExit>, candidate: LinearExit) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

/// Algorithmic region-exit candidates ordered by postdom maximisation (SAILR H2).
pub fn region_linearized_exit_candidates_algorithmic(
    host: &impl StructuringHost,
    start_idx: usize,
    targeted: &HashSet<u64>,
) -> Vec<LinearExit> {
    let mut candidates = Vec::new();
    let search_limit = host.block_count();

    for idx in (start_idx + 1)..search_limit {
        if host.dom_tree().dominates(start_idx, idx) {
            continue;
        }

        let mut reachable_from_region = false;
        for &p in &host.predecessors()[idx] {
            if host.dom_tree().dominates(start_idx, p) {
                reachable_from_region = true;
                break;
            }
        }

        if reachable_from_region {
            candidates.push(LinearExit::Join(idx));
        } else {
            let block_key = host.block_target_key(idx);
            if targeted.contains(&block_key) {
                candidates.push(LinearExit::Join(idx));
            }
        }
    }

    if candidates.len() > 1 {
        let imm_postdom_opt = host
            .cfg_facts()
            .immediate_postdominators()
            .immediate_postdominator(start_idx);

        if let Some(ipdom) = imm_postdom_opt {
            let ipdom_exit = LinearExit::Join(ipdom);
            if let Some(pos) = candidates.iter().position(|c| *c == ipdom_exit) {
                if pos != 0 {
                    candidates.swap(0, pos);
                }
                return candidates;
            }
        }

        let postdom = host.cfg_facts().postdominators();
        let dominated_nodes: Vec<usize> = (start_idx + 1..search_limit)
            .filter(|&i| host.dom_tree().dominates(start_idx, i))
            .collect();

        rank_linearized_exit_candidates(&mut candidates, &dominated_nodes, postdom);
    }

    candidates
}

/// Rank candidate exits by the SAILR H2 heuristic ("post-dominator maximization"):
/// each candidate join is scored by how many dominated region nodes it post-dominates.
pub fn rank_linearized_exit_candidates(
    candidates: &mut [LinearExit],
    dominated_nodes: &[usize],
    postdom: &crate::cfg_analysis::PostDomTree,
) {
    let score = |exit: &LinearExit| -> usize {
        let LinearExit::Join(join_idx) = *exit else {
            return 0;
        };
        dominated_nodes
            .iter()
            .filter(|&&n| {
                postdom
                    .postdominators()
                    .get(&n)
                    .is_some_and(|pdoms| pdoms.contains(&join_idx))
            })
            .count()
    };

    candidates.sort_by(|a, b| score(b).cmp(&score(a)));
}

/// Recover a linearized body for a failed structured region starting at `start_idx`.
pub fn try_recover_region_linearized_body(
    host: &mut impl StructuringHost,
    start_idx: usize,
    err: &MlilPreviewError,
    targeted: &HashSet<u64>,
    emitted_labels: &mut HashSet<u64>,
) -> Result<Option<(Vec<PreHirStmt>, usize)>, MlilPreviewError> {
    if !host.options().region_linearize_structuring {
        return Ok(None);
    }
    if host.options().conservative_irreducible_fallback {
        let scc = host.analyze_cfg_scc();
        if scc.is_irreducible_node(start_idx) {
            host.bump_region_linearize_rejected_irreducible_cfg();
            return Ok(None);
        }
    }
    if err.structuring_failure_kind().is_none() {
        host.bump_region_linearize_rejected_non_structuring_failure();
        return Ok(None);
    }

    let mut exits = Vec::new();
    if let Some(exit) = host.linear_exit(start_idx)? {
        push_unique_region_exit(&mut exits, exit);
    }
    for exit in region_linearized_exit_candidates_algorithmic(host, start_idx, targeted) {
        push_unique_region_exit(&mut exits, exit);
    }
    if exits.is_empty() {
        host.bump_region_linearize_rejected_no_exit();
        return Ok(None);
    }

    let mut lowered = None;
    for exit in exits {
        match host.lower_linear_body_for_region_recovery_detailed(start_idx, exit, None)? {
            LinearBodyLoweringOutcome::Lowered(result) => {
                lowered = Some(result);
                break;
            }
            LinearBodyLoweringOutcome::Rejected(reason) => {
                host.record_region_body_lowering_reject_reason(reason);
            }
        }
    }
    let Some((body, skip_to)) = lowered else {
        host.bump_region_linearize_rejected_body_lowering_failed();
        return Ok(None);
    };
    let mut body = std::rc::Rc::unwrap_or_clone(body);
    if skip_to <= start_idx {
        host.bump_region_linearize_rejected_non_advancing();
        return Ok(None);
    }

    let block_key = host.block_target_key(start_idx);
    if (start_idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
        body.insert(0, PreHirStmt::Label(block_label(block_key)));
    }

    host.bump_region_linearize_structuring();
    Ok(Some((cleanup_redundant_labels(body, None), skip_to)))
}

/// Linear fallback for a single SESE child region without discarding parent structure.
pub fn build_linear_sese_child_fallback(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    if host.sese_region_proof_budget_exceeded() {
        if structuring_diag_enabled() {
            eprintln!(
                "[DIAG] build_linear_sese_child_fallback: skipping payload lowering after {} proof-attempt ceiling",
                SESE_REGION_PROOF_BUDGET_CALLS
            );
        }
        return Err(MlilPreviewError::UnsupportedCfgRegionShape);
    }
    let exit_spec = if exit >= host.block_count() {
        LinearExit::Return
    } else {
        LinearExit::Join(exit)
    };
    let Some((body, _skip)) = host.lower_linear_body(entry, exit_spec)? else {
        return Err(MlilPreviewError::UnsupportedCfgRegionShape);
    };
    let mut body = std::rc::Rc::unwrap_or_clone(body);
    let targeted = host.collect_jump_targets()?;
    let block_key = host.block_target_key(entry);
    let entry_label = block_label(block_key);
    if (entry == 0 || targeted.contains(&block_key))
        && !body
            .iter()
            .any(|stmt| matches!(stmt, PreHirStmt::Label(label) if label == &entry_label))
    {
        body.insert(0, PreHirStmt::Label(entry_label));
    }
    Ok(cleanup_redundant_labels(body, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_analysis::PostDomTree;

    #[test]
    fn sailr_h2_ranks_join_by_number_of_dominated_nodes_it_postdominates() {
        // Construct a CFG where candidate exits join different numbers of dominated region nodes.
        // Node 0: Function entry -> reaches 1 (region head), and has a bypass edge to exit 6.
        // Node 1: Region start -> [2, 3] (dominated by 1: nodes 2, 3, 4, 5)
        // Node 2: [4]
        // Node 3: [5]
        // Node 4: [6, 7] (diverges to exit 6 or exit 7)
        // Node 5: [6]    (all paths go to exit 6)
        // Node 6: Candidate Exit A -> [8]
        // Node 7: Candidate Exit B -> [8]
        // Node 8: Final Exit -> []
        //
        // Dominated region nodes: [2, 3, 4, 5].
        // Node 5's only successor is 6 -> Node 6 post-dominates 5.
        // Node 3's only successor is 5, which goes to 6 -> Node 6 post-dominates 3.
        // Node 2's only successor is 4, which reaches both 6 and 7 -> neither 6 nor 7 post-dominates 2 or 4.
        //
        // Therefore:
        // Node 6 post-dominates 2 dominated nodes: {3, 5}.
        // Node 7 post-dominates 0 dominated nodes.
        //
        // SAILR H2 should rank Node 6 (Exit A) strictly ahead of Node 7 (Exit B).
        let successors = vec![
            vec![1, 6], // 0
            vec![2, 3], // 1
            vec![4],    // 2
            vec![5],    // 3
            vec![6, 7], // 4
            vec![6],    // 5
            vec![8],    // 6 (Exit A)
            vec![8],    // 7 (Exit B)
            vec![],     // 8
        ];
        let mut predecessors = vec![Vec::new(); successors.len()];
        for (u, succs) in successors.iter().enumerate() {
            for &v in succs {
                predecessors[v].push(u);
            }
        }

        let postdom = PostDomTree::analyze(&successors, &predecessors);
        let dominated_nodes = vec![2, 3, 4, 5];

        // Intentionally start with candidates in reverse order: [Exit B (7), Exit A (6)].
        let mut candidates = vec![LinearExit::Join(7), LinearExit::Join(6)];
        rank_linearized_exit_candidates(&mut candidates, &dominated_nodes, &postdom);

        assert_eq!(
            candidates,
            vec![LinearExit::Join(6), LinearExit::Join(7)],
            "SAILR H2 must rank Join(6) first because it post-dominates 2 region nodes while Join(7) post-dominates 0"
        );
    }
}
