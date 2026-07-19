//! Free-function owners for guarded-tail canonicalize / execution bodies (ADR 0012).

use super::pure_hir;
use super::types::*;
use crate::cleanup::{
    collect_referenced_label_counts, has_non_ignorable_payload, has_top_level_label,
    is_ignorable_discovery_stmt, single_goto_target, trim_ignorable_stmt_bounds,
};
use crate::guarded_tail_pure::{
    count_var_defs_stmt, count_var_reads_stmt, replace_var_in_expr, replace_var_in_stmt,
};
use crate::host::StructuringHost;
use crate::regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    RegionKind, RegionLegality, RegionRejectionReason,
};
use fission_midend_core::ir::{HirExpr, HirLValue, HirStmt, NirBindingOrigin, NirType};
use fission_midend_core::util::expr::expr_type;
use fission_midend_core::{negate_expr, simplify_logical_expr, strip_casts};
use crate::HashMap;
use crate::HashSet;

/// Typed counters for residual telemetry bumps from free-fn GT bodies.
///
/// Variant names mirror `StructuringTelemetry` field names for 1:1 mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum StructuringCounter {
    guarded_tail_rejected_side_effectful_callee_count,
    canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
    canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
    canonicalized_guarded_tail_shape_count,
    canonicalized_interleaved_join_use_count,
    canonicalized_local_nonfallthrough_alias_count,
    discovery_seen_guarded_tail_like_shape_count,
    guarded_tail_candidate_count,
    guarded_tail_exported_binding_count,
    guarded_tail_promoted_count,
    guarded_tail_replacement_plan_candidate_count,
    guarded_tail_replacement_plan_completed_count,
    guarded_tail_replacement_plan_merge_created_count,
    guarded_tail_replacement_plan_rejected_missing_merge_count,
    guarded_tail_replacement_plan_rejected_unstable_read_count,
    guarded_tail_replacement_read_count,
    guarded_tail_replacement_read_rejected_nondominated_count,
    guarded_tail_replacement_read_rejected_nonremovable_op_count,
    guarded_tail_replacement_read_rewritten_count,
    promoted_region_count,
    promotion_candidate_count,
}

