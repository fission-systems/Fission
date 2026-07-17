//! Short-circuit and/or conditional-chain free functions.

use super::{
    is_trivial_structuring_stmt, log_short_circuit_cache, shared_forward_linear_exit,
};
use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, LoweredTerminator, structuring_diag_enabled};
use fission_midend_core::ir::{HirBinaryOp, HirStmt, MlilPreviewError};
use fission_midend_core::{fold_logical_chain, negate_expr, simplify_logical_expr};

/// Dispatch short-circuit or / and-else / and patterns at `idx`.
pub fn try_lower_short_circuit_if(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    if let Some(lowered) = try_lower_short_circuit_or(host, idx)? {
        return Ok(Some(lowered));
    }
    if let Some(lowered) = try_lower_short_circuit_and_else(host, idx)? {
        return Ok(Some(lowered));
    }
    if let Some(lowered) = try_lower_short_circuit_and(host, idx)? {
        return Ok(Some(lowered));
    }
    Ok(None)
}

/// Chain of `if (!c0) goto join; if (!c1) goto join; ... body` → `if (c0 && c1 && ...) body`.
pub fn try_lower_short_circuit_and(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    let diag = structuring_diag_enabled();
    let mut conds = Vec::new();
    let mut current_idx = idx;
    let mut join_idx: Option<usize> = None;
    let mut first_prefix: Vec<HirStmt> = Vec::new();

    loop {
        let cond_prefix = host.lower_block_stmts(current_idx)?;
        if current_idx == idx {
            if !cond_prefix.iter().all(is_trivial_structuring_stmt) {
                host.bump_condition_fold_rejected_side_effect();
                return Ok(None);
            }
            first_prefix = cond_prefix;
        } else if !cond_prefix.is_empty() {
            host.bump_condition_fold_rejected_side_effect();
            return Ok(None);
        }

        let Some(next_idx) = host.fallthrough_index(current_idx) else {
            return Ok(None);
        };
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = host.lower_block_terminator(current_idx)?
        else {
            return Ok(None);
        };
        if false_target != Some(host.block_target_key(next_idx)) {
            return Ok(None);
        }
        let Some(current_join_idx) = host
            .find_block_index_by_address(true_target)
            .filter(|join_idx| *join_idx > current_idx)
        else {
            return Ok(None);
        };
        if let Some(join_idx) = join_idx {
            if join_idx != current_join_idx {
                return Ok(None);
            }
        } else {
            join_idx = Some(current_join_idx);
        }
        conds.push(negate_expr(cond));

        let next_is_conditional = matches!(
            host.lower_block_terminator(next_idx)?,
            LoweredTerminator::Cond { .. }
        );
        if next_is_conditional {
            current_idx = next_idx;
            continue;
        }

        let Some(join_idx) = join_idx else {
            return Ok(None);
        };
        log_short_circuit_cache(host, diag, "and", next_idx, LinearExit::Join(join_idx));
        let Some((then_body, skip_to)) =
            host.lower_linear_body(next_idx, LinearExit::Join(join_idx))?
        else {
            return Ok(None);
        };
        if conds.len() < 2 {
            return Ok(None);
        }

        host.bump_condition_fold_and(conds.len() - 1);

        let stmt = HirStmt::If {
            cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalAnd)),
            then_body,
            else_body: Vec::new(),
        };

        if first_prefix.is_empty() {
            return Ok(Some((stmt, skip_to)));
        } else {
            let mut wrapped = first_prefix;
            wrapped.push(stmt);
            return Ok(Some((HirStmt::Block(wrapped), skip_to)));
        }
    }
}

/// Short-circuit and with a non-empty else arm.
pub fn try_lower_short_circuit_and_else(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    let diag = structuring_diag_enabled();
    let mut conds = Vec::new();
    let mut current_idx = idx;
    let mut else_idx: Option<usize> = None;
    let mut first_prefix: Vec<HirStmt> = Vec::new();

    loop {
        let cond_prefix = host.lower_block_stmts(current_idx)?;
        if current_idx == idx {
            if !cond_prefix.iter().all(is_trivial_structuring_stmt) {
                host.bump_condition_fold_rejected_side_effect();
                return Ok(None);
            }
            first_prefix = cond_prefix;
        } else if !cond_prefix.is_empty() {
            host.bump_condition_fold_rejected_side_effect();
            return Ok(None);
        }

        let Some(next_idx) = host.fallthrough_index(current_idx) else {
            return Ok(None);
        };
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = host.lower_block_terminator(current_idx)?
        else {
            return Ok(None);
        };
        if false_target != Some(host.block_target_key(next_idx)) {
            return Ok(None);
        }
        let Some(current_else_idx) = host.find_block_index_by_address(true_target) else {
            return Ok(None);
        };
        if current_else_idx <= current_idx {
            return Ok(None);
        }
        if let Some(else_idx) = else_idx {
            if else_idx != current_else_idx {
                return Ok(None);
            }
        } else {
            else_idx = Some(current_else_idx);
        }
        conds.push(negate_expr(cond));

        let next_is_conditional = matches!(
            host.lower_block_terminator(next_idx)?,
            LoweredTerminator::Cond { .. }
        );
        if next_is_conditional {
            current_idx = next_idx;
            continue;
        }

        let Some(else_idx) = else_idx else {
            return Ok(None);
        };
        let then_idx = next_idx;
        let Some(exit) = shared_forward_linear_exit(host, idx, then_idx, else_idx)? else {
            return Ok(None);
        };
        log_short_circuit_cache(host, diag, "and_else", then_idx, exit);
        let Some((then_body, then_skip)) = host.lower_linear_body(then_idx, exit)? else {
            return Ok(None);
        };
        log_short_circuit_cache(host, diag, "and_else", else_idx, exit);
        let Some((else_body, else_skip)) = host.lower_linear_body(else_idx, exit)? else {
            return Ok(None);
        };
        if conds.len() < 2 {
            return Ok(None);
        }
        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
        };
        host.bump_condition_fold_and(conds.len() - 1);

        let stmt = HirStmt::If {
            cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalAnd)),
            then_body,
            else_body,
        };

        if first_prefix.is_empty() {
            return Ok(Some((stmt, skip_to)));
        } else {
            let mut wrapped = first_prefix;
            wrapped.push(stmt);
            return Ok(Some((HirStmt::Block(wrapped), skip_to)));
        }
    }
}

