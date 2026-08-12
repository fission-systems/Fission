//! SESE / multiblock driver free functions.
//!
//! Owns collapse-rule dispatch, the tier-1/2 collapse loop, final reconstruction,
//! and candidate consideration helpers. Residual host hooks cover CFG/lowering.

use crate::cfg_analysis::compute_follow_blocks;
use crate::collapse_loop::{collapse_loop_admission_enabled, try_virtualize_one_bad_edge};
use crate::conditionals::{
    plan_virtual_exit_if_else, try_lower_if, try_lower_if_else, try_lower_short_circuit_if,
    try_reduce_if_else_with_follow,
};
use crate::driver_pure::{region_kind_for_stmt, region_selector_or_condition};
use crate::graph::{StructureNode, capture_structuring_failure};
use crate::guarded_tail::promote_guarded_tail_regions_until_stable;
use crate::host::StructuringHost;
use crate::linear_recovery::{SESE_REGION_PROOF_BUDGET_CALLS, try_recover_region_linearized_body};
use crate::linear_types::structuring_diag_enabled;
use crate::loops::{
    try_lower_dowhile, try_lower_for, try_lower_infloop, try_lower_infloop_with_break,
    try_lower_multiblock_dowhile, try_lower_multiblock_infloop, try_lower_while,
};
use crate::regions::{RegionKind, RegionProof};
use crate::switch::try_lower_switch;
use fission_midend_core::ir::{MlilPreviewError};
use fission_midend_prehir::{PreHirStmt};
use crate::HashMap;
use crate::HashSet;

/// Collapse rule tags (Ghidra ActionStructureTransform analog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseRule {
    Switch,
    ForLoop,
    DoWhile,
    WhileDo,
    InfLoopBreak,
    InfLoop,
    Conditional,
    Sequence,
    Unstructured,
}

impl CollapseRule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Switch => "switch",
            Self::ForLoop => "for",
            Self::DoWhile => "do-while",
            Self::WhileDo => "while",
            Self::InfLoopBreak => "infloop-break",
            Self::InfLoop => "infloop",
            Self::Conditional => "conditional",
            Self::Sequence => "sequence",
            Self::Unstructured => "unstructured",
        }
    }
}

/// Active collapse rule order (matches pcode ACTIVE_COLLAPSE_RULES).
pub const ACTIVE_COLLAPSE_RULES: [CollapseRule; 9] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
    CollapseRule::Sequence,
    CollapseRule::Unstructured,
];

/// Ideal-rule subset for SESE tier-1 collapse.
pub const IDEAL_COLLAPSE_RULES: [CollapseRule; 7] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
];

/// Apply one collapse rule at `idx` via free-function `try_lower_*` owners.
pub fn apply_collapse_rule(
    host: &mut impl StructuringHost,
    rule: CollapseRule,
    idx: usize,
    follow: Option<usize>,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    match rule {
        CollapseRule::Switch => try_lower_switch(host, idx),
        CollapseRule::ForLoop => try_lower_for(host, idx),
        CollapseRule::DoWhile => {
            let mut dw = try_lower_dowhile(host, idx)?;
            if dw.is_none() {
                dw = try_lower_multiblock_dowhile(host, idx)?;
            }
            Ok(dw)
        }
        CollapseRule::WhileDo => try_lower_while(host, idx),
        CollapseRule::InfLoopBreak => try_lower_infloop_with_break(host, idx),
        CollapseRule::InfLoop => {
            let mut inf = try_lower_infloop(host, idx);
            if inf.is_err() || matches!(inf, Ok(None)) {
                inf = try_lower_multiblock_infloop(host, idx);
            }
            inf
        }
        CollapseRule::Conditional => {
            let mut cond = try_lower_short_circuit_if(host, idx);
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_reduce_if_else_with_follow(host, idx, follow);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if_else(host, idx);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if(host, idx);
            }
            cond
        }
        CollapseRule::Sequence | CollapseRule::Unstructured => Ok(None),
    }
}

/// Collapse candidate produced by tier-1 rule matching.
#[derive(Debug, Clone)]
pub struct CollapseCandidate {
    pub rule: CollapseRule,
    pub node: StructureNode,
}