pub fn canonicalize_interleaved_local_aliases(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        let local_refs = crate::guarded_tail::pure_hir::local_goto_positions_by_label(body);
        let mut alias_redirects = HashMap::default();
        let mut canonicalized_local_nonfallthrough = 0usize;
        let mut external_safe_redirect_labels = Vec::new();
        let segment_end = segment_start + body.len();

        for (idx, stmt) in body.iter().enumerate() {
            let HirStmt::Label(label) = stmt else {
                continue;
            };
            let Some(goto_positions) = local_refs.get(label) else {
                continue;
            };
            let total_refs = referenced.get(label).copied().unwrap_or(0);
            let (top_level_before, nested_before, refs_after) =
                crate::guarded_tail::pure_hir::classify_alias_ref_sites(body, idx, label);
            let local_ref_count = top_level_before + nested_before + refs_after;
            let external_ref_count = total_refs.saturating_sub(local_ref_count);
            let top_level_after_positions: Vec<usize> = goto_positions
                .iter()
                .copied()
                .filter(|pos| *pos > idx)
                .collect();
            let top_level_after_label_count = top_level_after_positions.len();
            let nested_after_label_count = refs_after.saturating_sub(top_level_after_label_count);
            let sink_equivalent_top_level_after_label_count =
                crate::guarded_tail::pure_hir::count_sink_equivalent_top_level_after_label_refs(
                    body,
                    full_body,
                    label,
                    idx,
                    &top_level_after_positions,
                    nested_after_label_count,
                    external_ref_count,
                );
            let effective_top_level_after_label_count = top_level_after_label_count
                .saturating_sub(sink_equivalent_top_level_after_label_count);
            let blocking_top_level_after_positions: Vec<usize> = top_level_after_positions
                .iter()
                .copied()
                .filter(|pos| {
                    !crate::guarded_tail::pure_hir::local_after_label_ref_is_sink_equivalent(
                        body, full_body, label, idx, *pos,
                    ) && !crate::guarded_tail::pure_hir::top_level_after_label_ref_is_dead_post_return(body, *pos, label)
                })
                .collect();
            if nested_before > 0 {
                return Err(
                    GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                );
            }
            let has_non_ignorable_gap =
                goto_positions.iter().filter(|pos| **pos < idx).any(|pos| {
                    body[pos + 1..idx]
                        .iter()
                        .any(|stmt| !is_ignorable_discovery_stmt(stmt))
                });
            let next_label_idx =
                (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let payload_end = next_label_idx.unwrap_or(body.len());
            let segment = &body[idx + 1..payload_end];
            let allow_top_level_after_label_redirect = if let Some(next_label_idx) = next_label_idx
            {
                if let HirStmt::Label(next_label) = &body[next_label_idx] {
                    nested_after_label_count == 0
                        && !blocking_top_level_after_positions.is_empty()
                        && blocking_top_level_after_positions
                            .iter()
                            .all(|pos| *pos < next_label_idx)
                        && crate::guarded_tail::pure_hir::is_local_alias_forward_segment_with_after_label_refs(
                            segment, label, next_label,
                        )
                } else {
                    false
                }
            } else {
                nested_after_label_count == 0
                    && !blocking_top_level_after_positions.is_empty()
                    && crate::guarded_tail::pure_hir::inferred_alias_forward_target_with_after_label_refs(segment, label)
                        .is_some()
            };

            if host.guarded_tail_trace_enabled()
                && sink_equivalent_top_level_after_label_count > 0
            {
                eprintln!(
                    "[GT-TRACE] candidate={} alias_after_sink_equiv label={} raw_after={} sink_equiv={} effective_after={}",
                    segment_start.saturating_sub(1),
                    label,
                    top_level_after_label_count,
                    sink_equivalent_top_level_after_label_count,
                    effective_top_level_after_label_count
                );
            }

            if nested_after_label_count > 0
                || (effective_top_level_after_label_count > 0
                    && !allow_top_level_after_label_redirect)
            {
                host.bump_structuring_counter(StructuringCounter::canonicalization_failed_alias_not_fallthrough_top_level_after_label_count, (effective_top_level_after_label_count) as usize);
                host.bump_structuring_counter(StructuringCounter::canonicalization_failed_alias_not_fallthrough_nested_after_label_count, (nested_after_label_count) as usize);
                return Err(GuardedTailCanonicalizationFailure::AliasNotFallthrough);
            }

            // Priority 1: If we have external refs with top-level-after-label + all top-level goto,
            // try forward-chain resolution first (allow reaching beyond immediate next label)
            let forward_chain_redirect = if allow_top_level_after_label_redirect
                && external_ref_count > 0
                && crate::guarded_tail::pure_hir::are_all_external_refs_top_level_goto(
                    full_body,
                    segment_start,
                    segment_end,
                    label,
                ) {
                let resolved = if next_label_idx.is_some() {
                    host.resolve_terminal_join_target(body, idx, label, referenced)
                        .map(|(resolved_label, _)| resolved_label)
                } else {
                    crate::guarded_tail::pure_hir::inferred_alias_forward_target_with_after_label_refs(segment, label)
                };
                resolved.and_then(|resolved_label| {
                    // Prefer forward-chain resolution if it goes beyond immediate next
                    if let Some(next_label_idx) = next_label_idx {
                        if let HirStmt::Label(next_label) = &body[next_label_idx] {
                            if resolved_label != *label && resolved_label != next_label.as_str() {
                                return Some(resolved_label);
                            }
                        }
                    } else if resolved_label != *label {
                        return Some(resolved_label);
                    }
                    None
                })
            } else {
                None
            };

            // Priority 2: Try immediate next-label redirect (only if forward-chain didn't apply)
            let immediate_next_redirect = if forward_chain_redirect.is_none() {
                if let Some(next_label_idx) = next_label_idx {
                    if let HirStmt::Label(next_label) = &body[next_label_idx] {
                        if crate::guarded_tail::pure_hir::is_local_alias_forward_segment(segment, next_label)
                            || allow_top_level_after_label_redirect
                        {
                            Some(next_label.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let next_redirect_label = forward_chain_redirect.or(immediate_next_redirect);

            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] candidate={} alias_redirect label={} local_refs={} external_refs={} resolved={}",
                    segment_start.saturating_sub(1),
                    label,
                    local_ref_count,
                    external_ref_count,
                    next_redirect_label.as_deref().unwrap_or("<none>")
                );
            }

            if let Some(next_label) = next_redirect_label {
                if external_ref_count > 0 {
                    let (
                        external_top_level_before,
                        external_nested_before,
                        external_top_level_after,
                        external_nested_after,
                    ) = crate::guarded_tail::pure_hir::classify_external_alias_ref_sites_detailed(
                        full_body,
                        segment_start,
                        segment_end,
                        label,
                    );
                    let nested_before_proof = if external_nested_before > 0 {
                        Some(crate::guarded_tail::pure_hir::build_nested_before_alias_ownership_proof(
                            full_body,
                            segment_start,
                            segment_end,
                            label,
                            external_nested_before,
                        ))
                    } else {
                        None
                    };
                    let effective_nested_before = nested_before_proof
                        .as_ref()
                        .map(|proof| proof.effective_nested_before())
                        .unwrap_or(external_nested_before);
                    let internalized_nested_before =
                        external_nested_before.saturating_sub(effective_nested_before);
                    let external_refs_after = external_top_level_after + external_nested_after;
                    if external_nested_after > 0 {
                        host.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    if host.guarded_tail_trace_enabled() {
                        if let Some(proof) = nested_before_proof.as_ref() {
                            eprintln!(
                                "[GT-TRACE] candidate={} alias_ownership label={} raw_nested_before={} internalized_nested_before={} class={:?} legality={:?} witnesses={:?}",
                                segment_start.saturating_sub(1),
                                proof.label,
                                proof.raw_nested_before,
                                proof.internalized_nested_before,
                                proof.class,
                                proof.legality_reason,
                                proof
                                    .witnesses
                                    .iter()
                                    .map(|w| (w.stmt_idx, &w.class, &w.cond))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    if effective_nested_before > 0 {
                        host.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    let effective_external_ref_count =
                        external_ref_count.saturating_sub(internalized_nested_before);
                    if external_top_level_before + external_top_level_after
                        != effective_external_ref_count
                    {
                        host.mark_alias_nonlocal_external_before();
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    external_safe_redirect_labels.push(label.clone());
                }
                if has_non_ignorable_gap {
                    if goto_positions.len() != 1
                        && !crate::guarded_tail::pure_hir::is_pure_multi_goto_gap_to_label(body, goto_positions, idx, label)
                    {
                        return Err(
                            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                        );
                    }
                    canonicalized_local_nonfallthrough += 1;
                } else if effective_top_level_after_label_count > 0 {
                    canonicalized_local_nonfallthrough += 1;
                }
                alias_redirects.insert(label.clone(), Some(next_label.clone()));
                continue;
            }
            if external_ref_count > 0 {
                let (external_top_level_before, external_nested_before, external_refs_after) =
                    crate::guarded_tail::pure_hir::classify_external_alias_ref_sites(
                        full_body,
                        segment_start,
                        segment_end,
                        label,
                    );
                host.mark_alias_nonlocal_from_external_sites(
                    external_top_level_before,
                    external_nested_before,
                    external_refs_after,
                );
                return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
            }
            if has_non_ignorable_gap {
                if segment.iter().any(|stmt| {
                    matches!(
                        stmt,
                        HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
                    )
                }) {
                    return Err(GuardedTailCanonicalizationFailure::PayloadCrossesJoin);
                }
                alias_redirects.insert(label.clone(), None);
                continue;
            }
            if segment.iter().any(|stmt| {
                matches!(
                    stmt,
                    HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
                )
            }) {
                return Err(GuardedTailCanonicalizationFailure::PayloadCrossesJoin);
            }
            alias_redirects.insert(label.clone(), None);
        }

        if alias_redirects.is_empty() {
            return Ok((body.to_vec(), Vec::new()));
        }

        host.bump_structuring_counter(StructuringCounter::canonicalized_interleaved_join_use_count, (alias_redirects.len()) as usize);
        host.bump_structuring_counter(StructuringCounter::canonicalized_local_nonfallthrough_alias_count, (canonicalized_local_nonfallthrough) as usize);
        let external_redirects = external_safe_redirect_labels
            .into_iter()
            .filter_map(|label| {
                crate::guarded_tail::pure_hir::resolve_alias_redirect(&label, &alias_redirects)
                    .filter(|resolved| resolved != &label)
                    .map(|resolved| (label, resolved))
            })
            .collect();
        Ok((
            body.iter()
                .filter_map(|stmt| match stmt {
                    HirStmt::Goto(label) if alias_redirects.contains_key(label) => {
                        match crate::guarded_tail::pure_hir::resolve_alias_redirect(label, &alias_redirects) {
                            Some(resolved) if resolved != *label => Some(HirStmt::Goto(resolved)),
                            Some(_) => Some(stmt.clone()),
                            None => None,
                        }
                    }
                    HirStmt::Label(label) if alias_redirects.contains_key(label) => None,
                    other => Some(other.clone()),
                })
                .collect(),
            external_redirects,
        ))
    }

pub fn canonicalize_guarded_tail_segment(
    host: &mut impl StructuringHost,
        segment: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        let mut flattened = Vec::new();
        crate::guarded_tail::pure_hir::flatten_guarded_tail_segment(segment, &mut flattened);
        let flatten_before_len = flattened.len();
        let collapsed_guards = crate::guarded_tail::pure_hir::collapse_duplicate_top_level_guard_ladder(&mut flattened);
        let factored_guard_clusters =
            crate::guarded_tail::pure_hir::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
                &mut flattened,
                full_body,
            );
        let collapsed_sink_returns =
            crate::guarded_tail::pure_hir::collapse_top_level_sink_to_return_goto_chain(&mut flattened, full_body);
        let Some((start, end)) = trim_ignorable_stmt_bounds(&flattened) else {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] candidate={} canonicalize flatten_before={} trim=<none> collapse_dup={} cluster={} sink={} first_reject={:?}",
                    segment_start.saturating_sub(1),
                    flatten_before_len,
                    collapsed_guards,
                    factored_guard_clusters,
                    collapsed_sink_returns,
                    GuardedTailCanonicalizationFailure::NonterminalJoinLabel
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] canonicalize_snapshot",
                    &flattened,
                    20,
                );
            }
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        };
        let (flattened, external_redirects) = canonicalize_interleaved_local_aliases(host, 
            &flattened[start..end],
            full_body,
            segment_start,
            referenced,
        )?;

        if host.guarded_tail_trace_enabled() {
            eprintln!(
                "[GT-TRACE] candidate={} canonicalize flatten_before={} trim=[{}, {}) flatten_after={} collapse_dup={} cluster={} sink={} redirects={:?}",
                segment_start.saturating_sub(1),
                flatten_before_len,
                start,
                end,
                flattened.len(),
                collapsed_guards,
                factored_guard_clusters,
                collapsed_sink_returns,
                external_redirects
            );
        }

        let mut canonical = Vec::new();
        let mut saw_payload = false;
        let mut saw_gap_after_payload = false;
        let mut removed_any = start > 0
            || end < flattened.len()
            || flattened.len() != end - start
            || collapsed_guards > 0
            || factored_guard_clusters > 0
            || collapsed_sink_returns > 0;
        let mut payload_entry_count = 0usize;
        let segment_ref_counts = crate::guarded_tail::pure_hir::goto_ref_counts(&flattened);
        let mut idx = 0usize;

        while idx < flattened.len() {
            let stmt = &flattened[idx];
            let trailing_has_non_ignorable = flattened[idx + 1..]
                .iter()
                .any(|stmt| !is_ignorable_discovery_stmt(stmt));
            match stmt {
                HirStmt::Label(label) => {
                    if referenced.get(label).copied().unwrap_or(0) > 0 {
                        let local_ref_count = segment_ref_counts.get(label).copied().unwrap_or(0);
                        let total_ref_count = referenced.get(label).copied().unwrap_or(0);
                        let terminalizable_target =
                            crate::guarded_tail::pure_hir::terminalizable_join_alias_target(&flattened, idx);
                        if total_ref_count > local_ref_count {
                            let (
                                external_top_level_before,
                                external_nested_before,
                                external_top_level_after,
                                external_nested_after,
                            ) = crate::guarded_tail::pure_hir::classify_external_alias_ref_sites_detailed(
                                full_body,
                                segment_start,
                                segment_start + flattened.len(),
                                label,
                            );
                            let nested_before_proof = if external_nested_before > 0 {
                                Some(crate::guarded_tail::pure_hir::build_nested_before_alias_ownership_proof(
                                    full_body,
                                    segment_start,
                                    segment_start + flattened.len(),
                                    label,
                                    external_nested_before,
                                ))
                            } else {
                                None
                            };
                            let effective_nested_before = nested_before_proof
                                .as_ref()
                                .map(|proof| proof.effective_nested_before())
                                .unwrap_or(external_nested_before);
                            let external_refs_after =
                                external_top_level_after + external_nested_after;
                            let only_top_level_external_refs =
                                effective_nested_before == 0 && external_nested_after == 0;
                            if host.guarded_tail_trace_enabled() {
                                if let Some(proof) = nested_before_proof.as_ref() {
                                    eprintln!(
                                        "[GT-TRACE] candidate={} terminalizable_alias label={} raw_nested_before={} internalized_nested_before={} class={:?} legality={:?}",
                                        segment_start.saturating_sub(1),
                                        proof.label,
                                        proof.raw_nested_before,
                                        proof.internalized_nested_before,
                                        proof.class,
                                        proof.legality_reason,
                                    );
                                }
                            }
                            if !only_top_level_external_refs || terminalizable_target.is_none() {
                                host.mark_alias_nonlocal_from_external_sites(
                                    external_top_level_before,
                                    external_nested_before,
                                    external_refs_after,
                                );
                                return Err(
                                    GuardedTailCanonicalizationFailure::AliasHasNonlocalRef,
                                );
                            }
                        }
                        if let Some((next_label, next_idx)) = terminalizable_target {
                            crate::guarded_tail::pure_hir::rewrite_goto_label_in_stmts(&mut canonical, label, &next_label);
                            removed_any = true;
                            host.bump_structuring_counter(StructuringCounter::canonicalized_interleaved_join_use_count, (1) as usize);
                            idx = next_idx;
                            continue;
                        }
                        canonical.push(stmt.clone());
                        idx += 1;
                        continue;
                    }
                    removed_any = true;
                    if saw_payload {
                        saw_gap_after_payload = true;
                    }
                }
                HirStmt::Block(body) if body.is_empty() => {
                    removed_any = true;
                    if saw_payload {
                        saw_gap_after_payload = true;
                    }
                }
                HirStmt::Return(_) => {
                    if saw_payload {
                        if trailing_has_non_ignorable {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                    } else {
                        saw_payload = true;
                        payload_entry_count += 1;
                    }
                    canonical.push(stmt.clone());
                }
                HirStmt::Goto(_) => {
                    if saw_payload {
                        let HirStmt::Goto(target) = stmt else {
                            unreachable!();
                        };
                        if let Some(return_stmt) =
                            crate::guarded_tail::pure_hir::resolve_terminal_tail_exit_stmt(full_body, target)
                        {
                            canonical.push(return_stmt);
                            idx += 1;
                            continue;
                        }
                        if trailing_has_non_ignorable {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        if flattened[..idx]
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(_)))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        if full_body
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == target))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        if flattened
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == target))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                    }
                    canonical.push(stmt.clone());
                }
                HirStmt::Break | HirStmt::Continue => {
                    if saw_payload {
                        return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                    }
                    canonical.push(stmt.clone());
                }
                other => {
                    if !saw_payload || saw_gap_after_payload {
                        payload_entry_count += 1;
                        saw_payload = true;
                        saw_gap_after_payload = false;
                    }
                    canonical.push(other.clone());
                }
            }
            idx += 1;
        }

        if payload_entry_count > 1 {
            return Err(GuardedTailCanonicalizationFailure::MultiplePayloadEntries);
        }
        if canonical.is_empty() || !has_non_ignorable_payload(&canonical) {
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        }
        if removed_any {
            host.bump_structuring_counter(StructuringCounter::canonicalized_guarded_tail_shape_count, (1) as usize);
        }
        Ok((canonical, external_redirects))
    }

