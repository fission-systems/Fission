use crate::prelude::*;

pub fn apply_switch_norm_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    if process_statement_list(&mut func.body) {
        changed = true;
    }
    changed
}

fn process_statement_list(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;

    // 1. Recurse into nested blocks first (bottom-up)
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(body);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let PreHirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(init_body);
                    }
                }
                if let Some(update_stmt) = update {
                    if let PreHirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(update_body);
                    }
                }
                changed |= process_statement_list(body);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= process_statement_list(then_body);
                changed |= process_statement_list(else_body);
            }
            PreHirStmt::Switch { cases, default, .. } => {
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
        if let PreHirStmt::If {
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
                                stmts[i] = PreHirStmt::Switch {
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
                                stmts[i] = PreHirStmt::Switch {
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

fn strip_casts(expr: &PreHirExpr) -> &PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &PreHirExpr) -> Option<String> {
    match strip_casts(expr) {
        PreHirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn get_single_switch(stmts: &[PreHirStmt]) -> Option<(&PreHirExpr, &[PreHirSwitchCase], &[PreHirStmt])> {
    let block = if stmts.len() == 1 {
        match &stmts[0] {
            PreHirStmt::Block(inner) => inner.as_slice(),
            _ => stmts,
        }
    } else {
        stmts
    };
    if block.len() == 1 {
        if let PreHirStmt::Switch {
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

fn is_cmp_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Lt
            | PreHirBinaryOp::Le
            | PreHirBinaryOp::Gt
            | PreHirBinaryOp::Ge
            | PreHirBinaryOp::SLt
            | PreHirBinaryOp::SLe
            | PreHirBinaryOp::SGt
            | PreHirBinaryOp::SGe
            | PreHirBinaryOp::Eq
            | PreHirBinaryOp::Ne
    )
}

fn swap_cmp_op(op: PreHirBinaryOp) -> Option<PreHirBinaryOp> {
    match op {
        PreHirBinaryOp::Lt => Some(PreHirBinaryOp::Gt),
        PreHirBinaryOp::Le => Some(PreHirBinaryOp::Ge),
        PreHirBinaryOp::Gt => Some(PreHirBinaryOp::Lt),
        PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Le),
        PreHirBinaryOp::SLt => Some(PreHirBinaryOp::SGt),
        PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGe),
        PreHirBinaryOp::SGt => Some(PreHirBinaryOp::SLt),
        PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLe),
        PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Eq),
        PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Ne),
        _ => None,
    }
}

fn negate_cmp_op(op: PreHirBinaryOp) -> Option<PreHirBinaryOp> {
    match op {
        PreHirBinaryOp::Lt => Some(PreHirBinaryOp::Ge),
        PreHirBinaryOp::Le => Some(PreHirBinaryOp::Gt),
        PreHirBinaryOp::Gt => Some(PreHirBinaryOp::Le),
        PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Lt),
        PreHirBinaryOp::SLt => Some(PreHirBinaryOp::SGe),
        PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGt),
        PreHirBinaryOp::SGt => Some(PreHirBinaryOp::SLe),
        PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLt),
        PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Ne),
        PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Eq),
        _ => None,
    }
}

fn match_var_const_comparison(expr: &PreHirExpr) -> Option<(String, PreHirBinaryOp, i64)> {
    let expr = strip_casts(expr);
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, .. } => match (strip_casts(lhs), strip_casts(rhs)) {
            (PreHirExpr::Var(name), PreHirExpr::Const(val, _)) => {
                if is_cmp_op(*op) {
                    Some((name.clone(), *op, *val))
                } else {
                    None
                }
            }
            (PreHirExpr::Const(val, _), PreHirExpr::Var(name)) => {
                if let Some(swapped_op) = swap_cmp_op(*op) {
                    Some((name.clone(), swapped_op, *val))
                } else {
                    None
                }
            }
            _ => None,
        },
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
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
