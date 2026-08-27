//! ActionPool: fixpoint rule sweep over HIR (Ghidra ActionPool analog).

use super::concept::GhidraActionConcept;
use super::pass::{Pass, PassCtx, PassOutcome};
use crate::ir::{PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};

pub trait Rule {
    fn name(&self) -> &'static str;
    fn apply_stmt(&self, _stmt: &mut PreHirStmt) -> bool {
        false
    }
    fn apply_expr(&self, _expr: &mut PreHirExpr) -> bool {
        false
    }
}

pub struct ActionPool {
    pub name: &'static str,
    pub concept: GhidraActionConcept,
    pub max_rounds: usize,
    pub rules: Vec<Box<dyn Rule>>,
}

impl ActionPool {
    pub fn new(name: &'static str, concept: GhidraActionConcept) -> Self {
        Self {
            name,
            concept,
            max_rounds: 15,
            rules: Vec::new(),
        }
    }

    pub fn rule(mut self, rule: Box<dyn Rule>) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn max_rounds(mut self, max_rounds: usize) -> Self {
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

pub fn apply_rules_to_stmts(stmts: &mut [PreHirStmt], rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_rules_to_stmt(stmt, rules);
    }
    changed
}

fn apply_rules_to_stmt(stmt: &mut PreHirStmt, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    for rule in rules {
        changed |= rule.apply_stmt(stmt);
    }

    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= apply_rules_to_lvalue(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= apply_rules_to_expr(expr, rules);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            changed |= apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rules);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |=
                apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), rules);
            changed |=
                apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), rules);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= apply_rules_to_expr(expr, rules);
            for case in cases {
                changed |= apply_rules_to_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    rules,
                );
            }
            changed |=
                apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), rules);
        }
        _ => {}
    }

    changed
}

fn apply_rules_to_lvalue(lhs: &mut PreHirLValue, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    match lhs {
        PreHirLValue::Deref { ptr, .. } => {
            changed |= apply_rules_to_expr(ptr, rules);
        }
        PreHirLValue::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }
    changed
}

fn apply_rules_to_expr(expr: &mut PreHirExpr, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. } => {
            changed |= apply_rules_to_expr(inner, rules);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= apply_rules_to_expr(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_expr(then_expr, rules);
            changed |= apply_rules_to_expr(else_expr, rules);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= apply_rules_to_expr(arg, rules);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
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