pub fn try_build_guarded_tail_witness(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<RegionShapeWitness, GuardedTailWitnessRejection>> {
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[idx]
        else {
            return None;
        };

        let (initial_target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
            let Some(label) = single_goto_target(then_body) else {
                return None;
            };
            (label.to_string(), false)
        } else if then_body.is_empty() {
            let Some(label) = single_goto_target(else_body) else {
                return None;
            };
            (label.to_string(), true)
        } else {
            return None;
        };

        let Some(original_label_idx) =
            crate::guarded_tail::pure_hir::find_top_level_label_after(body, idx, &initial_target_label)
        else {
            return None;
        };
        if !has_non_ignorable_payload(&body[idx + 1..original_label_idx]) {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::NonCanonicalLayout
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_label_idx],
                    20,
                );
            }
            host.mark_noncanonical_layout_rejection();
            return Some(Err(GuardedTailWitnessRejection::NonCanonicalLayout));
        }
        let original_tail_end = (original_label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if original_tail_end < body.len()
            && body[original_label_idx + 1..original_tail_end]
                .iter()
                .all(is_ignorable_discovery_stmt)
        {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_tail_end],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        let Some((resolved_target_label, resolved_label_idx)) =
            host.resolve_terminal_join_target(body, idx, &initial_target_label, referenced)
        else {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    GuardedTailWitnessRejection::MissingTerminalJoin
                );
                let upper = body.len().min(idx + 1 + 20);
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..upper],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::MissingTerminalJoin));
        };

        let (owned_join_label, label_idx) = host.find_earliest_owned_join_label_with_diag(
                body,
                idx,
                resolved_label_idx,
                referenced,
                host.guarded_tail_trace_enabled(),
            )
            .unwrap_or_else(|| (resolved_target_label.clone(), resolved_label_idx));
        let target_label = resolved_target_label.clone();

        if host.guarded_tail_trace_enabled() && label_idx != resolved_label_idx {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} owned_join_narrowed from={}({}) to={}({})",
                host.guarded_tail_function_address(),
                idx,
                resolved_target_label,
                resolved_label_idx,
                owned_join_label,
                label_idx
            );
        }

        if host.guarded_tail_trace_enabled() {
            let raw_middle = &body[idx + 1..label_idx];
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} raw_middle_len={}",
                host.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                raw_middle.len()
            );
        }

        let (middle, external_redirects) = match canonicalize_guarded_tail_segment(host, 
            &body[idx + 1..label_idx],
            body,
            idx + 1,
            referenced,
        ) {
            Ok(middle) => middle,
            Err(reason) => {
                if host.guarded_tail_trace_enabled() {
                    eprintln!(
                        "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                        host.guarded_tail_function_address(),
                        idx,
                        target_label,
                        label_idx,
                        reason
                    );
                    host.guarded_tail_trace_emit_snapshot(
                        "[GT-TRACE] reject_snapshot",
                        &body[idx + 1..label_idx],
                        20,
                    );
                }
                host.mark_guarded_tail_canonicalization_failure(reason);
                return Some(Err(map_guarded_tail_canonicalization_rejection(reason,
                )));
            }
        };
        if middle.is_empty() {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailCanonicalizationFailure::InterleavedJoinUses
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..label_idx],
                    20,
                );
            }
            host.mark_guarded_tail_canonicalization_failure(
                GuardedTailCanonicalizationFailure::InterleavedJoinUses,
            );
            return Some(Err(GuardedTailWitnessRejection::AliasInterleaveConflict));
        }

        if host.guarded_tail_trace_enabled() {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} canonical_middle_len={} external_redirects={:?}",
                host.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                middle.len(),
                external_redirects
            );
        }

        let tail_end = (label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if body[label_idx + 1..tail_end].is_empty() && label_idx + 1 != body.len() {
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..tail_end],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        Some(Ok(RegionShapeWitness {
            target_label,
            label_idx,
            keep_middle_when_cond_true,
            middle,
            external_redirects,
            terminal_join_present: true,
            follow_witness: true,
            side_entry_free: true,
            alias_interleave_legal: true,
        }))
    }

