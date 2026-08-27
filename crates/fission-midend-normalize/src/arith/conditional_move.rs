use crate::HashMap;
use crate::analysis::defuse::DefUseMap;
use crate::cleanup::expr_has_side_effects;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

pub fn apply_conditional_move_pass(func: &mut PreHirFunction) -> bool {
    let mut type_map = HashMap::default();
    for param in &func.params {
        type_map.insert(param.name.clone(), param.ty.clone());
    }
    for local in &func.locals {
        type_map.insert(local.name.clone(), local.ty.clone());
    }
    // Whole-function use counts, so Step 3 below can safely tell whether a
    // predicate temp sitting between a default assign and its guarding `if`
    // is read anywhere else before inlining it away.
    let use_counts = DefUseMap::build(&func.body).use_count;

    let mut changed = false;
    if rewrite_stmts(&mut func.body, &type_map, &use_counts) {
        changed = true;
    }
    changed
}

fn rewrite_stmts(
    stmts: &mut Vec<PreHirStmt>,
    type_map: &HashMap<String, NirType>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;

    // Step 1: Recursively simplify nested blocks first
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    type_map,
                    use_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    changed |= rewrite_stmt_nested(init_stmt.as_mut(), type_map, use_counts);
                }
                if let Some(update_stmt) = update {
                    changed |= rewrite_stmt_nested(update_stmt.as_mut(), type_map, use_counts);
                }
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    type_map,
                    use_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    type_map,
                    use_counts,
                );
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    type_map,
                    use_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= rewrite_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        type_map,
                        use_counts,
                    );
                }
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    type_map,
                    use_counts,
                );
            }
            _ => {}
        }
    }

    // Step 2: Handle If-Then-Else pattern (in-place replacement of If statement)
    for stmt in stmts.iter_mut() {
        if let PreHirStmt::If {
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
                *stmt = PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(lhs_name),
                    rhs: PreHirExpr::Select {
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

    // Step 3: Handle Default-Override pattern (merging adjacent statements,
    // or statements separated by exactly one single-use predicate temp --
    // see `try_match_default_override`).
    let mut i = 0;
    while i < stmts.len().saturating_sub(1) {
        let is_match = try_match_default_override(stmts, i, use_counts);

        if let Some((var_name, cond, override_val, default_val, consumed)) = is_match {
            let ty = type_map.get(&var_name).cloned().unwrap_or_else(|| {
                let et = expr_type(&override_val);
                if et != NirType::Unknown {
                    et
                } else {
                    expr_type(&default_val)
                }
            });
            stmts[i] = PreHirStmt::Assign {
                lhs: PreHirLValue::Var(var_name),
                rhs: PreHirExpr::Select {
                    cond: Box::new(cond),
                    then_expr: Box::new(override_val),
                    else_expr: Box::new(default_val),
                    ty,
                },
            };
            for _ in 0..consumed {
                stmts.remove(i + 1);
            }
            changed = true;
            // Do not increment i, examine the merged statement against the next one
        } else {
            i += 1;
        }
    }

    changed
}

/// Matches the "Default-Override" shape at `stmts[i]`:
/// `var = default; if (cond) { var = override; }` (with `else` empty),
/// returning `(var, cond, override, default, consumed)` where `consumed` is
/// how many statements after `stmts[i]` the match spans (so the caller knows
/// how many to remove).
///
/// Also tolerates exactly one intervening single-use predicate-defining
/// assignment between the default and the `if` -- GCC/Clang routinely
/// materialize a flag/comparison into a named temp one statement before
/// testing it (`xVar15 = uVar20 <= 0; if (!xVar15) ...`), which otherwise
/// defeats simple positional adjacency even though nothing about the shape
/// is actually different. Only inlines the two narrowest, unambiguous cond
/// shapes (`pred` bare, or `!pred`) -- not a general substitution -- and
/// only when `pred` is provably read nowhere else in the function (via the
/// whole-function `use_counts` built once in `apply_conditional_move_pass`)
/// and its definition is pure, so hoisting its evaluation into the merged
/// `Select` at `stmts[i]`'s position changes nothing observable.
fn try_match_default_override(
    stmts: &[PreHirStmt],
    i: usize,
    use_counts: &HashMap<String, usize>,
) -> Option<(String, PreHirExpr, PreHirExpr, PreHirExpr, usize)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(var_l),
        rhs: default_val,
    } = &stmts[i]
    else {
        return None;
    };

    if let Some((cond, override_val)) = match_default_override_if(var_l, stmts.get(i + 1)) {
        return Some((var_l.clone(), cond, override_val, default_val.clone(), 1));
    }

    if let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(pred_name),
        rhs: pred_expr,
    } = stmts.get(i + 1)?
    {
        if pred_name != var_l
            && !expr_has_side_effects(pred_expr)
            && use_counts.get(pred_name.as_str()).copied().unwrap_or(0) == 1
        {
            let (cond, override_val) = match_default_override_if(var_l, stmts.get(i + 2))?;
            let negated = cond_predicate_polarity(&cond, pred_name)?;
            let inlined_cond = if negated {
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(pred_expr.clone()),
                    ty: NirType::Bool,
                }
            } else {
                pred_expr.clone()
            };
            return Some((
                var_l.clone(),
                inlined_cond,
                override_val,
                default_val.clone(),
                2,
            ));
        }
    }

    None
}

