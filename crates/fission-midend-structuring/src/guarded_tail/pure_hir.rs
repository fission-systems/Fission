//! Pure HIR helpers for guarded-tail canonicalize / execution free-fn owners.
//! Extracted from PreviewBuilder pure methods (no host state).

#![allow(clippy::all)]
#![allow(dead_code)]

use super::types::*;
use crate::cleanup::{
    collect_referenced_label_counts, has_non_ignorable_payload, has_top_level_label,
    is_ignorable_discovery_stmt, single_goto_target, trim_ignorable_stmt_bounds,
};
use crate::guarded_tail_pure::{
    count_var_defs_stmt, count_var_reads_expr, count_var_reads_lvalue, count_var_reads_stmt,
    expr_contains_var, lvalue_contains_var, replace_var_in_expr, replace_var_in_lvalue,
    replace_var_in_stmt,
};
use crate::regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    RegionKind, RegionLegality, RegionRejectionReason,
};
use fission_midend_core::ir::{
    DirBinaryOp, DirExpr, DirLValue, DirStmt, DirUnaryOp, NirBindingOrigin, NirType,
};
use fission_midend_core::util_dir::{negate_expr, simplify_logical_expr, strip_casts};
use crate::HashMap;
use crate::HashSet;

pub fn guarded_tail_diag_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_GUARDED_TAIL_DIAG").is_some())
}

pub fn apply_guarded_tail_replacement_read(stmt: &mut DirStmt, merge: &GuardedTailSyntheticMerge) {
        let replacement_expr = DirExpr::Var(merge.replacement_target.clone());
        replace_var_in_stmt(stmt, &merge.binding_name, &replacement_expr);
    }

pub fn are_all_external_refs_top_level_goto(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> bool {
        let (_, external_nested_before, _, external_nested_after) =
            classify_external_alias_ref_sites_detailed(
                full_body,
                segment_start,
                segment_end,
                label,
            );
        external_nested_before == 0 && external_nested_after == 0
    }

pub fn build_nested_before_alias_ownership_proof(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
        raw_nested_before: usize,
    ) -> AliasOwnershipProof {
        let witnesses =
            classify_nested_before_alias_witnesses(full_body, segment_start, label);
        if raw_nested_before == 0 {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        }

        let anchor_idx = segment_start.saturating_sub(1);
        let current_label_idx = full_body.iter().enumerate().find_map(|(idx, stmt)| {
            (idx >= segment_start
                && idx < segment_end
                && matches!(stmt, DirStmt::Label(candidate) if candidate == label))
            .then_some(idx)
        });
        let Some(current_label_idx) = current_label_idx else {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        };
        let terminal_label_idx = (current_label_idx + 1..full_body.len())
            .find(|idx| matches!(full_body[*idx], DirStmt::Label(_)));
        let Some(terminal_label_idx) = terminal_label_idx else {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        };

        let raw_refs = collect_referenced_label_counts(full_body)
            .get(label)
            .copied()
            .unwrap_or(0);
        let guard_family_internalized =
            count_internalized_guard_family_nested_conditional_entries(
                full_body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
            );
        if guard_family_internalized >= raw_nested_before {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: raw_nested_before,
                class: NestedBeforeOwnershipClass::GuardFamilyInternalizable,
                legality_reason: AliasOwnershipLegalityReason::Complete,
                witnesses,
            };
        }

        let paired_boundary_internalized = count_internalized_paired_nested_boundary_refs(
            full_body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
        );
        if paired_boundary_internalized >= raw_nested_before {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: raw_nested_before,
                class: NestedBeforeOwnershipClass::PairedBoundaryInternalizable,
                legality_reason: AliasOwnershipLegalityReason::Complete,
                witnesses,
            };
        }

        let class = if witnesses.iter().any(|w| {
            matches!(
                w.class,
                NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
            )
        }) {
            NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
        } else if witnesses.iter().all(|w| {
            matches!(
                w.class,
                NestedBeforeOwnershipClass::NestedBeforeExternalOwner
            )
        }) {
            NestedBeforeOwnershipClass::NestedBeforeExternalOwner
        } else {
            NestedBeforeOwnershipClass::NestedBeforeUnknown
        };
        let legality_reason = match class {
            NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload => {
                AliasOwnershipLegalityReason::NonlocalPayload
            }
            NestedBeforeOwnershipClass::NestedBeforeCrossesTerminalJoin => {
                AliasOwnershipLegalityReason::CrossesTerminalJoin
            }
            NestedBeforeOwnershipClass::NestedBeforeExternalOwner => {
                AliasOwnershipLegalityReason::ExternalOwner
            }
            _ => AliasOwnershipLegalityReason::Unknown,
        };

        AliasOwnershipProof {
            label: label.to_string(),
            raw_nested_before,
            internalized_nested_before: 0,
            class,
            legality_reason,
            witnesses,
        }
    }

pub fn classify_alias_ref_sites(
        body: &[DirStmt],
        label_idx: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        let mut top_level_before = 0usize;
        let mut nested_before = 0usize;
        let mut refs_after = 0usize;

        for (idx, stmt) in body.iter().enumerate() {
            let ref_count = stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            if idx >= label_idx {
                refs_after += ref_count;
                continue;
            }
            match stmt {
                DirStmt::Goto(target) if target == label => top_level_before += 1,
                _ => nested_before += ref_count,
            }
        }

        (top_level_before, nested_before, refs_after)
    }

pub fn classify_external_alias_ref_sites(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        let (top_level_before, nested_before, top_level_after, nested_after) =
            classify_external_alias_ref_sites_detailed(
                full_body,
                segment_start,
                segment_end,
                label,
            );

        (
            top_level_before,
            nested_before,
            top_level_after + nested_after,
        )
    }

pub fn classify_external_alias_ref_sites_detailed(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize, usize) {
        let mut top_level_before = 0usize;
        let mut nested_before = 0usize;
        let mut top_level_after = 0usize;
        let mut nested_after = 0usize;

        for (idx, stmt) in full_body.iter().enumerate() {
            if idx >= segment_start && idx < segment_end {
                continue;
            }
            let ref_count = stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            if idx < segment_start {
                match stmt {
                    DirStmt::Goto(target) if target == label => top_level_before += 1,
                    _ => nested_before += ref_count,
                }
            } else {
                match stmt {
                    DirStmt::Goto(target) if target == label => top_level_after += 1,
                    _ => nested_after += ref_count,
                }
            }
        }

        (
            top_level_before,
            nested_before,
            top_level_after,
            nested_after,
        )
    }

pub fn classify_nested_before_alias_witnesses(
        full_body: &[DirStmt],
        segment_start: usize,
        label: &str,
    ) -> Vec<NestedBeforeAliasWitness> {
        let mut witnesses = Vec::new();
        for (stmt_idx, stmt) in full_body.iter().enumerate() {
            if stmt_idx >= segment_start {
                break;
            }
            if stmt_contains_goto_label(stmt, label) == 0 {
                continue;
            }
            if matches!(stmt, DirStmt::Goto(target) if target == label) {
                continue;
            }

            let class = if classify_nested_before_nonlocal_payload(stmt, label) {
                NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
            } else if stmt_is_single_branch_if_to_label(stmt, label).is_some() {
                NestedBeforeOwnershipClass::NestedBeforeExternalOwner
            } else {
                NestedBeforeOwnershipClass::NestedBeforeUnknown
            };
            witnesses.push(NestedBeforeAliasWitness {
                stmt_idx,
                cond: stmt_is_single_branch_if_to_label(stmt, label).cloned(),
                class,
            });
        }
        witnesses
    }

pub fn classify_nested_before_nonlocal_payload(stmt: &DirStmt, label: &str) -> bool {
        let DirStmt::If {
            then_body,
            else_body,
            ..
        } = stmt
        else {
            return false;
        };

        let then_target = single_goto_target(then_body);
        let else_target = single_goto_target(else_body);
        if matches!(then_target, Some(target) if target == label) && else_body.is_empty() {
            return false;
        }
        if matches!(else_target, Some(target) if target == label) && then_body.is_empty() {
            return false;
        }
        stmt_contains_goto_label(stmt, label) > 0
    }

pub fn classify_stmt_read_kind(
        stmt: &DirStmt,
        name: &str,
    ) -> Option<GuardedTailReadKind> {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                if expr_contains_var(rhs, name) {
                    Some(GuardedTailReadKind::AssignRhs)
                } else if lvalue_contains_var(lhs, name) {
                    Some(GuardedTailReadKind::NestedExpr)
                } else {
                    None
                }
            }
            DirStmt::Expr(DirExpr::Call { args, .. })
                if args.iter().any(|arg| expr_contains_var(arg, name)) =>
            {
                Some(GuardedTailReadKind::CallArg)
            }
            DirStmt::Expr(expr) if expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::NestedExpr)
            }
            DirStmt::Expr(_) => None,
            DirStmt::If { cond, .. } if expr_contains_var(cond, name) => {
                Some(GuardedTailReadKind::ConditionExpr)
            }
            DirStmt::Switch { expr, .. } if expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::SwitchSelector)
            }
            DirStmt::Return(Some(expr)) if expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::ReturnExpr)
            }
            DirStmt::Return(_) => None,
            DirStmt::Block(stmts) | DirStmt::While { body: stmts, .. } => stmts
                .iter()
                .find_map(|stmt| classify_stmt_read_kind(stmt, name)),
            DirStmt::DoWhile { body, cond } => body
                .iter()
                .find_map(|stmt| classify_stmt_read_kind(stmt, name))
                .or_else(|| {
                    if expr_contains_var(cond, name) {
                        Some(GuardedTailReadKind::ConditionExpr)
                    } else {
                        None
                    }
                }),
            DirStmt::Switch { cases, default, .. } => cases
                .iter()
                .flat_map(|case| case.body.iter())
                .chain(default.iter())
                .find_map(|stmt| classify_stmt_read_kind(stmt, name)),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => then_body
                .iter()
                .chain(else_body.iter())
                .find_map(|stmt| classify_stmt_read_kind(stmt, name)),
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => init
                .iter()
                .find_map(|stmt| classify_stmt_read_kind(stmt, name))
                .or_else(|| {
                    cond.as_ref()
                        .filter(|expr| expr_contains_var(expr, name))
                        .map(|_| GuardedTailReadKind::ConditionExpr)
                })
                .or_else(|| {
                    update
                        .iter()
                        .find_map(|stmt| classify_stmt_read_kind(stmt, name))
                })
                .or_else(|| {
                    body.iter()
                        .find_map(|stmt| classify_stmt_read_kind(stmt, name))
                }),
            DirStmt::VaStart { va_list, .. } if expr_contains_var(va_list, name) => {
                Some(GuardedTailReadKind::NestedExpr)
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue
            | DirStmt::VaStart { .. } => None,
        }
    }