/// Build a structured-region proof from a recovered statement shape.
pub fn build_region_proof(start_idx: usize, skip_to: usize, stmt: &PreHirStmt) -> Option<RegionProof> {
    let kind = region_kind_for_stmt(stmt)?;
    Some(RegionProof::structured(
        kind,
        start_idx,
        skip_to,
        region_selector_or_condition(stmt),
    ))
}

/// Consider one collapse-rule result and maybe push a [`CollapseCandidate`].
pub fn consider_structured_candidate(
    host: &mut impl StructuringHost,
    rule: CollapseRule,
    start_idx: usize,
    targeted: &HashSet<u64>,
    last_structuring_failure: &mut Option<MlilPreviewError>,
    candidates: &mut Vec<CollapseCandidate>,
    result: Result<Option<(PreHirStmt, usize)>, MlilPreviewError>,
) -> Result<(), MlilPreviewError> {
    // Drain unconditionally: a rejected/failed attempt's pushes must not leak
    // into a different rule's attempt at the same `start_idx`.
    let extra_members = host.take_extra_absorbed_members();
    if let Some((stmt, skip_to)) = capture_structuring_failure(result, last_structuring_failure)? {
        let accepted = if matches!(rule, CollapseRule::Switch) {
            let region: HashSet<usize> = (start_idx..skip_to).collect();
            !host.region_has_external_entry(&region, start_idx)
        } else {
            host.accept_structured_region(start_idx, skip_to, targeted)
        };
        if accepted {
            let Some(mut proof) = build_region_proof(start_idx, skip_to, &stmt) else {
                return Ok(());
            };
            if !extra_members.is_empty() {
                proof.members.extend(extra_members);
                proof.members.sort_unstable();
                proof.members.dedup();
            }
            host.record_region_candidate(&proof);
            candidates.push(CollapseCandidate {
                rule,
                node: StructureNode::region(usize::MAX, stmt, skip_to, proof),
            });
        }
    }
    Ok(())
}

/// Select among tier-1 candidates (stable first-match order).
pub fn select_structured_candidate(
    candidates: Vec<CollapseCandidate>,
) -> Option<CollapseCandidate> {
    candidates.into_iter().next()
}

/// Tier-1 / tier-2 collapse loop + virtualize, then final reconstruction.
///
/// Collapse rules operate on the raw CFG and don't know about the caller's
/// `[entry, exit)` boundary, so a rule can legitimately consume blocks past
/// `exit` (e.g. a do-while whose latch/exit lies beyond a SESE sub-region
/// boundary chosen by `find_sese_regions`). Returns, alongside the body:
/// - the *achieved* exit -- the true exclusive upper bound of what was
///   consumed starting at `entry`, which can exceed the requested `exit`.
/// - any block indices absorbed outside even that achieved range (e.g. a
///   do-while's embedded early-return guard target, absorbed via
///   `record_extra_absorbed_member`).
///
/// Callers that compose this region's result into an *enclosing* region's
/// `child_map` (see `sese_discovery.rs`'s `sese_structure_region`) must use
/// the achieved exit -- not the SESE-tree's a-priori `child.exit` -- as the
/// child's key range, and fold the extra members into that entry's own
/// `RegionProof.members`. Otherwise blocks already consumed by this call
/// silently reappear as independently-scanned residuals once this region is
/// no longer the top-level call.
pub fn build_sese_region_body(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    child_map: HashMap<usize, (Vec<PreHirStmt>, usize, RegionProof)>,
) -> Result<(Vec<PreHirStmt>, usize, Vec<usize>), MlilPreviewError> {
    build_sese_region_body_impl(host, entry, exit, child_map, None)
}

/// Build one admitted arm while restricting ownership and final surface to an
/// explicit CFG membership set. This runs only inside an isolated host fork.
pub fn build_sese_region_body_for_members(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    child_map: HashMap<usize, (Vec<PreHirStmt>, usize, RegionProof)>,
    members: &HashSet<usize>,
) -> Result<(Vec<PreHirStmt>, usize, Vec<usize>), MlilPreviewError> {
    if !members.contains(&entry) {
        return Err(MlilPreviewError::UnsupportedCfgRegionShape);
    }
    build_sese_region_body_impl(host, entry, exit, child_map, Some(members))
}

