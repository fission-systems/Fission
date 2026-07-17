use crate::prelude::*;

pub fn apply_switch_norm_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    if process_statement_list(&mut func.body) {
        changed = true;
    }
    changed
}

fn process_statement_list(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;

    // 1. Recurse into nested blocks first (bottom-up)
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(init_body);
                    }
                }
                if let Some(update_stmt) = update {
                    if let HirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(update_body);
                    }
                }
                changed |= process_statement_list(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= process_statement_list(then_body);
                changed |= process_statement_list(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= process_statement_list(&mut case.body);
                }
                changed |= process_statement_list(default);
            }
            _ => {}
        }
    }

    // 2. Process switch guard folding at the current level
    let mut i = 0;
    while i < stmts.len() {
        if let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if let Some((var_name, _op, _limit)) = match_var_const_comparison(cond) {
                // Check if one side is a single Switch on `var_name`
                let then_switch = get_single_switch(then_body);
                let else_switch = get_single_switch(else_body);

                match (then_switch, else_switch) {
                    (Some((sw_expr, sw_cases, sw_default)), None) => {
                        if get_var_name(sw_expr) == Some(var_name.clone()) {
                            if sw_default.is_empty()
                                || else_body.is_empty()
                                || sw_default == else_body
                            {
                                let new_default = if sw_default.is_empty() {
                                    else_body.clone()
                                } else {
                                    sw_default.to_vec()
                                };
                                stmts[i] = HirStmt::Switch {
                                    expr: sw_expr.clone(),
                                    cases: sw_cases.to_vec(),
                                    default: new_default,
                                };
                                changed = true;
                            }
                        }
                    }
                    (None, Some((sw_expr, sw_cases, sw_default))) => {
                        if get_var_name(sw_expr) == Some(var_name.clone()) {
                            if sw_default.is_empty()
                                || then_body.is_empty()
                                || sw_default == then_body
                            {
                                let new_default = if sw_default.is_empty() {
                                    then_body.clone()
                                } else {
                                    sw_default.to_vec()
                                };
                                stmts[i] = HirStmt::Switch {
                                    expr: sw_expr.clone(),
                                    cases: sw_cases.to_vec(),
                                    default: new_default,
                                };
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }

    changed
}

fn strip_casts(expr: &HirExpr) -> &HirExpr {
    match expr {
        HirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &HirExpr) -> Option<String> {
    match strip_casts(expr) {
        HirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn get_single_switch(stmts: &[HirStmt]) -> Option<(&HirExpr, &[HirSwitchCase], &[HirStmt])> {
    let block = if stmts.len() == 1 {
        match &stmts[0] {
            HirStmt::Block(inner) => inner.as_slice(),
            _ => stmts,
        }
    } else {
        stmts
    };
    if block.len() == 1 {
        if let HirStmt::Switch {
            expr,
            cases,
            default,
        } = &block[0]
        {
            return Some((expr, cases, default));
        }
    }
    None
}

fn is_cmp_op(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
            | HirBinaryOp::Eq
            | HirBinaryOp::Ne
    )
}

fn swap_cmp_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    match op {
        HirBinaryOp::Lt => Some(HirBinaryOp::Gt),
        HirBinaryOp::Le => Some(HirBinaryOp::Ge),
        HirBinaryOp::Gt => Some(HirBinaryOp::Lt),
        HirBinaryOp::Ge => Some(HirBinaryOp::Le),
        HirBinaryOp::SLt => Some(HirBinaryOp::SGt),
        HirBinaryOp::SLe => Some(HirBinaryOp::SGe),
        HirBinaryOp::SGt => Some(HirBinaryOp::SLt),
        HirBinaryOp::SGe => Some(HirBinaryOp::SLe),
        HirBinaryOp::Eq => Some(HirBinaryOp::Eq),
        HirBinaryOp::Ne => Some(HirBinaryOp::Ne),
        _ => None,
    }
}

fn negate_cmp_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    match op {
        HirBinaryOp::Lt => Some(HirBinaryOp::Ge),
        HirBinaryOp::Le => Some(HirBinaryOp::Gt),
        HirBinaryOp::Gt => Some(HirBinaryOp::Le),
        HirBinaryOp::Ge => Some(HirBinaryOp::Lt),
        HirBinaryOp::SLt => Some(HirBinaryOp::SGe),
        HirBinaryOp::SLe => Some(HirBinaryOp::SGt),
        HirBinaryOp::SGt => Some(HirBinaryOp::SLe),
        HirBinaryOp::SGe => Some(HirBinaryOp::SLt),
        HirBinaryOp::Eq => Some(HirBinaryOp::Ne),
        HirBinaryOp::Ne => Some(HirBinaryOp::Eq),
        _ => None,
    }
}

fn match_var_const_comparison(expr: &HirExpr) -> Option<(String, HirBinaryOp, i64)> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary { op, lhs, rhs, .. } => match (strip_casts(lhs), strip_casts(rhs)) {
            (HirExpr::Var(name), HirExpr::Const(val, _)) => {
                if is_cmp_op(*op) {
                    Some((name.clone(), *op, *val))
                } else {
                    None
                }
            }
            (HirExpr::Const(val, _), HirExpr::Var(name)) => {
                if let Some(swapped_op) = swap_cmp_op(*op) {
                    Some((name.clone(), swapped_op, *val))
                } else {
                    None
                }
            }
            _ => None,
        },
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: inner,
            ..
        } => {
            let (name, inner_op, val) = match_var_const_comparison(inner)?;
            let negated_op = negate_cmp_op(inner_op)?;
            Some((name, negated_op, val))
        }
        _ => None,
    }
}