pub fn collapse_duplicate_top_level_guard_ladder(stmts: &mut Vec<DirStmt>) -> usize {
        let mut removed = 0usize;
        let mut i = 0usize;

        while i < stmts.len() {
            let Some((cond_i, target_i)) = top_level_guard_goto_signature(&stmts[i]) else {
                i += 1;
                continue;
            };

            // Keep this narrowly scoped: only allow empty blocks between duplicates.
            // Crossing labels can change ownership/fallthrough interpretation.
            let mut j = i + 1;
            while j < stmts.len() {
                match &stmts[j] {
                    DirStmt::Block(body) if body.is_empty() => j += 1,
                    _ => break,
                }
            }
            if j >= stmts.len() {
                i += 1;
                continue;
            }

            let Some((cond_j, target_j)) = top_level_guard_goto_signature(&stmts[j]) else {
                i += 1;
                continue;
            };

            if cond_i == cond_j && target_i == target_j {
                stmts.remove(j);
                removed += 1;
                // Keep `i` to fold guard ladders of length >= 3.
                continue;
            }

            i += 1;
        }

        removed
    }

pub fn collapse_top_level_sink_to_return_goto_chain(
        stmts: &mut [DirStmt],
        full_body: &[DirStmt],
    ) -> usize {
        let mut rewritten = 0usize;

        for idx in 0..stmts.len() {
            let target = match &stmts[idx] {
                DirStmt::Goto(target) => target.clone(),
                _ => continue,
            };

            // Restrict to guard-only prefixes so we don't consume payload-tail
            // exits that are already handled by canonical tail logic.
            if !stmts[..idx].iter().all(stmt_is_guard_prefix_safe) {
                continue;
            }

            // Keep this narrow: collapse only when the target label is unique
            // and the existing terminal-safe resolver proves a return sink.
            if top_level_label_definition_count(full_body, &target) != 1 {
                continue;
            }

            let Some(DirStmt::Return(ret)) =
                resolve_terminal_tail_exit_stmt(full_body, &target)
            else {
                continue;
            };

            stmts[idx] = DirStmt::Return(ret);
            rewritten += 1;
        }

        rewritten
    }

pub fn collect_guarded_tail_candidate_reads(
        body: &[DirStmt],
        middle: &[DirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> Vec<GuardedTailReplacementRead> {
        let mut reads = Vec::new();
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if stmt_idx >= if_idx && stmt_idx <= label_idx {
                continue;
            }
            let ref_count = stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::ExternalForwardGoto,
                });
            }
        }
        for (stmt_idx, stmt) in middle.iter().enumerate() {
            let ref_count = stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::MiddleGoto,
                });
            }
        }
        reads
    }

pub fn condition_matches_assumption(
        expr: &DirExpr,
        assumption: &ConditionAssumption,
    ) -> Option<bool> {
        if expr == &assumption.expr {
            return Some(assumption.value);
        }
        if let DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: inner,
            ..
        } = expr
            && inner.as_ref() == &assumption.expr
        {
            return Some(!assumption.value);
        }
        if let DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: inner,
            ..
        } = &assumption.expr
            && inner.as_ref() == expr
        {
            return Some(!assumption.value);
        }
        None
    }

pub fn count_goto_refs_in_stmt(stmt: &DirStmt, out: &mut HashMap<String, usize>) {
        match stmt {
            DirStmt::Goto(label) => {
                *out.entry(label.clone()).or_insert(0) += 1;
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for nested in then_body {
                    count_goto_refs_in_stmt(nested, out);
                }
                for nested in else_body {
                    count_goto_refs_in_stmt(nested, out);
                }
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                for nested in body {
                    count_goto_refs_in_stmt(nested, out);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for nested in &case.body {
                        count_goto_refs_in_stmt(nested, out);
                    }
                }
                for nested in default {
                    count_goto_refs_in_stmt(nested, out);
                }
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }

pub fn count_sink_equivalent_top_level_after_label_refs(
        body: &[DirStmt],
        full_body: &[DirStmt],
        label: &str,
        label_idx: usize,
        top_level_after_positions: &[usize],
        nested_after_label_count: usize,
        external_ref_count: usize,
    ) -> usize {
        if nested_after_label_count > 0 || external_ref_count > 0 {
            return 0;
        }
        top_level_after_positions
            .iter()
            .copied()
            .filter(|pos| {
                local_after_label_ref_is_sink_equivalent(
                    body, full_body, label, label_idx, *pos,
                )
            })
            .count()
    }

pub fn count_top_level_goto_refs_in_range(
        body: &[DirStmt],
        label: &str,
        start_exclusive: usize,
        end_exclusive: usize,
    ) -> usize {
        if start_exclusive + 1 >= end_exclusive {
            return 0;
        }
        body[start_exclusive + 1..end_exclusive]
            .iter()
            .filter(|stmt| matches!(stmt, DirStmt::Goto(target) if target == label))
            .count()
    }

pub fn effective_middle_refs_for_promotion(
        middle: &[DirStmt],
        label: &str,
        middle_refs: usize,
    ) -> usize {
        if middle_is_join_label_only_glue(middle, label) {
            return 0;
        }
        middle_refs.saturating_sub(trailing_middle_fallthrough_equivalent_refs(
            middle, label,
        ))
    }

pub fn evaluate_condition_assumptions(
        expr: &DirExpr,
        assumptions: &[ConditionAssumption],
    ) -> Option<bool> {
        assumptions
            .iter()
            .find_map(|assumption| condition_matches_assumption(expr, assumption))
    }

pub fn expr_is_pure_value(expr: &DirExpr) -> bool {
        match expr {
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => true,
            DirExpr::Cast { expr, .. } => expr_is_pure_value(expr),
            DirExpr::Unary { expr, .. } => expr_is_pure_value(expr),
            DirExpr::Binary { lhs, rhs, .. } => {
                expr_is_pure_value(lhs) && expr_is_pure_value(rhs)
            }
            DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
                expr_is_pure_value(base)
            }
            DirExpr::Index { base, index, .. } => {
                expr_is_pure_value(base) && expr_is_pure_value(index)
            }
            DirExpr::AggregateCopy { src, .. } => expr_is_pure_value(src),
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                expr_is_pure_value(cond)
                    && expr_is_pure_value(then_expr)
                    && expr_is_pure_value(else_expr)
            }
            DirExpr::Call { target, args, .. } => {
                guarded_tail_call_target_is_known_pure_helper(target)
                    && args.iter().all(expr_is_pure_value)
            }
            DirExpr::Load { .. } => false,
        }
    }

pub fn factor_duplicate_top_level_guard_cluster_with_trivial_gap(
        stmts: &mut Vec<DirStmt>,
        full_body: &[DirStmt],
    ) -> usize {
        let mut removed = 0usize;
        let mut i = 0usize;

        while i < stmts.len() {
            let Some((cond_i, target_i)) = top_level_guard_goto_signature(&stmts[i]) else {
                i += 1;
                continue;
            };

            let mut j = i + 1;
            let mut duplicate_at = None;
            while j < stmts.len() {
                if let Some((cond_j, target_j)) = top_level_guard_goto_signature(&stmts[j]) {
                    if cond_i == cond_j && target_i == target_j {
                        duplicate_at = Some(j);
                    }
                    break;
                }
                if stmt_is_guard_cluster_trivial_gap(&stmts[j], full_body) {
                    j += 1;
                    continue;
                }
                break;
            }

            if let Some(j) = duplicate_at {
                stmts.remove(j);
                removed += 1;
                // Keep `i` for chains with >= 3 same-family guards.
                continue;
            }

            i += 1;
        }

        removed
    }

pub fn find_guarded_tail_preexisting_source(
        body: &[DirStmt],
        if_idx: usize,
        binding_name: &str,
    ) -> Option<DirExpr> {
        for stmt in body[..if_idx].iter().rev() {
            match stmt {
                DirStmt::Assign {
                    lhs: DirLValue::Var(name),
                    rhs,
                } if name == binding_name && expr_is_pure_value(rhs) => {
                    return Some(rhs.clone());
                }
                DirStmt::Return(_)
                | DirStmt::Break
                | DirStmt::Continue
                | DirStmt::If { .. }
                | DirStmt::Switch { .. }
                | DirStmt::While { .. }
                | DirStmt::DoWhile { .. }
                | DirStmt::For { .. } => return None,
                DirStmt::Label(_)
                | DirStmt::Goto(_)
                | DirStmt::Assign { .. }
                | DirStmt::VaStart { .. }
                | DirStmt::Expr(_)
                | DirStmt::Block(_) => {}
            }
        }
        None
    }

pub fn find_top_level_label_after(
        body: &[DirStmt],
        start_idx: usize,
        label: &str,
    ) -> Option<usize> {
        (start_idx + 1..body.len()).find(
            |pos| matches!(body.get(*pos), Some(DirStmt::Label(candidate)) if candidate == label),
        )
    }

pub fn flatten_guarded_tail_segment(segment: &[DirStmt], out: &mut Vec<DirStmt>) {
        for stmt in segment {
            match stmt {
                DirStmt::Block(body) => flatten_guarded_tail_segment(body, out),
                other => out.push(other.clone()),
            }
        }
    }

pub fn goto_ref_counts(body: &[DirStmt]) -> HashMap<String, usize> {
        let mut out = HashMap::default();
        for stmt in body {
            count_goto_refs_in_stmt(stmt, &mut out);
        }
        out
    }

pub fn guarded_tail_middle_is_execution_safe(middle: &[DirStmt], label: &str) -> bool {
        middle
            .iter()
            .all(|stmt| guarded_tail_stmt_is_execution_safe(stmt, label))
    }

pub fn guarded_tail_stmt_is_execution_safe(stmt: &DirStmt, label: &str) -> bool {
        match stmt {
            DirStmt::Assign { .. } => true,
            DirStmt::VaStart { .. } => true,
            DirStmt::Expr(_) => true,
            DirStmt::Goto(_) => true,
            DirStmt::Block(body) => guarded_tail_middle_is_execution_safe(body, label),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                guarded_tail_middle_is_execution_safe(then_body, label)
                    && guarded_tail_middle_is_execution_safe(else_body, label)
            }
            DirStmt::Label(_)
            | DirStmt::Switch { .. }
            | DirStmt::While { .. }
            | DirStmt::DoWhile { .. }
            | DirStmt::For { .. }
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => true,
        }
    }

pub fn inferred_alias_forward_target_with_after_label_refs(
        segment: &[DirStmt],
        label: &str,
    ) -> Option<String> {
        let mut inferred_target = None::<String>;
        let mut saw_forward_goto = false;

        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt)
                || stmt_is_pure_value_expr(stmt)
                || stmt_is_pure_value_assign(stmt)
            {
                continue;
            }

            let DirStmt::Goto(target) = stmt else {
                return None;
            };
            if target == label {
                continue;
            }

            match inferred_target.as_deref() {
                Some(existing) if existing != target => return None,
                Some(_) => {}
                None => inferred_target = Some(target.clone()),
            }
            saw_forward_goto = true;
        }

        let target = inferred_target?;
        if !saw_forward_goto {
            return None;
        }
        segment
            .iter()
            .all(|stmt| stmt_is_alias_forward_safe(stmt, label, &target))
            .then_some(target)
    }

