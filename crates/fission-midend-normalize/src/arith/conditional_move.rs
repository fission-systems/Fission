use crate::prelude::*;
use fission_midend_core::expr_type;
use std::collections::HashMap;

pub fn apply_conditional_move_pass(func: &mut HirFunction) -> bool {
    let mut type_map = HashMap::new();
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

fn rewrite_stmts(stmts: &mut Vec<HirStmt>, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // Step 1: Recursively simplify nested blocks first
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= rewrite_stmts(body, type_map);
            }
            HirStmt::For {
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
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_stmts(then_body, type_map);
                changed |= rewrite_stmts(else_body, type_map);
            }
            HirStmt::Switch { cases, default, .. } => {
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
        if let HirStmt::If {
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
                *stmt = HirStmt::Assign {
                    lhs: HirLValue::Var(lhs_name),
                    rhs: HirExpr::Select {
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
                    HirStmt::Assign {
                        lhs: HirLValue::Var(var_l),
                        rhs: default_val,
                    },
                    HirStmt::If {
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
            stmts[i] = HirStmt::Assign {
                lhs: HirLValue::Var(var_name),
                rhs: HirExpr::Select {
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

fn rewrite_stmt_nested(stmt: &mut HirStmt, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= rewrite_stmts(body, type_map);
        }
        HirStmt::For {
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
        HirStmt::If {
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
                *stmt = HirStmt::Assign {
                    lhs: HirLValue::Var(lhs_name),
                    rhs: HirExpr::Select {
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
        HirStmt::Switch { cases, default, .. } => {
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
fn match_single_assign(body: &[HirStmt]) -> Option<(String, HirExpr)> {
    if body.len() != 1 {
        return None;
    }
    match &body[0] {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => Some((name.clone(), rhs.clone())),
        _ => None,
    }
}

/// Matches `then_body = [x = a;]` and `else_body = [x = b;]`, returning `Some((x, a, b))`
fn match_if_then_else(
    then_body: &[HirStmt],
    else_body: &[HirStmt],
) -> Option<(String, HirExpr, HirExpr)> {
    let (var_then, expr_then) = match_single_assign(then_body)?;
    let (var_else, expr_else) = match_single_assign(else_body)?;
    if var_then == var_else {
        Some((var_then, expr_then, expr_else))
    } else {
        None
    }
}
