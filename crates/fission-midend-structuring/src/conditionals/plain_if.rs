//! Plain if (then-only) lowering free functions.

use super::{
    forward_join_idx_from_address, is_forward_exit_from, log_try_lower_if_reject_diag,
};
use crate::host::StructuringHost;
use crate::linear_types::{IfLoweringBudget, LinearExit, LoweredTerminator, structuring_diag_enabled};
use fission_midend_core::ir::{DirExpr, DirStmt, MlilPreviewError};
use fission_midend_core::util_dir::negate_expr;

struct PlainIfCandidate {
    cond_prefix: Vec<DirStmt>,
    cond: DirExpr,
    body_idx: usize,
    exit: LinearExit,
    block_addr: u64,
}

fn classify_plain_if_candidate(
    host: &mut impl StructuringHost,
    idx: usize,
    budget: &mut IfLoweringBudget,
    diag: bool,
) -> Result<Option<PlainIfCandidate>, MlilPreviewError> {
    let block_addr = host.block_start_address(idx);
    if budget.checkpoint("cond_prefix_pre") {
        return Ok(None);
    }
    let cond_prefix = host.lower_block_stmts(idx)?;
    if budget.checkpoint("cond_prefix_post") {
        return Ok(None);
    }

    let Some(next_idx) = host.fallthrough_index(idx) else {
        log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_no_unique_follow");
        return Ok(None);
    };

    if budget.checkpoint("terminator_pre") {
        return Ok(None);
    }
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(idx)?
    else {
        log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_not_conditional");
        return Ok(None);
    };
    if budget.checkpoint("terminator_post") {
        return Ok(None);
    }

    let next_addr = host.block_target_key(next_idx);
    let (cond, body_idx, exit) = if true_target == next_addr {
        let exit = if let Some(join_addr) = false_target {
            let Some(join_idx) = forward_join_idx_from_address(host, idx, join_addr) else {
                log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_nonforward_join");
                return Ok(None);
            };
            let expected = LinearExit::Join(join_idx);
            let actual = host.linear_exit_with_budget(next_idx, Some(budget))?;
            if actual != Some(expected) && actual != Some(LinearExit::Return) {
                log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_open_body_tail");
                return Ok(None);
            }
            expected
        } else {
            let Some(exit) = host.linear_exit_with_budget(next_idx, Some(budget))? else {
                log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_no_unique_follow");
                return Ok(None);
            };
            if !is_forward_exit_from(idx, exit) {
                log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_nonforward_join");
                return Ok(None);
            }
            exit
        };
        (cond, next_idx, exit)
    } else if false_target == Some(next_addr) {
        let Some(join_idx) = forward_join_idx_from_address(host, idx, true_target) else {
            log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_nonforward_join");
            return Ok(None);
        };
        let exit = LinearExit::Join(join_idx);
        let actual = host.linear_exit_with_budget(next_idx, Some(budget))?;
        if actual != Some(exit) && actual != Some(LinearExit::Return) {
            log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_open_body_tail");
            return Ok(None);
        }
        (negate_expr(cond), next_idx, exit)
    } else {
        log_try_lower_if_reject_diag(diag, idx, block_addr, "rejected_no_unique_follow");
        return Ok(None);
    };

    Ok(Some(PlainIfCandidate {
        cond_prefix,
        cond,
        body_idx,
        exit,
        block_addr,
    }))
}

/// Lower a plain if (then-only) region starting at `idx`.
pub fn try_lower_if(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
    let diag = structuring_diag_enabled();
    let mut budget = IfLoweringBudget::new(
        host.options(),
        idx,
        host.block_start_address(idx),
        "try_lower_if",
        host.structuring_total_work_counter(),
    );
    if diag {
        eprintln!(
            "[DIAG] try_lower_if start: idx={} block=0x{:x} x86_guard={}",
            idx,
            host.block_start_address(idx),
            budget.enabled
        );
    }

    let result = (|| {
        let Some(candidate) = classify_plain_if_candidate(host, idx, &mut budget, diag)? else {
            return Ok(None);
        };

        if diag {
            eprintln!(
                "[DIAG] try_lower_if chosen_exit: idx={} block=0x{:x} body_idx={} exit={:?}",
                idx, candidate.block_addr, candidate.body_idx, candidate.exit
            );
            eprintln!(
                "[DIAG] try_lower_if lower_linear_body {}: idx={} block=0x{:x} body_idx={} exit={:?}",
                if host.has_linear_body_cache(candidate.body_idx, candidate.exit) {
                    "cache_hit"
                } else {
                    "cache_miss"
                },
                idx,
                candidate.block_addr,
                candidate.body_idx,
                candidate.exit
            );
        }

        let Some((body, skip_to)) = host.lower_linear_body_with_budget(
            candidate.body_idx,
            candidate.exit,
            Some(&mut budget),
        )?
        else {
            return Ok(None);
        };

        let stmt = DirStmt::If {
            cond: candidate.cond,
            then_body: body,
            else_body: Vec::new(),
        };
        if candidate.cond_prefix.is_empty() {
            Ok(Some((stmt, skip_to)))
        } else {
            let mut wrapped = candidate.cond_prefix;
            wrapped.push(stmt);
            Ok(Some((DirStmt::Block(wrapped), skip_to)))
        }
    })();

    if diag {
        eprintln!(
            "[DIAG] try_lower_if done: idx={} block=0x{:x} elapsed={:.3}s success={} budget_tripped={}",
            idx,
            host.block_start_address(idx),
            budget.start.elapsed().as_secs_f64(),
            matches!(result, Ok(Some(_))),
            budget.tripped
        );
    }
    result
}