pub fn internalized_guard_family_nested_before_refs_for_join_owner(
        body: &[DirStmt],
        if_idx: usize,
        label: &str,
        candidate_cond: &DirExpr,
    ) -> usize {
        body.iter()
            .take(if_idx)
            .filter(|stmt| {
                stmt_contains_goto_label(stmt, label) > 0
                    && stmt_is_single_branch_if_to_label(stmt, label).is_some_and(
                        |entry_cond| exprs_share_guard_family(candidate_cond, entry_cond),
                    )
            })
            .count()
    }

pub fn is_local_alias_forward_segment(segment: &[DirStmt], next_label: &str) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) {
                continue;
            }
            match stmt {
                DirStmt::Goto(label) if !saw_forward_goto && label == next_label => {
                    saw_forward_goto = true;
                }
                _ => return false,
            }
        }
        saw_forward_goto
    }

pub fn is_local_alias_forward_segment_with_after_label_refs(
        segment: &[DirStmt],
        label: &str,
        next_label: &str,
    ) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if matches!(stmt, DirStmt::Goto(target) if target == next_label) {
                saw_forward_goto = true;
            }
            if !stmt_is_alias_forward_safe(stmt, label, next_label) {
                return false;
            }
        }
        saw_forward_goto
    }

pub fn is_pure_multi_goto_gap_to_label(
        body: &[DirStmt],
        goto_positions: &[usize],
        label_idx: usize,
        label: &str,
    ) -> bool {
        let Some(start) = goto_positions.iter().copied().min() else {
            return false;
        };
        if goto_positions.iter().any(|pos| *pos >= label_idx) {
            return false;
        }
        body[start + 1..label_idx].iter().all(|stmt| {
            is_ignorable_discovery_stmt(stmt)
                || stmt_is_pure_value_expr(stmt)
                || matches!(stmt, DirStmt::Goto(target) if target == label)
        })
    }

pub fn is_trivial_join_forward_or_pure_segment(
        segment: &[DirStmt],
        next_label: &str,
    ) -> bool {
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) || stmt_is_pure_value_expr(stmt) {
                continue;
            }
            match stmt {
                DirStmt::Goto(label) if label == next_label => {}
                _ => return false,
            }
        }
        true
    }

pub fn is_trivial_join_forward_segment(segment: &[DirStmt], next_label: &str) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) {
                continue;
            }
            match stmt {
                DirStmt::Goto(label) if label == next_label => {
                    saw_forward_goto = true;
                }
                _ => return false,
            }
        }
        saw_forward_goto
    }

pub fn local_after_label_ref_is_sink_equivalent(
        body: &[DirStmt],
        full_body: &[DirStmt],
        label: &str,
        label_idx: usize,
        after_label_pos: usize,
    ) -> bool {
        let Some(DirStmt::Goto(target)) = body.get(after_label_pos) else {
            return false;
        };
        if after_label_pos <= label_idx || target != label {
            return false;
        }
        if top_level_label_definition_count(full_body, label) != 1 {
            return false;
        }

        let Some(DirStmt::Return(sink_return)) =
            resolve_terminal_tail_exit_stmt(full_body, label)
        else {
            return false;
        };

        let next_label_idx = (after_label_pos + 1..body.len())
            .find(|pos| matches!(body[*pos], DirStmt::Label(_)))
            .unwrap_or(body.len());

        body[after_label_pos + 1..next_label_idx]
            .iter()
            .all(|stmt| {
                stmt_is_sink_equivalent_after_label_gap(stmt, full_body, &sink_return)
            })
    }

pub fn local_forward_branch_target(
        then_body: &[DirStmt],
        else_body: &[DirStmt],
    ) -> Option<(String, bool)> {
        if let Some(label) = single_goto_target(then_body)
            && else_body.is_empty()
        {
            return Some((label.to_string(), true));
        }
        if let Some(label) = single_goto_target(else_body)
            && then_body.is_empty()
        {
            return Some((label.to_string(), false));
        }
        None
    }

pub fn local_goto_positions_by_label(body: &[DirStmt]) -> HashMap<String, Vec<usize>> {
        let mut refs = HashMap::default();
        for (idx, stmt) in body.iter().enumerate() {
            if let DirStmt::Goto(label) = stmt {
                refs.entry(label.clone()).or_insert_with(Vec::new).push(idx);
            }
        }
        refs
    }

pub fn middle_is_join_label_only_glue(middle: &[DirStmt], label: &str) -> bool {
        middle.iter().all(|stmt| {
            is_ignorable_discovery_stmt(stmt)
                || matches!(stmt, DirStmt::Goto(target) if target == label)
        })
    }

pub fn outside_refs_are_elidable_next_flow(
        body: &[DirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> bool {
        let mut found = false;
        for (idx, stmt) in body.iter().enumerate() {
            if idx >= if_idx && idx <= label_idx {
                continue;
            }
            let ref_count = stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            found = true;
            match stmt {
                DirStmt::Goto(target) if target == label && idx < if_idx => {}
                _ => return false,
            }
        }
        found
    }

pub fn outside_refs_preserve_forward_owner(
        body: &[DirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> bool {
        let mut found = false;
        for (idx, stmt) in body.iter().enumerate() {
            if idx >= if_idx && idx <= label_idx {
                continue;
            }
            let ref_count = stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            found = true;
            match stmt {
                DirStmt::Goto(target) if target == label && idx < label_idx => {}
                _ => return false,
            }
        }
        found
    }

pub fn resolve_alias_redirect(
        label: &str,
        redirects: &HashMap<String, Option<String>>,
    ) -> Option<String> {
        let mut current = label.to_string();
        let mut seen = HashSet::default();
        while let Some(next) = redirects.get(&current) {
            if !seen.insert(current.clone()) {
                return Some(current);
            }
            match next {
                Some(next_label) => current = next_label.clone(),
                None => return None,
            }
        }
        Some(current)
    }

pub fn resolve_guarded_tail_else_source(
        body: &[DirStmt],
        if_idx: usize,
        binding_name: &str,
        cache: &mut GuardedTailReplacementCache,
    ) -> Option<DirExpr> {
        if let Some(expr) = cache.else_sources.get(binding_name) {
            return Some(expr.clone());
        }
        let expr = find_guarded_tail_preexisting_source(body, if_idx, binding_name)?;
        cache
            .else_sources
            .insert(binding_name.to_string(), expr.clone());
        Some(expr)
    }

pub fn resolve_terminal_tail_exit_stmt(
        body: &[DirStmt],
        target_label: &str,
    ) -> Option<DirStmt> {
        let mut current = target_label.to_string();
        let mut seen = HashSet::default();

        loop {
            if !seen.insert(current.clone()) {
                return None;
            }

            // Safe subcase guard: no external re-entry into any hop label.
            // The only allowed predecessor is the unique previous hop goto.
            let ref_count = body
                .iter()
                .map(|stmt| stmt_contains_goto_label(stmt, &current))
                .sum::<usize>();
            if ref_count != 1 {
                return None;
            }

            let label_idx = body
                .iter()
                .position(|stmt| matches!(stmt, DirStmt::Label(label) if label == &current))?;
            let next_label_idx = body[label_idx + 1..]
                .iter()
                .position(|stmt| matches!(stmt, DirStmt::Label(_)))
                .map(|offset| label_idx + 1 + offset)
                .unwrap_or(body.len());

            let mut terminal_return: Option<Option<DirExpr>> = None;
            let mut terminal_goto: Option<String> = None;

            for stmt in &body[label_idx + 1..next_label_idx] {
                if is_ignorable_discovery_stmt(stmt)
                    || stmt_is_pure_value_expr(stmt)
                    || stmt_is_pure_value_assign(stmt)
                {
                    // Terminal exit must be the last meaningful statement in the hop.
                    if terminal_return.is_some() || terminal_goto.is_some() {
                        return None;
                    }
                    continue;
                }

                match stmt {
                    DirStmt::Return(ret) => {
                        if terminal_return.is_some() || terminal_goto.is_some() {
                            return None;
                        }
                        terminal_return = Some(ret.clone());
                    }
                    DirStmt::Goto(next) => {
                        if terminal_return.is_some() || terminal_goto.is_some() {
                            return None;
                        }
                        terminal_goto = Some(next.clone());
                    }
                    // Keep nested/nonlocal control-flow crossing forbidden.
                    DirStmt::Break
                    | DirStmt::Continue
                    | DirStmt::If { .. }
                    | DirStmt::Switch { .. }
                    | DirStmt::While { .. }
                    | DirStmt::DoWhile { .. }
                    | DirStmt::For { .. }
                    | DirStmt::Block(_)
                    | DirStmt::VaStart { .. }
                    | DirStmt::Assign { .. }
                    | DirStmt::Expr(_)
                    | DirStmt::Label(_) => return None,
                }
            }

            if let Some(ret) = terminal_return {
                return Some(DirStmt::Return(ret));
            }
            if let Some(next) = terminal_goto {
                current = next;
                continue;
            }
            return None;
        }
    }

pub fn rewrite_goto_label_in_stmt(stmt: &mut DirStmt, from: &str, to: &str) {
        match stmt {
            DirStmt::Goto(label) => {
                if label == from {
                    *label = to.to_string();
                }
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for nested in then_body {
                    rewrite_goto_label_in_stmt(nested, from, to);
                }
                for nested in else_body {
                    rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                for nested in body {
                    rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for nested in &mut case.body {
                        rewrite_goto_label_in_stmt(nested, from, to);
                    }
                }
                for nested in default {
                    rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }

pub fn rewrite_goto_label_in_stmts(stmts: &mut [DirStmt], from: &str, to: &str) {
        for stmt in stmts {
            rewrite_goto_label_in_stmt(stmt, from, to);
        }
    }

pub fn rewrite_guarded_tail_sequence(
        stmts: &[DirStmt],
        join_label: &str,
        assumptions: &[ConditionAssumption],
    ) -> GuardedTailRewriteResult {
        let mut out = Vec::with_capacity(stmts.len());
        let mut idx = 0usize;
        while idx < stmts.len() {
            match &stmts[idx] {
                DirStmt::Goto(target) if target == join_label => {
                    return GuardedTailRewriteResult {
                        stmts: out,
                        exits_to_join: true,
                        unresolved_join_refs: 0,
                    };
                }
                DirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    if let Some((branch_label, branch_when_true)) =
                        local_forward_branch_target(then_body, else_body)
                        && branch_label != join_label
                        && let Some(label_pos) = (idx + 1..stmts.len()).find(|pos| {
                            matches!(
                                stmts.get(*pos),
                                Some(DirStmt::Label(candidate)) if candidate == &branch_label
                            )
                        })
                    {
                        let mut target_assumptions = assumptions.to_vec();
                        target_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: branch_when_true,
                        });
                        let target_rewritten = rewrite_guarded_tail_sequence(
                            &stmts[label_pos + 1..],
                            join_label,
                            &target_assumptions,
                        );

                        let mut fallthrough_assumptions = assumptions.to_vec();
                        fallthrough_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: !branch_when_true,
                        });
                        let fallthrough_rewritten = rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..label_pos],
                            join_label,
                            &fallthrough_assumptions,
                        );

                        let target_body = target_rewritten.stmts.clone();
                        let target_exits = target_rewritten.exits_to_join;

                        let (then_result, then_exits, else_result, else_exits) = if branch_when_true
                        {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                target_body,
                                target_exits,
                                fallthrough_body,
                                fallthrough_exits,
                            )
                        } else {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                fallthrough_body,
                                fallthrough_exits,
                                target_body,
                                target_exits,
                            )
                        };

                        out.push(DirStmt::If {
                            cond: cond.clone(),
                            then_body: then_result,
                            else_body: else_result,
                        });
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: then_exits && else_exits,
                            unresolved_join_refs: target_rewritten.unresolved_join_refs
                                + fallthrough_rewritten.unresolved_join_refs,
                        };
                    }

                    if let Some(value) = evaluate_condition_assumptions(cond, assumptions) {
                        let mut next_assumptions = assumptions.to_vec();
                        next_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value,
                        });
                        let chosen = if value { then_body } else { else_body };
                        let rewritten = rewrite_guarded_tail_sequence(
                            chosen,
                            join_label,
                            &next_assumptions,
                        );
                        out.extend(rewritten.stmts);
                        if rewritten.exits_to_join {
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: rewritten.unresolved_join_refs,
                            };
                        }
                        idx += 1;
                        continue;
                    }

                    let mut then_assumptions = assumptions.to_vec();
                    then_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: true,
                    });
                    let then_rewritten = rewrite_guarded_tail_sequence(
                        then_body,
                        join_label,
                        &then_assumptions,
                    );
                    let mut else_assumptions = assumptions.to_vec();
                    else_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: false,
                    });
                    let else_rewritten = rewrite_guarded_tail_sequence(
                        else_body,
                        join_label,
                        &else_assumptions,
                    );

                    if then_rewritten.exits_to_join || else_rewritten.exits_to_join {
                        let rest = rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..],
                            join_label,
                            assumptions,
                        );
                        if then_rewritten.exits_to_join && else_rewritten.exits_to_join {
                            out.push(DirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: else_rewritten.stmts,
                            });
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: then_rewritten.unresolved_join_refs
                                    + else_rewritten.unresolved_join_refs
                                    + rest.unresolved_join_refs,
                            };
                        }

                        if then_rewritten.exits_to_join {
                            let mut continue_body = else_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(DirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: continue_body,
                            });
                        } else {
                            let mut continue_body = then_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(DirStmt::If {
                                cond: cond.clone(),
                                then_body: continue_body,
                                else_body: else_rewritten.stmts,
                            });
                        }
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: false,
                            unresolved_join_refs: then_rewritten.unresolved_join_refs
                                + else_rewritten.unresolved_join_refs
                                + rest.unresolved_join_refs,
                        };
                    }

                    out.push(DirStmt::If {
                        cond: cond.clone(),
                        then_body: then_rewritten.stmts,
                        else_body: else_rewritten.stmts,
                    });
                }
                DirStmt::Goto(target) => {
                    out.push(DirStmt::Goto(target.clone()));
                }
                DirStmt::Block(inner) => {
                    let rewritten =
                        rewrite_guarded_tail_sequence(inner, join_label, assumptions);
                    out.push(DirStmt::Block(rewritten.stmts));
                    if rewritten.exits_to_join {
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: true,
                            unresolved_join_refs: rewritten.unresolved_join_refs,
                        };
                    }
                }
                stmt => out.push(stmt.clone()),
            }
            idx += 1;
        }

        let unresolved_join_refs = out
            .iter()
            .map(|stmt| stmt_contains_goto_label(stmt, join_label))
            .sum();
        GuardedTailRewriteResult {
            stmts: out,
            exits_to_join: false,
            unresolved_join_refs,
        }
    }

