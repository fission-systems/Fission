//! Size-based admission budgets for action groups.

use super::super::ir::HirFunction;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PassBudget {
    pub(crate) stmt_limit: usize,
    pub(crate) block_limit: usize,
    pub(crate) round_limit: usize,
}

impl PassBudget {
    pub(crate) fn allows_body_cleanup(self, stmt_count: usize, block_count: usize) -> bool {
        stmt_count <= self.stmt_limit && block_count <= self.block_limit
    }
}

pub(crate) const EARLY_CLEANUP_BLOCK_STMT_LIMIT: usize = 2000;
pub(crate) const EARLY_CLEANUP_BLOCK_BLOCK_LIMIT: usize = 300;
pub(crate) const LARGE_FUNCTION_STMT_THRESHOLD: usize = 220;
pub(crate) const LARGE_FUNCTION_LOCAL_THRESHOLD: usize = 160;
pub(crate) const TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS: usize = 6;

pub(crate) fn is_large_hir_function(func: &HirFunction) -> bool {
    count_hir_stmts(&func.body) > LARGE_FUNCTION_STMT_THRESHOLD
        || func.locals.len() > LARGE_FUNCTION_LOCAL_THRESHOLD
}

pub(crate) fn count_hir_stmts(stmts: &[super::super::ir::HirStmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        count += 1;
        match stmt {
            super::super::ir::HirStmt::Block(body) => count += count_hir_stmts(body),
            super::super::ir::HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_hir_stmts(then_body);
                count += count_hir_stmts(else_body);
            }
            super::super::ir::HirStmt::While { body, .. }
            | super::super::ir::HirStmt::DoWhile { body, .. }
            | super::super::ir::HirStmt::For { body, .. } => count += count_hir_stmts(body),
            super::super::ir::HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count += count_hir_stmts(&case.body);
                }
                count += count_hir_stmts(default);
            }
            _ => {}
        }
    }
    count
}

pub(crate) fn count_hir_blocks(stmts: &[super::super::ir::HirStmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt {
            super::super::ir::HirStmt::Block(body) => count += 1 + count_hir_blocks(body),
            super::super::ir::HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_hir_blocks(then_body);
                count += count_hir_blocks(else_body);
            }
            super::super::ir::HirStmt::While { body, .. }
            | super::super::ir::HirStmt::DoWhile { body, .. }
            | super::super::ir::HirStmt::For { body, .. } => count += count_hir_blocks(body),
            super::super::ir::HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count += count_hir_blocks(&case.body);
                }
                count += count_hir_blocks(default);
            }
            _ => {}
        }
    }
    count
}

pub(crate) fn hir_shape(func: &HirFunction) -> (usize, usize) {
    (count_hir_stmts(&func.body), func.locals.len())
}
