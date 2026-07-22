use crate::prelude::*;

pub fn apply_switch_norm_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;
    if process_statement_list(&mut func.body) {
        changed = true;
    }
    changed
}

fn process_statement_list(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;

    // 1. Recurse into nested blocks first (bottom-up)
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(body);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let DirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(init_body);
                    }
                }
                if let Some(update_stmt) = update {
                    if let DirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(update_body);
                    }
                }
                changed |= process_statement_list(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= process_statement_list(then_body);
                changed |= process_statement_list(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
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
        if let DirStmt::If {
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
                                stmts[i] = DirStmt::Switch {
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
                                stmts[i] = DirStmt::Switch {
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

fn strip_casts(expr: &DirExpr) -> &DirExpr {
    match expr {
        DirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &DirExpr) -> Option<String> {
    match strip_casts(expr) {
        DirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn get_single_switch(stmts: &[DirStmt]) -> Option<(&DirExpr, &[DirSwitchCase], &[DirStmt])> {
    let block = if stmts.len() == 1 {
        match &stmts[0] {
            DirStmt::Block(inner) => inner.as_slice(),
            _ => stmts,
        }
    } else {
        stmts
    };
    if block.len() == 1 {
        if let DirStmt::Switch {
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

fn is_cmp_op(op: DirBinaryOp) -> bool {
    matches!(
        op,
        DirBinaryOp::Lt
            | DirBinaryOp::Le
            | DirBinaryOp::Gt
            | DirBinaryOp::Ge
            | DirBinaryOp::SLt
            | DirBinaryOp::SLe
            | DirBinaryOp::SGt
            | DirBinaryOp::SGe
            | DirBinaryOp::Eq
            | DirBinaryOp::Ne
    )
}

fn swap_cmp_op(op: DirBinaryOp) -> Option<DirBinaryOp> {
    match op {
        DirBinaryOp::Lt => Some(DirBinaryOp::Gt),
        DirBinaryOp::Le => Some(DirBinaryOp::Ge),
        DirBinaryOp::Gt => Some(DirBinaryOp::Lt),
        DirBinaryOp::Ge => Some(DirBinaryOp::Le),
        DirBinaryOp::SLt => Some(DirBinaryOp::SGt),
        DirBinaryOp::SLe => Some(DirBinaryOp::SGe),
        DirBinaryOp::SGt => Some(DirBinaryOp::SLt),
        DirBinaryOp::SGe => Some(DirBinaryOp::SLe),
        DirBinaryOp::Eq => Some(DirBinaryOp::Eq),
        DirBinaryOp::Ne => Some(DirBinaryOp::Ne),
        _ => None,
    }
}

fn negate_cmp_op(op: DirBinaryOp) -> Option<DirBinaryOp> {
    match op {
        DirBinaryOp::Lt => Some(DirBinaryOp::Ge),
        DirBinaryOp::Le => Some(DirBinaryOp::Gt),
        DirBinaryOp::Gt => Some(DirBinaryOp::Le),
        DirBinaryOp::Ge => Some(DirBinaryOp::Lt),
        DirBinaryOp::SLt => Some(DirBinaryOp::SGe),
        DirBinaryOp::SLe => Some(DirBinaryOp::SGt),
        DirBinaryOp::SGt => Some(DirBinaryOp::SLe),
        DirBinaryOp::SGe => Some(DirBinaryOp::SLt),
        DirBinaryOp::Eq => Some(DirBinaryOp::Ne),
        DirBinaryOp::Ne => Some(DirBinaryOp::Eq),
        _ => None,
    }
}

fn match_var_const_comparison(expr: &DirExpr) -> Option<(String, DirBinaryOp, i64)> {
    let expr = strip_casts(expr);
    match expr {
        DirExpr::Binary { op, lhs, rhs, .. } => match (strip_casts(lhs), strip_casts(rhs)) {
            (DirExpr::Var(name), DirExpr::Const(val, _)) => {
                if is_cmp_op(*op) {
                    Some((name.clone(), *op, *val))
                } else {
                    None
                }
            }
            (DirExpr::Const(val, _), DirExpr::Var(name)) => {
                if let Some(swapped_op) = swap_cmp_op(*op) {
                    Some((name.clone(), swapped_op, *val))
                } else {
                    None
                }
            }
            _ => None,
        },
        DirExpr::Unary {
            op: DirUnaryOp::Not,
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
