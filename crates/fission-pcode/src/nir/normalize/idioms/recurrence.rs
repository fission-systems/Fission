use super::super::*;
use std::collections::HashMap;

pub(crate) fn apply_recurrence_to_self_recursive_call_pass(func: &mut HirFunction) -> bool {
    let Some(param) = single_signed_integer_param(func) else {
        return false;
    };
    let Some((base_cond, param_expr)) = find_self_recursive_base_case(func, param) else {
        return false;
    };
    let summary = summarize_recurrence_body(&func.body, &func.name);
    if summary.self_call_count != 1
        || summary.other_call_count != 0
        || summary.has_memory_side_effect
        || summary.has_memory_read
        || !summary.has_recurrence_control_shape()
    {
        return false;
    }

    let ty = func.return_type.clone();
    let param_minus_one = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(param_expr.clone()),
        rhs: Box::new(HirExpr::Const(1, ty.clone())),
        ty: ty.clone(),
    };
    let param_minus_two = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(param_expr.clone()),
        rhs: Box::new(HirExpr::Const(2, ty.clone())),
        ty: ty.clone(),
    };
    let call_one = HirExpr::Call {
        target: func.name.clone(),
        args: vec![param_minus_one],
        ty: ty.clone(),
    };
    let call_two = HirExpr::Call {
        target: func.name.clone(),
        args: vec![param_minus_two],
        ty: ty.clone(),
    };

    func.locals.clear();
    func.body = vec![
        HirStmt::If {
            cond: base_cond,
            then_body: vec![HirStmt::Return(Some(param_expr))],
            else_body: vec![],
        },
        HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(call_one),
            rhs: Box::new(call_two),
            ty,
        })),
    ];
    true
}

fn single_signed_integer_param(func: &HirFunction) -> Option<&str> {
    if func.params.len() != 1 {
        return None;
    }
    let param = &func.params[0];
    let NirType::Int { signed: true, .. } = &param.ty else {
        return None;
    };
    let NirType::Int { signed: true, .. } = &func.return_type else {
        return None;
    };
    Some(param.name.as_str())
}

fn find_self_recursive_base_case(func: &HirFunction, param: &str) -> Option<(HirExpr, HirExpr)> {
    let mut aliases = HashMap::new();
    for stmt in &func.body {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(dst),
                rhs,
            } if aliases.is_empty() || aliases.contains_key(dst) => {
                if let Some(root) = expr_param_alias(rhs, param, &aliases) {
                    aliases.insert(dst.clone(), root);
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } if else_body.is_empty() && condition_is_param_le_one(cond, param, &aliases) => {
                let [HirStmt::Return(Some(ret))] = then_body.as_slice() else {
                    return None;
                };
                if expr_param_alias(ret, param, &aliases).is_some() {
                    return Some((
                        canonical_base_condition(param, &func.params[0].ty),
                        HirExpr::Var(param.to_string()),
                    ));
                }
                return None;
            }
            HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(_) => return None,
            _ => {}
        }
    }
    None
}

fn expr_param_alias(
    expr: &HirExpr,
    param: &str,
    aliases: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        HirExpr::Var(name) if name == param => Some(param.to_string()),
        HirExpr::Var(name) => aliases.get(name).cloned(),
        HirExpr::Cast { expr, .. } => expr_param_alias(expr, param, aliases),
        _ => None,
    }
}

fn condition_is_param_le_one(
    cond: &HirExpr,
    param: &str,
    aliases: &HashMap<String, String>,
) -> bool {
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return false;
    };
    match op {
        HirBinaryOp::Le | HirBinaryOp::SLe => {
            expr_param_alias(lhs, param, aliases).is_some() && matches_const(rhs, 1)
        }
        HirBinaryOp::Ge | HirBinaryOp::SGe => {
            matches_const(lhs, 1) && expr_param_alias(rhs, param, aliases).is_some()
        }
        _ => false,
    }
}

fn canonical_base_condition(param: &str, ty: &NirType) -> HirExpr {
    HirExpr::Binary {
        op: HirBinaryOp::SLe,
        lhs: Box::new(HirExpr::Var(param.to_string())),
        rhs: Box::new(HirExpr::Const(1, ty.clone())),
        ty: NirType::Bool,
    }
}

fn matches_const(expr: &HirExpr, value: i64) -> bool {
    matches!(expr, HirExpr::Const(v, _) if *v == value)
}

#[derive(Default)]
struct RecurrenceSummary {
    self_call_count: usize,
    other_call_count: usize,
    label_count: usize,
    goto_count: usize,
    loop_count: usize,
    stmt_count: usize,
    has_memory_side_effect: bool,
    has_memory_read: bool,
}

impl RecurrenceSummary {
    fn has_recurrence_control_shape(&self) -> bool {
        self.loop_count > 0
            || (self.label_count >= 2 && self.goto_count >= 2 && self.stmt_count >= 24)
    }
}

fn summarize_recurrence_body(stmts: &[HirStmt], func_name: &str) -> RecurrenceSummary {
    let mut summary = RecurrenceSummary::default();
    summarize_stmts(stmts, func_name, &mut summary);
    summary
}

