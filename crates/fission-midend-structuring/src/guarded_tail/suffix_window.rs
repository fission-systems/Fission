//! Guarded-tail suffix-window free functions (ADR 0012).
//!
//! Pure helpers: [`super::pure_hir`]. Callee-analysis pure checks live here;
//! host residual only supplies `NirCallEffectSummary` / binary provenance facts.

use super::pure_hir::*;
use super::types::*;
use crate::cleanup::{has_non_ignorable_payload, is_ignorable_discovery_stmt};
use crate::guarded_tail::bodies::StructuringCounter;
use crate::host::StructuringHost;
use fission_midend_core::ir::{
    CallEffectSummarySource, HirStmt, NirCallEffectSummary, parse_call_target_address,
};
use std::collections::{HashMap, HashSet};

/// Pure: does this preview-callee summary make a call unsafe for suffix ownership?
pub fn nir_call_summary_is_preview_unsafe(summary: &NirCallEffectSummary) -> bool {
    summary.source == Some(CallEffectSummarySource::PreviewCalleeAnalysis)
        && (summary.writes_memory == Some(true)
            || summary.may_call_unknown == Some(true)
            || summary.may_exit == Some(true))
}

/// Pure: if `summary` is a preview-unsafe callee for `stmt`'s call, return target name.
pub fn preview_unsafe_callee_target(
    stmt: &HirStmt,
    summary: Option<&NirCallEffectSummary>,
) -> Option<String> {
    let (target, _args, _return_used) = suffix_call_expr(stmt)?;
    let summary = summary?;
    nir_call_summary_is_preview_unsafe(summary).then(|| target.to_string())
}

/// Facts the host gathers for optional GT-TRACE callee provenance dumps.
#[derive(Debug, Clone, Default)]
pub struct SuffixCallProvenanceFacts {
    pub target_addr: Option<u64>,
    pub import: bool,
    pub binary_function_present: bool,
    pub target_ref_present: bool,
    pub target_ref_provenance: String,
    pub effect_summary: Option<NirCallEffectSummary>,
}

/// Pure emit path for unknown-call provenance diagnostics (no host state).
pub fn emit_suffix_unknown_call_provenance_trace(
    stmt_idx: usize,
    stmt: &HirStmt,
    facts: &SuffixCallProvenanceFacts,
) {
    let Some((target, _args, return_used)) = suffix_call_expr(stmt) else {
        return;
    };
    let internal = !facts.import && facts.target_addr.is_some();
    let summary_available = facts.import
        || call_target_is_known_pure_helper(target)
        || call_target_is_memory_mutating(target)
        || call_target_is_control_effect(target)
        || facts.effect_summary.is_some();
    let writes_memory = facts
        .effect_summary
        .as_ref()
        .and_then(|summary| summary.writes_memory)
        .map(|value| if value { "yes" } else { "no" })
        .unwrap_or("unknown");
    let may_call_unknown = facts
        .effect_summary
        .as_ref()
        .and_then(|summary| summary.may_call_unknown)
        .map(|value| if value { "yes" } else { "no" })
        .unwrap_or("unknown");
    let may_exit = facts
        .effect_summary
        .as_ref()
        .and_then(|summary| summary.may_exit)
        .map(|value| if value { "yes" } else { "no" })
        .unwrap_or("unknown");
    let effect_summary_source = facts
        .effect_summary
        .as_ref()
        .and_then(|summary| summary.source)
        .map(|source| format!("{source:?}"))
        .unwrap_or_else(|| "None".to_string());

    eprintln!(
        "[GT-TRACE] suffix-unknown-call-provenance stmt_idx={} target={} target_addr={:?} internal={} import={} summary_available={}",
        stmt_idx, target, facts.target_addr, internal, facts.import, summary_available
    );
    eprintln!(
        "[GT-TRACE] suffix-unknown-call-summary target={} binary_function_present={} target_ref_present={} target_ref_provenance={} effect_summary_source={}",
        target,
        facts.binary_function_present,
        facts.target_ref_present,
        facts.target_ref_provenance,
        effect_summary_source,
    );
    eprintln!(
        "[GT-TRACE] suffix-unknown-call-effect target={} writes_memory={} writes_global=unknown may_call_unknown={} may_exit={} return_used={}",
        target, writes_memory, may_call_unknown, may_exit, return_used
    );
}

/// Host-facing free entry: look up summary via host residual and apply pure check.
pub fn suffix_call_uses_preview_unsafe_callee(
    host: &impl StructuringHost,
    stmt: &HirStmt,
) -> Option<String> {
    let (target, _, _) = suffix_call_expr(stmt)?;
    let summary = host.call_effect_summary_for_target(target);
    preview_unsafe_callee_target(stmt, summary.as_ref())
}

