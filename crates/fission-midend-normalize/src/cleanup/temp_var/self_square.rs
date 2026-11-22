use super::super::utils::{count_var_uses_in_stmt, is_trivial_temp_name};
use crate::prelude::*;

/// Collapse pure temp self-square chains:
/// `t = a; t = t * t; a = t`  →  `a = a * a`
/// when `t` is a trivial temp and has no other live uses in between.
///
/// Measured on x86 `imul` squaring for power-class loops (`base *= base`).
pub fn collapse_temp_self_square_assigns(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    changed |= collapse_temp_self_square_linear(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |= collapse_temp_self_square_assigns(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                        );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |= collapse_temp_self_square_assigns(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                        );
                    }
                }
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_temp_self_square_assigns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    );
                }
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }
    changed
}

fn collapse_temp_self_square_linear(stmts: &mut Vec<PreHirStmt>) -> bool {
    if stmts.len() < 3 {
        return false;
    }
    let mut changed = false;
    let mut i = 0usize;
    while i + 2 < stmts.len() {
        let Some((t_name, a_name, ty)) = match_temp_self_square_window(stmts, i) else {
            i += 1;
            continue;
        };
        // Refuse if `t` is used outside this three-stmt window in this linear list.
        let other_t_uses: usize = stmts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx < i || *idx > i + 2)
            .map(|(_, s)| count_var_uses_in_stmt(s, &t_name))
            .sum();
        if other_t_uses > 0 {
            i += 1;
            continue;
        }
        stmts[i] = PreHirStmt::Assign {
            lhs: PreHirLValue::Var(a_name.clone()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(PreHirExpr::Var(a_name.clone())),
                rhs: Box::new(PreHirExpr::Var(a_name)),
                ty,
            },
        };
        stmts.remove(i + 2);
        stmts.remove(i + 1);
        changed = true;
    }
    changed
}

fn match_temp_self_square_window(
    stmts: &[PreHirStmt],
    i: usize,
) -> Option<(String, String, NirType)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(t1),
        rhs: PreHirExpr::Var(a1),
    } = &stmts[i]
    else {
        return None;
    };
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(t2),
        rhs:
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs,
                rhs,
                ty,
            },
    } = &stmts[i + 1]
    else {
        return None;
    };
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(a2),
        rhs: PreHirExpr::Var(t3),
    } = &stmts[i + 2]
    else {
        return None;
    };
    let (PreHirExpr::Var(tl), PreHirExpr::Var(tr)) = (lhs.as_ref(), rhs.as_ref()) else {
        return None;
    };
    if t1 != t2 || t1 != t3 || t1 != tl || t1 != tr || a1 != a2 {
        return None;
    }
    if !is_trivial_temp_name(t1) || t1 == a1 {
        return None;
    }
    Some((t1.clone(), a1.clone(), ty.clone()))
}
