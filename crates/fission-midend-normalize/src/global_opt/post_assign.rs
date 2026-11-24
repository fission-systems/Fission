//! Post-assignment value representative recovery.
//!
//! The HIR builder lowers p-code SSA values into mutable C-like variables.  In
//! straight-line regions, a branch condition can still contain the exact pure
//! expression that was just assigned to a representative variable:
//!
//! ```text
//! x = x + 2;
//! if (x + 2 - end) goto loop;
//! ```
//!
//! In the original SSA value graph the condition consumes the assigned value,
//! not the post-assignment C expression `x + 2`.  This pass tracks pure
//! expression representatives inside a single straight-line statement list and
//! rewrites control expressions to use the latest representative.

use super::super::analysis::expr_key::{PureExprMap, invalidate_pure_map, pure_expr_key};
use crate::HashSet;
use crate::prelude::*;

pub fn apply_post_assign_value_representative_pass(func: &mut PreHirFunction) -> bool {
    let mut addr_taken = HashSet::default();
    collect_addr_taken_in_stmts(&func.body, &mut addr_taken);
    let mut reps = PureExprMap::default();
    stabilize_stmts(&mut func.body, &mut reps, &addr_taken)
}

fn collect_addr_taken_in_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => collect_addr_taken_in_expr(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_addr_taken_in_expr(lhs, out);
            collect_addr_taken_in_expr(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_addr_taken_in_expr(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_addr_taken_in_expr(base, out);
            collect_addr_taken_in_expr(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_addr_taken_in_expr(cond, out);
            collect_addr_taken_in_expr(then_expr, out);
            collect_addr_taken_in_expr(else_expr, out);
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_addr_taken_in_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                match lhs {
                    PreHirLValue::Var(_) => {}
                    PreHirLValue::Deref { ptr, .. } => collect_addr_taken_in_expr(ptr, out),
                    PreHirLValue::Index { base, index, .. } => {
                        collect_addr_taken_in_expr(base, out);
                        collect_addr_taken_in_expr(index, out);
                    }
                    PreHirLValue::FieldAccess { base, .. } => {
                        collect_addr_taken_in_expr(base, out);
                    }
                }
                collect_addr_taken_in_expr(rhs, out);
            }
            PreHirStmt::Expr(expr)
            | PreHirStmt::Return(Some(expr))
            | PreHirStmt::VaStart { va_list: expr, .. } => collect_addr_taken_in_expr(expr, out),
            PreHirStmt::Block(body) => collect_addr_taken_in_stmts(body, out),
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                collect_addr_taken_in_expr(cond, out);
                collect_addr_taken_in_stmts(body, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_addr_taken_in_expr(cond, out);
                collect_addr_taken_in_stmts(then_body, out);
                collect_addr_taken_in_stmts(else_body, out);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_addr_taken_in_stmts(std::slice::from_ref(init.as_ref()), out);
                }
                if let Some(cond) = cond {
                    collect_addr_taken_in_expr(cond, out);
                }
                collect_addr_taken_in_stmts(body, out);
                if let Some(update) = update {
                    collect_addr_taken_in_stmts(std::slice::from_ref(update.as_ref()), out);
                }
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_addr_taken_in_expr(expr, out);
                for case in cases {
                    collect_addr_taken_in_stmts(&case.body, out);
                }
                collect_addr_taken_in_stmts(default, out);
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn expr_has_call_pa(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_has_call_pa(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_has_call_pa(lhs) || expr_has_call_pa(rhs),
        PreHirExpr::Index { base, index, .. } => expr_has_call_pa(base) || expr_has_call_pa(index),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => expr_has_call_pa(cond) || expr_has_call_pa(then_expr) || expr_has_call_pa(else_expr),
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn lvalue_base_var_pa(lhs: &PreHirLValue) -> Option<&str> {
    let mut cur = match lhs {
        PreHirLValue::Var(_) | PreHirLValue::Deref { .. } => return None,
        PreHirLValue::Index { base, .. } | PreHirLValue::FieldAccess { base, .. } => base.as_ref(),
    };
    loop {
        match cur {
            PreHirExpr::Var(name) => return Some(name.as_str()),
            PreHirExpr::FieldAccess { base, .. }
            | PreHirExpr::Index { base, .. }
            | PreHirExpr::Cast { expr: base, .. } => cur = base,
            _ => return None,
        }
    }
}

fn collect_modified_in_stmt(
    stmt: &PreHirStmt,
    addr_taken: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Var(name) => {
                    out.insert(name.clone());
                }
                PreHirLValue::Deref { ptr, .. } => {
                    out.extend(addr_taken.iter().cloned());
                    if expr_has_call_pa(ptr) {
                        out.extend(addr_taken.iter().cloned());
                    }
                }
                PreHirLValue::Index { base, index, .. } => {
                    if let Some(root) = lvalue_base_var_pa(lhs) {
                        out.insert(root.to_string());
                    }
                    out.extend(addr_taken.iter().cloned());
                    if expr_has_call_pa(base) || expr_has_call_pa(index) {
                        out.extend(addr_taken.iter().cloned());
                    }
                }
                PreHirLValue::FieldAccess { base, .. } => {
                    if let Some(root) = lvalue_base_var_pa(lhs) {
                        out.insert(root.to_string());
                    }
                    out.extend(addr_taken.iter().cloned());
                    if expr_has_call_pa(base) {
                        out.extend(addr_taken.iter().cloned());
                    }
                }
            }
            if expr_has_call_pa(rhs) {
                out.extend(addr_taken.iter().cloned());
            }
        }
        PreHirStmt::VaStart { va_list, .. } => {
            if let PreHirExpr::Var(name) = va_list {
                out.insert(name.clone());
            }
            out.extend(addr_taken.iter().cloned());
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            if expr_has_call_pa(expr) {
                out.extend(addr_taken.iter().cloned());
            }
        }
        PreHirStmt::Block(body) => collect_modified_in_stmts(body, addr_taken, out),
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            if expr_has_call_pa(cond) {
                out.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(body, addr_taken, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            if expr_has_call_pa(cond) {
                out.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(then_body, addr_taken, out);
            collect_modified_in_stmts(else_body, addr_taken, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_modified_in_stmt(init, addr_taken, out);
            }
            if let Some(cond) = cond
                && expr_has_call_pa(cond)
            {
                out.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(body, addr_taken, out);
            if let Some(update) = update {
                collect_modified_in_stmt(update, addr_taken, out);
            }
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            if expr_has_call_pa(expr) {
                out.extend(addr_taken.iter().cloned());
            }
            for case in cases {
                collect_modified_in_stmts(&case.body, addr_taken, out);
            }
            collect_modified_in_stmts(default, addr_taken, out);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_modified_in_stmts(
    stmts: &[PreHirStmt],
    addr_taken: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    for stmt in stmts {
        collect_modified_in_stmt(stmt, addr_taken, out);
    }
}

fn invalidate_vars(reps: &mut PureExprMap, vars: &HashSet<String>) {
    for var in vars {
        invalidate_pure_map(reps, var);
        invalidate_representative(reps, var);
    }
}

fn stabilize_stmts(
    stmts: &mut [PreHirStmt],
    reps: &mut PureExprMap,
    addr_taken: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= stabilize_stmt(stmt, reps, addr_taken);
    }
    changed
}

fn stabilize_stmt(
    stmt: &mut PreHirStmt,
    reps: &mut PureExprMap,
    addr_taken: &HashSet<String>,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            if let PreHirLValue::Var(name) = lhs {
                if expr_has_call_pa(rhs) {
                    invalidate_vars(reps, addr_taken);
                }
                invalidate_pure_map(reps, name);
                invalidate_representative(reps, name);
                if is_representable_expr(rhs) {
                    if let Some(key) = pure_expr_key(rhs) {
                        reps.insert(key, name.clone());
                    }
                }
            } else {
                changed |= stabilize_lvalue(lhs, reps);
                changed |= stabilize_expr(rhs, reps);
                if let Some(root) = lvalue_base_var_pa(lhs) {
                    invalidate_pure_map(reps, root);
                    invalidate_representative(reps, root);
                }
                invalidate_vars(reps, addr_taken);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= stabilize_expr(cond, reps);
            if expr_has_call_pa(cond) {
                invalidate_vars(reps, addr_taken);
            }
            let mut then_reps = reps.clone();
            let mut else_reps = reps.clone();
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                &mut then_reps,
                addr_taken,
            );
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                &mut else_reps,
                addr_taken,
            );
            reps.clear();
        }
        PreHirStmt::While { cond, body } => {
            let mut modified = HashSet::default();
            if expr_has_call_pa(cond) {
                modified.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(body, addr_taken, &mut modified);
            invalidate_vars(reps, &modified);
            changed |= stabilize_expr(cond, reps);
            if expr_has_call_pa(cond) {
                invalidate_vars(reps, addr_taken);
            }
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                &mut body_reps,
                addr_taken,
            );
            reps.clear();
        }
        PreHirStmt::DoWhile { body, cond } => {
            let mut modified = HashSet::default();
            if expr_has_call_pa(cond) {
                modified.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(body, addr_taken, &mut modified);
            invalidate_vars(reps, &modified);
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                &mut body_reps,
                addr_taken,
            );
            changed |= stabilize_expr(cond, &mut body_reps);
            reps.clear();
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= stabilize_stmt(init, reps, addr_taken);
            }
            let mut modified = HashSet::default();
            if let Some(cond) = cond.as_ref()
                && expr_has_call_pa(cond)
            {
                modified.extend(addr_taken.iter().cloned());
            }
            collect_modified_in_stmts(body, addr_taken, &mut modified);
            if let Some(update) = update.as_ref() {
                collect_modified_in_stmt(update, addr_taken, &mut modified);
            }
            invalidate_vars(reps, &modified);
            if let Some(cond) = cond {
                changed |= stabilize_expr(cond, reps);
                if expr_has_call_pa(cond) {
                    invalidate_vars(reps, addr_taken);
                }
            }
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                &mut body_reps,
                addr_taken,
            );
            if let Some(update) = update {
                changed |= stabilize_stmt(update, &mut body_reps, addr_taken);
            }
            reps.clear();
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= stabilize_expr(expr, reps);
            if expr_has_call_pa(expr) {
                invalidate_vars(reps, addr_taken);
            }
            for case in cases {
                let mut case_reps = reps.clone();
                changed |= stabilize_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    &mut case_reps,
                    addr_taken,
                );
            }
            let mut default_reps = reps.clone();
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                &mut default_reps,
                addr_taken,
            );
            reps.clear();
        }
        PreHirStmt::Block(body) => {
            changed |= stabilize_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                reps,
                addr_taken,
            );
        }
        PreHirStmt::VaStart { va_list: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
            if let PreHirExpr::Var(name) = expr {
                invalidate_pure_map(reps, name);
                invalidate_representative(reps, name);
            }
            invalidate_vars(reps, addr_taken);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= stabilize_expr(expr, reps);
            if expr_has_call_pa(expr) {
                invalidate_vars(reps, addr_taken);
            }
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {
            reps.clear();
        }
    }
    changed
}