/// Chain of `if (c0) goto body; if (c1) goto body; ...` → `if (c0 || c1 || ...) body`.
pub fn try_lower_short_circuit_or(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    let diag = structuring_diag_enabled();

    let first_prefix = host.lower_block_stmts(idx)?;
    if !first_prefix.iter().all(is_trivial_structuring_stmt) {
        host.bump_condition_fold_rejected_side_effect();
        return Ok(None);
    }

    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };
    let Some(mut next_idx) = host.fallthrough_index(idx) else {
        return Ok(None);
    };
    if false_target != Some(host.block_target_key(next_idx)) {
        return Ok(None);
    }
    let Some(body_idx) = host
        .find_block_index_by_address(true_target)
        .filter(|body_idx| *body_idx > idx)
    else {
        return Ok(None);
    };

    let mut conds = vec![cond];
    loop {
        let is_conditional_chain = matches!(
            host.lower_block_terminator(next_idx)?,
            LoweredTerminator::Cond { true_target, .. }
                if host.find_block_index_by_address(true_target) == Some(body_idx)
        );
        if !is_conditional_chain {
            let false_entry_idx = next_idx;
            if conds.len() == 1
                && let Some(LinearExit::Join(join_idx)) = host.linear_exit(body_idx)?
                && join_idx > idx
                && (false_entry_idx == join_idx
                    || (host.is_trivial_forwarding_block(false_entry_idx, join_idx)
                        && !host.forwarding_block_defines_return_tail_live_in(
                            false_entry_idx,
                            join_idx,
                        )))
            {
                log_short_circuit_cache(
                    host,
                    diag,
                    "or_single_guarded_body",
                    body_idx,
                    LinearExit::Join(join_idx),
                );
                let Some((then_body, skip_to)) =
                    host.lower_linear_body(body_idx, LinearExit::Join(join_idx))?
                else {
                    return Ok(None);
                };

                let stmt = HirStmt::If {
                    cond: conds[0].clone(),
                    then_body,
                    else_body: Vec::new(),
                };

                if first_prefix.is_empty() {
                    return Ok(Some((stmt, skip_to)));
                } else {
                    let mut wrapped = first_prefix;
                    wrapped.push(stmt);
                    return Ok(Some((HirStmt::Block(wrapped), skip_to)));
                }
            }
            let Some(exit) = shared_forward_linear_exit(host, idx, body_idx, false_entry_idx)?
            else {
                return Ok(None);
            };
            log_short_circuit_cache(host, diag, "or", false_entry_idx, exit);
            let Some((false_body, false_skip)) =
                host.lower_linear_body(false_entry_idx, exit)?
            else {
                return Ok(None);
            };
            if !false_body.is_empty() {
                return Ok(None);
            }
            log_short_circuit_cache(host, diag, "or", body_idx, exit);
            let Some((then_body, then_skip)) = host.lower_linear_body(body_idx, exit)? else {
                return Ok(None);
            };
            let skip_to = match exit {
                LinearExit::Join(join_idx) => join_idx,
                LinearExit::Return | LinearExit::End => then_skip.max(false_skip),
            };

            host.bump_condition_fold_or(conds.len() - 1);
            let stmt = HirStmt::If {
                cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalOr)),
                then_body,
                else_body: Vec::new(),
            };

            if first_prefix.is_empty() {
                return Ok(Some((stmt, skip_to)));
            } else {
                let mut wrapped = first_prefix;
                wrapped.push(stmt);
                return Ok(Some((HirStmt::Block(wrapped), skip_to)));
            }
        }

        let next_prefix = host.lower_block_stmts(next_idx)?;
        if !next_prefix.is_empty() {
            host.bump_condition_fold_rejected_side_effect();
            return Ok(None);
        }

        let LoweredTerminator::Cond {
            cond, false_target, ..
        } = host.lower_block_terminator(next_idx)?
        else {
            return Ok(None);
        };
        conds.push(cond);
        let Some(chain_next_idx) = host.fallthrough_index(next_idx) else {
            return Ok(None);
        };
        if false_target != Some(host.block_target_key(chain_next_idx)) {
            return Ok(None);
        }
        next_idx = chain_next_idx;
    }
}
