use crate::HashMap;
use crate::analysis::defuse::DefUseMap;
use crate::cleanup::{expr_has_side_effects, expr_reads_memory};
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
            // The `if` runs after `var = default`, so its condition and its
            // override read the defaulted `var`. A `Select` in place of the
            // default runs before it and would read the previous value.
            let reads_var =
                expr_reads_var(&cond, &var_name) || expr_reads_var(&override_val, &var_name);
            let rhs = if reads_var {
                default_override_is_nonzero_test(&var_name, &cond, &override_val, &default_val, &ty)
            } else if default_survives_select(&cond, &default_val) {
                Some(PreHirExpr::Select {
                    cond: Box::new(cond),
                    then_expr: Box::new(override_val),
                    else_expr: Box::new(default_val),
                    ty,
                })
            } else {
                None
            };
            let Some(rhs) = rhs else {
                i += 1;
                continue;
            };
            stmts[i] = PreHirStmt::Assign {
                lhs: PreHirLValue::Var(var_name),
                rhs,
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

/// `var = default; if (cond) { var = override; }` evaluates `default` every
/// time, and before `cond`. The `Select` evaluates `cond` first and
/// `default` only when it is false. The two agree when `default` has no
/// effect to lose, and `cond` has none that could change what `default`
/// reads.
fn default_survives_select(cond: &PreHirExpr, default_val: &PreHirExpr) -> bool {
    if expr_has_side_effects(default_val) {
        return false;
    }
    if !expr_has_side_effects(cond) {
        return true;
    }
    let mut reads = crate::HashSet::default();
    crate::analysis::defuse::collect_expr_vars(default_val, &mut reads);
    reads.is_empty() && !expr_reads_memory(default_val)
}

fn expr_reads_var(expr: &PreHirExpr, name: &str) -> bool {
    let mut vars = crate::HashSet::default();
    crate::analysis::defuse::collect_expr_vars(expr, &mut vars);
    vars.contains(name)
}

fn strip_casts(expr: &PreHirExpr) -> &PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other,
    }
}

fn is_var(expr: &PreHirExpr, name: &str) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Var(v) if v == name)
}

fn is_zero(expr: &PreHirExpr) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Const(0, _))
}