/// If `stmt` is `If { cond, then_body: [var_l = override;], else_body: [] }`,
/// returns `(cond, override)`.
fn match_default_override_if(
    var_l: &str,
    stmt: Option<&PreHirStmt>,
) -> Option<(PreHirExpr, PreHirExpr)> {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt?
    else {
        return None;
    };
    if !else_body.is_empty() {
        return None;
    }
    let (var_r, override_val) = match_single_assign(then_body)?;
    if var_r == var_l {
        Some((cond.clone(), override_val))
    } else {
        None
    }
}

/// `true` if `cond` is exactly `!Var(pred_name)`, `false` if exactly
/// `Var(pred_name)`, `None` for anything else -- deliberately narrow so this
/// never has to reason about a compound condition.
fn cond_predicate_polarity(cond: &PreHirExpr, pred_name: &str) -> Option<bool> {
    match cond {
        PreHirExpr::Var(name) if name == pred_name => Some(false),
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            PreHirExpr::Var(name) if name == pred_name => Some(true),
            _ => None,
        },
        _ => None,
    }
}

fn rewrite_stmt_nested(
    stmt: &mut PreHirStmt,
    type_map: &HashMap<String, NirType>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            changed |= rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                use_counts,
            );
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init_stmt) = init {
                changed |= rewrite_stmt_nested(init_stmt.as_mut(), type_map, use_counts);
            }
            if let Some(update_stmt) = update {
                changed |= rewrite_stmt_nested(update_stmt.as_mut(), type_map, use_counts);
            }
            changed |= rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                use_counts,
            );
        }
        PreHirStmt::If {
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
                *stmt = PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(lhs_name),
                    rhs: PreHirExpr::Select {
                        cond: Box::new(cond.clone()),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                        ty,
                    },
                };
                changed = true;
            } else {
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    type_map,
                    use_counts,
                );
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    type_map,
                    use_counts,
                );
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                changed |= rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    type_map,
                    use_counts,
                );
            }
            changed |= rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                type_map,
                use_counts,
            );
        }
        _ => {}
    }
    changed
}

/// Matches a single assignment to a variable in a block, e.g. `[x = val;]`
fn match_single_assign(body: &[PreHirStmt]) -> Option<(String, PreHirExpr)> {
    if body.len() != 1 {
        return None;
    }
    match &body[0] {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => Some((name.clone(), rhs.clone())),
        _ => None,
    }
}

/// Matches `then_body = [x = a;]` and `else_body = [x = b;]`, returning `Some((x, a, b))`
fn match_if_then_else(
    then_body: &[PreHirStmt],
    else_body: &[PreHirStmt],
) -> Option<(String, PreHirExpr, PreHirExpr)> {
    let (var_then, expr_then) = match_single_assign(then_body)?;
    let (var_else, expr_else) = match_single_assign(else_body)?;
    if var_then == var_else {
        Some((var_then, expr_then, expr_else))
    } else {
        None
    }
}
