use crate::prelude::*;
use fission_midend_dir::util::expr_type;
use crate::HashMap;

pub fn apply_conditional_move_pass(func: &mut DirFunction) -> bool {
    let mut type_map = HashMap::default();
    for param in &func.params {
        type_map.insert(param.name.clone(), param.ty.clone());
    }
    for local in &func.locals {
        type_map.insert(local.name.clone(), local.ty.clone());
    }

    let mut changed = false;
    if rewrite_stmts(&mut func.body, &type_map) {
        changed = true;
    }
    changed
}

fn rewrite_stmts(stmts: &mut Vec<DirStmt>, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // Step 1: Recursively simplify nested blocks first
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= rewrite_stmts(body, type_map);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    changed |= rewrite_stmt_nested(init_stmt.as_mut(), type_map);
                }
                if let Some(update_stmt) = update {
                    changed |= rewrite_stmt_nested(update_stmt.as_mut(), type_map);
                }
                changed |= rewrite_stmts(body, type_map);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_stmts(then_body, type_map);
                changed |= rewrite_stmts(else_body, type_map);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= rewrite_stmts(&mut case.body, type_map);
                }
                changed |= rewrite_stmts(default, type_map);
            }
            _ => {}
        }
    }

    // Step 2: Handle If-Then-Else pattern (in-place replacement of If statement)
    for stmt in stmts.iter_mut() {
        if let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        {
            if let Some((lhs_name, then_expr, else_expr)) = match_if_then_else(then_body, else_body)
            {
                let ty = type_map.get(&lhs_name).cloned().unwrap_or_else(|| {
                    let et = expr_type(&then_expr);
                    if et != NirType::Unknown {
                        et
                    } else {
                        expr_type(&else_expr)
                    }
                });
                *stmt = DirStmt::Assign {
                    lhs: DirLValue::Var(lhs_name),
                    rhs: DirExpr::Select {
                        cond: Box::new(cond.clone()),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                        ty,
                    },
                };
                changed = true;
            }
        }
    }

    // Step 3: Handle Default-Override pattern (merging adjacent statements)
    let mut i = 0;
    while i < stmts.len().saturating_sub(1) {
        let is_match = {
            let left = &stmts[i];
            let right = &stmts[i + 1];
            match (left, right) {
                (
                    DirStmt::Assign {
                        lhs: DirLValue::Var(var_l),
                        rhs: default_val,
                    },
                    DirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    },
                ) if else_body.is_empty() => {
                    if let Some((var_r, override_val)) = match_single_assign(then_body) {
                        if var_l == &var_r {
                            Some((
                                var_l.clone(),
                                cond.clone(),
                                override_val,
                                default_val.clone(),
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };

        if let Some((var_name, cond, override_val, default_val)) = is_match {
            let ty = type_map.get(&var_name).cloned().unwrap_or_else(|| {
                let et = expr_type(&override_val);
                if et != NirType::Unknown {
                    et
                } else {
                    expr_type(&default_val)
                }
            });
            stmts[i] = DirStmt::Assign {
                lhs: DirLValue::Var(var_name),
                rhs: DirExpr::Select {
                    cond: Box::new(cond),
                    then_expr: Box::new(override_val),
                    else_expr: Box::new(default_val),
                    ty,
                },
            };
            stmts.remove(i + 1);
            changed = true;
            // Do not increment i, examine the merged statement against the next one
        } else {
            i += 1;
        }
    }

    changed
}

fn rewrite_stmt_nested(stmt: &mut DirStmt, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= rewrite_stmts(body, type_map);
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init_stmt) = init {
                changed |= rewrite_stmt_nested(init_stmt.as_mut(), type_map);
            }
            if let Some(update_stmt) = update {
                changed |= rewrite_stmt_nested(update_stmt.as_mut(), type_map);
            }
            changed |= rewrite_stmts(body, type_map);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            // Check if this If itself is matchable
            if let Some((lhs_name, then_expr, else_expr)) = match_if_then_else(then_body, else_body)
            {
                let ty = type_map.get(&lhs_name).cloned().unwrap_or_else(|| {
                    let et = expr_type(&then_expr);
                    if et != NirType::Unknown {
                        et
                    } else {
                        expr_type(&else_expr)
                    }
                });
                *stmt = DirStmt::Assign {
                    lhs: DirLValue::Var(lhs_name),
                    rhs: DirExpr::Select {
                        cond: Box::new(cond.clone()),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                        ty,
                    },
                };
                changed = true;
            } else {
                changed |= rewrite_stmts(then_body, type_map);
                changed |= rewrite_stmts(else_body, type_map);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                changed |= rewrite_stmts(&mut case.body, type_map);
            }
            changed |= rewrite_stmts(default, type_map);
        }
        _ => {}
    }
    changed
}

/// Matches a single assignment to a variable in a block, e.g. `[x = val;]`
fn match_single_assign(body: &[DirStmt]) -> Option<(String, DirExpr)> {
    if body.len() != 1 {
        return None;
    }
    match &body[0] {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => Some((name.clone(), rhs.clone())),
        _ => None,
    }
}

/// Matches `then_body = [x = a;]` and `else_body = [x = b;]`, returning `Some((x, a, b))`
fn match_if_then_else(
    then_body: &[DirStmt],
    else_body: &[DirStmt],
) -> Option<(String, DirExpr, DirExpr)> {
    let (var_then, expr_then) = match_single_assign(then_body)?;
    let (var_else, expr_else) = match_single_assign(else_body)?;
    if var_then == var_else {
        Some((var_then, expr_then, expr_else))
    } else {
        None
    }
}
