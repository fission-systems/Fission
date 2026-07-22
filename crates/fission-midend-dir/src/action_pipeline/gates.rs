//! Centralized admission gates for normalize action groups.

use crate::ir::DirFunction;
use super::budget::{
    EARLY_CLEANUP_BLOCK_BLOCK_LIMIT, EARLY_CLEANUP_BLOCK_STMT_LIMIT,
    LARGE_FUNCTION_LOCAL_THRESHOLD, LARGE_FUNCTION_STMT_THRESHOLD, count_hir_stmts,
};
use super::group::Gate;

pub const INIT_CLEANUP_STMT_LIMIT: usize = 600;
pub const INIT_CLEANUP_BLOCK_LIMIT: usize = 120;
pub const INIT_CLEANUP_ROUND_LIMIT: usize = 12;
pub const JUMP_RESOLVER_CANDIDATE_LIMIT: usize = 16;
pub const CLEANUP_LARGE_BODY_STMT_THRESHOLD: usize = 500;
pub const CLEANUP_LARGE_BODY_ROUND_LIMIT: usize = 6;
pub const CLEANUP_DEFAULT_ROUND_LIMIT: usize = 16;
pub const MERGE_TYPE_MAX_ROUNDS: usize = 4;
pub const RULE_POOL_MAX_ROUNDS: usize = 15;
pub const STRUCTURING_TIME_CEILING_SECS: f64 = 4.5;
pub const TRACE_DAG_FOLLOW_BLOCK_LIMIT: usize = 500;

pub fn gate_not_large_function() -> Gate {
    Gate::Custom(|func| {
        count_hir_stmts(&func.body) <= LARGE_FUNCTION_STMT_THRESHOLD
            && func.locals.len() <= LARGE_FUNCTION_LOCAL_THRESHOLD
    })
}

pub fn body_exceeds_early_cleanup_budget(body: &[crate::ir::DirStmt]) -> bool {
    count_hir_stmts(body) > EARLY_CLEANUP_BLOCK_STMT_LIMIT
        || super::budget::count_hir_blocks(body) > EARLY_CLEANUP_BLOCK_BLOCK_LIMIT
}

pub fn cleanup_round_limit_for(func: &DirFunction) -> usize {
    if count_hir_stmts(&func.body) > CLEANUP_LARGE_BODY_STMT_THRESHOLD {
        CLEANUP_LARGE_BODY_ROUND_LIMIT
    } else {
        CLEANUP_DEFAULT_ROUND_LIMIT
    }
}
