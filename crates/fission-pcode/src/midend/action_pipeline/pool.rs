//! ActionPool: fixpoint rule sweep over HIR (Ghidra ActionPool analog).

use super::super::ir::{HirExpr, HirFunction, HirLValue, HirStmt};
use super::concept::GhidraActionConcept;
use super::pass::{Pass, PassCtx, PassOutcome};

pub(crate) trait Rule {
    fn name(&self) -> &'static str;
    fn apply_stmt(&self, _stmt: &mut HirStmt) -> bool {
        false
    }
    fn apply_expr(&self, _expr: &mut HirExpr) -> bool {
        false
    }
}

pub(crate) struct ActionPool {
    pub(crate) name: &'static str,
    pub(crate) concept: GhidraActionConcept,
    pub(crate) max_rounds: usize,
    pub(crate) rules: Vec<Box<dyn Rule>>,
}

impl ActionPool {
    pub(crate) fn new(name: &'static str, concept: GhidraActionConcept) -> Self {
        Self {
            name,
            concept,
            max_rounds: 15,
            rules: Vec::new(),
        }
    }

    pub(crate) fn rule(mut self, rule: Box<dyn Rule>) -> Self {
        self.rules.push(rule);
        self
    }

    pub(crate) fn max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds;
        self
    }
}

impl Pass for ActionPool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn concept(&self) -> GhidraActionConcept {
        self.concept
    }

    fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome {
        let mut changed = false;
        let mut loop_changed = true;
        let mut round = 0;

        while loop_changed && round < self.max_rounds {
            loop_changed = false;
            round += 1;
            loop_changed |= apply_rules_to_stmts(&mut ctx.func.body, &self.rules);
            changed |= loop_changed;
        }

        PassOutcome::from_bool(changed)
    }
}

pub(crate) fn apply_rules_to_stmts(stmts: &mut [HirStmt], rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_rules_to_stmt(stmt, rules);
    }
    changed
}

fn apply_rules_to_stmt(stmt: &mut HirStmt, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    for rule in rules {
        changed |= rule.apply_stmt(stmt);
    }

    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= apply_rules_to_lvalue(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= apply_rules_to_expr(expr, rules);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            changed |= apply_rules_to_stmts(body, rules);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_stmts(then_body, rules);
            changed |= apply_rules_to_stmts(else_body, rules);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= apply_rules_to_expr(expr, rules);
            for case in cases {
                changed |= apply_rules_to_stmts(&mut case.body, rules);
            }
            changed |= apply_rules_to_stmts(default, rules);
        }
        _ => {}
    }

    changed
}

fn apply_rules_to_lvalue(lhs: &mut HirLValue, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    match lhs {
        HirLValue::Deref { ptr, .. } => {
            changed |= apply_rules_to_expr(ptr, rules);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }
    changed
}

fn apply_rules_to_expr(expr: &mut HirExpr, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::Load { ptr: inner, .. } => {
            changed |= apply_rules_to_expr(inner, rules);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= apply_rules_to_expr(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_expr(then_expr, rules);
            changed |= apply_rules_to_expr(else_expr, rules);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= apply_rules_to_expr(arg, rules);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }

    for rule in rules {
        changed |= rule.apply_expr(expr);
    }

    changed
}
