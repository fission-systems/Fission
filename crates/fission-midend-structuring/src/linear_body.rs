//! Linear-body structuring free functions (`lower_linear_body*`, exits, conditional tails).
//!
//! Entry points take [`crate::host::StructuringHost`]. Multiblock whole-function
//! fallback and p-code trivial-op checks remain host-side in `fission-pcode`.

use crate::helpers::block_label;
use crate::host::StructuringHost;
use crate::linear_types::{
    ConditionalTailKey, ConditionalTailLoweringResult, ConditionalTailMismatchSubtype,
    IfLoweringBudget, LinearBodyCacheKey, LinearBodyCachedOutcome, LinearBodyLoweringOutcome,
    LinearBodyRejectReason, LinearExit, LoweredTerminator, MAX_LINEAR_STRUCTURING_DEPTH,
    MAX_REGION_FOLLOW_DISCOVERY_STEPS, MAX_REGION_JOIN_TRAMPOLINE_DISTANCE,
    MAX_REGION_SHARED_TAIL_STEPS, MAX_REGION_TARGET_CANONICALIZE_STEPS,
    NormalizedConditionalTailArm,
};
use crate::cfg_analysis::PostDomTree;
use fission_midend_core::ir::{HirExpr, HirStmt, MlilPreviewError};
use fission_midend_core::negate_expr;
use std::collections::{HashMap, HashSet};

pub fn has_linear_body_cache(host: &impl StructuringHost, start_idx: usize, exit: LinearExit) -> bool {
        host.linear_body_cache_get(&LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery: false,
        }).is_some()
    }

pub fn lower_linear_body(host: &mut impl StructuringHost, 
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        lower_linear_body_with_budget(host, start_idx, exit, None)
    }

pub fn lower_linear_body_with_budget(host: &mut impl StructuringHost, 
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = host.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
            host.options(),
                start_idx,
                start_addr,
                "lower_linear_body_auto",
                host.structuring_start(),
            ));
            auto_budget.as_mut().unwrap()
        };
        let detailed =
            lower_linear_body_cached(host, start_idx, exit, 0, Some(budget_ref), false)?;
        Ok(match &detailed {
            LinearBodyLoweringOutcome::Lowered(lowered) => Some(lowered.clone()),
            LinearBodyLoweringOutcome::Rejected(_) => None,
        })
    }

pub fn lower_linear_body_for_region_recovery_detailed(host: &mut impl StructuringHost, 
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = host.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
            host.options(),
                start_idx,
                start_addr,
                "lower_linear_body_detailed_auto",
                host.structuring_start(),
            ));
            auto_budget.as_mut().unwrap()
        };
        lower_linear_body_cached(host, start_idx, exit, 0, Some(budget_ref), true)
    }

pub fn lower_linear_body_cached(host: &mut impl StructuringHost, 
        start_idx: usize,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        let key = LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery,
        };
        if let Some(cached) = host.linear_body_cache_get(&key) {
            match cached {
                LinearBodyCachedOutcome::Lowered(lowered) => {
                    return Ok(LinearBodyLoweringOutcome::Lowered(lowered.clone()));
                }
                LinearBodyCachedOutcome::Rejected(reason) => {
                    return Ok(LinearBodyLoweringOutcome::Rejected(reason));
                }
            }
        }
        if !host.linear_body_active_insert(key) {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::RevisitCycle,
            ));
        }

        let result = lower_linear_body_with_depth_detailed(host, 
            start_idx,
            exit,
            depth,
            budget.as_deref_mut(),
            region_recovery,
        )?;

        host.linear_body_active_remove(&key);
        let should_cache = budget.map_or(true, |b| !b.tripped)
            || matches!(
                result,
                LinearBodyLoweringOutcome::Rejected(LinearBodyRejectReason::BudgetTripped)
            );
        if should_cache {
            let cached = match &result {
                LinearBodyLoweringOutcome::Lowered(lowered) => {
                    LinearBodyCachedOutcome::Lowered(lowered.clone())
                }
                LinearBodyLoweringOutcome::Rejected(reason) => {
                    LinearBodyCachedOutcome::Rejected(*reason)
                }
            };
            host.linear_body_cache_insert(key, cached);
        }
        Ok(result)
    }

