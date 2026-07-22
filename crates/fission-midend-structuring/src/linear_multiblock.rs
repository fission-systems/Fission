//! Linear multiblock fallback free functions (ADR 0012).
//!
//! P-code opcode triviality checks remain host residual; this owner assembles
//! the multiblock HIR body from host lowering primitives.

use crate::cleanup::{cleanup_redundant_labels_protecting, finalize_structured_body};
use crate::guarded_tail::{discover_guarded_tail_candidates, promote_guarded_tail_regions_until_stable};
use crate::helpers::{block_label, recovered_switch_case_values};
use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, LoweredTerminator};
use crate::regions::EmitReadyDecision;
use crate::switch::{SWITCH_CHAIN_PARSE_BUDGET_MAX, canonicalize_switch_target, try_lower_switch};
use fission_midend_core::ir::{DispatcherProofUnit, MlilPreviewError};
use fission_midend_dir::{DirExpr, DirStmt, DirSwitchCase};
use crate::HashSet;

/// Admit switch-chain recovery from CFG shape alone (no p-code opcodes).
pub fn switch_recovery_cfg_admitted(host: &impl StructuringHost, start_idx: usize) -> bool {
    let Some(start_successors) = host.successors().get(start_idx) else {
        return false;
    };
    if start_successors.len() > 2 {
        return true;
    }
    if start_successors.len() != 2 {
        return false;
    }

    let mut cursor = start_idx;
    let mut conditional_nodes = 0usize;
    let mut visited = HashSet::default();
    let max_steps = host
        .successors()
        .len()
        .min(SWITCH_CHAIN_PARSE_BUDGET_MAX)
        .max(1);
    for _ in 0..max_steps {
        if !visited.insert(cursor) {
            return false;
        }
        let Some(successors) = host.successors().get(cursor) else {
            return false;
        };
        if successors.len() != 2 || successors.iter().any(|succ| *succ <= cursor) {
            return false;
        }
        conditional_nodes += 1;
        if conditional_nodes >= 2 {
            return true;
        }
        let Some(next_cursor) = successors.iter().copied().min() else {
            return false;
        };
        cursor = next_cursor;
    }
    false
}

/// Lower an emit-ready dispatcher proof into a structured switch statement.
pub fn lower_structured_switch_terminator(
    host: &mut impl StructuringHost,
    expr: &DirExpr,
    targets: &[u64],
    default_target: Option<u64>,
    min_val: i64,
    proof: Option<&DispatcherProofUnit>,
) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
    let emit_ready = EmitReadyDecision::from_dispatcher_proof(proof);
    let Some(proof) = proof else {
        return Ok(None);
    };
    if !emit_ready.emit_ready {
        return Ok(None);
    }

    let mut exits = Vec::new();
    let mut indexed_cases = Vec::new();
    let (recovered_cases, used_proof_payload) =
        recovered_switch_case_values(targets, default_target, min_val, Some(proof));
    if used_proof_payload {
        host.bump_proof_payload_direct_emit();
    }
    for (value, target) in recovered_cases {
        if Some(target) == default_target {
            continue;
        }
        let Some(case_idx) = host.find_block_index_by_address(target) else {
            return Ok(None);
        };
        let case_idx = canonicalize_switch_target(host, case_idx);
        exits.push(case_idx);
        indexed_cases.push((value, case_idx));
    }
    if indexed_cases.len() < 2 {
        return Ok(None);
    }

    let default_idx = if let Some(default_target) = default_target {
        let Some(default_idx) = host.find_block_index_by_address(default_target) else {
            return Ok(None);
        };
        let default_idx = canonicalize_switch_target(host, default_idx);
        exits.push(default_idx);
        Some(default_idx)
    } else {
        None
    };

    let Some(exit) = host.shared_exit_for_indices(&exits)? else {
        return Ok(None);
    };

    let mut cases = Vec::new();
    let mut max_skip = 0usize;
    for (value, case_idx) in indexed_cases {
        let Some((case_body, skip_to)) = host.lower_linear_body(case_idx, exit)? else {
            return Ok(None);
        };
        max_skip = max_skip.max(skip_to);
        cases.push(DirSwitchCase {
            values: vec![value],
            body: case_body,
        });
    }
    crate::helpers::merge_equivalent_switch_cases(&mut cases);

    let default = if let Some(default_idx) = default_idx {
        let Some((default_body, default_skip)) = host.lower_linear_body(default_idx, exit)? else {
            return Ok(None);
        };
        max_skip = max_skip.max(default_skip);
        default_body
    } else {
        Vec::new()
    };

    let skip_to = match exit {
        LinearExit::Join(join_idx) => join_idx,
        LinearExit::Return | LinearExit::End => max_skip,
    };
    Ok(Some((
        DirStmt::Switch {
            expr: expr.clone(),
            cases,
            default,
        },
        skip_to,
    )))
}