pub fn try_build_guarded_tail_trial(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<GuardedTailTrial, GuardedTailWitnessRejection>> {
        let witness = try_build_guarded_tail_witness(host, body, idx, referenced)?;
        if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
            match &witness {
                Ok(witness) => eprintln!(
                    "[DIAG] guarded-tail trial idx={} label={} middle_stmts={} redirects={}",
                    idx,
                    witness.target_label,
                    witness.middle.len(),
                    witness.external_redirects.len(),
                ),
                Err(reason) => eprintln!(
                    "[DIAG] guarded-tail trial idx={} rejected={:?}",
                    idx, reason
                ),
            }
        }
        Some(witness.map(|witness| GuardedTailTrial {
            follow_block: Some(witness.target_label.clone()),
            candidate_reads: crate::guarded_tail::pure_hir::collect_guarded_tail_candidate_reads(
                body,
                &witness.middle,
                idx,
                witness.label_idx,
                &witness.target_label,
            ),
            witness,
        }))
    }

pub fn verify_guarded_tail_trial(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
    ) -> GuardedTailVerification {
        let witness = &trial.witness;
        let legality = witness.region_legality();
        host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_candidate_count, (1) as usize);
        let follow_tail = if witness.label_idx + 1 < body.len() {
            &body[witness.label_idx + 1..]
        } else {
            &[]
        };
        if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} legality={:?}",
                idx, witness.target_label, legality
            );
        }

        if !legality.is_complete_for(RegionKind::GuardedTail) {
            host.record_guarded_tail_blockgraph_proof(
                idx,
                witness,
                BlockGraphRegionProof::reason_from_legality(legality),
            );
            if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} incomplete_legality",
                    idx, witness.target_label
                );
            }
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    host.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    GuardedTailExecutionRejection::Witness(
                        GuardedTailWitnessRejection::NonCanonicalLayout
                    )
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &witness.middle,
                    20,
                );
            }
            host.record_guarded_tail_blockgraph_proof(
                idx,
                witness,
                BlockGraphLegalityReason::MustEmitLabelConflict,
            );
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: witness.middle.clone(),
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::Witness(
                    GuardedTailWitnessRejection::NonCanonicalLayout,
                )),
            };
        }

        let rewritten =
            crate::guarded_tail::pure_hir::rewrite_guarded_tail_sequence(&witness.middle, &witness.target_label, &[]);
        let (outside_refs, middle_refs) = crate::guarded_tail::pure_hir::surviving_label_refs_after_guarded_tail_promotion(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
        );
        let effective_middle_refs = crate::guarded_tail::pure_hir::effective_middle_refs_for_promotion(
            &rewritten.stmts,
            &witness.target_label,
            middle_refs,
        );
        let execution_safe =
            crate::guarded_tail::pure_hir::guarded_tail_middle_is_execution_safe(&rewritten.stmts, &witness.target_label);
        if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} outside_refs={} middle_refs={} effective_middle_refs={} unresolved_join_refs={} execution_safe={}",
                idx,
                witness.target_label,
                outside_refs,
                middle_refs,
                effective_middle_refs,
                rewritten.unresolved_join_refs,
                execution_safe,
            );
        }
        if let Some(rejection) = classify_must_emit_label_rejection(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
            outside_refs,
            middle_refs,
        ) {
            { let __msg = format!(
                "must_emit_label label={} owner=guarded_tail_verify surviving_ref_kind={:?} outside_refs={} middle_refs={} effective_middle_refs={} candidate={} label_idx={}",
                witness.target_label,
                rejection,
                outside_refs,
                middle_refs,
                effective_middle_refs,
                idx,
                witness.label_idx,
            ); host.emit_ready_trace(&__msg); }
            host.mark_promotion_gate_rejection(rejection);
            if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=MustEmitLabelConflict({:?})",
                    idx, witness.target_label, rejection
                );
            }
            if host.guarded_tail_trace_enabled() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject=MustEmitLabelConflict({:?})",
                    host.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    rejection
                );
                host.guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &rewritten.stmts,
                    20,
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: rewritten.stmts,
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::MustEmitLabelConflict),
            };
        }
        let removable_ops_legal = execution_safe
            && !has_top_level_label(&rewritten.stmts)
            && rewritten.unresolved_join_refs == 0;
        let exported_bindings = match collect_guarded_tail_exported_bindings(host, &rewritten.stmts, follow_tail)
        {
            Ok(bindings) => bindings,
            Err(reason) => {
                if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                    eprintln!(
                        "[DIAG] guarded-tail verify idx={} label={} exported_bindings_rejected={:?}",
                        idx, witness.target_label, reason
                    );
                }
                if host.guarded_tail_trace_enabled() {
                    eprintln!(
                        "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                        host.guarded_tail_function_address(),
                        idx,
                        witness.target_label,
                        witness.label_idx,
                        reason
                    );
                    host.guarded_tail_trace_emit_snapshot(
                        "[GT-TRACE] reject_snapshot",
                        &rewritten.stmts,
                        20,
                    );
                }
                host.record_guarded_tail_blockgraph_proof(
                    idx,
                    witness,
                    BlockGraphLegalityReason::EmitReadyIncomplete,
                );
                return GuardedTailVerification {
                    region_legality: legality,
                    replacement_complete: false,
                    removable_ops_legal,
                    rewritten_middle: rewritten.stmts,
                    exported_bindings: Vec::new(),
                    rejection_reason: Some(reason),
                };
            }
        };
        let replacement_complete = removable_ops_legal && effective_middle_refs == 0;

        if replacement_complete
            && exported_bindings.iter().any(|binding| {
                !binding.read_sites.is_empty()
                    && crate::guarded_tail::pure_hir::find_guarded_tail_preexisting_source(body, idx, &binding.binding_name)
                        .is_none()
            })
        {
            host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_rejected_missing_merge_count, (1) as usize);
            if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                let missing = exported_bindings
                    .iter()
                    .filter(|binding| {
                        !binding.read_sites.is_empty()
                            && crate::guarded_tail::pure_hir::find_guarded_tail_preexisting_source(
                                body,
                                idx,
                                &binding.binding_name,
                            )
                            .is_none()
                    })
                    .map(|binding| binding.binding_name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=ReplacementIncomplete(missing_else_source=[{}])",
                    idx, witness.target_label, missing
                );
            }
            host.record_guarded_tail_blockgraph_proof(
                idx,
                witness,
                BlockGraphLegalityReason::EmitReadyIncomplete,
            );
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: Some(GuardedTailExecutionRejection::ReplacementIncomplete),
            };
        }

        if replacement_complete {
            host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_completed_count, (1) as usize);
            if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} replacement_complete exported_bindings={}",
                    idx,
                    witness.target_label,
                    exported_bindings.len()
                );
            }
            host.record_guarded_tail_blockgraph_proof(
                idx,
                witness,
                BlockGraphLegalityReason::Complete,
            );
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: true,
                removable_ops_legal: true,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: None,
            };
        }

        if !removable_ops_legal || effective_middle_refs > 0 {
            host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_rejected_unstable_read_count, (1) as usize);
            for binding in &exported_bindings {
                if binding.read_sites.is_empty() {
                    continue;
                }
                let read_kinds = binding
                    .read_sites
                    .iter()
                    .map(|read| format!("{:?}@{}", read.kind, read.stmt_idx))
                    .collect::<Vec<_>>()
                    .join(",");
                { let __msg = format!(
                    "unstable_read binding={} def_stmt_idx={} read_kinds=[{}] removable_ops_legal={} effective_middle_refs={} candidate={} join_label={}",
                    binding.binding_name,
                    binding.def_stmt_idx,
                    read_kinds,
                    removable_ops_legal,
                    effective_middle_refs,
                    idx,
                    witness.target_label,
                ); host.emit_ready_trace(&__msg); }
            }
        }
        if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} rejected={:?} removable_ops_legal={} effective_middle_refs={}",
                idx,
                witness.target_label,
                if !removable_ops_legal {
                    GuardedTailExecutionRejection::MustEmitLabelConflict
                } else {
                    GuardedTailExecutionRejection::ReplacementIncomplete
                },
                removable_ops_legal,
                effective_middle_refs
            );
        }

        if host.guarded_tail_trace_enabled() {
            let reason = if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                GuardedTailExecutionRejection::ReplacementIncomplete
            };
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                host.guarded_tail_function_address(),
                idx,
                witness.target_label,
                witness.label_idx,
                reason
            );
            host.guarded_tail_trace_emit_snapshot(
                "[GT-TRACE] reject_snapshot",
                &rewritten.stmts,
                20,
            );
        }

        host.record_guarded_tail_blockgraph_proof(
            idx,
            witness,
            if !removable_ops_legal {
                BlockGraphLegalityReason::MustEmitLabelConflict
            } else {
                BlockGraphLegalityReason::EmitReadyIncomplete
            },
        );
        GuardedTailVerification {
            region_legality: legality,
            replacement_complete,
            removable_ops_legal,
            rewritten_middle: rewritten.stmts,
            exported_bindings,
            rejection_reason: Some(if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_rejected_missing_merge_count, (1) as usize);
                GuardedTailExecutionRejection::ReplacementIncomplete
            }),
        }
    }