pub fn lower_linear_body_with_depth_detailed(host: &mut impl StructuringHost, 
        start_idx: usize,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_linear_body_depth")
        {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }

        if let LinearExit::Join(join_idx) = exit {
            if start_idx == join_idx {
                return Ok(LinearBodyLoweringOutcome::Lowered((Vec::new(), start_idx)));
            }
        }

        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if let Some(budget) = budget.as_deref_mut()
                && budget.checkpoint("lower_linear_body_loop")
            {
                return Ok(LinearBodyLoweringOutcome::Rejected(
                    LinearBodyRejectReason::BudgetTripped,
                ));
            }
            if !visited.insert(idx) {
                return Ok(LinearBodyLoweringOutcome::Rejected(
                    LinearBodyRejectReason::RevisitCycle,
                ));
            }

            let terminator = host.lower_block_terminator(idx)?;
            if region_recovery
                && let LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } = terminator
            {
                let tail_lowering = lower_conditional_tail(host, 
                    idx,
                    cond,
                    true_target,
                    false_target,
                    exit,
                    depth + 1,
                    budget.as_deref_mut(),
                    region_recovery,
                )?;
                let (tail_stmt, skip_to) = match tail_lowering {
                    ConditionalTailLoweringResult::Lowered(lowered) => lowered,
                    ConditionalTailLoweringResult::Mismatch(subtype) => {
                        host.record_conditional_tail_mismatch_subtype(subtype);
                        host.record_conditional_tail_mismatch_sample(
                            idx,
                            host.find_block_index_by_address(true_target),
                            false_target
                                .and_then(|target| host.find_block_index_by_address(target)),
                            exit,
                            subtype,
                            "lower_linear_body_with_depth_detailed",
                        );
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::ConditionalTailExitMismatch,
                        ));
                    }
                };
                body.extend(host.lower_block_stmts(idx)?);
                body.push(tail_stmt);
                return Ok(LinearBodyLoweringOutcome::Lowered((body, skip_to)));
            }

            body.extend(host.lower_block_stmts(idx)?);
            match terminator {
                LoweredTerminator::Return(expr) => {
                    body.push(HirStmt::Return(expr));
                    return Ok(LinearBodyLoweringOutcome::Lowered((body, idx + 1)));
                }
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = host.find_block_index_by_address(target) else {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::TargetIndexMissing,
                        ));
                    };
                    if exit == LinearExit::Join(next_idx) {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, next_idx)?
                        {
                            body.push(HirStmt::Return(Some(expr)));
                        }
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    if host.active_switch_targets().contains(&next_idx) {
                        body.push(HirStmt::Goto(block_label(target)));
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    if body.is_empty()
                        && host.is_trivial_forwarding_block(idx, next_idx)
                        && linear_exit_with_budget(host, next_idx, budget.as_deref_mut())?
                            == Some(exit)
                    {
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    let can_inline = if region_recovery {
                        can_inline_linear_successor_for_region(host, idx, next_idx, &visited, exit)
                    } else {
                        can_inline_linear_successor(host, idx, next_idx, &visited)
                    };
                    if can_inline {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(LinearBodyLoweringOutcome::Rejected(
                        LinearBodyRejectReason::SuccessorInlineRejected,
                    ));
                }
                LoweredTerminator::Fallthrough(None) => {
                    if exit != LinearExit::End {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::ExitMismatch,
                        ));
                    }
                    return Ok(LinearBodyLoweringOutcome::Lowered((
                        body,
                        host.block_count(),
                    )));
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let tail_lowering = lower_conditional_tail(host, 
                        idx,
                        cond,
                        true_target,
                        false_target,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    let (tail_stmt, skip_to) = match tail_lowering {
                        ConditionalTailLoweringResult::Lowered(lowered) => lowered,
                        ConditionalTailLoweringResult::Mismatch(subtype) => {
                            if region_recovery {
                                host.record_conditional_tail_mismatch_subtype(subtype);
                                host.record_conditional_tail_mismatch_sample(
                                    idx,
                                    host.find_block_index_by_address(true_target),
                                    false_target.and_then(|target| {
                                        host.find_block_index_by_address(target)
                                    }),
                                    exit,
                                    subtype,
                                    "lower_linear_body_with_depth_detailed",
                                );
                            }
                            return Ok(LinearBodyLoweringOutcome::Rejected(
                                LinearBodyRejectReason::ConditionalTailExitMismatch,
                            ));
                        }
                    };
                    body.push(tail_stmt);
                    return Ok(LinearBodyLoweringOutcome::Lowered((body, skip_to)));
                }
                _ => {
                    return Ok(LinearBodyLoweringOutcome::Rejected(
                        LinearBodyRejectReason::UnsupportedTerminator,
                    ));
                }
            }
        }
    }