/// Host-facing free entry: gather provenance facts via residual, then pure emit.
pub fn trace_suffix_unknown_call_provenance(
    host: &impl StructuringHost,
    stmt_idx: usize,
    stmt: &HirStmt,
) {
    let Some((target, _, _)) = suffix_call_expr(stmt) else {
        return;
    };
    let target_addr = parse_call_target_address(target);
    let facts = host.suffix_call_provenance_facts(target, target_addr);
    emit_suffix_unknown_call_provenance_trace(stmt_idx, stmt, &facts);
}

pub fn classify_suffix_stmt_with_diag(
    host: &mut impl StructuringHost,
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        if is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, HirStmt::Block(inner) if inner.is_empty())
        {
            return Ok(());
        }
        if stmt_is_pure_value_expr(stmt) || stmt_is_pure_value_assign(stmt) {
            return Ok(());
        }
        if let HirStmt::Goto(target) = stmt {
            if target == next_label
                || stmt_is_sink_safe_return_goto_for_owned_tail(stmt, body)
            {
                return Ok(());
            }
            let next_stmt_label_idx = (stmt_idx + 1..body.len())
                .find(|pos| matches!(body[*pos], HirStmt::Label(_)))
                .unwrap_or(body.len());
            for trailing_idx in stmt_idx + 1..next_stmt_label_idx {
                let trailing = &body[trailing_idx];
                if is_ignorable_discovery_stmt(trailing)
                    || matches!(trailing, HirStmt::Block(inner) if inner.is_empty())
                {
                    continue;
                }
                if suffix_stmt_has_nested_or_nonlocal_ref(trailing) {
                    return Err(SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx });
                }
                if !stmt_is_pure_value_expr(trailing)
                    && !stmt_is_pure_value_assign(trailing)
                    && !matches!(trailing, HirStmt::Goto(target) if target == next_label)
                {
                    return Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx });
                }
            }
            let label_count = top_level_label_definition_count_for_owned_tail(body, target);
            if label_count == 0 {
                let terminal_label = body
                    .get(terminal_label_idx)
                    .and_then(|stmt| match stmt {
                        HirStmt::Label(label) => Some(label.as_str()),
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
            HirStmt::Switch { .. }
                | HirStmt::While { .. }
                | HirStmt::DoWhile { .. }
                | HirStmt::For { .. }
                | HirStmt::Break
                | HirStmt::Continue
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
                ) {
                    if let Some(target) = host.suffix_call_uses_preview_unsafe_callee(stmt) {
                        host.bump_structuring_counter(StructuringCounter::guarded_tail_rejected_side_effectful_callee_count, (1) as usize);
                        eprintln!(
                            "[GT-TRACE] suffix-side-effectful-callee-stop stmt_idx={} target={} source=PreviewCalleeAnalysis",
                            stmt_idx, target
                        );
                        eprintln!(
                            "[GT-TRACE] guarded-tail-rejection subtype=PreviewCalleeAnalysisUnsafe target={}",
                            target
                        );
                    }
                    host.trace_suffix_unknown_call_provenance(stmt_idx, stmt);
                }
            }
            eprintln!(
                "[GT-TRACE] suffix-side-effect-shape stmt_idx={} kind={:?} stmt={:?}",
                stmt_idx, side_effect_kind, stmt
            );
        }
        Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx })
    }

pub fn suffix_is_nonowned_terminal_tail_with_diag(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
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
        let mut seen = HashSet::new();

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
                .find(|pos| matches!(body[*pos], HirStmt::Label(_)))
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
            let HirStmt::Label(terminal_label) = &body[terminal_label_idx] else {
                unreachable!();
            };
            let HirStmt::Label(next_label) = &body[next_label_idx] else {
                unreachable!();
            };
            for (offset, stmt) in body[current_label_idx + 1..next_label_idx]
                .iter()
                .enumerate()
            {
                let stmt_idx = current_label_idx + 1 + offset;
                if matches!(stmt, HirStmt::Goto(target) if target == terminal_label)
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
                    && !matches!(stmt, HirStmt::Block(inner) if inner.is_empty())
                {
                    return Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx });
                }
                classify_suffix_stmt_with_diag(host, 
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

pub fn candidate_window_can_shrink_to_label_with_diag(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
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
        let suffix_result = suffix_is_nonowned_terminal_tail_with_diag(host, 
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

pub fn find_earliest_owned_join_label_with_diag(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        if anchor_idx + 1 >= terminal_label_idx {
            return None;
        }

        for candidate_label_idx in anchor_idx + 1..terminal_label_idx {
            let HirStmt::Label(candidate_label) = &body[candidate_label_idx] else {
                continue;
            };
            let has_payload = has_non_ignorable_payload(&body[anchor_idx + 1..candidate_label_idx]);
            let suffix_result = candidate_window_can_shrink_to_label_with_diag(host, 
                body,
                anchor_idx,
                candidate_label,
                candidate_label_idx,
                terminal_label_idx,
                referenced,
            );
            let suffix_safe = suffix_result.is_ok();
            if trace_enabled {
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
                        Some(HirStmt::Label(label)) => label.as_str(),
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

#[cfg(test)]
#[path = "suffix_window_tests.rs"]
mod suffix_window_tests;