fn stabilize_lvalue(lhs: &mut PreHirLValue, reps: &PureExprMap) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => stabilize_expr(ptr, reps),
        PreHirLValue::Index { base, index, .. } => {
            let base_changed = stabilize_expr(base, reps);
            let index_changed = stabilize_expr(index, reps);
            base_changed || index_changed
        }
        PreHirLValue::FieldAccess { base, .. } => stabilize_expr(base, reps),
    }
}

fn stabilize_expr(expr: &mut PreHirExpr, reps: &PureExprMap) -> bool {
    if let Some(key) = pure_expr_key(expr) {
        if let Some(name) = reps.get(&key) {
            *expr = PreHirExpr::Var(name.clone());
            return true;
        }
    }

    let mut changed = false;
    match expr {
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= stabilize_expr(lhs, reps);
            changed |= stabilize_expr(rhs, reps);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= stabilize_expr(arg, reps);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= stabilize_expr(base, reps);
            changed |= stabilize_expr(index, reps);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= stabilize_expr(cond, reps);
            changed |= stabilize_expr(then_expr, reps);
            changed |= stabilize_expr(else_expr, reps);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    changed
}

fn is_representable_expr(expr: &PreHirExpr) -> bool {
    !matches!(
        expr,
        PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(_, _)
    ) && pure_expr_key(expr).is_some()
}

fn invalidate_representative(map: &mut PureExprMap, defined_var: &str) {
    map.retain(|_, representative| representative != defined_var);
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_do_while_condition() {
        let mut func = PreHirFunction {
            name: "test_post_assign_do_while".to_string(),
            int_param_offsets: Vec::new(),
            return_type: NirType::Unknown,
            body: vec![PreHirStmt::DoWhile {
                body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("p".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(PreHirExpr::Const(2, int(64))),
                        ty: int(64),
                    },
                }]
                .into(),
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(PreHirExpr::Const(2, int(64))),
                        ty: int(64),
                    }),
                    rhs: Box::new(var("end")),
                    ty: int(64),
                },
            }],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let PreHirStmt::DoWhile { cond, .. } = &func.body[0] else {
            panic!("expected do-while");
        };
        let PreHirExpr::Binary { lhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(lhs.as_ref(), &var("p"));
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_if_condition() {
        let mut func = PreHirFunction {
            name: "test_post_assign_if".to_string(),
            int_param_offsets: Vec::new(),
            return_type: NirType::Unknown,
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("i".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(PreHirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(var("i")),
                            rhs: Box::new(PreHirExpr::Const(1, int(32))),
                            ty: int(32),
                        }),
                        ty: int(32),
                    },
                    then_body: vec![PreHirStmt::Goto("loop".to_string())].into(),
                    else_body: vec![].into(),
                },
            ],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let PreHirStmt::If { cond, .. } = &func.body[1] else {
            panic!("expected if");
        };
        let PreHirExpr::Binary { rhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(rhs.as_ref(), &var("i"));
    }

    #[test]
    fn invalidates_representative_when_representative_var_is_reassigned() {
        let mut func = PreHirFunction {
            name: "test_post_assign_invalidation".to_string(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("r".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("a")),
                        rhs: Box::new(var("b")),
                        ty: int(32),
                    },
                },
                // r is reassigned
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("r".to_string()),
                    rhs: PreHirExpr::Const(10, int(32)),
                },
                // if (a + b) must NOT be rewritten to if (r)
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("a")),
                        rhs: Box::new(var("b")),
                        ty: int(32),
                    },
                    then_body: vec![PreHirStmt::Return(None)].into(),
                    else_body: vec![].into(),
                },
            ],
            ..Default::default()
        };

        apply_post_assign_value_representative_pass(&mut func);
        let PreHirStmt::If { cond, .. } = &func.body[2] else {
            panic!()
        };
        assert!(matches!(cond, PreHirExpr::Binary { .. }));
    }

    #[test]
    fn does_not_carry_pre_loop_representative_when_loop_mutates_operand() {
        let add_ab = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("a")),
            rhs: Box::new(var("b")),
            ty: int(32),
        };
        let mut func = PreHirFunction {
            name: "test_post_assign_while_invalidation".to_string(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("r".to_string()),
                    rhs: add_ab.clone(),
                },
                PreHirStmt::While {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Lt,
                        lhs: Box::new(add_ab),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    },
                    body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("a".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(var("a")),
                            rhs: Box::new(PreHirExpr::Const(1, int(32))),
                            ty: int(32),
                        },
                    }]
                    .into(),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_post_assign_value_representative_pass(&mut func));
        let PreHirStmt::While { cond, .. } = &func.body[1] else {
            panic!("expected while");
        };
        let PreHirExpr::Binary { lhs, .. } = cond else {
            panic!("expected binary cond");
        };
        assert!(matches!(
            lhs.as_ref(),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn does_not_carry_pre_loop_representative_into_do_while_or_for_when_mutated() {
        let add_ab = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("a")),
            rhs: Box::new(var("b")),
            ty: int(32),
        };
        let mut func = PreHirFunction {
            name: "test_post_assign_do_for_invalidation".to_string(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("r".to_string()),
                    rhs: add_ab.clone(),
                },
                PreHirStmt::DoWhile {
                    body: vec![
                        PreHirStmt::If {
                            cond: PreHirExpr::Binary {
                                op: PreHirBinaryOp::Lt,
                                lhs: Box::new(add_ab.clone()),
                                rhs: Box::new(var("n")),
                                ty: NirType::Bool,
                            },
                            then_body: vec![PreHirStmt::Break].into(),
                            else_body: vec![].into(),
                        },
                        PreHirStmt::Assign {
                            lhs: PreHirLValue::Var("a".to_string()),
                            rhs: PreHirExpr::Binary {
                                op: PreHirBinaryOp::Add,
                                lhs: Box::new(var("a")),
                                rhs: Box::new(PreHirExpr::Const(1, int(32))),
                                ty: int(32),
                            },
                        },
                    ]
                    .into(),
                    cond: PreHirExpr::Const(1, int(32)),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("r2".to_string()),
                    rhs: add_ab.clone(),
                },
                PreHirStmt::For {
                    init: None,
                    cond: Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Lt,
                        lhs: Box::new(add_ab),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    }),
                    update: Some(Box::new(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("b".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(var("b")),
                            rhs: Box::new(PreHirExpr::Const(1, int(32))),
                            ty: int(32),
                        },
                    })),
                    body: vec![].into(),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_post_assign_value_representative_pass(&mut func));
    }
}