pub fn merge_terminal_exits(host: &mut impl StructuringHost, lhs: LinearExit, rhs: LinearExit) -> Option<LinearExit> {
        match (lhs, rhs) {
            (LinearExit::Return, LinearExit::Return) | (LinearExit::End, LinearExit::End) => {
                host.bump_rule_block_if_no_exit();
                Some(lhs)
            }
            (LinearExit::Join(idx), LinearExit::Return)
            | (LinearExit::Return, LinearExit::Join(idx)) => Some(LinearExit::Join(idx)),
            (LinearExit::End, LinearExit::Return) | (LinearExit::Return, LinearExit::End) => {
                Some(LinearExit::End)
            }
            _ => None,
        }
    }

pub fn shared_linear_exit(host: &mut impl StructuringHost, 
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let lhs = linear_exit(host, lhs_idx)?;
        let rhs = linear_exit(host, rhs_idx)?;

        if lhs.is_some() && lhs == rhs {
            Ok(lhs)
        } else if let (Some(l_exit), Some(r_exit)) = (lhs, rhs) {
            Ok(merge_terminal_exits(host, l_exit, r_exit))
        } else {
            Ok(None)
        }
    }

pub fn shared_exit_for_indices(host: &mut impl StructuringHost, 
        indices: &[usize],
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let mut iter = indices.iter().copied();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let mut shared = linear_exit(host, first)?;
        for idx in iter {
            if shared == Some(LinearExit::Join(idx)) {
                continue;
            }
            let exit = linear_exit(host, idx)?;
            if exit == Some(LinearExit::Join(first)) {
                shared = Some(LinearExit::Join(first));
                continue;
            }
            if shared.is_some() && shared == exit {
                continue;
            }
            if let (Some(s_exit), Some(c_exit)) = (shared, exit) {
                if let Some(merged) = merge_terminal_exits(host, s_exit, c_exit) {
                    shared = Some(merged);
                    continue;
                }
            }
            return Ok(None);
        }

        let mut exits_set = std::collections::HashSet::new();
        for idx in indices {
            exits_set.insert(*idx);
        }

        while let Some(LinearExit::Join(target)) = shared {
            if exits_set.contains(&target) {
                let next_exit = linear_exit(host, target)?;
                if next_exit == shared {
                    break;
                }
                shared = next_exit;
            } else {
                break;
            }
        }

        Ok(shared)
    }

pub fn linear_exit(host: &mut impl StructuringHost, 
        start_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        linear_exit_with_budget(host, start_idx, None)
    }