fn build_sese_region_body_impl(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    child_map: HashMap<usize, (Vec<PreHirStmt>, usize, RegionProof)>,
    allowed_members: Option<&HashSet<usize>>,
) -> Result<(Vec<PreHirStmt>, usize, Vec<usize>), MlilPreviewError> {
    let diag = structuring_diag_enabled();
    if host.sese_region_proof_budget_exceeded() {
        if diag {
            eprintln!(
                "[DIAG] build_sese_region_body: aborting structuring entry due to {} proof-attempt ceiling",
                SESE_REGION_PROOF_BUDGET_CALLS
            );
        }
        return Err(MlilPreviewError::UnsupportedCfgRegionShape);
    }

    let targeted = host.collect_jump_targets()?;
    let mut emitted_labels = HashSet::default();
    let follow_blocks = compute_follow_blocks(
        host.successors(),
        host.predecessors(),
        host.cfg_facts(),
        host.block_count(),
    );

    let mut active_child_map = child_map;
    active_child_map.retain(|&k, &mut (_, child_exit, _)| child_exit > k);
    let mut progress = true;
    let mut tier1_failures: HashMap<usize, MlilPreviewError> = HashMap::default();
    let mut collapse_iterations = 0;

    while progress {
        if host.sese_region_proof_budget_exceeded() {
            if diag {
                eprintln!(
                    "[DIAG] build_sese_region_body: aborting collapse loop due to {} proof-attempt ceiling",
                    SESE_REGION_PROOF_BUDGET_CALLS
                );
            }
            return Err(MlilPreviewError::UnsupportedCfgRegionShape);
        }
        progress = false;
        collapse_iterations += 1;
        if collapse_iterations > 100 {
            if diag {
                eprintln!(
                    "[DIAG] build_sese_region_body collapsing loop: tripped budget at {} iterations",
                    collapse_iterations
                );
            }
            break;
        }

        // Block indices already owned by some accepted region's (possibly
        // non-contiguous) `proof.members`, beyond that region's own
        // `[start, child_exit)` key range -- e.g. a do-while's absorbed
        // early-return guard target. Recomputed each outer iteration since
        // `active_child_map` can change.
        let owned_elsewhere = crate::graph::BlockOwnership::from_members(
            active_child_map
                .values()
                .flat_map(|(_, _, proof)| proof.members.iter().copied()),
        );

        // Tier 1: ideal structured rules
        let mut idx = entry;
        while idx < exit {
            if allowed_members.is_some_and(|members| !members.contains(&idx)) {
                idx += 1;
                continue;
            }
            if let Some((_, child_exit, _)) = active_child_map.get(&idx) {
                idx = *child_exit;
                continue;
            }
            if owned_elsewhere.contains(idx) {
                idx += 1;
                continue;
            }

            let block_key = host.block_target_key(idx);
            let has_same_start_peer = host.has_same_start_address_peer(idx);
            let is_orphan_unreachable = idx != 0
                && host.predecessors().get(idx).is_some_and(|p| p.is_empty())
                && !targeted.contains(&block_key)
                && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }

            let mut ideal_candidates = Vec::new();
            let follow = follow_blocks.get(idx).copied().flatten();
            let mut last_structuring_failure = None;
            // Prove ownership before any rule at this block lowers statements.
            // This deferred candidate is stable immediately before plain-if.
            let virtual_exit_plan = allowed_members
                .is_none()
                .then(|| {
                    host.fallthrough_index(idx).and_then(|fallthrough_idx| {
                        plan_virtual_exit_if_else(
                            host.successors(),
                            host.predecessors(),
                            host.cfg_facts().immediate_postdominators(),
                            idx,
                            fallthrough_idx,
                            host.block_count(),
                        )
                    })
                })
                .flatten();

            for rule in ACTIVE_COLLAPSE_RULES {
                if matches!(rule, CollapseRule::Sequence | CollapseRule::Unstructured) {
                    continue;
                }
                let rule_started = diag.then(std::time::Instant::now);
                if diag {
                    eprintln!(
                        "[DIAG] structuring rule start: rule={} block={idx}",
                        rule.name()
                    );
                }
                let res = if matches!(rule, CollapseRule::Conditional)
                    && virtual_exit_plan.is_some()
                {
                    let mut cond = try_lower_short_circuit_if(host, idx);
                    if cond.is_err() || matches!(cond, Ok(None)) {
                        cond = try_reduce_if_else_with_follow(host, idx, follow);
                    }
                    if cond.is_err() || matches!(cond, Ok(None)) {
                        cond = try_lower_if_else(host, idx);
                    }
                    // The deferred complex candidate precedes plain-if/goto.
                    cond
                } else {
                    apply_collapse_rule(host, rule, idx, follow)
                };
                if let Some(started) = rule_started {
                    eprintln!(
                        "[DIAG] structuring rule finish: rule={} block={idx} elapsed_ms={:.3} outcome={}",
                        rule.name(),
                        started.elapsed().as_secs_f64() * 1000.0,
                        match &res {
                            Ok(Some(_)) => "candidate",
                            Ok(None) => "none",
                            Err(_) => "error",
                        }
                    );
                }

                consider_structured_candidate(
                    host,
                    rule,
                    idx,
                    &targeted,
                    &mut last_structuring_failure,
                    &mut ideal_candidates,
                    res,
                )?;
            }
            if let Some(ref err) = last_structuring_failure {
                tier1_failures.insert(idx, err.clone());
            }

            if let Some(members) = allowed_members {
                ideal_candidates.retain(|candidate| {
                    candidate.node.proof.as_ref().is_some_and(|proof| {
                        proof.members.iter().all(|member| members.contains(member))
                    })
                });
            }

            if let Some(best) = select_structured_candidate(ideal_candidates) {
                let skip_to = best.node.skip_to;
                if skip_to <= idx {
                    if diag {
                        eprintln!(
                            "[DIAG] select_structured_candidate returned non-advancing skip_to: {} <= {}",
                            skip_to, idx
                        );
                    }
                    idx += 1;
                    continue;
                }
                let proof = best.node.proof.clone().expect("structured region proof");
                host.record_selected_region(&best.node);
                // Exclusive emission (CollapseStructure-style): outer region
                // absorbs [idx, skip_to). Drop nested map entries that would
                // re-surface the same blocks in final reconstruction.
                active_child_map.retain(|&k, _| k < idx || k >= skip_to);
                active_child_map.insert(idx, (best.node.statements, skip_to, proof));
                progress = true;
                break;
            }

            idx += 1;
        }

        if progress {
            continue;
        }

        // Ghidra's IfNoExit-equivalent rule runs only after every ordinary
        // collapse rule has reached a graph-wide fixed point. The plan still
        // reserves its stable Conditional slot before plain-if above, but the
        // expensive recursive execution is deferred until this full scan has
        // found no ordinary candidate anywhere in the region.
        if allowed_members.is_none() {
            let mut idx = entry;
            while idx < exit {
                if let Some((_, child_exit, _)) = active_child_map.get(&idx) {
                    idx = *child_exit;
                    continue;
                }
                if owned_elsewhere.contains(idx) {
                    idx += 1;
                    continue;
                }
                let Some(fallthrough_idx) = host.fallthrough_index(idx) else {
                    idx += 1;
                    continue;
                };
                let Some(plan) = plan_virtual_exit_if_else(
                    host.successors(),
                    host.predecessors(),
                    host.cfg_facts().immediate_postdominators(),
                    idx,
                    fallthrough_idx,
                    host.block_count(),
                ) else {
                    idx += 1;
                    continue;
                };

                fn children_for_members(
                    child_map: &HashMap<usize, (Vec<PreHirStmt>, usize, RegionProof)>,
                    members: &[usize],
                ) -> crate::host::StructuredChildMap {
                    let members: HashSet<usize> = members.iter().copied().collect();
                    child_map
                        .iter()
                        .filter(|(_, (_, _, proof))| {
                            proof.members.iter().all(|member| members.contains(member))
                        })
                        .map(|(&start, child)| (start, child.clone()))
                        .collect()
                }

                let first_children =
                    children_for_members(&active_child_map, &plan.first_arm.members);
                let second_children =
                    children_for_members(&active_child_map, &plan.second_arm.members);
                if diag {
                    eprintln!(
                        "[DIAG] virtual-exit conditional fixed-point admission: block={} first_members={} first_children={} second_members={} second_children={} shared_tail={}",
                        idx,
                        plan.first_arm.members.len(),
                        first_children.len(),
                        plan.second_arm.members.len(),
                        second_children.len(),
                        plan.shared_tail.len(),
                    );
                }
                if let Some((stmt, skip_to)) = host.lower_virtual_exit_if_else_isolated(
                    idx,
                    plan.clone(),
                    first_children,
                    second_children,
                )? {
                    let Some(mut proof) = build_region_proof(idx, skip_to, &stmt) else {
                        idx += 1;
                        continue;
                    };
                    proof.members.extend(plan.first_arm.members.iter().copied());
                    proof.members.extend(plan.second_arm.members.iter().copied());
                    proof.members.push(idx);
                    proof.members.sort_unstable();
                    proof.members.dedup();
                    let node = StructureNode::region(usize::MAX, stmt, skip_to, proof.clone());
                    host.record_region_candidate(&proof);
                    host.record_selected_region(&node);
                    active_child_map.retain(|&k, _| !proof.members.contains(&k));
                    active_child_map.insert(idx, (node.statements, skip_to, proof));
                    progress = true;
                    break;
                }
                idx += 1;
            }
            if progress {
                continue;
            }
        }

        // Tier 2: deferred linearization fallback
        let mut idx = entry;
        while idx < exit {
            if allowed_members.is_some_and(|members| !members.contains(&idx)) {
                idx += 1;
                continue;
            }
            if let Some((_, child_exit, _)) = active_child_map.get(&idx) {
                idx = *child_exit;
                continue;
            }
            if owned_elsewhere.contains(idx) {
                idx += 1;
                continue;
            }

            let block_key = host.block_target_key(idx);
            let has_same_start_peer = host.has_same_start_address_peer(idx);
            let is_orphan_unreachable = idx != 0
                && host.predecessors().get(idx).is_some_and(|p| p.is_empty())
                && !targeted.contains(&block_key)
                && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }

            let last_structuring_failure = tier1_failures.remove(&idx);
            if let Some(err) = last_structuring_failure {
                let mut temp_emitted_labels = emitted_labels.clone();
                if let Some((recovered_body, skip_to)) = try_recover_region_linearized_body(
                    host,
                    idx,
                    &err,
                    &targeted,
                    &mut temp_emitted_labels,
                )? {
                    emitted_labels = temp_emitted_labels;
                    let dummy_proof =
                        RegionProof::structured(RegionKind::Sequence, idx, skip_to, None);
                    if skip_to > idx {
                        active_child_map.retain(|&k, _| k < idx || k >= skip_to);
                    }
                    active_child_map.insert(idx, (recovered_body, skip_to, dummy_proof));
                    progress = true;
                    break;
                }
            }

            idx += 1;
        }

        // Tier 3: CollapseStructure collapseAll step — virtualize one likely
        // unstructured edge (LoopBody emitLikelyEdges → TraceDAG → FAS) and
        // retry structured rules. Ghidra never multi-emits; it removes edges.
        if !progress && allowed_members.is_none() && collapse_loop_admission_enabled() {
            if try_virtualize_one_bad_edge(host, entry, exit)? {
                if diag {
                    eprintln!(
                        "[DIAG] build_sese_region_body: collapseAll virtualized edge, continuing collapse loop"
                    );
                }
                progress = true;
            }
        }
    }

    // The true consumed upper bound starting at `entry`: a collapse rule can
    // legitimately structure past the requested `exit` (see doc comment).
    let achieved_exit = active_child_map
        .get(&entry)
        .map(|&(_, child_exit, _)| child_exit.max(exit))
        .unwrap_or(exit);

    // Any accepted region's `proof.members` outside its own `[start, child_exit)`
    // key range was absorbed beyond the naive range formula (guard-absorption
    // etc). Surface that to the caller so composition into an enclosing
    // region's `child_map` doesn't silently drop it back to a bare range.
    let mut region_extra_members: Vec<usize> = Vec::new();
    for (&start, (_, child_exit, proof)) in active_child_map.iter() {
        for &m in &proof.members {
            if m < start || m >= *child_exit {
                region_extra_members.push(m);
            }
        }
    }
    region_extra_members.sort_unstable();
    region_extra_members.dedup();

    let body = reconstruct_sese_final_body_impl(
        host,
        entry,
        exit,
        &active_child_map,
        &targeted,
        diag,
        allowed_members,
    )?;
    Ok((body, achieved_exit, region_extra_members))
}

