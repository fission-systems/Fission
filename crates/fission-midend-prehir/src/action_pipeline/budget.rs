//! Size-based admission budgets for action groups.

use crate::ir::PreHirFunction;

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

pub fn is_large_hir_function(func: &PreHirFunction) -> bool {
    count_hir_stmts(&func.body) > LARGE_FUNCTION_STMT_THRESHOLD
        || func.locals.len() > LARGE_FUNCTION_LOCAL_THRESHOLD
}

use std::collections::HashSet;

pub fn count_hir_stmts(stmts: &[crate::ir::PreHirStmt]) -> usize {
    let mut visited = HashSet::new();
    count_hir_stmts_visited(stmts, &mut visited)
}

fn count_hir_stmts_visited(
    stmts: &[crate::ir::PreHirStmt],
    visited: &mut HashSet<*const Vec<crate::ir::PreHirStmt>>,
) -> usize {
    let mut count = 0;
    for stmt in stmts {
        count += 1;
        match stmt {
            crate::ir::PreHirStmt::Block(body) => {
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_stmts_visited(body, visited);
                }
            }
            crate::ir::PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if visited.insert(std::rc::Rc::as_ptr(then_body)) {
                    count += count_hir_stmts_visited(then_body, visited);
                }
                if visited.insert(std::rc::Rc::as_ptr(else_body)) {
                    count += count_hir_stmts_visited(else_body, visited);
                }
            }
            crate::ir::PreHirStmt::While { body, .. }
            | crate::ir::PreHirStmt::DoWhile { body, .. } => {
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_stmts_visited(body, visited);
                }
            }
            crate::ir::PreHirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    count += count_hir_stmts_visited(std::slice::from_ref(s.as_ref()), visited);
                }
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_stmts_visited(body, visited);
                }
                if let Some(s) = update {
                    count += count_hir_stmts_visited(std::slice::from_ref(s.as_ref()), visited);
                }
            }
            crate::ir::PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    if visited.insert(std::rc::Rc::as_ptr(&case.body)) {
                        count += count_hir_stmts_visited(&case.body, visited);
                    }
                }
                if visited.insert(std::rc::Rc::as_ptr(default)) {
                    count += count_hir_stmts_visited(default, visited);
                }
            }
            _ => {}
        }
    }
    count
}

pub fn count_hir_blocks(stmts: &[crate::ir::PreHirStmt]) -> usize {
    let mut visited = HashSet::new();
    count_hir_blocks_visited(stmts, &mut visited)
}

fn count_hir_blocks_visited(
    stmts: &[crate::ir::PreHirStmt],
    visited: &mut HashSet<*const Vec<crate::ir::PreHirStmt>>,
) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt {
            crate::ir::PreHirStmt::Block(body) => {
                count += 1;
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_blocks_visited(body, visited);
                }
            }
            crate::ir::PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if visited.insert(std::rc::Rc::as_ptr(then_body)) {
                    count += count_hir_blocks_visited(then_body, visited);
                }
                if visited.insert(std::rc::Rc::as_ptr(else_body)) {
                    count += count_hir_blocks_visited(else_body, visited);
                }
            }
            crate::ir::PreHirStmt::While { body, .. }
            | crate::ir::PreHirStmt::DoWhile { body, .. } => {
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_blocks_visited(body, visited);
                }
            }
            crate::ir::PreHirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    count += count_hir_blocks_visited(std::slice::from_ref(s.as_ref()), visited);
                }
                if visited.insert(std::rc::Rc::as_ptr(body)) {
                    count += count_hir_blocks_visited(body, visited);
                }
                if let Some(s) = update {
                    count += count_hir_blocks_visited(std::slice::from_ref(s.as_ref()), visited);
                }
            }
            crate::ir::PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    if visited.insert(std::rc::Rc::as_ptr(&case.body)) {
                        count += count_hir_blocks_visited(&case.body, visited);
                    }
                }
                if visited.insert(std::rc::Rc::as_ptr(default)) {
                    count += count_hir_blocks_visited(default, visited);
                }
            }
            _ => {}
        }
    }
    count
}

pub fn hir_shape(func: &PreHirFunction) -> (usize, usize) {
    (count_hir_stmts(&func.body), func.locals.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{PreHirExpr, PreHirStmt};
    use std::rc::Rc;

    #[test]
    fn shared_rc_block_dag_does_not_multiply_count() {
        let shared_leaf = Rc::new(vec![PreHirStmt::Return(None)]);
        let branch1 = PreHirStmt::If {
            cond: PreHirExpr::Const(1, crate::ir::NirType::Bool),
            then_body: shared_leaf.clone(),
            else_body: shared_leaf.clone(),
        };
        let branch2 = PreHirStmt::If {
            cond: PreHirExpr::Const(1, crate::ir::NirType::Bool),
            then_body: shared_leaf.clone(),
            else_body: shared_leaf.clone(),
        };
        let stmts = vec![branch1, branch2];

        // 2 If stmts + 1 shared return = 3 total stmts
        let stmt_count = count_hir_stmts(&stmts);
        assert_eq!(stmt_count, 3);
    }
}