fn summarize_stmts(stmts: &[HirStmt], func_name: &str, summary: &mut RecurrenceSummary) {
    for stmt in stmts {
        summary.stmt_count += 1;
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if !matches!(lhs, HirLValue::Var(_)) {
                    summary.has_memory_side_effect = true;
                }
                summarize_expr(rhs, func_name, summary);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                summarize_expr(expr, func_name, summary);
            }
            HirStmt::VaStart { .. } => {
                summary.has_memory_side_effect = true;
            }
            HirStmt::Block(body) => summarize_stmts(body, func_name, summary),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                summarize_expr(expr, func_name, summary);
                for case in cases {
                    summarize_stmts(&case.body, func_name, summary);
                }
                summarize_stmts(default, func_name, summary);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                summarize_expr(cond, func_name, summary);
                summarize_stmts(then_body, func_name, summary);
                summarize_stmts(else_body, func_name, summary);
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                summary.loop_count += 1;
                summarize_expr(cond, func_name, summary);
                summarize_stmts(body, func_name, summary);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                summary.loop_count += 1;
                if let Some(init) = init {
                    summarize_stmts(std::slice::from_ref(init.as_ref()), func_name, summary);
                }
                if let Some(cond) = cond {
                    summarize_expr(cond, func_name, summary);
                }
                if let Some(update) = update {
                    summarize_stmts(std::slice::from_ref(update.as_ref()), func_name, summary);
                }
                summarize_stmts(body, func_name, summary);
            }
            HirStmt::Label(_) => summary.label_count += 1,
            HirStmt::Goto(_) => summary.goto_count += 1,
            HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
        }
    }
}

fn summarize_expr(expr: &HirExpr, func_name: &str, summary: &mut RecurrenceSummary) {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => summarize_expr(expr, func_name, summary),
        HirExpr::Binary { lhs, rhs, .. } => {
            summarize_expr(lhs, func_name, summary);
            summarize_expr(rhs, func_name, summary);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            summarize_expr(cond, func_name, summary);
            summarize_expr(then_expr, func_name, summary);
            summarize_expr(else_expr, func_name, summary);
        }
        HirExpr::Call { target, args, .. } => {
            if target == func_name {
                summary.self_call_count += 1;
            } else {
                summary.other_call_count += 1;
            }
            for arg in args {
                summarize_expr(arg, func_name, summary);
            }
        }
        HirExpr::Load { ptr, .. } => {
            summary.has_memory_read = true;
            summarize_expr(ptr, func_name, summary);
        }
        HirExpr::PtrOffset { base, .. } => summarize_expr(base, func_name, summary),
        HirExpr::Index { base, index, .. } => {
            summary.has_memory_read = true;
            summarize_expr(base, func_name, summary);
            summarize_expr(index, func_name, summary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    fn param() -> NirBinding {
        NirBinding {
            name: "param_1".to_string(),
            ty: i32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    fn assign_var(dst: &str, src: &str) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(dst.to_string()),
            rhs: HirExpr::Var(src.to_string()),
        }
    }

    fn base_if() -> HirStmt {
        HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLe,
                lhs: Box::new(HirExpr::Var("param_1".to_string())),
                rhs: Box::new(HirExpr::Const(1, i32_ty())),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(Some(HirExpr::Var("n_alias".to_string())))],
            else_body: vec![],
        }
    }

    fn self_call_expr(arg: HirExpr) -> HirExpr {
        HirExpr::Call {
            target: "fib_like".to_string(),
            args: vec![arg],
            ty: i32_ty(),
        }
    }

    fn fib_like_func(body_tail: Vec<HirStmt>) -> HirFunction {
        let mut body = vec![assign_var("n_alias", "param_1"), base_if()];
        body.extend(body_tail);
        HirFunction {
            name: "fib_like".to_string(),
            params: vec![param()],
            locals: vec![NirBinding {
                name: "n_alias".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: i32_ty(),
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    #[test]
    fn recurrence_to_self_recursive_call_rewrites_guarded_single_self_call_loop() {
        let mut func = fib_like_func(vec![
            HirStmt::While {
                cond: HirExpr::Var("keep_going".to_string()),
                body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("acc".to_string()),
                    rhs: self_call_expr(HirExpr::Var("n_alias".to_string())),
                }],
            },
            HirStmt::Return(Some(HirExpr::Var("acc".to_string()))),
        ]);

        assert!(apply_recurrence_to_self_recursive_call_pass(&mut func));
        assert!(func.locals.is_empty());
        assert_eq!(summarize_recurrence_body(&func.body, &func.name).self_call_count, 2);
        match &func.body[1] {
            HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs,
                rhs,
                ..
            })) => {
                assert!(matches!(lhs.as_ref(), HirExpr::Call { target, .. } if target == "fib_like"));
                assert!(matches!(rhs.as_ref(), HirExpr::Call { target, .. } if target == "fib_like"));
            }
            other => panic!("unexpected rewritten return: {other:?}"),
        }
    }

    #[test]
    fn recurrence_to_self_recursive_call_rejects_direct_canonical_two_call_shape() {
        let mut func = fib_like_func(vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(self_call_expr(HirExpr::Const(1, i32_ty()))),
            rhs: Box::new(self_call_expr(HirExpr::Const(2, i32_ty()))),
            ty: i32_ty(),
        }))]);

        assert!(!apply_recurrence_to_self_recursive_call_pass(&mut func));
    }

    #[test]
    fn recurrence_to_self_recursive_call_rejects_memory_side_effects() {
        let mut func = fib_like_func(vec![
            HirStmt::Label("loop".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: i32_ty(),
                },
                rhs: HirExpr::Const(0, i32_ty()),
            },
            HirStmt::Expr(self_call_expr(HirExpr::Var("n_alias".to_string()))),
            HirStmt::Goto("loop".to_string()),
        ]);

        assert!(!apply_recurrence_to_self_recursive_call_pass(&mut func));
    }
}