/// Promote guarded-tail regions to a fixed point (free entry).
pub fn promote_guarded_tails(host: &mut impl StructuringHost, body: &mut Vec<PreHirStmt>) {
    promote_guarded_tail_regions_until_stable(host, body);
    if structuring_diag_enabled() {
        // keep quiet unless already enabled elsewhere
    }
}

/// Whether a residual CFG block's body was consumed by a proven structured region.
///
/// A `Label(block_K)` surfaced inside another structured child is not ownership
/// evidence: the child can expose a jump target without materializing block K's
/// statements. Label-definition deduplication is handled separately.
fn structured_region_owns_block(
    idx: usize,
    structured_ownership: &crate::graph::BlockOwnership,
) -> bool {
    structured_ownership.contains(idx)
}

/// Final SESE reconstruction: surface the structure graph only.
///
/// Graph-only contract (Ghidra `CollapseStructure` invariant):
/// - Structured regions in `active_child_map` own their `[start, skip_to)` range.
/// - Labels already present inside any structured region body are not emitted
///   twice, but they do not by themselves consume the corresponding CFG body.
/// - Residual blocks are only those never absorbed into a structure node; their
///   labels and statements are surfaced through independent decisions.
pub fn reconstruct_sese_final_body(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    active_child_map: &HashMap<usize, (Vec<PreHirStmt>, usize, crate::regions::RegionProof)>,
    targeted: &HashSet<u64>,
    diag: bool,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    reconstruct_sese_final_body_impl(
        host,
        entry,
        exit,
        active_child_map,
        targeted,
        diag,
        None,
    )
}

