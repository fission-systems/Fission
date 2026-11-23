use super::variable_reads::collect_vars_in_expr;
use crate::HashSet;
use crate::prelude::*;

pub(super) fn collect_copy_merge_barrier_vars(
    stmts: &[PreHirStmt],
    stack_state_vars: HashSet<String>,
) -> HashSet<String> {
    let mut collector = CopyMergeBarrierCollector {
        stack_state_vars,
        ..Default::default()
    };
    collector.visit_stmts(stmts);
    collector.barrier_vars
}

#[derive(Default)]
struct CopyMergeBarrierCollector {
    barrier_vars: HashSet<String>,
    stack_state_vars: HashSet<String>,
}

impl CopyMergeBarrierCollector {
    fn visit_stmts(&mut self, stmts: &[PreHirStmt]) {
        for stmt in stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &PreHirStmt) {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Var(lhs_name) = lhs
                    && self.expr_is_load_derived_barrier(rhs)
                {
                    self.barrier_vars.insert(lhs_name.clone());
                }
                if let PreHirLValue::Var(lhs_name) = lhs
                    && self.stack_state_vars.contains(lhs_name)
                    && let PreHirExpr::Var(rhs_name) = rhs
                    && !self.stack_state_vars.contains(rhs_name)
                {
                    self.barrier_vars.insert(lhs_name.clone());
                }
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => self.visit_expr(expr),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                self.visit_stmts(body);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.visit_stmt(init);
                }
                if let Some(cond) = cond {
                    self.visit_expr(cond);
                }
                if let Some(update) = update {
                    self.visit_stmt(update);
                }
                self.visit_stmts(body);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
            }
            PreHirStmt::VaStart { va_list, .. } => self.visit_expr(va_list),
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &PreHirLValue) {
        match lval {
            PreHirLValue::Var(_) => {}
            PreHirLValue::Deref { ptr, .. } => {
                collect_vars_in_expr(ptr, &mut self.barrier_vars);
                self.visit_expr(ptr);
            }
            PreHirLValue::Index { base, index, .. } => {
                collect_vars_in_expr(base, &mut self.barrier_vars);
                self.visit_expr(base);
                self.visit_expr(index);
            }
            PreHirLValue::FieldAccess { base, .. } => self.visit_expr(base),
        }
    }

    fn visit_expr(&mut self, expr: &PreHirExpr) {
        match expr {
            PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(_, _) => {}
            PreHirExpr::Load { ptr, .. } => {
                collect_vars_in_expr(ptr, &mut self.barrier_vars);
                self.visit_expr(ptr);
            }
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => self.visit_expr(expr),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            PreHirExpr::Index { base, index, .. } => {
                collect_vars_in_expr(base, &mut self.barrier_vars);
                self.visit_expr(base);
                self.visit_expr(index);
            }
        }
    }

    fn expr_is_load_derived_barrier(&self, expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Load { .. } => true,
            PreHirExpr::Var(name) => self.barrier_vars.contains(name),
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
                self.expr_is_load_derived_barrier(expr)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.expr_is_load_derived_barrier(lhs) || self.expr_is_load_derived_barrier(rhs)
            }
            PreHirExpr::Call { args, .. } => args
                .iter()
                .any(|arg| self.expr_is_load_derived_barrier(arg)),
            PreHirExpr::PtrOffset { base, .. }
            | PreHirExpr::FieldAccess { base, .. }
            | PreHirExpr::AggregateCopy { src: base, .. } => {
                self.expr_is_load_derived_barrier(base)
            }
            PreHirExpr::Index { base, index, .. } => {
                self.expr_is_load_derived_barrier(base) || self.expr_is_load_derived_barrier(index)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_is_load_derived_barrier(cond)
                    || self.expr_is_load_derived_barrier(then_expr)
                    || self.expr_is_load_derived_barrier(else_expr)
            }
            PreHirExpr::Const(_, _)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => false,
        }
    }
}
