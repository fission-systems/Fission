//! If/else and postdom-follow if/else free functions.

use super::{forward_join_idx_from_address, shared_forward_linear_exit};
use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, LoweredTerminator};
use fission_midend_core::ir::{MlilPreviewError};
use fission_midend_dir::{DirStmt};
use fission_midend_dir::util::negate_expr;
use crate::HashSet;

/// Follow a linear single-predecessor chain to a Return within `[start, follow)`.
pub fn try_lower_return_chain_arm(
    host: &mut impl StructuringHost,
    start_idx: usize,
    follow_idx: usize,
) -> Result<Option<(Vec<DirStmt>, usize)>, MlilPreviewError> {
    let mut body: Vec<DirStmt> = Vec::new();
    let mut visited: HashSet<usize> = HashSet::default();
    let mut idx = start_idx;
    loop {
        if idx >= follow_idx || !visited.insert(idx) {
            return Ok(None);
        }
        body.extend(host.lower_block_stmts(idx)?);
        match host.lower_block_terminator(idx)? {
            LoweredTerminator::Return(expr) => {
                body.push(DirStmt::Return(expr));
                return Ok(Some((body, follow_idx)));
            }
            LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                let Some(next_idx) = host.find_block_index_by_address(target) else {
                    return Ok(None);
                };
                if next_idx == follow_idx {
                    return Ok(None);
                }
                if next_idx >= follow_idx {
                    return Ok(None);
                }
                if !host.can_inline_linear_successor(idx, next_idx, &visited) {
                    return Ok(None);
                }
                idx = next_idx;
            }
            _ => return Ok(None),
        }
    }
}

/// Lower a diamond if/else when both arms share a forward linear exit.
pub fn try_lower_if_else(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
    let cond_prefix = host.lower_block_stmts(idx)?;
    if idx + 2 >= host.block_count() {
        return Ok(None);
    }
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target: Some(false_target),
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };

    let Some(next_idx) = host.fallthrough_index(idx) else {
        return Ok(None);
    };
    let next_addr = host.block_target_key(next_idx);

    let (cond, then_idx, else_idx) = if true_target == next_addr {
        let Some(else_idx) = forward_join_idx_from_address(host, idx, false_target) else {
            return Ok(None);
        };
        (cond, next_idx, else_idx)
    } else if false_target == next_addr {
        let Some(then_idx) = forward_join_idx_from_address(host, idx, true_target) else {
            return Ok(None);
        };
        (negate_expr(cond), next_idx, then_idx)
    } else {
        return Ok(None);
    };

    let Some(exit) = shared_forward_linear_exit(host, idx, then_idx, else_idx)? else {
        return Ok(None);
    };
    let Some((then_body, then_skip)) = host.lower_linear_body(then_idx, exit)? else {
        return Ok(None);
    };
    let Some((else_body, else_skip)) = host.lower_linear_body(else_idx, exit)? else {
        return Ok(None);
    };
    let skip_to = match exit {
        LinearExit::Join(join_idx) => join_idx,
        LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
    };
    let stmt = DirStmt::If {
        cond,
        then_body,
        else_body,
    };
    if cond_prefix.is_empty() {
        Ok(Some((stmt, skip_to)))
    } else {
        let mut wrapped = cond_prefix;
        wrapped.push(stmt);
        Ok(Some((DirStmt::Block(wrapped), skip_to)))
    }
}

/// Postdominance-guided if-then-else using a precomputed follow block.
pub fn try_reduce_if_else_with_follow(
    host: &mut impl StructuringHost,
    idx: usize,
    follow: Option<usize>,
) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
    let Some(follow_idx) = follow else {
        return Ok(None);
    };
    if follow_idx <= idx || follow_idx >= host.block_count() {
        return Ok(None);
    }

    let cond_prefix = host.lower_block_stmts(idx)?;

    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target: Some(false_target),
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };

    let Some(next_idx) = host.fallthrough_index(idx) else {
        return Ok(None);
    };
    let next_addr = host.block_target_key(next_idx);

    let (cond, then_idx, else_idx) = if true_target == next_addr {
        let Some(else_idx) = forward_join_idx_from_address(host, idx, false_target) else {
            return Ok(None);
        };
        (cond, next_idx, else_idx)
    } else if false_target == next_addr {
        let Some(then_idx) = forward_join_idx_from_address(host, idx, true_target) else {
            return Ok(None);
        };
        (negate_expr(cond), next_idx, then_idx)
    } else {
        return Ok(None);
    };

    let exit = LinearExit::Join(follow_idx);

    if then_idx >= follow_idx || else_idx >= follow_idx {
        return Ok(None);
    }

    let (then_body, _) = match host.lower_linear_body(then_idx, exit)? {
        Some(result) => result,
        None => match try_lower_return_chain_arm(host, then_idx, follow_idx)? {
            Some(result) => result,
            None => return Ok(None),
        },
    };
    let (else_body, _) = match host.lower_linear_body(else_idx, exit)? {
        Some(result) => result,
        None => match try_lower_return_chain_arm(host, else_idx, follow_idx)? {
            Some(result) => result,
            None => return Ok(None),
        },
    };

    let stmt = DirStmt::If {
        cond,
        then_body,
        else_body,
    };
    if cond_prefix.is_empty() {
        Ok(Some((stmt, follow_idx)))
    } else {
        let mut wrapped = cond_prefix;
        wrapped.push(stmt);
        Ok(Some((DirStmt::Block(wrapped), follow_idx)))
    }
}