pub fn statement_sequence_always_terminates(stmts: &[DirStmt]) -> bool {
    for stmt in stmts {
        if stmt_always_terminates(stmt) {
            return true;
        }
    }
    false
}

pub fn stmt_always_terminates(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Return(_) | DirStmt::Break | DirStmt::Continue => true,
        DirStmt::Block(inner) => statement_sequence_always_terminates(inner),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            statement_sequence_always_terminates(then_body)
                && statement_sequence_always_terminates(else_body)
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .all(|case| statement_sequence_always_terminates(&case.body))
                && statement_sequence_always_terminates(default)
        }
        _ => false,
    }
}

pub fn stmt_contains_goto_label(stmt: &DirStmt, label: &str) -> usize {
        match stmt {
            DirStmt::Goto(target) => usize::from(target == label),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body
                    .iter()
                    .map(|stmt| stmt_contains_goto_label(stmt, label))
                    .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| stmt_contains_goto_label(stmt, label))
                        .sum::<usize>()
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => body
                .iter()
                .map(|stmt| stmt_contains_goto_label(stmt, label))
                .sum(),
            DirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| stmt_contains_goto_label(stmt, label))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| stmt_contains_goto_label(stmt, label))
                        .sum::<usize>()
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => 0,
        }
    }

pub fn stmt_is_alias_forward_safe(stmt: &DirStmt, label: &str, next_label: &str) -> bool {
        if is_ignorable_discovery_stmt(stmt)
            || stmt_is_pure_value_expr(stmt)
            || stmt_is_pure_value_assign(stmt)
        {
            return true;
        }

        match stmt {
            DirStmt::Goto(target) => target == next_label || target == label,
            DirStmt::Block(body) => body
                .iter()
                .all(|stmt| stmt_is_alias_forward_safe(stmt, label, next_label)),
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                expr_is_pure_value(cond)
                    && then_body
                        .iter()
                        .all(|stmt| stmt_is_alias_forward_safe(stmt, label, next_label))
                    && else_body
                        .iter()
                        .all(|stmt| stmt_is_alias_forward_safe(stmt, label, next_label))
            }
            _ => false,
        }
    }

pub fn stmt_is_guard_cluster_trivial_gap(stmt: &DirStmt, full_body: &[DirStmt]) -> bool {
        if matches!(stmt, DirStmt::Label(_)) {
            return false;
        }
        is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, DirStmt::Block(body) if body.is_empty())
            || stmt_is_sink_safe_return_goto(stmt, full_body)
    }

pub fn stmt_is_guard_prefix_safe(stmt: &DirStmt) -> bool {
        is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, DirStmt::Label(_))
            || matches!(stmt, DirStmt::Block(body) if body.is_empty())
            || top_level_guard_goto_signature(stmt).is_some()
    }

pub fn stmt_is_pure_nested_single_branch_goto_to_label(stmt: &DirStmt, label: &str) -> bool {
        let DirStmt::If {
            then_body,
            else_body,
            ..
        } = stmt
        else {
            return false;
        };

        let then_target = single_goto_target(then_body);
        let else_target = single_goto_target(else_body);
        matches!(then_target, Some(target) if target == label) && else_body.is_empty()
            || matches!(else_target, Some(target) if target == label) && then_body.is_empty()
    }

pub fn stmt_is_pure_value_assign(stmt: &DirStmt) -> bool {
        matches!(
            stmt,
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs,
            } if expr_is_pure_value(rhs) && !suffix_expr_contains_call(rhs)
        )
    }

pub fn stmt_is_pure_value_expr(stmt: &DirStmt) -> bool {
        matches!(
            stmt,
            DirStmt::Expr(expr)
                if expr_is_pure_value(expr) && !suffix_expr_contains_call(expr)
        )
    }

pub fn stmt_is_sink_equivalent_after_label_gap(
        stmt: &DirStmt,
        full_body: &[DirStmt],
        sink_return: &Option<DirExpr>,
    ) -> bool {
        if is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, DirStmt::Block(body) if body.is_empty())
        {
            return true;
        }
        let DirStmt::Goto(target) = stmt else {
            return false;
        };
        if top_level_label_definition_count(full_body, target) != 1 {
            return false;
        }
        matches!(
            resolve_terminal_tail_exit_stmt(full_body, target),
            Some(DirStmt::Return(ret)) if ret == *sink_return
        )
    }

pub fn stmt_is_sink_safe_return_goto(stmt: &DirStmt, full_body: &[DirStmt]) -> bool {
        let DirStmt::Goto(target) = stmt else {
            return false;
        };
        if top_level_label_definition_count(full_body, target) != 1 {
            return false;
        }
        matches!(
            resolve_terminal_tail_exit_stmt(full_body, target),
            Some(DirStmt::Return(_))
        )
    }

pub fn suffix_expr_contains_call(expr: &DirExpr) -> bool {
        match expr {
            DirExpr::Call { .. } => true,
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => suffix_expr_contains_call(expr),
            DirExpr::Binary { lhs, rhs, .. } => {
                suffix_expr_contains_call(lhs) || suffix_expr_contains_call(rhs)
            }
            DirExpr::Load { ptr, .. }
            | DirExpr::PtrOffset { base: ptr, .. }
            | DirExpr::FieldAccess { base: ptr, .. } => suffix_expr_contains_call(ptr),
            DirExpr::Index { base, index, .. } => {
                suffix_expr_contains_call(base) || suffix_expr_contains_call(index)
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                suffix_expr_contains_call(cond)
                    || suffix_expr_contains_call(then_expr)
                    || suffix_expr_contains_call(else_expr)
            }
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => false,
        }
    }