pub fn linear_exit_with_budget(host: &mut impl StructuringHost, 
        start_idx: usize,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        if let Some(cached) = host.linear_exit_cache_get(start_idx) {
            return Ok(cached);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("linear_exit_start")
        {
            return Ok(None);
        }
        let result =
            linear_exit_from(host, start_idx, &mut HashSet::new(), 0, budget.as_deref_mut())?;
        let should_cache = budget.as_deref().is_none_or(|budget| !budget.tripped);
        if should_cache {
            host.linear_exit_cache_insert(start_idx, result);
        }
        Ok(result)
    }

pub fn linear_exit_from(host: &mut impl StructuringHost, 
        idx: usize,
        visited: &mut HashSet<usize>,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(None);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("linear_exit_depth")
        {
            return Ok(None);
        }
        if !visited.insert(idx) {
            return Ok(None);
        }
        match host.lower_block_terminator(idx)? {
            LoweredTerminator::Return(_) => Ok(Some(LinearExit::Return)),
            LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                let Some(next_idx) = host.find_block_index_by_address(target) else {
                    return Ok(None);
                };
                if can_inline_linear_successor(host, idx, next_idx, visited) {
                    linear_exit_from(host, next_idx, visited, depth + 1, budget.as_deref_mut())
                } else {
                    Ok(Some(LinearExit::Join(next_idx)))
                }
            }
            LoweredTerminator::Fallthrough(None) => Ok(Some(LinearExit::End)),
            LoweredTerminator::Cond {
                true_target,
                false_target,
                ..
            } => {
                let Some(false_target) = false_target else {
                    return Ok(None);
                };
                let Some(true_idx) = host.find_block_index_by_address(true_target) else {
                    return Ok(None);
                };
                let Some(false_idx) = host.find_block_index_by_address(false_target) else {
                    return Ok(None);
                };
                let mut true_visited = visited.clone();
                let mut false_visited = visited.clone();
                let true_exit = linear_exit_from(host, 
                    true_idx,
                    &mut true_visited,
                    depth + 1,
                    budget.as_deref_mut(),
                )?;
                let false_exit = linear_exit_from(host, 
                    false_idx,
                    &mut false_visited,
                    depth + 1,
                    budget.as_deref_mut(),
                )?;
                if true_exit.is_some() && true_exit == false_exit {
                    Ok(true_exit)
                } else if let (Some(t_exit), Some(f_exit)) = (true_exit, false_exit) {
                    Ok(merge_terminal_exits(host, t_exit, f_exit))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

pub fn can_inline_linear_successor(host: &impl StructuringHost, 
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool {
        if next_idx <= idx {
            return false;
        }
        // Dom invariant fast-path: if `idx` dominates `next_idx` in the global dominator
        // tree AND every structural predecessor of `next_idx` is either `idx`, in the current
        // visited set, or itself dominated by `idx`, then the inline is provably safe: every
        // path from the CFG entry to `next_idx` goes through `idx`.
        if host.dom_tree().dominates(idx, next_idx)
            && host.predecessors()[next_idx].iter().all(|&pred| {
                pred == idx || visited.contains(&pred) || host.dom_tree().dominates(idx, pred)
            })
        {
            return true;
        }
        if host.predecessors()[next_idx]
            .iter()
            .all(|pred| *pred == idx || visited.contains(pred))
        {
            return true;
        }
        if host.successors()[next_idx].len() == 1 {
            let forwarded = host.successors()[next_idx][0];
            if host.predecessors()[next_idx].iter().all(|pred| {
                *pred == idx
                    || visited.contains(pred)
                    || host.is_trivial_forwarding_block(*pred, next_idx)
            }) && host.is_trivial_forwarding_block(next_idx, forwarded)
            {
                return true;
            }
        }
        host.predecessors()[next_idx].len() == 1
            && host.predecessors()[next_idx][0] == idx
            && host.is_trivial_linear_tail(next_idx)
    }

pub fn can_inline_linear_successor_for_region(host: &impl StructuringHost, 
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
        exit: LinearExit,
    ) -> bool {
        if next_idx <= idx {
            return false;
        }
        if can_inline_linear_successor(host, idx, next_idx, visited) {
            return true;
        }
        let LinearExit::Join(join_idx) = exit else {
            return false;
        };
        if next_idx >= join_idx {
            return false;
        }
        canonicalize_region_target_for_exit(host, idx, next_idx, exit)
            .is_some_and(|normalized| normalized == join_idx)
    }

pub fn lower_conditional_tail(host: &mut impl StructuringHost, 
        origin_idx: usize,
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<ConditionalTailLoweringResult, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::DepthOrBudgetExceeded,
            ));
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_conditional_tail")
        {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::DepthOrBudgetExceeded,
            ));
        }
        let Some(false_target) = false_target else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };
        let Some(true_idx) = host.find_block_index_by_address(true_target) else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };
        let Some(false_idx) = host.find_block_index_by_address(false_target) else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };

        let true_arm = if region_recovery {
            normalize_conditional_tail_arm_for_region(host, origin_idx, true_idx, exit)
        } else {
            NormalizedConditionalTailArm {
                canonical_idx: true_idx,
                effective_start_idx: true_idx,
                reaches_join_trivially: false,
            }
        };
        let false_arm = if region_recovery {
            normalize_conditional_tail_arm_for_region(host, origin_idx, false_idx, exit)
        } else {
            NormalizedConditionalTailArm {
                canonical_idx: false_idx,
                effective_start_idx: false_idx,
                reaches_join_trivially: false,
            }
        };

        let key = ConditionalTailKey {
            true_idx: true_arm.effective_start_idx,
            false_idx: false_arm.effective_start_idx,
            exit,
            region_recovery,
        };
        if !host.conditional_tail_active_insert(key) {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        }

        let result = (|| {
            if true_arm.reaches_join_trivially
                && let LinearBodyLoweringOutcome::Lowered((false_body, skip_to)) =
                    lower_linear_body_cached(
                        host,
                        false_arm.effective_start_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond: negate_expr(cond.clone()),
                        then_body: false_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            if false_arm.reaches_join_trivially
                && let LinearBodyLoweringOutcome::Lowered((true_body, skip_to)) =
                    lower_linear_body_cached(
                        host,
                        true_arm.effective_start_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond: cond.clone(),
                        then_body: true_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            let mut fallback_mismatch_subtype =
                ConditionalTailMismatchSubtype::NoCommonFollowInWindow;
            if region_recovery && let LinearExit::Join(join_idx) = exit {
                let shared_tail_entries = match find_shared_tail_entries_for_region(host, 
                    origin_idx,
                    true_arm.canonical_idx,
                    false_arm.canonical_idx,
                    join_idx,
                ) {
                    Ok(candidates) => candidates,
                    Err(subtype) => {
                        fallback_mismatch_subtype = subtype;
                        if matches!(
                            subtype,
                            ConditionalTailMismatchSubtype::FollowBeyondWindow
                                | ConditionalTailMismatchSubtype::SideEntryOrExit
                                | ConditionalTailMismatchSubtype::ComplexArmShape
                                | ConditionalTailMismatchSubtype::DepthOrBudgetExceeded
                        ) {
                            return Ok(ConditionalTailLoweringResult::Mismatch(subtype));
                        }
                        Vec::new()
                    }
                };
                for shared_tail_entry_idx in shared_tail_entries {
                    if shared_tail_entry_idx == join_idx {
                        continue;
                    }
                    let shared_exit = LinearExit::Join(shared_tail_entry_idx);
                    let true_branch = lower_linear_body_cached(host, 
                        true_arm.canonical_idx,
                        shared_exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    let false_branch = lower_linear_body_cached(host, 
                        false_arm.canonical_idx,
                        shared_exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    match (true_branch, false_branch) {
                        (
                            LinearBodyLoweringOutcome::Lowered((then_body, then_skip)),
                            LinearBodyLoweringOutcome::Lowered((else_body, else_skip)),
                        ) => {
                            match lower_linear_body_cached(host, 
                                shared_tail_entry_idx,
                                exit,
                                depth + 1,
                                budget.as_deref_mut(),
                                region_recovery,
                            )? {
                                LinearBodyLoweringOutcome::Lowered((
                                    shared_tail_body,
                                    shared_skip,
                                )) => {
                                    let mut block_stmts = vec![HirStmt::If {
                                        cond: cond.clone(),
                                        then_body,
                                        else_body,
                                    }];
                                    block_stmts.extend(shared_tail_body);
                                    return Ok(ConditionalTailLoweringResult::Lowered((
                                        HirStmt::Block(block_stmts),
                                        shared_skip.max(then_skip.max(else_skip)),
                                    )));
                                }
                                LinearBodyLoweringOutcome::Rejected(_) => {
                                    fallback_mismatch_subtype =
                                        ConditionalTailMismatchSubtype::FollowTailLoweringFailed;
                                }
                            }
                        }
                        (
                            LinearBodyLoweringOutcome::Rejected(_),
                            LinearBodyLoweringOutcome::Rejected(_),
                        ) => {
                            fallback_mismatch_subtype =
                                ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed;
                        }
                        _ => {
                            fallback_mismatch_subtype =
                                ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed;
                        }
                    }
                }

                return Ok(ConditionalTailLoweringResult::Mismatch(
                    fallback_mismatch_subtype,
                ));
            }

            let true_branch = lower_linear_body_cached(host, 
                true_arm.effective_start_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            let false_branch = lower_linear_body_cached(host, 
                false_arm.effective_start_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            match (true_branch, false_branch) {
                (
                    LinearBodyLoweringOutcome::Lowered((then_body, then_skip)),
                    LinearBodyLoweringOutcome::Lowered((else_body, else_skip)),
                ) => Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    },
                    then_skip.max(else_skip),
                ))),
                (
                    LinearBodyLoweringOutcome::Rejected(_),
                    LinearBodyLoweringOutcome::Rejected(_),
                ) => Ok(ConditionalTailLoweringResult::Mismatch(
                    if fallback_mismatch_subtype
                        == ConditionalTailMismatchSubtype::NoCommonFollowInWindow
                    {
                        ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed
                    } else {
                        fallback_mismatch_subtype
                    },
                )),
                (LinearBodyLoweringOutcome::Rejected(_), LinearBodyLoweringOutcome::Lowered(_))
                | (LinearBodyLoweringOutcome::Lowered(_), LinearBodyLoweringOutcome::Rejected(_)) => {
                    Ok(ConditionalTailLoweringResult::Mismatch(
                        if fallback_mismatch_subtype
                            == ConditionalTailMismatchSubtype::NoCommonFollowInWindow
                        {
                            ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed
                        } else {
                            fallback_mismatch_subtype
                        },
                    ))
                }
            }
        })();
        host.conditional_tail_active_remove(&key);
        result
    }

pub fn normalize_conditional_tail_arm_for_region(host: &impl StructuringHost, 
        origin_idx: usize,
        start_idx: usize,
        exit: LinearExit,
    ) -> NormalizedConditionalTailArm {
        let canonical_idx = canonicalize_region_target_for_exit(host, origin_idx, start_idx, exit)
            .unwrap_or(start_idx);
        if let LinearExit::Join(join_idx) = exit {
            let reaches_join_trivially =
                trivial_region_chain_reaches_join(host, origin_idx, start_idx, join_idx);
            let effective_start_idx = if reaches_join_trivially {
                start_idx
            } else {
                canonical_idx
            };
            return NormalizedConditionalTailArm {
                canonical_idx,
                effective_start_idx,
                reaches_join_trivially,
            };
        }
        NormalizedConditionalTailArm {
            canonical_idx,
            effective_start_idx: canonical_idx,
            reaches_join_trivially: false,
        }
    }

pub fn find_shared_tail_entries_for_region(host: &impl StructuringHost, 
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> Result<Vec<usize>, ConditionalTailMismatchSubtype> {
        let (window, reached_beyond_window) = collect_local_recovery_window_nodes(host, 
            origin_idx,
            true_start_idx,
            false_start_idx,
            join_idx,
        )?;
        if !window.contains(&true_start_idx) || !window.contains(&false_start_idx) {
            return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
        }
        let postdom = compute_local_postdom_sets(host, &window, join_idx)?;
        let true_postdom = postdom
            .get(&true_start_idx)
            .ok_or(ConditionalTailMismatchSubtype::ComplexArmShape)?;
        let false_postdom = postdom
            .get(&false_start_idx)
            .ok_or(ConditionalTailMismatchSubtype::ComplexArmShape)?;

        let mut common_candidates = true_postdom
            .intersection(false_postdom)
            .copied()
            .filter(|idx| *idx != join_idx)
            .collect::<Vec<_>>();
        common_candidates.sort_unstable();
        common_candidates.dedup();

        if common_candidates.is_empty() {
            if reached_beyond_window {
                return Err(ConditionalTailMismatchSubtype::FollowBeyondWindow);
            }
            return Err(ConditionalTailMismatchSubtype::NoCommonFollowInWindow);
        }
        let mut viable = common_candidates
            .into_iter()
            .filter(|candidate| {
                !shared_follow_candidate_has_side_edge(host, origin_idx, &window, *candidate)
            })
            .collect::<Vec<_>>();
        if viable.is_empty() {
            return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
        }
        viable.sort_unstable_by(|a, b| b.cmp(a));
        viable.dedup();
        Ok(viable)
    }

pub fn collect_local_recovery_window_nodes(host: &impl StructuringHost, 
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> Result<(HashSet<usize>, bool), ConditionalTailMismatchSubtype> {
        let mut nodes = HashSet::new();
        let mut reached_beyond = false;
        for start_idx in [true_start_idx, false_start_idx] {
            if start_idx <= origin_idx {
                return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
            }
            if start_idx > join_idx {
                return Err(ConditionalTailMismatchSubtype::FollowBeyondWindow);
            }
            let mut stack = vec![(start_idx, 0usize)];
            while let Some((idx, depth)) = stack.pop() {
                if depth > MAX_REGION_FOLLOW_DISCOVERY_STEPS {
                    reached_beyond = true;
                    continue;
                }
                if idx <= origin_idx {
                    return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
                }
                if idx > join_idx {
                    reached_beyond = true;
                    continue;
                }
                if !nodes.insert(idx) {
                    continue;
                }
                if idx == join_idx {
                    continue;
                }
                for &succ in &host.successors()[idx] {
                    if succ <= origin_idx {
                        return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
                    }
                    if succ > join_idx {
                        reached_beyond = true;
                        continue;
                    }
                    stack.push((succ, depth + 1));
                }
            }
        }
        nodes.insert(join_idx);
        if window_contains_cycle(host, &nodes) {
            return Err(ConditionalTailMismatchSubtype::ComplexArmShape);
        }
        Ok((nodes, reached_beyond))
    }

pub fn window_contains_cycle(host: &impl StructuringHost, window: &HashSet<usize>) -> bool {
    fn dfs(
        successors: &[Vec<usize>],
        node: usize,
        window: &HashSet<usize>,
        visiting: &mut HashSet<usize>,
        visited: &mut HashSet<usize>,
    ) -> bool {
        if visiting.contains(&node) {
            return true;
        }
        if visited.contains(&node) {
            return false;
        }
        visiting.insert(node);
        for succ in &successors[node] {
            if !window.contains(&succ) {
                continue;
            }
            if dfs(successors, *succ, window, visiting, visited) {
                return true;
            }
        }
        visiting.remove(&node);
        visited.insert(node);
        false
    }

    let successors = host.successors();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in window {
        if !visited.contains(node)
            && dfs(successors, *node, window, &mut visiting, &mut visited)
        {
            return true;
        }
    }
    false
}

pub fn compute_local_postdom_sets(host: &impl StructuringHost, 
        window: &HashSet<usize>,
        join_idx: usize,
    ) -> Result<HashMap<usize, HashSet<usize>>, ConditionalTailMismatchSubtype> {
        let Some(postdom_tree) =
            PostDomTree::analyze_window_with_exit(host.successors(), window, join_idx)
        else {
            return Err(ConditionalTailMismatchSubtype::ComplexArmShape);
        };

        for idx in window {
            if *idx == join_idx {
                continue;
            }
            let has_window_succ = host.successors()[*idx]
                .iter()
                .copied()
                .any(|succ| window.contains(&succ));
            if !has_window_succ {
                return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
            }
        }

        Ok(postdom_tree
            .postdominators()
            .iter()
            .map(|(node, set)| (*node, set.clone()))
            .collect())
    }

pub fn shared_follow_candidate_has_side_edge(host: &impl StructuringHost, 
        origin_idx: usize,
        window: &HashSet<usize>,
        candidate_idx: usize,
    ) -> bool {
        for pred in &host.predecessors()[candidate_idx] {
            if *pred <= origin_idx || !window.contains(pred) {
                return true;
            }
        }
        for &succ in &host.successors()[candidate_idx] {
            if !window.contains(&succ) {
                return true;
            }
        }
        false
    }

pub fn collect_region_trivial_forward_chain(host: &impl StructuringHost, 
        origin_idx: usize,
        start_idx: usize,
        join_idx: usize,
    ) -> Vec<usize> {
        if start_idx <= origin_idx || start_idx > join_idx {
            return Vec::new();
        }
        let mut chain = vec![start_idx];
        let mut current = start_idx;
        let mut steps = 0usize;
        let mut seen = HashSet::from([start_idx]);
        while current != join_idx && steps < MAX_REGION_SHARED_TAIL_STEPS {
            if host.successors()[current].len() != 1 {
                break;
            }
            let next_idx = host.successors()[current][0];
            if next_idx > join_idx
                || !seen.insert(next_idx)
                || !host.is_trivial_forwarding_block(current, next_idx)
            {
                break;
            }
            chain.push(next_idx);
            current = next_idx;
            steps += 1;
        }
        chain
    }

pub fn trivial_region_chain_reaches_join(host: &impl StructuringHost, 
        origin_idx: usize,
        start_idx: usize,
        join_idx: usize,
    ) -> bool {
        if start_idx == join_idx {
            return true;
        }
        collect_region_trivial_forward_chain(host, origin_idx, start_idx, join_idx)
            .last()
            .copied()
            == Some(join_idx)
    }

pub fn canonicalize_region_target_for_exit(host: &impl StructuringHost, 
        origin_idx: usize,
        target_idx: usize,
        exit: LinearExit,
    ) -> Option<usize> {
        if target_idx <= origin_idx {
            return None;
        }
        let mut current = target_idx;
        let mut steps = 0usize;
        let mut visited = HashSet::from([target_idx]);
        loop {
            if let LinearExit::Join(join_idx) = exit {
                if current == join_idx {
                    return Some(current);
                }
                if current < join_idx
                    && join_idx - current <= MAX_REGION_JOIN_TRAMPOLINE_DISTANCE
                    && host.is_trivial_forwarding_block(current, join_idx)
                {
                    return Some(join_idx);
                }
            }
            if steps >= MAX_REGION_TARGET_CANONICALIZE_STEPS {
                break;
            }
            let next_idx = if host.successors()[current].len() == 1 {
                host.successors()[current][0]
            } else {
                break;
            };
            if !visited.insert(next_idx) || !host.is_trivial_forwarding_block(current, next_idx) {
                break;
            }
            current = next_idx;
            steps += 1;
        }
        Some(current)
    }

pub fn find_shared_tail_entries_for_region_for_test(host: &impl StructuringHost, 
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> (Vec<usize>, Option<&'static str>) {
        match find_shared_tail_entries_for_region(host, 
            origin_idx,
            true_start_idx,
            false_start_idx,
            join_idx,
        ) {
            Ok(value) => (value, None),
            Err(ConditionalTailMismatchSubtype::NoCommonFollowInWindow) => {
                (Vec::new(), Some("NoCommonFollowInWindow"))
            }
            Err(ConditionalTailMismatchSubtype::FollowBeyondWindow) => {
                (Vec::new(), Some("FollowBeyondWindow"))
            }
            Err(ConditionalTailMismatchSubtype::SideEntryOrExit) => {
                (Vec::new(), Some("SideEntryOrExit"))
            }
            Err(ConditionalTailMismatchSubtype::ComplexArmShape) => {
                (Vec::new(), Some("ComplexArmShape"))
            }
            Err(ConditionalTailMismatchSubtype::DepthOrBudgetExceeded) => {
                (Vec::new(), Some("DepthOrBudgetExceeded"))
            }
            Err(ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed) => {
                (Vec::new(), Some("OneArmBodyLoweringFailed"))
            }
            Err(ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed) => {
                (Vec::new(), Some("BothArmsBodyLoweringFailed"))
            }
            Err(ConditionalTailMismatchSubtype::FollowTailLoweringFailed) => {
                (Vec::new(), Some("FollowTailLoweringFailed"))
            }
        }
    }

pub fn canonicalize_region_target_for_exit_for_test(host: &impl StructuringHost, 
        origin_idx: usize,
        target_idx: usize,
        exit: LinearExit,
    ) -> Option<usize> {
        canonicalize_region_target_for_exit(host, origin_idx, target_idx, exit)
    }
