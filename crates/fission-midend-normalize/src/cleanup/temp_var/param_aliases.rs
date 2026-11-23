use super::super::utils::expr_mentions_var;
use crate::prelude::*;

/// Hoist pure `v = param_N` copies above their first use when a prior pass
/// left a use-before-def (observed after cmov/flag recovery on x86-32 clamp).
///
/// Only applies to top-level straight-line bodies: a single dominating
/// definition of `v` that is a param alias, with an earlier pure use.
pub fn hoist_param_alias_copies_before_first_use(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0usize;
    while i < stmts.len() {
        let (name, param) = match &stmts[i] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Var(param),
            } if param.starts_with("param_") => (name.clone(), param.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        // Only hoist when this is the sole top-level def of `name`.
        let def_count = stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(n),
                        ..
                    } if n == &name
                )
            })
            .count();
        if def_count != 1 {
            i += 1;
            continue;
        }
        if let Some(first_use) = first_top_level_use_index_of_var(stmts, &name) {
            if first_use < i {
                let stmt = stmts.remove(i);
                stmts.insert(first_use, stmt);
                changed = true;
                // Restart so chained hoists are ordered correctly.
                i = 0;
                continue;
            }
        }
        let _ = param;
        i += 1;
    }
    // Recurse into structured bodies.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        hoist_param_alias_copies_before_first_use(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        );
                }
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }
    changed
}

fn first_top_level_use_index_of_var(stmts: &[PreHirStmt], name: &str) -> Option<usize> {
    for (idx, stmt) in stmts.iter().enumerate() {
        if stmt_uses_var(stmt, name) {
            return Some(idx);
        }
    }
    None
}

fn stmt_uses_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var(lhs, name) || expr_mentions_var(rhs, name)
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::Return(Some(expr))
        | PreHirStmt::VaStart { va_list: expr, .. } => expr_mentions_var(expr, name),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_mentions_var(cond, name)
                || then_body.iter().any(|s| stmt_uses_var(s, name))
                || else_body.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => body.iter().any(|s| stmt_uses_var(s, name)),
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || cond.as_ref().is_some_and(|c| expr_mentions_var(c, name))
                || update.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || body.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_mentions_var(expr, name)
                || cases
                    .iter()
                    .any(|c| c.body.iter().any(|s| stmt_uses_var(s, name)))
                || default.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn lvalue_uses_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false, // definition, not a use
        PreHirLValue::Deref { ptr, .. } => expr_mentions_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_mentions_var(base, name),
    }
}