pub fn surviving_label_refs_after_guarded_tail_promotion(
        body: &[DirStmt],
        middle: &[DirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> (usize, usize) {
        let outside_refs: usize = body
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx < if_idx || *idx > label_idx)
            .map(|(_, stmt)| stmt_contains_goto_label(stmt, label))
            .sum();
        let middle_refs: usize = middle
            .iter()
            .map(|stmt| stmt_contains_goto_label(stmt, label))
            .sum();
        (outside_refs, middle_refs)
    }

pub fn terminalizable_join_alias_target(
        body: &[DirStmt],
        label_idx: usize,
    ) -> Option<(String, usize)> {
        let DirStmt::Label(_) = &body[label_idx] else {
            return None;
        };
        let next_label_idx =
            (label_idx + 1..body.len()).find(|pos| matches!(body[*pos], DirStmt::Label(_)))?;
        let DirStmt::Label(next_label) = &body[next_label_idx] else {
            return None;
        };
        let segment = &body[label_idx + 1..next_label_idx];
        if is_trivial_join_forward_segment(segment, next_label)
            || is_trivial_join_forward_or_pure_segment(segment, next_label)
            || segment.iter().all(is_ignorable_discovery_stmt)
        {
            return Some((next_label.clone(), next_label_idx));
        }
        None
    }

pub fn top_level_after_label_ref_is_dead_post_return(
        body: &[DirStmt],
        after_label_pos: usize,
        label: &str,
    ) -> bool {
        let Some(DirStmt::Goto(target)) = body.get(after_label_pos) else {
            return false;
        };
        if target != label {
            return false;
        }

        let mut saw_terminal_return = false;
        for stmt in &body[..after_label_pos] {
            if is_ignorable_discovery_stmt(stmt)
                || matches!(stmt, DirStmt::Block(inner) if inner.is_empty())
            {
                continue;
            }
            match stmt {
                DirStmt::Return(_) => saw_terminal_return = true,
                _ => saw_terminal_return = false,
            }
        }

        saw_terminal_return
    }

pub fn top_level_guard_goto_signature(stmt: &DirStmt) -> Option<(&DirExpr, &str)> {
        let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if !else_body.is_empty() {
            return None;
        }
        match then_body.as_slice() {
            [DirStmt::Goto(label)] => Some((cond, label.as_str())),
            _ => None,
        }
    }

pub fn top_level_label_definition_count(body: &[DirStmt], label: &str) -> usize {
        body.iter()
            .filter(|stmt| matches!(stmt, DirStmt::Label(candidate) if candidate == label))
            .count()
    }

pub fn trailing_middle_fallthrough_equivalent_refs(
        middle: &[DirStmt],
        label: &str,
    ) -> usize {
        let mut trailing = 0usize;
        for stmt in middle.iter().rev() {
            if is_ignorable_discovery_stmt(stmt) {
                continue;
            }
            match stmt {
                DirStmt::Goto(target) if target == label => trailing += 1,
                _ => break,
            }
        }
        trailing
    }


pub fn build_nested_boundary_pair_trace(
        refs: &[NestedBoundaryRefTrace],
    ) -> NestedBoundaryPairTrace {
        let conds = refs
            .iter()
            .filter_map(|entry| entry.cond.clone())
            .collect::<Vec<_>>();
        let pair = refs.len() == 2
            && refs
                .iter()
                .all(|entry| entry.kind == ExternalEntryRefKind::NestedConditionalGoto)
            && conds.len() == 2;
        let (same_guard_family, relation_reason) = if pair {
            let reason = guard_family_match_reason(&conds[0], &conds[1]);
            (
                exprs_share_guard_family(&conds[0], &conds[1]),
                Some(reason),
            )
        } else {
            (false, None)
        };

        NestedBoundaryPairTrace {
            ref_count: refs.len(),
            same_guard_family,
            relation_reason,
            conds,
        }
    }

pub fn classify_external_entry_ref_kind(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> Option<(ExternalEntryRefKind, usize)> {
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if stmt_contains_goto_label(stmt, label) == 0 {
                continue;
            }
            if stmt_idx == anchor_idx {
                continue;
            }
            if stmt_idx > anchor_idx
                && stmt_idx < terminal_label_idx
                && matches!(stmt, DirStmt::Goto(target) if target == label)
            {
                continue;
            }
            return Some((
                classify_external_entry_ref_kind_for_stmt(stmt, label),
                stmt_idx,
            ));
        }
        None
    }

pub fn classify_external_entry_ref_kind_for_stmt(
        stmt: &DirStmt,
        label: &str,
    ) -> ExternalEntryRefKind {
        match stmt {
            DirStmt::Goto(target) if target == label => ExternalEntryRefKind::TopLevelExternalGoto,
            DirStmt::If { .. } if stmt_contains_goto_label(stmt, label) > 0 => {
                ExternalEntryRefKind::NestedConditionalGoto
            }
            DirStmt::Switch { .. }
            | DirStmt::While { .. }
            | DirStmt::DoWhile { .. }
            | DirStmt::For { .. }
                if stmt_contains_goto_label(stmt, label) > 0 =>
            {
                ExternalEntryRefKind::LoopSwitchDerived
            }
            DirStmt::Block(_) if stmt_contains_goto_label(stmt, label) > 0 => {
                ExternalEntryRefKind::AliasRedirectDerived
            }
            _ => ExternalEntryRefKind::UnknownExternalEntry,
        }
    }

pub fn collect_nested_boundary_ref_traces(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> Vec<NestedBoundaryRefTrace> {
        let mut refs = Vec::new();
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if stmt_contains_goto_label(stmt, label) == 0 {
                continue;
            }
            if stmt_idx > anchor_idx
                && stmt_idx < terminal_label_idx
                && matches!(stmt, DirStmt::Goto(target) if target == label)
            {
                continue;
            }
            refs.push(NestedBoundaryRefTrace {
                stmt_idx,
                kind: classify_external_entry_ref_kind_for_stmt(stmt, label),
                cond: stmt_is_single_branch_if_to_label(stmt, label).cloned(),
            });
        }
        refs
    }

pub fn count_candidate_internal_top_level_refs_in_suffix_window(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= terminal_label_idx {
            return 0;
        }
        body[anchor_idx + 1..terminal_label_idx]
            .iter()
            .filter(|stmt| matches!(stmt, DirStmt::Goto(target) if target == label))
            .count()
    }

pub fn count_internalized_guard_family_nested_conditional_entries(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= current_label_idx {
            return 0;
        }

        let mut count = 0usize;
        for stmt_idx in anchor_idx + 1..current_label_idx {
            let internalized = nested_conditional_entry_is_guard_family_internal(
                body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
                stmt_idx,
            );
            if guarded_tail_diag_enabled()
                && let Some(cond) = body
                    .get(stmt_idx)
                    .and_then(|stmt| stmt_is_single_goto_then_if_to_label(stmt, label))
            {
                eprintln!(
                    "[GT-TRACE] nested-entry-probe label={} cond={:?} ref_stmt_idx={} internalized={}",
                    label, cond, stmt_idx, internalized
                );
            }
            if !internalized {
                continue;
            }
            count += 1;
            if guarded_tail_diag_enabled()
                && let Some(cond) = body
                    .get(stmt_idx)
                    .and_then(|stmt| stmt_is_single_goto_then_if_to_label(stmt, label))
            {
                eprintln!(
                    "[GT-TRACE] nested-entry-internalized label={} cond={:?} ref_stmt_idx={}",
                    label, cond, stmt_idx
                );
            }
        }
        count
    }

pub fn count_internalized_paired_nested_boundary_refs(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        raw_refs: usize,
    ) -> usize {
        if raw_refs != 2 {
            return 0;
        }
        let label_idx = body
            .iter()
            .position(|stmt| matches!(stmt, DirStmt::Label(candidate) if candidate == label));
        if !label_idx.is_some_and(|idx| idx >= current_label_idx && idx < terminal_label_idx) {
            return 0;
        }
        if count_internalized_guard_family_nested_conditional_entries(
            body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
        ) > 0
        {
            return 0;
        }

        let refs =
            collect_nested_boundary_ref_traces(body, label, anchor_idx, terminal_label_idx);
        let pair_trace = build_nested_boundary_pair_trace(&refs);
        if pair_trace.ref_count != 2
            || !pair_trace.same_guard_family
            || pair_trace.relation_reason != Some("ExactExpr")
            || !refs
                .iter()
                .all(|entry| entry.kind == ExternalEntryRefKind::NestedConditionalGoto)
        {
            return 0;
        }

        if guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] paired-nested-boundary-internalized label={} refs={:?} relation={}",
                label,
                refs.iter().map(|entry| entry.stmt_idx).collect::<Vec<_>>(),
                pair_trace.relation_reason.unwrap_or("Unknown"),
            );
        }
        2
    }

pub fn count_suffix_safe_self_terminal_refs_in_suffix_window(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= terminal_label_idx {
            return 0;
        }

        let mut count = 0usize;
        for stmt_idx in anchor_idx + 1..terminal_label_idx {
            if !matches!(body.get(stmt_idx), Some(DirStmt::Goto(target)) if target == label) {
                continue;
            }
            let Some(next_label_idx) =
                (stmt_idx + 1..body.len()).find(|pos| matches!(body[*pos], DirStmt::Label(_)))
            else {
                continue;
            };
            if next_label_idx > terminal_label_idx {
                continue;
            }
            if suffix_stmt_is_terminal_join_owned_safe(body, stmt_idx, next_label_idx, label)
            {
                count += 1;
            }
        }
        count
    }

pub fn exprs_share_guard_family(lhs: &DirExpr, rhs: &DirExpr) -> bool {
        if lhs == rhs {
            return true;
        }
        if let DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } = lhs
            && expr.as_ref() == rhs
        {
            return true;
        }
        if let DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } = rhs
            && expr.as_ref() == lhs
        {
            return true;
        }
        false
    }

pub fn find_terminal_guard_family_match_excluding(
        body: &[DirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &DirExpr,
        excluded_stmt_idx: Option<usize>,
    ) -> Option<DirExpr> {
        let Some(DirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return None;
        };
        if current_label_idx + 1 >= terminal_label_idx {
            return None;
        }
        if guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] guard-family-match-scan entry_cond={:?} terminal_label={} excluded_stmt_idx={:?}",
                entry_cond, terminal_label, excluded_stmt_idx
            );
        }

        let mut candidate_count = 0usize;
        for (offset, stmt) in body[current_label_idx + 1..terminal_label_idx]
            .iter()
            .enumerate()
        {
            let absolute_idx = current_label_idx + 1 + offset;
            if excluded_stmt_idx == Some(absolute_idx) {
                continue;
            }
            let Some(suffix_cond) = stmt_is_single_branch_if_to_label(stmt, terminal_label)
            else {
                continue;
            };
            candidate_count += 1;
            let shares = exprs_share_guard_family(entry_cond, suffix_cond);
            let reason = guard_family_match_reason(entry_cond, suffix_cond);
            if guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] guard-family-match-candidate stmt_idx={} suffix_cond={:?} shares={} reason={}",
                    absolute_idx, suffix_cond, shares, reason
                );
            }
            if shares {
                return Some(suffix_cond.clone());
            }
        }

        if guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] guard-family-match-miss entry_cond={:?} terminal_label={} candidate_count={}",
                entry_cond, terminal_label, candidate_count
            );
        }
        None
    }

pub fn guard_family_match_reason(lhs: &DirExpr, rhs: &DirExpr) -> &'static str {
        if lhs == rhs {
            return "ExactExpr";
        }
        match lhs {
            DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr,
                ..
            } if expr.as_ref() == rhs => "EntryNegatesCandidate",
            _ => match rhs {
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr,
                    ..
                } if expr.as_ref() == lhs => "CandidateNegatesEntry",
                _ => "NoGuardFamilyRelation",
            },
        }
    }