pub fn build_guarded_tail_execution_plan(
    host: &mut impl StructuringHost,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
        verification: &GuardedTailVerification,
    ) -> Result<GuardedTailExecutionPlan, GuardedTailExecutionRejection> {
        let mut rewritten_middle = verification.rewritten_middle.clone();
        let mut synthetic_merges = Vec::new();
        let mut replacement_cache = GuardedTailReplacementCache::default();
        let mut exported_bindings = verification.exported_bindings.clone();
        exported_bindings.sort_by_key(|binding| binding.def_stmt_idx);
        let mut obsolete_defs = Vec::new();

        for binding_idx in 0..exported_bindings.len() {
            let binding_name = exported_bindings[binding_idx].binding_name.clone();
            let replacement_source = exported_bindings[binding_idx].replacement_source.clone();
            let def_stmt_idx = exported_bindings[binding_idx].def_stmt_idx;
            for stmt in rewritten_middle
                .iter_mut()
                .skip(def_stmt_idx.saturating_add(1))
            {
                crate::guarded_tail_pure::replace_var_in_stmt(stmt, &binding_name, &replacement_source);
            }
            for later_binding in exported_bindings.iter_mut().skip(binding_idx + 1) {
                crate::guarded_tail_pure::replace_var_in_expr(
                    &mut later_binding.replacement_source,
                    &binding_name,
                    &replacement_source,
                );
            }
            if rewritten_middle
                .iter()
                .skip(def_stmt_idx.saturating_add(1))
                .all(|stmt| crate::guarded_tail_pure::count_var_reads_stmt(stmt, &binding_name) == 0)
            {
                obsolete_defs.push(def_stmt_idx);
            }

            let else_value = if exported_bindings[binding_idx].read_sites.is_empty() {
                continue;
            } else if let Some(expr) = crate::guarded_tail::pure_hir::resolve_guarded_tail_else_source(
                body,
                idx,
                &binding_name,
                &mut replacement_cache,
            ) {
                expr
            } else {
                host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_rejected_missing_merge_count, (1) as usize);
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            };

            let ty = expr_type(&replacement_source);
            let replacement_target = host.alloc_temp_binding(
                ty,
                Some(NirBindingOrigin::TempPreserved),
            );
            host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_plan_merge_created_count, (1) as usize);
            synthetic_merges.push(GuardedTailSyntheticMerge {
                binding_name,
                replacement_target,
                then_value: replacement_source,
                else_value,
                read_sites: exported_bindings[binding_idx].read_sites.clone(),
            });
        }
        obsolete_defs.sort_unstable();
        obsolete_defs.dedup();
        for def_idx in obsolete_defs.into_iter().rev() {
            if def_idx < rewritten_middle.len() {
                rewritten_middle.remove(def_idx);
            }
        }
        Ok(GuardedTailExecutionPlan {
            synthetic_merges,
            redirects: trial.witness.external_redirects.clone(),
            rewritten_middle,
        })
    }