/// Build a linear multiblock body (optional switch recovery at admitted CFGs).
pub fn build_linear_multiblock_body(
    host: &mut impl StructuringHost,
    try_switch_recovery: bool,
) -> Result<Vec<DirStmt>, MlilPreviewError> {
    let mut body = Vec::new();
    let targeted = host.collect_jump_targets()?;
    let mut emitted_labels = HashSet::default();
    let mut idx = 0usize;
    while idx < host.block_count() {
        let block_key = host.block_target_key(idx);
        let is_orphan_unreachable = idx != 0
            && host.predecessors().get(idx).is_some_and(|p| p.is_empty())
            && !targeted.contains(&block_key);
        if is_orphan_unreachable {
            idx += 1;
            continue;
        }
        if try_switch_recovery
            && switch_recovery_cfg_admitted(host, idx)
            && let Some((switch_stmt, skip_to)) = try_lower_switch(host, idx)?
        {
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                body.push(DirStmt::Label(block_label(block_key)));
            }
            body.push(switch_stmt);
            idx = skip_to;
            continue;
        }
        let block_key = host.block_target_key(idx);
        let block_start = host.block_start_address(idx);
        if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
            body.push(DirStmt::Label(block_label(block_key)));
        }
        body.extend(host.lower_block_stmts(idx)?);
        match host.lower_block_terminator(idx)? {
            LoweredTerminator::Return(expr) => body.push(DirStmt::Return(expr)),
            LoweredTerminator::Goto(target) => {
                if let Some(target_idx) = host.find_block_index_by_address(target)
                    && let Some(expr) =
                        host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                {
                    body.push(DirStmt::Return(Some(expr)));
                } else if host.next_block_address(idx) != Some(target) {
                    body.push(DirStmt::Goto(block_label(target)));
                }
            }
            LoweredTerminator::Fallthrough(Some(target)) => {
                if let Some(target_idx) = host.find_block_index_by_address(target)
                    && let Some(expr) =
                        host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                {
                    body.push(DirStmt::Return(Some(expr)));
                }
            }
            LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } => {
                let then_body = if let Some(true_idx) = host.find_block_index_by_address(true_target)
                    && let Some(expr) =
                        host.lower_return_join_expr_for_predecessor(idx, true_idx)?
                {
                    vec![DirStmt::Return(Some(expr))]
                } else {
                    vec![DirStmt::Goto(block_label(true_target))]
                };
                let else_body = if let Some(false_target) = false_target {
                    if let Some(false_idx) = host.find_block_index_by_address(false_target)
                        && let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, false_idx)?
                    {
                        vec![DirStmt::Return(Some(expr))]
                    } else {
                        vec![DirStmt::Goto(block_label(false_target))]
                    }
                } else {
                    Vec::new()
                };
                body.push(DirStmt::If {
                    cond,
                    then_body,
                    else_body,
                });
            }
            LoweredTerminator::Fallthrough(None) => {}
            LoweredTerminator::Unsupported {
                evidence,
                target_expr,
            } => {
                host.note_unsupported_terminator_emit(block_start);
                body.push(host.emit_unsupported_control_surface(evidence, target_expr));
            }
            LoweredTerminator::Switch {
                expr,
                targets,
                default_target,
                min_val,
                proof,
            } => {
                if let Some((switch_stmt, skip_to)) = lower_structured_switch_terminator(
                    host,
                    &expr,
                    &targets,
                    default_target,
                    min_val,
                    proof.as_ref(),
                )? {
                    body.push(switch_stmt);
                    idx = skip_to;
                    continue;
                }
                let cases = if let Some(proof) = proof.as_ref()
                    && EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready
                {
                    host.bump_proof_payload_direct_emit();
                    proof
                        .recovered_cases
                        .iter()
                        .filter(|(_, target)| Some(*target) != default_target)
                        .map(|(value, target)| DirSwitchCase {
                            values: vec![*value],
                            body: vec![DirStmt::Goto(block_label(*target))],
                        })
                        .collect()
                } else {
                    targets
                        .into_iter()
                        .filter(|target| Some(*target) != default_target)
                        .enumerate()
                        .map(|(i, t)| DirSwitchCase {
                            values: vec![min_val + i as i64],
                            body: vec![DirStmt::Goto(block_label(t))],
                        })
                        .collect()
                };
                body.push(DirStmt::Switch {
                    expr,
                    cases,
                    default: default_target
                        .map(block_label)
                        .map(DirStmt::Goto)
                        .into_iter()
                        .collect(),
                });
            }
        }
        idx += 1;
    }
    let protected = host.lsda_landing_pad_labels();
    let mut body = cleanup_redundant_labels_protecting(body, &protected);
    promote_guarded_tail_regions_until_stable(host, &mut body);
    discover_guarded_tail_candidates(host, &body);
    Ok(finalize_structured_body(&protected, body))
}