pub fn nested_conditional_entry_is_guard_family_internal(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        stmt_idx: usize,
    ) -> bool {
        if stmt_idx <= anchor_idx || stmt_idx >= current_label_idx {
            return false;
        }
        let Some(stmt) = body.get(stmt_idx) else {
            return false;
        };
        let Some(entry_cond) = stmt_is_single_goto_then_if_to_label(stmt, label) else {
            return false;
        };
        let matched_cond = find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            None,
        );
        let result = matched_cond.is_some();
        if guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] nested-entry-guard-family-proof label={} ref_stmt_idx={} entry_cond={:?} matched_cond={:?} result={}",
                label, stmt_idx, entry_cond, matched_cond, result
            );
            if !result {
                let boundary = nested_entry_boundary_context(
                    body,
                    label,
                    anchor_idx,
                    current_label_idx,
                    terminal_label_idx,
                );
                eprintln!(
                    "[GT-TRACE] nested-entry-boundary label={} label_idx={:?} in_current_suffix_window={} raw_refs={} internal_candidate_refs={} suffix_safe_refs={} external_pre_guard_internalization={} external_entry_kind={:?} external_ref_stmt_idx={:?}",
                    label,
                    boundary.label_idx,
                    boundary.label_in_current_suffix_window,
                    boundary.raw_refs,
                    boundary.internal_candidate_refs,
                    boundary.suffix_safe_refs,
                    boundary.external_pre_guard_internalization,
                    boundary.external_entry_kind,
                    boundary.external_entry_ref_stmt_idx,
                );
                let boundary_refs = collect_nested_boundary_ref_traces(
                    body,
                    label,
                    anchor_idx,
                    terminal_label_idx,
                );
                for boundary_ref in &boundary_refs {
                    if let Some(stmt) = body.get(boundary_ref.stmt_idx) {
                        eprintln!(
                            "[GT-TRACE] nested-boundary-ref label={} ref_idx={} kind={:?} cond={:?} stmt={:?}",
                            label,
                            boundary_ref.stmt_idx,
                            boundary_ref.kind,
                            boundary_ref.cond,
                            stmt
                        );
                    }
                }
                let pair_trace = build_nested_boundary_pair_trace(&boundary_refs);
                eprintln!(
                    "[GT-TRACE] nested-boundary-pair label={} count={} same_guard_family={} relation_reason={:?} conds={:?}",
                    label,
                    pair_trace.ref_count,
                    pair_trace.same_guard_family,
                    pair_trace.relation_reason,
                    pair_trace.conds,
                );
            }
        }
        result
    }

pub fn nested_entry_boundary_context(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> NestedEntryBoundaryContext {
        let referenced = collect_referenced_label_counts(body);
        let raw_refs = referenced.get(label).copied().unwrap_or(0);
        let label_idx = body
            .iter()
            .position(|stmt| matches!(stmt, DirStmt::Label(candidate) if candidate == label));
        let label_in_current_suffix_window =
            label_idx.is_some_and(|idx| idx >= current_label_idx && idx < terminal_label_idx);
        let internal_candidate_refs =
            count_candidate_internal_top_level_refs_in_suffix_window(
                body,
                label,
                anchor_idx,
                terminal_label_idx,
            );
        let suffix_safe_refs = count_suffix_safe_self_terminal_refs_in_suffix_window(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        )
        .min(internal_candidate_refs);
        let (external_entry_kind, external_entry_ref_stmt_idx) =
            match classify_external_entry_ref_kind(
                body,
                label,
                anchor_idx,
                terminal_label_idx,
            ) {
                Some((kind, stmt_idx)) => (Some(kind), Some(stmt_idx)),
                None => (None, None),
            };

        NestedEntryBoundaryContext {
            label_idx,
            label_in_current_suffix_window,
            raw_refs,
            internal_candidate_refs,
            suffix_safe_refs,
            external_pre_guard_internalization: raw_refs.saturating_sub(internal_candidate_refs),
            external_entry_kind,
            external_entry_ref_stmt_idx,
        }
    }

pub fn stmt_is_single_branch_if_to_label<'b>(
        stmt: &'b DirStmt,
        label: &str,
    ) -> Option<&'b DirExpr> {
        let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if matches!(single_goto_target(then_body), Some(target) if target == label)
            && else_body.is_empty()
        {
            return Some(cond);
        }
        if matches!(single_goto_target(else_body), Some(target) if target == label)
            && then_body.is_empty()
        {
            return Some(cond);
        }
        None
    }

pub fn stmt_is_single_goto_then_if_to_label<'b>(
        stmt: &'b DirStmt,
        label: &str,
    ) -> Option<&'b DirExpr> {
        let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if !else_body.is_empty() {
            return None;
        }
        matches!(single_goto_target(then_body), Some(target) if target == label).then_some(cond)
    }

pub fn suffix_stmt_is_terminal_join_owned_safe(
        body: &[DirStmt],
        stmt_idx: usize,
        next_label_idx: usize,
        terminal_label: &str,
    ) -> bool {
        let DirStmt::Goto(target) = &body[stmt_idx] else {
            return false;
        };
        if target != terminal_label {
            return false;
        }
        if top_level_label_definition_count_for_owned_tail(body, terminal_label) != 1 {
            return false;
        }

        for trailing_stmt in &body[stmt_idx + 1..next_label_idx] {
            if is_ignorable_discovery_stmt(trailing_stmt)
                || matches!(trailing_stmt, DirStmt::Block(inner) if inner.is_empty())
                || stmt_is_pure_value_expr(trailing_stmt)
                || stmt_is_pure_value_assign(trailing_stmt)
            {
                continue;
            }

            match trailing_stmt {
                DirStmt::Goto(target) if target == terminal_label => continue,
                DirStmt::Break
                | DirStmt::Continue
                | DirStmt::Switch { .. }
                | DirStmt::While { .. }
                | DirStmt::DoWhile { .. }
                | DirStmt::For { .. }
                | DirStmt::If { .. }
                | DirStmt::Block(_)
                | DirStmt::VaStart { .. }
                | DirStmt::Assign { .. }
                | DirStmt::Expr(_)
                | DirStmt::Return(_)
                | DirStmt::Label(_) => return false,
                DirStmt::Goto(_) => return false,
            }
        }

        true
    }

pub fn top_level_label_definition_count_for_owned_tail(body: &[DirStmt], label: &str) -> usize {
        body.iter()
            .filter(|stmt| matches!(stmt, DirStmt::Label(candidate) if candidate == label))
            .count()
    }

pub fn call_target_is_control_effect(target: &str) -> bool {
        let lowered = target.to_ascii_lowercase();
        matches!(
            lowered.as_str(),
            "abort"
                | "exit"
                | "_exit"
                | "panic"
                | "__assert_fail"
                | "longjmp"
                | "_longjmp"
                | "raiseexception"
                | "__cxa_throw"
        )
    }

pub fn call_target_is_known_pure_helper(target: &str) -> bool {
        guarded_tail_call_target_is_known_pure_helper(target)
    }

pub fn call_target_is_memory_mutating(target: &str) -> bool {
        let lowered = target.to_ascii_lowercase();
        matches!(
            lowered.as_str(),
            "memcpy"
                | "memmove"
                | "memset"
                | "strcpy"
                | "strncpy"
                | "strcat"
                | "strncat"
                | "wcscpy"
                | "wcsncpy"
                | "wmemcpy"
                | "wmemmove"
                | "wmemset"
        )
    }

pub fn candidate_window_can_shrink_to_label(
        body: &[DirStmt],
        anchor_idx: usize,
        candidate_label: &str,
        candidate_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        if candidate_label_idx >= terminal_label_idx {
            return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: candidate_label_idx,
                label: candidate_label.to_string(),
            });
        }
        let suffix_result = suffix_is_nonowned_terminal_tail(
            body,
            anchor_idx,
            candidate_label,
            candidate_label_idx,
            terminal_label_idx,
            referenced,
        );
        if !has_non_ignorable_payload(&body[anchor_idx + 1..candidate_label_idx]) {
            return match suffix_result {
                Err(SuffixTailRejection::SuffixHasExternalEntry { .. }) => suffix_result,
                _ => Err(SuffixTailRejection::SuffixHasLabelCrossing {
                    stmt_idx: candidate_label_idx,
                    label: candidate_label.to_string(),
                }),
            };
        }
        suffix_result
    }

pub fn classify_nested_suffix_shape(
        stmt: &DirStmt,
        body: &[DirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> NestedSuffixShapeKind {
        let Some(DirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return NestedSuffixShapeKind::NestedUnknown;
        };
        match stmt {
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let then_target = single_goto_target(then_body);
                let else_target = single_goto_target(else_body);
                if then_body.len() > 1 || else_body.len() > 1 {
                    return NestedSuffixShapeKind::NestedMultiStmtBranch;
                }
                if then_target.is_some() && else_target.is_some() {
                    return NestedSuffixShapeKind::NestedBothBranches;
                }
                if let Some(target) = then_target {
                    if target == terminal_label {
                        return NestedSuffixShapeKind::NestedCrossesTerminalJoin;
                    }
                    if target != next_label {
                        return NestedSuffixShapeKind::NestedNonlocalTarget;
                    }
                    if !suffix_window_has_terminal_guard_family_match(
                        body,
                        current_label_idx,
                        terminal_label_idx,
                        cond,
                    ) {
                        return NestedSuffixShapeKind::NestedGuardFamilyMismatch;
                    }
                    return NestedSuffixShapeKind::NestedSingleGotoThen;
                }
                if let Some(target) = else_target {
                    if target == terminal_label {
                        return NestedSuffixShapeKind::NestedCrossesTerminalJoin;
                    }
                    if target != next_label {
                        return NestedSuffixShapeKind::NestedNonlocalTarget;
                    }
                    if !suffix_window_has_terminal_guard_family_match(
                        body,
                        current_label_idx,
                        terminal_label_idx,
                        cond,
                    ) {
                        return NestedSuffixShapeKind::NestedGuardFamilyMismatch;
                    }
                    return NestedSuffixShapeKind::NestedSingleGotoElse;
                }
                NestedSuffixShapeKind::NestedUnknown
            }
            DirStmt::Block(inner) if !inner.is_empty() => NestedSuffixShapeKind::NestedUnknown,
            _ => NestedSuffixShapeKind::NestedUnknown,
        }
    }

pub fn classify_suffix_call_effect_shape(stmt: &DirStmt) -> SuffixCallEffectShapeKind {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs: DirExpr::Call { target, args, .. },
            }
            | DirStmt::Expr(DirExpr::Call { target, args, .. }) => {
                if call_target_is_control_effect(target) {
                    return SuffixCallEffectShapeKind::ControlEffectCall;
                }
                if call_target_is_memory_mutating(target) {
                    return SuffixCallEffectShapeKind::MemoryMutatingCall;
                }
                if call_target_is_known_pure_helper(target)
                    && args.iter().all(expr_is_pure_value)
                {
                    return SuffixCallEffectShapeKind::PureKnownHelperCall;
                }
                match stmt {
                    DirStmt::Assign { .. } => SuffixCallEffectShapeKind::ReturnValueAssignedLocal,
                    DirStmt::Expr(DirExpr::Call { ty, .. }) if matches!(ty, NirType::Unknown) => {
                        SuffixCallEffectShapeKind::VoidUnknownCall
                    }
                    DirStmt::Expr(DirExpr::Call { .. }) => {
                        SuffixCallEffectShapeKind::ReturnValueIgnoredCall
                    }
                    _ => SuffixCallEffectShapeKind::UnknownCallEffect,
                }
            }
            DirStmt::Assign { .. } | DirStmt::Expr(_) | DirStmt::VaStart { .. } => {
                SuffixCallEffectShapeKind::UnknownCallEffect
            }
            _ => SuffixCallEffectShapeKind::UnknownCallEffect,
        }
    }