pub fn execute_guarded_tail_plan(
    host: &mut impl StructuringHost,
        body: &mut Vec<HirStmt>,
        idx: usize,
        trial: GuardedTailTrial,
        plan: GuardedTailExecutionPlan,
        cond: HirExpr,
    ) {
        let mut then_body = plan.rewritten_middle;
        let mut else_body = Vec::new();
        for merge in &plan.synthetic_merges {
            then_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.then_value.clone(),
            });
            else_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.else_value.clone(),
            });
        }
        let replacement = HirStmt::If {
            cond: if trial.witness.keep_middle_when_cond_true {
                cond
            } else {
                negate_expr(cond)
            },
            then_body,
            else_body,
        };

        for (from, to) in &plan.redirects {
            crate::guarded_tail::pure_hir::rewrite_goto_label_in_stmts(body, from, to);
        }

        body[idx] = replacement;
        body.drain(idx + 1..=trial.witness.label_idx);
        let tail_start = idx + 1;
        for merge in &plan.synthetic_merges {
            for read in &merge.read_sites {
                let stmt_idx = tail_start + read.stmt_idx;
                if let Some(stmt) = body.get_mut(stmt_idx) {
                    crate::guarded_tail::pure_hir::apply_guarded_tail_replacement_read(stmt, merge);
                    host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_read_rewritten_count, (1) as usize);
                }
            }
        }

        // Insert the label back if it is still referenced anywhere in the body!
        let label_name = trial.witness.target_label.clone();
        let remaining_refs: usize = body
            .iter()
            .map(|stmt| crate::guarded_tail::pure_hir::stmt_contains_goto_label(stmt, &label_name))
            .sum();
        if remaining_refs > 0 {
            body.insert(idx + 1, HirStmt::Label(label_name));
        }

        host.bump_structuring_counter(StructuringCounter::guarded_tail_promoted_count, (1) as usize);
        host.bump_structuring_counter(StructuringCounter::promoted_region_count, (1) as usize);
    }