/// `var = d; if (var != 0) { var = 1; }` leaves `var` at 1 when `d` is
/// nonzero and at `d`, which is then 0, when it is not -- so `var = d != 0`.
/// It is how a target without a set-on-condition instruction computes a
/// boolean, e.g. ARM's `cmp` + predicated `movne r0, #1`. The condition reads
/// `var`, so the general merge does not apply; this shape needs no
/// substitution because `d` stays evaluated exactly once.
fn default_override_is_nonzero_test(
    var: &str,
    cond: &PreHirExpr,
    override_val: &PreHirExpr,
    default_val: &PreHirExpr,
    ty: &NirType,
) -> Option<PreHirExpr> {
    if !matches!(strip_casts(override_val), PreHirExpr::Const(1, _)) {
        return None;
    }
    if expr_reads_var(default_val, var) {
        return None;
    }
    let tests_var_nonzero = match strip_casts(cond) {
        PreHirExpr::Var(name) => name == var,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => (is_var(lhs, var) && is_zero(rhs)) || (is_zero(lhs) && is_var(rhs, var)),
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => matches!(
            strip_casts(expr),
            PreHirExpr::Binary { op: PreHirBinaryOp::Eq, lhs, rhs, .. }
                if (is_var(lhs, var) && is_zero(rhs)) || (is_zero(lhs) && is_var(rhs, var))
        ),
        _ => false,
    };
    if !tests_var_nonzero {
        return None;
    }
    let default_ty = expr_type(default_val);
    let zero_ty = if default_ty == NirType::Unknown {
        ty.clone()
    } else {
        default_ty
    };
    Some(PreHirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs: Box::new(default_val.clone()),
            rhs: Box::new(PreHirExpr::Const(0, zero_ty)),
            ty: NirType::Bool,
        }),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_owned())
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn guarded(cond: PreHirExpr, then: PreHirStmt) -> PreHirStmt {
        PreHirStmt::If {
            cond,
            then_body: Rc::new(vec![then]),
            else_body: Rc::new(vec![]),
        }
    }

    fn load() -> PreHirExpr {
        PreHirExpr::Load {
            ptr: Box::new(var("p")),
            ty: u32_ty(),
        }
    }

    fn run(body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
        let mut func = PreHirFunction {
            body,
            ..Default::default()
        };
        apply_conditional_move_pass(&mut func);
        func.body
    }

    /// A default/override pair whose condition does not read the variable
    /// still merges into a select.
    #[test]
    fn merges_an_override_whose_condition_reads_something_else() {
        let body = run(vec![
            assign("x", PreHirExpr::Const(3, u32_ty())),
            guarded(var("c"), assign("x", PreHirExpr::Const(7, u32_ty()))),
        ]);
        assert!(matches!(
            &body[..],
            [PreHirStmt::Assign {
                rhs: PreHirExpr::Select { .. },
                ..
            }]
        ));
    }

    /// The `if` tests the value the default just stored. A select in the
    /// default's place would test whatever `x` held before.
    #[test]
    fn keeps_an_override_whose_condition_reads_the_defaulted_variable() {
        let gt5 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Gt,
            lhs: Box::new(var("x")),
            rhs: Box::new(PreHirExpr::Const(5, u32_ty())),
            ty: NirType::Bool,
        };
        let body = run(vec![
            assign("x", load()),
            guarded(gt5, assign("x", PreHirExpr::Const(7, u32_ty()))),
        ]);
        assert_eq!(
            body.len(),
            2,
            "merged into a select that reads the old x: {body:?}"
        );
        assert!(matches!(&body[1], PreHirStmt::If { .. }));
    }

    fn call(target: &str) -> PreHirExpr {
        PreHirExpr::Call {
            origin: None,
            target: target.to_owned(),
            args: Vec::new(),
            ty: u32_ty(),
        }
    }

    /// The default runs whether or not the override fires. As the else arm
    /// of a select, its call would be skipped whenever `c` holds.
    #[test]
    fn keeps_an_override_whose_default_has_an_effect() {
        let body = run(vec![
            assign("x", call("produce")),
            guarded(var("c"), assign("x", PreHirExpr::Const(7, u32_ty()))),
        ]);
        assert_eq!(body.len(), 2, "the call became conditional: {body:?}");
        assert!(matches!(&body[1], PreHirStmt::If { .. }));
    }

    /// The default is read before the condition runs. Behind a condition
    /// that calls, only a default the call cannot reach may move.
    #[test]
    fn keeps_a_default_a_calling_condition_could_change() {
        let body = run(vec![
            assign("x", load()),
            guarded(call("probe"), assign("x", PreHirExpr::Const(1, u32_ty()))),
        ]);
        assert_eq!(body.len(), 2, "the load moved past the call: {body:?}");

        let body = run(vec![
            assign("x", PreHirExpr::Const(0, u32_ty())),
            guarded(call("probe"), assign("x", PreHirExpr::Const(1, u32_ty()))),
        ]);
        assert!(matches!(
            &body[..],
            [PreHirStmt::Assign {
                rhs: PreHirExpr::Select { .. },
                ..
            }]
        ));
    }

    /// `x = d; if (x) x = 1;` is `x = d != 0`: the override fires exactly
    /// when `d` is nonzero, and otherwise `x` keeps `d`, which is 0.
    #[test]
    fn folds_a_set_to_one_when_nonzero_override_into_a_nonzero_test() {
        let body = run(vec![
            assign("x", load()),
            guarded(var("x"), assign("x", PreHirExpr::Const(1, u32_ty()))),
        ]);
        let [PreHirStmt::Assign { rhs, .. }] = &body[..] else {
            panic!("expected one assignment, got {body:?}");
        };
        let PreHirExpr::Cast { expr, .. } = rhs else {
            panic!("expected a cast of the nonzero test, got {rhs:?}");
        };
        assert!(matches!(
            expr.as_ref(),
            PreHirExpr::Binary { op: PreHirBinaryOp::Ne, lhs, rhs, .. }
                if matches!(lhs.as_ref(), PreHirExpr::Load { .. })
                    && matches!(rhs.as_ref(), PreHirExpr::Const(0, _))
        ));
    }
}