pub fn classify_suffix_side_effect_shape(stmt: &DirStmt) -> SuffixSideEffectShapeKind {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Deref { .. } | DirLValue::Index { .. },
                ..
            } => SuffixSideEffectShapeKind::MemoryWrite,
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs,
            } if suffix_expr_contains_call(rhs) => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs,
            } if expr_is_pure_value(rhs) => match rhs {
                DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => {
                    SuffixSideEffectShapeKind::PureTempAssign
                }
                _ => SuffixSideEffectShapeKind::PureRegisterAssign,
            },
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs: DirExpr::Load { ptr, .. },
            } if expr_is_pure_value(ptr) => SuffixSideEffectShapeKind::MemoryReadOnlyAssign,
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs,
            } if expr_contains_load(rhs) => SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if expr_contains_var(rhs, name)
                || matches!(rhs, DirExpr::AggregateCopy { .. }) =>
            {
                SuffixSideEffectShapeKind::CompoundAssignOrPhiLike
            }
            DirStmt::Expr(DirExpr::Call { .. }) | DirStmt::VaStart { .. } => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            DirStmt::Expr(DirExpr::Load { .. }) => SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
            DirStmt::Expr(expr) if suffix_expr_contains_call(expr) => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            DirStmt::Expr(expr) if expr_contains_load(expr) => {
                SuffixSideEffectShapeKind::VolatileOrUnknownLoad
            }
            DirStmt::Assign { .. } => SuffixSideEffectShapeKind::UnknownSideEffect,
            _ => SuffixSideEffectShapeKind::UnknownSideEffect,
        }
    }

pub fn classify_suffix_stmt(
        stmt: &DirStmt,
        body: &[DirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        if is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, DirStmt::Block(inner) if inner.is_empty())
        {
            return Ok(());
        }
        if stmt_is_pure_value_expr(stmt) || stmt_is_pure_value_assign(stmt) {
            return Ok(());
        }
        if let DirStmt::Goto(target) = stmt {
            if target == next_label
                || stmt_is_sink_safe_return_goto_for_owned_tail(stmt, body)
            {
                return Ok(());
            }
            let next_stmt_label_idx = (stmt_idx + 1..body.len())
                .find(|pos| matches!(body[*pos], DirStmt::Label(_)))
                .unwrap_or(body.len());
            for trailing_idx in stmt_idx + 1..next_stmt_label_idx {
                let trailing = &body[trailing_idx];
                if is_ignorable_discovery_stmt(trailing)
                    || matches!(trailing, DirStmt::Block(inner) if inner.is_empty())
                {
                    continue;
                }
                if suffix_stmt_has_nested_or_nonlocal_ref(trailing) {
                    return Err(SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx });
                }
                if !stmt_is_pure_value_expr(trailing)
                    && !stmt_is_pure_value_assign(trailing)
                    && !matches!(trailing, DirStmt::Goto(target) if target == next_label)
                {
                    return Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx });
                }
            }
            let label_count = top_level_label_definition_count_for_owned_tail(body, target);
            if label_count == 0 {
                let terminal_label = body
                    .get(terminal_label_idx)
                    .and_then(|stmt| match stmt {
                        DirStmt::Label(label) => Some(label.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                return Err(if next_label == terminal_label {
                    SuffixTailRejection::SuffixAliasRedirectUnresolved {
                        stmt_idx,
                        label: target.clone(),
                    }
                } else {
                    SuffixTailRejection::SuffixHasNonTerminalGoto {
                        stmt_idx,
                        target: target.clone(),
                    }
                });
            }
            if label_count != 1 {
                return Err(SuffixTailRejection::SuffixAliasRedirectUnresolved {
                    stmt_idx,
                    label: target.clone(),
                });
            }
            if resolve_suffix_redirect_to_terminal(body, target, next_label) {
                return Ok(());
            }
            return Err(SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx,
                target: target.clone(),
            });
        }
        if matches!(
            stmt,
            DirStmt::Switch { .. }
                | DirStmt::While { .. }
                | DirStmt::DoWhile { .. }
                | DirStmt::For { .. }
                | DirStmt::Break
                | DirStmt::Continue
        ) {
            return Err(SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx });
        }
        if suffix_stmt_has_nested_or_nonlocal_ref(stmt) {
            let kind = classify_nested_suffix_shape(
                stmt,
                body,
                current_label_idx,
                terminal_label_idx,
                next_label,
            );
            if kind == NestedSuffixShapeKind::NestedCrossesTerminalJoin
                && nested_terminal_join_tail_is_guard_family_owned_safe(
                    body,
                    stmt_idx,
                    current_label_idx,
                    terminal_label_idx,
                )
            {
                if guarded_tail_diag_enabled() {
                    eprintln!(
                        "[GT-TRACE] nested-terminal-join-tail-internalized stmt_idx={} kind={:?} stmt={:?}",
                        stmt_idx, kind, stmt
                    );
                }
                return Ok(());
            }
            if guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] nested-suffix-shape stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, kind, stmt
                );
            }
            return Err(SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx });
        }
        let side_effect_kind = classify_suffix_side_effect_shape(stmt);
        if side_effect_kind == SuffixSideEffectShapeKind::MemoryReadOnlyAssign
            && suffix_memory_read_only_assign_is_owned_safe(
                body,
                stmt_idx,
                terminal_label_idx,
            )
        {
            if guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] suffix-memory-readonly-assign-internalized stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, side_effect_kind, stmt
                );
            }
            return Ok(());
        }
        if side_effect_kind == SuffixSideEffectShapeKind::CallExprSideEffect {
            let call_kind = classify_suffix_call_effect_shape(stmt);
            if call_kind == SuffixCallEffectShapeKind::PureKnownHelperCall
                && suffix_known_pure_helper_call_is_owned_safe(
                    body,
                    stmt_idx,
                    terminal_label_idx,
                )
            {
                if guarded_tail_diag_enabled() {
                    eprintln!(
                        "[GT-TRACE] suffix-known-pure-helper-call-internalized stmt_idx={} kind={:?} stmt={:?}",
                        stmt_idx, call_kind, stmt
                    );
                }
                return Ok(());
            }
        }
        if guarded_tail_diag_enabled() {
            if side_effect_kind == SuffixSideEffectShapeKind::CallExprSideEffect {
                let call_kind = classify_suffix_call_effect_shape(stmt);
                eprintln!(
                    "[GT-TRACE] suffix-call-effect-shape stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, call_kind, stmt
                );
                if matches!(
                    call_kind,
                    SuffixCallEffectShapeKind::VoidUnknownCall
                        | SuffixCallEffectShapeKind::ReturnValueAssignedLocal
                        | SuffixCallEffectShapeKind::ReturnValueIgnoredCall
                        | SuffixCallEffectShapeKind::UnknownCallEffect
                ) {}
            }
            eprintln!(
                "[GT-TRACE] suffix-side-effect-shape stmt_idx={} kind={:?} stmt={:?}",
                stmt_idx, side_effect_kind, stmt
            );
        }
        Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx })
    }

pub fn compute_suffix_external_entry_budget(
        body: &[DirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        raw_refs: usize,
        rewrites: usize,
    ) -> SuffixExternalEntryBudget {
        let internal_candidate_refs =
            count_candidate_internal_top_level_refs_in_suffix_window(
                body,
                label,
                anchor_idx,
                terminal_label_idx,
            );
        let suffix_safe_refs = count_suffix_safe_self_terminal_refs_in_suffix_window(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        )
        .min(internal_candidate_refs);
        let guard_family_internalized_refs =
            count_internalized_guard_family_nested_conditional_entries(
                body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
            );
        let paired_nested_boundary_refs = count_internalized_paired_nested_boundary_refs(
            body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
        );
        let internal_top_level_refs = internal_candidate_refs.saturating_sub(suffix_safe_refs);
        let effective_external_refs = raw_refs
            .saturating_sub(internal_top_level_refs)
            .saturating_sub(suffix_safe_refs);
        let effective_external_refs =
            effective_external_refs.saturating_sub(guard_family_internalized_refs);
        let effective_external_refs =
            effective_external_refs.saturating_sub(paired_nested_boundary_refs);
        let allowed_external_refs = usize::from(rewrites == 0);

        SuffixExternalEntryBudget {
            raw_refs,
            internal_top_level_refs,
            suffix_safe_refs,
            guard_family_internalized_refs,
            paired_nested_boundary_refs,
            effective_external_refs,
            allowed_external_refs,
        }
    }

pub fn expr_contains_load(expr: &DirExpr) -> bool {
        match expr {
            DirExpr::Load { .. } => true,
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => expr_contains_load(expr),
            DirExpr::Binary { lhs, rhs, .. } => {
                expr_contains_load(lhs) || expr_contains_load(rhs)
            }
            DirExpr::Call { args, .. } => args.iter().any(expr_contains_load),
            DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
                expr_contains_load(base)
            }
            DirExpr::Index { base, index, .. } => {
                expr_contains_load(base) || expr_contains_load(index)
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                expr_contains_load(cond)
                    || expr_contains_load(then_expr)
                    || expr_contains_load(else_expr)
            }
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => false,
        }
    }

pub fn find_earliest_owned_join_label(
        body: &[DirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        if anchor_idx + 1 >= terminal_label_idx {
            return None;
        }

        for candidate_label_idx in anchor_idx + 1..terminal_label_idx {
            let DirStmt::Label(candidate_label) = &body[candidate_label_idx] else {
                continue;
            };
            let has_payload = has_non_ignorable_payload(&body[anchor_idx + 1..candidate_label_idx]);
            let suffix_result = candidate_window_can_shrink_to_label(
                body,
                anchor_idx,
                candidate_label,
                candidate_label_idx,
                terminal_label_idx,
                referenced,
            );
            let suffix_safe = suffix_result.is_ok();
            if guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] owned-join candidate anchor={} label={} label_idx={} terminal_idx={} payload_before={} suffix_safe={}",
                    anchor_idx,
                    candidate_label,
                    candidate_label_idx,
                    terminal_label_idx,
                    has_payload,
                    suffix_safe
                );
            }
            if trace_enabled
                && anchor_idx == 35
                && let Err(reason) = &suffix_result
                && let Some(stmt) = body.get(reason.stmt_idx())
            {
                eprintln!(
                    "[GT-TRACE] candidate={} join_label={} early_label={} first_fail={:?} stmt_idx={} first_fail_stmt={:?}",
                    anchor_idx,
                    match body.get(terminal_label_idx) {
                        Some(DirStmt::Label(label)) => label.as_str(),
                        _ => "<missing-terminal-label>",
                    },
                    candidate_label,
                    reason,
                    reason.stmt_idx(),
                    stmt
                );
            }
            if has_payload && suffix_safe {
                return Some((candidate_label.clone(), candidate_label_idx));
            }
        }

        None
    }

