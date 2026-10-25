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

    fn run(&self, ctx: &mut PassCtx<'_, '_>) -> PassOutcome {
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
        PreHirStmt::VaStart { va_list, .. } => {
            changed |= apply_rules_to_expr(va_list, rules);
        }
        PreHirStmt::Block(body) => {
            changed |= apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rules);
        }
        PreHirStmt::While { cond, body } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rules);
        }
        PreHirStmt::DoWhile { body, cond } => {
            changed |= apply_rules_to_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rules);
            changed |= apply_rules_to_expr(cond, rules);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init_stmt) = init {
                changed |= apply_rules_to_stmt(init_stmt, rules);
            }
            if let Some(cond_expr) = cond {
                changed |= apply_rules_to_expr(cond_expr, rules);
            }
            if let Some(update_stmt) = update {
                changed |= apply_rules_to_stmt(update_stmt, rules);
            }
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
        PreHirLValue::FieldAccess { base, .. } => {
            changed |= apply_rules_to_expr(base, rules);
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
        PreHirExpr::FieldAccess { base, .. } => {
            changed |= apply_rules_to_expr(base, rules);
        }
        _ => {}
    }

    for rule in rules {
        changed |= rule.apply_expr(expr);
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{NirType, PreHirBinding, PreHirFunction, PreHirLValue, PreHirStmt};
    use std::rc::Rc;

    struct VarRenameRule {
        from: String,
        to: String,
    }

    impl Rule for VarRenameRule {
        fn name(&self) -> &'static str {
            "var_rename"
        }

        fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
            if let PreHirExpr::Var(name) = expr {
                if name == &self.from {
                    *name = self.to.clone();
                    return true;
                }
            }
            false
        }
    }

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    #[test]
    fn action_pool_traverses_loop_headers_vastart_and_field_access() {
        let pool = ActionPool::new(
            "test_pool",
            super::super::concept::GhidraActionConcept::Normalize,
        )
        .rule(Box::new(VarRenameRule {
            from: "old_var".to_string(),
            to: "new_var".to_string(),
        }));

        let mut func = PreHirFunction {
            name: "test_traversal".to_string(),
            params: vec![],
            locals: vec![],
            body: vec![
                PreHirStmt::While {
                    cond: PreHirExpr::Var("old_var".to_string()),
                    body: Rc::new(vec![]),
                },
                PreHirStmt::DoWhile {
                    body: Rc::new(vec![]),
                    cond: PreHirExpr::Var("old_var".to_string()),
                },
                PreHirStmt::For {
                    init: Some(Box::new(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("dummy".to_string()),
                        rhs: PreHirExpr::Var("old_var".to_string()),
                    })),
                    cond: Some(PreHirExpr::Var("old_var".to_string())),
                    update: Some(Box::new(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("dummy".to_string()),
                        rhs: PreHirExpr::Var("old_var".to_string()),
                    })),
                    body: Rc::new(vec![]),
                },
                PreHirStmt::VaStart {
                    va_list: PreHirExpr::Var("old_var".to_string()),
                    last_named_param: "p".to_string(),
                },
                PreHirStmt::Expr(PreHirExpr::FieldAccess {
                    base: Box::new(PreHirExpr::Var("old_var".to_string())),
                    field_name: "f".to_string(),
                    offset: 0,
                    ty: u32_ty(),
                }),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::FieldAccess {
                        base: Box::new(PreHirExpr::Var("old_var".to_string())),
                        field_name: "f".to_string(),
                        offset: 0,
                        ty: u32_ty(),
                    },
                    rhs: PreHirExpr::Const(0, u32_ty()),
                },
            ],
            ..Default::default()
        };

        let mut ctx = crate::action_pipeline::PassCtx {
            func: &mut func,
            perf: false,
            diag: false,
            stats: None,
            decomp_facts: None,
        };

        let outcome = pool.run(&mut ctx);
        assert!(outcome.changed());

        // Verify all "old_var"s were rewritten to "new_var"
        if let PreHirStmt::While { cond, .. } = &func.body[0] {
            assert_eq!(cond, &PreHirExpr::Var("new_var".to_string()));
        } else {
            panic!("expected While");
        }
        if let PreHirStmt::DoWhile { cond, .. } = &func.body[1] {
            assert_eq!(cond, &PreHirExpr::Var("new_var".to_string()));
        } else {
            panic!("expected DoWhile");
        }
        if let PreHirStmt::For {
            init, cond, update, ..
        } = &func.body[2]
        {
            if let Some(init_stmt) = init {
                if let PreHirStmt::Assign { rhs, .. } = init_stmt.as_ref() {
                    assert_eq!(rhs, &PreHirExpr::Var("new_var".to_string()));
                }
            }
            assert_eq!(cond.as_ref(), Some(&PreHirExpr::Var("new_var".to_string())));
            if let Some(update_stmt) = update {
                if let PreHirStmt::Assign { rhs, .. } = update_stmt.as_ref() {
                    assert_eq!(rhs, &PreHirExpr::Var("new_var".to_string()));
                }
            }
        } else {
            panic!("expected For");
        }
        if let PreHirStmt::VaStart { va_list, .. } = &func.body[3] {
            assert_eq!(va_list, &PreHirExpr::Var("new_var".to_string()));
        } else {
            panic!("expected VaStart");
        }
        if let PreHirStmt::Expr(PreHirExpr::FieldAccess { base, .. }) = &func.body[4] {
            assert_eq!(base.as_ref(), &PreHirExpr::Var("new_var".to_string()));
        } else {
            panic!("expected FieldAccess expr");
        }
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::FieldAccess { base, .. },
            ..
        } = &func.body[5]
        {
            assert_eq!(base.as_ref(), &PreHirExpr::Var("new_var".to_string()));
        } else {
            panic!("expected FieldAccess lhs");
        }
    }
}