/// Reconstruct the currently admitted graph for an exact member set without
/// running another collapse pass. This is the stable fixed-point alternative
/// used to score a deferred complex-arm candidate before its isolated host is
/// committed.
pub fn reconstruct_sese_final_body_for_members(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    active_child_map: &HashMap<usize, (Vec<PreHirStmt>, usize, crate::regions::RegionProof)>,
    allowed_members: &HashSet<usize>,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    let targeted = host.collect_jump_targets()?;
    reconstruct_sese_final_body_impl(
        host,
        entry,
        exit,
        active_child_map,
        &targeted,
        false,
        Some(allowed_members),
    )
}

fn reconstruct_sese_final_body_impl(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    active_child_map: &HashMap<usize, (Vec<PreHirStmt>, usize, crate::regions::RegionProof)>,
    targeted: &HashSet<u64>,
    diag: bool,
    allowed_members: Option<&HashSet<usize>>,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    use crate::cleanup::{child_body_has_entry_label, collect_defined_labels};
    use crate::graph::{
        BlockOwnership, StructureEdgeFlags, StructureGraph, StructureNode, surface_structure_graph,
    };
    use crate::helpers::{block_label, recovered_switch_case_values};
    use crate::linear_types::LoweredTerminator;
    use crate::regions::EmitReadyDecision;
    use fission_midend_prehir::PreHirSwitchCase;

    let mut graph = StructureGraph::default();
    let mut emitted_labels: HashSet<u64> = HashSet::default();
    // Block indices owned by proven structured regions. RegionProof.members is
    // the canonical ownership source; ranges and labels are presentation facts.
    let mut structured_ownership = BlockOwnership::default();
    // Labels already defined inside structured region bodies — exclusive surface.
    let mut structure_defined_labels: HashSet<String> = HashSet::default();
    for (_, (child_body, _, proof)) in active_child_map.iter() {
        structured_ownership.extend(proof.members.iter().copied());
        structure_defined_labels.extend(collect_defined_labels(child_body));
    }
    let mut previous_node_id = None;

        let mut idx = entry;
        while idx < exit {
            if allowed_members.is_some_and(|members| !members.contains(&idx)) {
                idx += 1;
                continue;
            }
            let block_key = host.block_target_key(idx);
            let has_same_start_peer = host.has_same_start_address_peer(idx);
            let is_orphan_unreachable = idx != 0
                && host.predecessors()[idx].is_empty()
                && !targeted.contains(&block_key)
                && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }

            if let Some((child_body, child_exit, child_proof)) = active_child_map.get(&idx) {
                let mut node_statements = child_body.clone();
                let header_label = block_label(block_key);
                if (idx == 0 || targeted.contains(&block_key))
                    && emitted_labels.insert(block_key)
                    && !child_body_has_entry_label(child_body, &header_label)
                    && !structure_defined_labels.contains(&header_label)
                {
                    node_statements.insert(0, PreHirStmt::Label(header_label.clone()));
                    structure_defined_labels.insert(header_label);
                }

                let node = StructureNode::region_body(
                    graph.next_node_id(),
                    node_statements,
                    *child_exit,
                    child_proof.clone(),
                );

                let node_id = graph.push(node).map_err(|conflict| {
                    if diag {
                        eprintln!(
                            "[DIAG] final reconstruction: duplicate block ownership block={} existing_node={} attempted_node={}",
                            conflict.block, conflict.existing_owner, conflict.attempted_owner
                        );
                    }
                    MlilPreviewError::UnsupportedCfgRegionShape
                })?;
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
                let next_idx = *child_exit;
                if next_idx <= idx {
                    if diag {
                        eprintln!(
                            "[DIAG] final reconstruction SESE scan: non-advancing child_exit: {} <= {}",
                            next_idx, idx
                        );
                    }
                    idx += 1;
                    continue;
                }
                idx = next_idx;
                continue;
            }

            // Graph residual only: never re-emit a block body owned by a proven
            // structured region. A label inside a child is not sufficient
            // ownership evidence; the residual statements may still be unique.
            let residual_label = block_label(block_key);
            if structured_region_owns_block(idx, &structured_ownership) {
                if diag {
                    eprintln!(
                        "[DIAG] final reconstruction: skip range-owned residual block idx={} key=0x{:x} label={}",
                        idx, block_key, residual_label
                    );
                }
                idx += 1;
                continue;
            }

            let mut node_body = Vec::new();
            let mut explicit_edge_surface = false;
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                node_body.push(PreHirStmt::Label(residual_label.clone()));
                structure_defined_labels.insert(residual_label);
            }
            node_body.extend(host.lower_block_stmts(idx)?);
            match host.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => node_body.push(PreHirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if let Some(target_idx) = host.find_block_index_by_address(target) {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(PreHirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        } else if host.next_block_address(idx) != Some(target) {
                            node_body.push(PreHirStmt::Goto(block_label(target)));
                            explicit_edge_surface = true;
                        }
                    } else if host.next_block_address(idx) != Some(target) {
                        node_body.push(PreHirStmt::Goto(block_label(target)));
                        explicit_edge_surface = true;
                    }
                }
                LoweredTerminator::Fallthrough(Some(target)) => {
                    if let Some(target_idx) = host.find_block_index_by_address(target) {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(PreHirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        }
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = host.next_block_address(idx);
                    let true_idx = host.find_block_index_by_address(true_target);
                    let false_idx =
                        false_target.and_then(|target| host.find_block_index_by_address(target));
                    let true_virtual =
                        true_idx.is_some_and(|ti| crate::collapse_loop::is_virtual_goto_edge(host, idx, ti));
                    let false_virtual =
                        false_idx.is_some_and(|fi| crate::collapse_loop::is_virtual_goto_edge(host, idx, fi));
                    let mut then_body = if true_virtual || next_addr != Some(true_target) {
                        vec![PreHirStmt::Goto(block_label(true_target))]
                    } else {
                        Vec::new()
                    };
                    if let Some(true_idx) = true_idx {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, true_idx)?
                        {
                            then_body = vec![PreHirStmt::Return(Some(expr))];
                        }
                    }
                    let else_body = match false_target {
                        Some(false_target) => {
                            let mut else_body = if false_virtual || Some(false_target) != next_addr
                            {
                                vec![PreHirStmt::Goto(block_label(false_target))]
                            } else {
                                Vec::new()
                            };
                            if let Some(false_idx) = false_idx {
                                if let Some(expr) =
                                    host.lower_return_join_expr_for_predecessor(idx, false_idx)?
                                {
                                    else_body = vec![PreHirStmt::Return(Some(expr))];
                                }
                            }
                            else_body
                        }
                        _ => Vec::new(),
                    };
                    node_body.push(PreHirStmt::If {
                        cond,
                        then_body: std::rc::Rc::new(then_body),
                        else_body: std::rc::Rc::new(else_body),
                    });
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Unsupported {
                    evidence,
                    target_expr,
                } => {
                    node_body.push(host.emit_unsupported_control_surface(evidence, target_expr));
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    let cases: Vec<PreHirSwitchCase> = if let Some(proof) = proof.as_ref() {
                        if EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready {
                            proof
                                .recovered_cases
                                .iter()
                                .filter(|(_, target)| Some(*target) != default_target)
                                .map(|(value, target)| PreHirSwitchCase {
                                    values: vec![*value],
                                    body: std::rc::Rc::new(vec![PreHirStmt::Goto(block_label(*target))]),
                                })
                                .collect()
                        } else {
                            recovered_switch_case_values(
                                &targets,
                                default_target,
                                min_val,
                                Some(proof),
                            )
                            .0
                            .into_iter()
                            .map(|(value, target)| PreHirSwitchCase {
                                values: vec![value],
                                body: std::rc::Rc::new(vec![PreHirStmt::Goto(block_label(target))]),
                            })
                            .collect()
                        }
                    } else if let Some(parsed) = crate::switch::parse_switch_chain(host, idx).ok().flatten() {
                        parsed
                            .cases
                            .into_iter()
                            .filter(|(_, block_idx)| {
                                let target = host.block_target_key(*block_idx);
                                Some(target) != default_target
                            })
                            .map(|(value, block_idx)| PreHirSwitchCase {
                                values: vec![value],
                                body: std::rc::Rc::new(vec![PreHirStmt::Goto(block_label(
                                    host.block_target_key(block_idx),
                                ))]),
                            })
                            .collect()
                    } else {
                        targets
                            .into_iter()
                            .filter(|target| Some(*target) != default_target)
                            .enumerate()
                            .map(|(i, t)| PreHirSwitchCase {
                                values: vec![min_val + i as i64],
                                body: std::rc::Rc::new(vec![PreHirStmt::Goto(block_label(t))]),
                            })
                            .collect()
                    };
                    node_body.push(PreHirStmt::Switch {
                        expr,
                        cases,
                        default: std::rc::Rc::new(
                            default_target
                                .map(block_label)
                                .map(PreHirStmt::Goto)
                                .into_iter()
                                .collect(),
                        ),
                    });
                    explicit_edge_surface = true;
                }
            }
            if explicit_edge_surface {
                let node_id = graph.next_node_id();
                let node_id = graph
                    .push(StructureNode::unstructured(node_id, node_body, idx))
                    .map_err(|_| MlilPreviewError::UnsupportedCfgRegionShape)?;
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            } else {
                let node_id = graph.next_node_id();
                let node_id = graph
                    .push(StructureNode::basic(node_id, node_body, idx))
                    .map_err(|_| MlilPreviewError::UnsupportedCfgRegionShape)?;
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            }
            idx += 1;
        }


    let mut body = surface_structure_graph(graph);
    crate::guarded_tail::promote_guarded_tail_regions_until_stable(host, &mut body);
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditionals::count_explicit_gotos;

    #[test]
    fn structured_label_does_not_own_residual_block() {
        let ownership = crate::graph::BlockOwnership::from_members([1usize, 2]);
        let structured_labels =
            HashSet::from_iter(["block_10".to_string(), "block_30".to_string()]);

        assert!(
            structured_labels.contains("block_30"),
            "precondition: another child surfaced the residual label"
        );
        assert!(
            !structured_region_owns_block(3, &ownership),
            "textual label presence must not consume block 3's statements"
        );
        assert!(
            structured_region_owns_block(2, &ownership),
            "proven region range must still consume block 2 exactly once"
        );
    }

    #[test]
    fn explicit_goto_cost_counts_nested_control_scopes() {
        use fission_midend_core::ir::NirType;

        let body = vec![PreHirStmt::If {
            cond: fission_midend_prehir::PreHirExpr::Const(1, NirType::Bool),
            then_body: std::rc::Rc::new(vec![PreHirStmt::Goto("then".into())]),
            else_body: std::rc::Rc::new(vec![PreHirStmt::While {
                cond: fission_midend_prehir::PreHirExpr::Const(1, NirType::Bool),
                body: std::rc::Rc::new(vec![PreHirStmt::Goto("loop".into())]),
            }]),
        }];

        assert_eq!(count_explicit_gotos(&body), 2);
    }
}