pub fn nested_terminal_join_tail_is_guard_family_owned_safe(
        body: &[DirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(DirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return false;
        };
        let Some(stmt) = body.get(stmt_idx) else {
            return false;
        };
        let Some(entry_cond) = stmt_is_single_branch_if_to_label(stmt, terminal_label) else {
            return false;
        };
        let matched_cond = find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            Some(stmt_idx),
        );
        let result = matched_cond.is_some();
        if guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] nested-terminal-join-proof stmt_idx={} terminal_label={} entry_cond={:?} matched_cond={:?} result={}",
                stmt_idx, terminal_label, entry_cond, matched_cond, result
            );
        }
        result
    }

pub fn resolve_suffix_redirect_to_terminal(
        body: &[DirStmt],
        target_label: &str,
        next_label: &str,
    ) -> bool {
        if target_label == next_label {
            return true;
        }
        if top_level_label_definition_count_for_owned_tail(body, target_label) != 1 {
            return false;
        }
        let Some(mut current_idx) = body
            .iter()
            .position(|stmt| matches!(stmt, DirStmt::Label(label) if label == target_label))
        else {
            return false;
        };
        let mut current = target_label.to_string();
        let mut seen = HashSet::default();

        while current != next_label {
            if !seen.insert(current.clone()) {
                return false;
            }

            let next_label_idx = (current_idx + 1..body.len())
                .find(|pos| matches!(body[*pos], DirStmt::Label(_)))
                .unwrap_or(body.len());

            let mut terminal_return = false;
            let mut terminal_goto = None::<String>;
            for stmt in &body[current_idx + 1..next_label_idx] {
                match stmt {
                    DirStmt::Goto(target) => terminal_goto = Some(target.clone()),
                    DirStmt::Return(_) => terminal_return = true,
                    stmt if is_ignorable_discovery_stmt(stmt) => {}
                    stmt if stmt_is_pure_value_expr(stmt) => {}
                    stmt if stmt_is_pure_value_assign(stmt) => {}
                    DirStmt::Block(inner) if inner.is_empty() => {}
                    _ => return false,
                }
            }

            if terminal_return {
                return true;
            }
            let Some(next_target) = terminal_goto else {
                return false;
            };
            if next_target == next_label {
                return true;
            }
            let Some(next_idx) = body
                .iter()
                .position(|stmt| matches!(stmt, DirStmt::Label(label) if label == &next_target))
            else {
                return false;
            };
            current = next_target;
            current_idx = next_idx;
        }

        true
    }

pub fn stmt_is_sink_safe_return_goto_for_owned_tail(stmt: &DirStmt, body: &[DirStmt]) -> bool {
        let DirStmt::Goto(target) = stmt else {
            return false;
        };
        if top_level_label_definition_count_for_owned_tail(body, target) != 1 {
            return false;
        }
        matches!(
            resolve_terminal_tail_exit_stmt(body, target),
            Some(DirStmt::Return(_))
        )
    }

pub fn stmt_reads_binding_only_in_owned_safe_context(stmt: &DirStmt, name: &str) -> bool {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                if lvalue_contains_var(lhs, name) {
                    return false;
                }
                !expr_contains_var(rhs, name) || expr_is_pure_value(rhs)
            }
            DirStmt::Expr(expr) => {
                !expr_contains_var(expr, name) || expr_is_pure_value(expr)
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                (!expr_contains_var(cond, name) || expr_is_pure_value(cond))
                    && then_body
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && else_body
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            DirStmt::Block(stmts) => stmts
                .iter()
                .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name)),
            DirStmt::VaStart { va_list, .. } => !expr_contains_var(va_list, name),
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                !expr_contains_var(expr, name)
                    && cases.iter().all(|case| {
                        case.body.iter().all(|stmt| {
                            stmt_reads_binding_only_in_owned_safe_context(stmt, name)
                        })
                    })
                    && default
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            DirStmt::While { cond, body } | DirStmt::DoWhile { cond, body } => {
                !expr_contains_var(cond, name)
                    && body
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.iter()
                    .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && cond
                        .as_ref()
                        .is_none_or(|expr| !expr_contains_var(expr, name))
                    && update
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && body
                        .iter()
                        .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            DirStmt::Return(Some(expr)) => !expr_contains_var(expr, name),
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => true,
        }
    }

pub fn suffix_call_expr(stmt: &DirStmt) -> Option<(&str, &[DirExpr], bool)> {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(_),
                rhs: DirExpr::Call { target, args, .. },
            } => Some((target.as_str(), args.as_slice(), true)),
            DirStmt::Expr(DirExpr::Call { target, args, .. }) => {
                Some((target.as_str(), args.as_slice(), false))
            }
            _ => None,
        }
    }

pub fn suffix_is_nonowned_terminal_tail(
        body: &[DirStmt],
        anchor_idx: usize,
        start_label: &str,
        start_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        if start_label_idx >= terminal_label_idx {
            return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: start_label_idx,
                label: start_label.to_string(),
            });
        }

        let mut current_label = start_label.to_string();
        let mut current_label_idx = start_label_idx;
        let mut rewrites = 0usize;
        let mut seen = HashSet::default();

        while current_label_idx < terminal_label_idx {
            if !seen.insert(current_label.clone()) {
                return Err(SuffixTailRejection::SuffixAliasRedirectUnresolved {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            }

            let raw_refs = referenced.get(&current_label).copied().unwrap_or(0);
            let budget = compute_suffix_external_entry_budget(
                body,
                &current_label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
                raw_refs,
                rewrites,
            );
            if guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] suffix-budget label={} raw_refs={} internal_refs={} suffix_safe_refs={} guard_family_internalized_refs={} paired_nested_boundary_refs={} effective_external={} allowed_external={}",
                    current_label,
                    budget.raw_refs,
                    budget.internal_top_level_refs,
                    budget.suffix_safe_refs,
                    budget.guard_family_internalized_refs,
                    budget.paired_nested_boundary_refs,
                    budget.effective_external_refs,
                    budget.allowed_external_refs,
                );
            }
            if budget.effective_external_refs > budget.allowed_external_refs {
                if guarded_tail_diag_enabled()
                    && let Some((kind, ref_stmt_idx)) = classify_external_entry_ref_kind(
                        body,
                        &current_label,
                        anchor_idx,
                        terminal_label_idx,
                    )
                    && let Some(ref_stmt) = body.get(ref_stmt_idx)
                {
                    eprintln!(
                        "[GT-TRACE] suffix-external-entry label={} external_entry_kind={:?} ref_stmt_idx={} ref_stmt={:?}",
                        current_label, kind, ref_stmt_idx, ref_stmt
                    );
                }
                return Err(SuffixTailRejection::SuffixHasExternalEntry {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            }

            let Some(next_label_idx) = (current_label_idx + 1..body.len())
                .find(|pos| matches!(body[*pos], DirStmt::Label(_)))
            else {
                return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            };
            if next_label_idx > terminal_label_idx {
                return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                    stmt_idx: next_label_idx,
                    label: current_label,
                });
            }
            let DirStmt::Label(terminal_label) = &body[terminal_label_idx] else {
                unreachable!();
            };
            let DirStmt::Label(next_label) = &body[next_label_idx] else {
                unreachable!();
            };
            for (offset, stmt) in body[current_label_idx + 1..next_label_idx]
                .iter()
                .enumerate()
            {
                let stmt_idx = current_label_idx + 1 + offset;
                if matches!(stmt, DirStmt::Goto(target) if target == terminal_label)
                    && suffix_stmt_is_terminal_join_owned_safe(
                        body,
                        stmt_idx,
                        next_label_idx,
                        terminal_label,
                    )
                {
                    continue;
                }
                if rewrites == 0
                    && next_label_idx == terminal_label_idx
                    && !is_ignorable_discovery_stmt(stmt)
                    && !matches!(stmt, DirStmt::Block(inner) if inner.is_empty())
                {
                    return Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx });
                }
                classify_suffix_stmt(
                    stmt,
                    body,
                    stmt_idx,
                    current_label_idx,
                    terminal_label_idx,
                    next_label,
                )?;
            }

            current_label = next_label.clone();
            current_label_idx = next_label_idx;
            rewrites += 1;
        }

        Ok(())
    }

pub fn suffix_known_pure_helper_call_is_owned_safe(
        body: &[DirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(DirStmt::Assign {
            lhs: DirLValue::Var(binding_name),
            rhs: DirExpr::Call { target, args, .. },
        }) = body.get(stmt_idx)
        else {
            return false;
        };

        let args_pure = args.iter().all(expr_is_pure_value);
        let target_known_pure = call_target_is_known_pure_helper(target);
        let no_redefine = body[stmt_idx + 1..]
            .iter()
            .map(|stmt| count_var_defs_stmt(stmt, binding_name))
            .sum::<usize>()
            == 0;
        let pre_terminal_owned_safe = body[stmt_idx + 1..terminal_label_idx]
            .iter()
            .all(|stmt| stmt_reads_binding_only_in_owned_safe_context(stmt, binding_name));
        let no_terminal_escape = body[terminal_label_idx..]
            .iter()
            .all(|stmt| count_var_reads_stmt(stmt, binding_name) == 0);
        let result = target_known_pure
            && args_pure
            && no_redefine
            && pre_terminal_owned_safe
            && no_terminal_escape;

        if guarded_tail_diag_enabled() && target_known_pure && args_pure {
            eprintln!(
                "[GT-TRACE] known-pure-helper-proof stmt_idx={} target={} args_pure={} no_redefine={} pre_terminal_owned_safe={} no_terminal_escape={} result={}",
                stmt_idx,
                target,
                args_pure,
                no_redefine,
                pre_terminal_owned_safe,
                no_terminal_escape,
                result
            );
        }

        result
    }

pub fn suffix_memory_read_only_assign_is_owned_safe(
        body: &[DirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(DirStmt::Assign {
            lhs: DirLValue::Var(binding_name),
            rhs: DirExpr::Load { ptr, ty },
        }) = body.get(stmt_idx)
        else {
            return false;
        };

        if !expr_is_pure_value(ptr) || matches!(ty, NirType::Unknown) {
            return false;
        }

        if body[stmt_idx + 1..]
            .iter()
            .map(|stmt| count_var_defs_stmt(stmt, binding_name))
            .sum::<usize>()
            > 0
        {
            return false;
        }

        if body[stmt_idx + 1..terminal_label_idx]
            .iter()
            .any(|stmt| !stmt_reads_binding_only_in_owned_safe_context(stmt, binding_name))
        {
            return false;
        }

        body[terminal_label_idx..]
            .iter()
            .all(|stmt| count_var_reads_stmt(stmt, binding_name) == 0)
    }

pub fn suffix_stmt_has_nested_or_nonlocal_ref(stmt: &DirStmt) -> bool {
        match stmt {
            DirStmt::If { .. } => true,
            DirStmt::Block(inner) => !inner.is_empty(),
            _ => false,
        }
    }

pub fn suffix_window_has_terminal_guard_family_match(
        body: &[DirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &DirExpr,
    ) -> bool {
        find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            None,
        )
        .is_some()
    }
