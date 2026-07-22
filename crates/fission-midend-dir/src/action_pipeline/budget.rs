//! Size-based admission budgets for action groups.

use crate::ir::DirFunction;

#[derive(Debug, Clone, Copy)]
pub struct PassBudget {
    pub stmt_limit: usize,
    pub block_limit: usize,
    pub round_limit: usize,
}

impl PassBudget {
    pub fn allows_body_cleanup(self, stmt_count: usize, block_count: usize) -> bool {
        stmt_count <= self.stmt_limit && block_count <= self.block_limit
    }
}

pub const EARLY_CLEANUP_BLOCK_STMT_LIMIT: usize = 2000;
pub const EARLY_CLEANUP_BLOCK_BLOCK_LIMIT: usize = 300;
pub const LARGE_FUNCTION_STMT_THRESHOLD: usize = 220;
pub const LARGE_FUNCTION_LOCAL_THRESHOLD: usize = 160;
pub const TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS: usize = 6;

pub fn is_large_hir_function(func: &DirFunction) -> bool {
    count_hir_stmts(&func.body) > LARGE_FUNCTION_STMT_THRESHOLD
        || func.locals.len() > LARGE_FUNCTION_LOCAL_THRESHOLD
}

pub fn count_hir_stmts(stmts: &[crate::ir::DirStmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        count += 1;
        match stmt {
            crate::ir::DirStmt::Block(body) => count += count_hir_stmts(body),
            crate::ir::DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_hir_stmts(then_body);
                count += count_hir_stmts(else_body);
            }
            crate::ir::DirStmt::While { body, .. }
            | crate::ir::DirStmt::DoWhile { body, .. }
            | crate::ir::DirStmt::For { body, .. } => count += count_hir_stmts(body),
            crate::ir::DirStmt::Switch { cases, default, .. } => {
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

pub fn count_hir_blocks(stmts: &[crate::ir::DirStmt]) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt {
            crate::ir::DirStmt::Block(body) => count += 1 + count_hir_blocks(body),
            crate::ir::DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_hir_blocks(then_body);
                count += count_hir_blocks(else_body);
            }
            crate::ir::DirStmt::While { body, .. }
            | crate::ir::DirStmt::DoWhile { body, .. }
            | crate::ir::DirStmt::For { body, .. } => count += count_hir_blocks(body),
            crate::ir::DirStmt::Switch { cases, default, .. } => {
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

pub fn hir_shape(func: &DirFunction) -> (usize, usize) {
    (count_hir_stmts(&func.body), func.locals.len())
}