pub fn discover_guarded_tail_candidates_in_body(
    host: &mut impl StructuringHost, body: &[HirStmt]) {
        for stmt in body {
            match stmt {
                HirStmt::Block(inner)
                | HirStmt::While { body: inner, .. }
                | HirStmt::DoWhile { body: inner, .. }
                | HirStmt::For { body: inner, .. } => {
                    discover_guarded_tail_candidates_in_body(host, inner);
                }
                HirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    discover_guarded_tail_candidates_in_body(host, then_body);
                    discover_guarded_tail_candidates_in_body(host, else_body);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        discover_guarded_tail_candidates_in_body(host, &case.body);
                    }
                    discover_guarded_tail_candidates_in_body(host, default);
                }
                HirStmt::Assign { .. }
                | HirStmt::VaStart { .. }
                | HirStmt::Expr(_)
                | HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue => {}
            }
        }

        let referenced = collect_referenced_label_counts(body);
        for idx in 0..body.len() {
            let HirStmt::If { .. } = &body[idx] else {
                continue;
            };
            let Some(trial) = try_build_guarded_tail_trial(host, body, idx, &referenced) else {
                continue;
            };
            host.bump_structuring_counter(StructuringCounter::discovery_seen_guarded_tail_like_shape_count, (1) as usize);
            let trial = match trial {
                Ok(trial) => trial,
                Err(reason) => {
                    host.mark_guarded_tail_execution_rejection(
                        GuardedTailExecutionRejection::Witness(reason),
                    );
                    match reason {
                        GuardedTailWitnessRejection::MissingTerminalJoin => {
                            host.mark_guarded_tail_canonicalization_failure(
                                GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                            );
                        }
                        GuardedTailWitnessRejection::AliasInterleaveConflict => {}
                        GuardedTailWitnessRejection::NonCanonicalLayout => {}
                        GuardedTailWitnessRejection::AmbiguousFollow
                        | GuardedTailWitnessRejection::SideEntryConflict => {}
                    }
                    continue;
                }
            };
            let verification = verify_guarded_tail_trial(host, body, idx, &trial);
            if let Some(reason) = verification.rejection_reason {
                host.mark_guarded_tail_execution_rejection(reason);
                continue;
            }

            host.bump_structuring_counter(StructuringCounter::guarded_tail_candidate_count, (1) as usize);
            host.bump_structuring_counter(StructuringCounter::promotion_candidate_count, (1) as usize);
        }
    }

