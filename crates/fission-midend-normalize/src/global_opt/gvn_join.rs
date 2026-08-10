//! Join-point GVN-lite: hoist a shared pure RHS when both arms of `if` begin
//! with `x = E` and `y = E` (`x ≠ y`) and `pure_expr_key(E)` matches.
//!
//! Complements [`super::branch_hoist`] (same LHS, common prefix) and local CSE
//! (single linear block).  Hoisting introduces `__gvn_join_* = E` and rewrites
//! the first assignment in each arm to use the temp so
//! [`super::phi_recovery::copy_propagation_pass`] can eliminate copies.

use super::super::analysis::expr_key::pure_expr_key;
use super::super::analysis::preservation::preserved_binding_origin;
use super::super::cleanup::expr_has_side_effects;
use fission_midend_core::wave_stats;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

/// Hoist duplicate pure RHS on the first statement of both `if` arms when LHS
/// names differ.  Returns `true` if changed.
pub fn apply_gvn_join_hoist_pass(func: &mut PreHirFunction) -> bool {
    let mut ctr = func.locals.len() as u32;
    hoist_stmts(
        &mut func.body,
        &mut func.locals,
        func.params.as_slice(),
        &mut ctr,
    )
}

fn hoist_stmts(
    stmts: &mut Vec<PreHirStmt>,
    locals: &mut Vec<PreHirBinding>,
    params: &[PreHirBinding],
    ctr: &mut u32,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= hoist_stmt_deep(stmt, locals, params, ctr);
    }
    let mut i = 0;
    while i < stmts.len() {
        if let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &mut stmts[i]
        {
            if let Some((rhs, x, y)) = try_join_pair(then_body.as_slice(), else_body.as_slice()) {
                let tmp = alloc_temp_name(locals, params, ctr);
                let ty = expr_type(&rhs);
                locals.push(PreHirBinding {
                    name: tmp.clone(),
                    ty,
                    surface_type_name: None,
                    origin: Some(preserved_binding_origin()),
                    initializer: None,
                });
                wave_stats::add_gvn_join_preserved(1);
                let hoist = PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(tmp.clone()),
                    rhs,
                };
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body)[0] = PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(x),
                    rhs: PreHirExpr::Var(tmp.clone()),
                };
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body)[0] = PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(y),
                    rhs: PreHirExpr::Var(tmp),
                };
                stmts.insert(i, hoist);
                changed = true;
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    changed
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

    #[test]
    fn gvn_join_hoist_marks_temp_preserved() {
        let mut func = PreHirFunction {
            name: "test_gvn_join_preserved".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".to_string())),
                        rhs: Box::new(PreHirExpr::Var("b".to_string())),
                        ty: int(32),
                    },
                }].into(),
                else_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".to_string())),
                        rhs: Box::new(PreHirExpr::Var("b".to_string())),
                        ty: int(32),
                    },
                }].into(),
            }],
            ..Default::default()
        };

        assert!(apply_gvn_join_hoist_pass(&mut func));
        assert!(func.locals.iter().any(|binding| {
            binding.name.starts_with("__gvn_join_") && binding.preserves_materialization()
        }));
    }
}

fn hoist_stmt_deep(
    stmt: &mut PreHirStmt,
    locals: &mut Vec<PreHirBinding>,
    params: &[PreHirBinding],
    ctr: &mut u32,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), locals, params, ctr);
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), locals, params, ctr);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), locals, params, ctr);
        }
        PreHirStmt::For {
            init, body, update, ..
        } => {
            if let Some(i) = init {
                changed |= hoist_stmt_deep(i, locals, params, ctr);
            }
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), locals, params, ctr);
            if let Some(u) = update {
                changed |= hoist_stmt_deep(u, locals, params, ctr);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body), locals, params, ctr);
            }
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), locals, params, ctr);
        }
        PreHirStmt::Block(body) => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), locals, params, ctr);
        }
        _ => {}
    }
    changed
}

fn try_join_pair(
    then_body: &[PreHirStmt],
    else_body: &[PreHirStmt],
) -> Option<(PreHirExpr, String, String)> {
    if then_body.is_empty() || else_body.is_empty() {
        return None;
    }
    let (
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(a),
            rhs: ra,
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(b),
            rhs: rb,
        },
    ) = (&then_body[0], &else_body[0])
    else {
        return None;
    };
    if a == b {
        return None;
    }
    if expr_has_side_effects(ra) || expr_has_side_effects(rb) {
        return None;
    }
    let ka = pure_expr_key(ra)?;
    let kb = pure_expr_key(rb)?;
    if ka != kb {
        return None;
    }
    Some((ra.clone(), a.clone(), b.clone()))
}

fn alloc_temp_name(locals: &[PreHirBinding], params: &[PreHirBinding], ctr: &mut u32) -> String {
    loop {
        let name = format!("__gvn_join_{}", ctr);
        *ctr = ctr.wrapping_add(1);
        if !locals.iter().any(|b| b.name == name) && !params.iter().any(|p| p.name == name) {
            return name;
        }
    }
}