pub fn collect_guarded_tail_exported_bindings(
    host: &mut impl StructuringHost,
        middle: &[HirStmt],
        follow_tail: &[HirStmt],
    ) -> Result<Vec<GuardedTailExportedBinding>, GuardedTailExecutionRejection> {
        let mut bindings = Vec::new();
        for (def_stmt_idx, stmt) in middle.iter().enumerate() {
            let HirStmt::Assign {
                lhs: HirLValue::Var(binding_name),
                rhs,
            } = stmt
            else {
                continue;
            };
            let mut read_sites = Vec::new();
            let mut follow_redefined = false;
            let mut nondominated_reads = 0usize;
            let always_terminates = crate::guarded_tail::pure_hir::statement_sequence_always_terminates(middle);
            if crate::guarded_tail::pure_hir::guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-DEBUG] binding_name={} middle={:?} always_terminates={}",
                    binding_name, middle, always_terminates
                );
            }
            if !always_terminates {
                for (stmt_idx, stmt) in follow_tail.iter().enumerate() {
                    let reads_here = crate::guarded_tail::pure_hir::classify_stmt_read_kind(stmt, binding_name);
                    let defs_here = crate::guarded_tail_pure::count_var_defs_stmt(stmt, binding_name);
                    if follow_redefined {
                        if reads_here.is_some() {
                            nondominated_reads += 1;
                        }
                        continue;
                    }
                    if let Some(kind) = reads_here {
                        read_sites.push(GuardedTailReplacementRead { stmt_idx, kind });
                    }
                    if defs_here > 0 {
                        follow_redefined = true;
                    }
                }
            }
            if read_sites.is_empty() {
                continue;
            }
            host.bump_structuring_counter(StructuringCounter::guarded_tail_exported_binding_count, (1) as usize);
            host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_read_count, (read_sites.len()) as usize);

            if !crate::guarded_tail::pure_hir::expr_is_pure_value(rhs) {
                host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_read_rejected_nonremovable_op_count, (read_sites.len()) as usize);
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if middle
                .iter()
                .map(|stmt| crate::guarded_tail_pure::count_var_defs_stmt(stmt, binding_name))
                .sum::<usize>()
                != 1
            {
                host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_read_rejected_nondominated_count, (read_sites.len()) as usize);
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if nondominated_reads > 0 {
                host.bump_structuring_counter(StructuringCounter::guarded_tail_replacement_read_rejected_nondominated_count, (read_sites.len() + nondominated_reads) as usize);
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }

            bindings.push(GuardedTailExportedBinding {
                def_stmt_idx,
                binding_name: binding_name.clone(),
                replacement_source: rhs.clone(),
                read_sites,
            });
        }
        Ok(bindings)
    }

pub fn map_guarded_tail_canonicalization_rejection(reason: GuardedTailCanonicalizationFailure,
    ) -> GuardedTailWitnessRejection {
        match reason {
            GuardedTailCanonicalizationFailure::InterleavedJoinUses
            | GuardedTailCanonicalizationFailure::AliasNotFallthrough
            | GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors
            | GuardedTailCanonicalizationFailure::AliasHasNonlocalRef
            | GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                GuardedTailWitnessRejection::AliasInterleaveConflict
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                GuardedTailWitnessRejection::AmbiguousFollow
            }
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries
            | GuardedTailCanonicalizationFailure::NestedTailEscape => {
                GuardedTailWitnessRejection::NonCanonicalLayout
            }
        }
    }

pub fn classify_must_emit_label_rejection(
        _body: &[HirStmt],
        _middle: &[HirStmt],
        _if_idx: usize,
        _label_idx: usize,
        _label: &str,
        _outside_refs: usize,
        _middle_refs: usize,
    ) -> Option<PromotionGateRejection> {
        None
    }

