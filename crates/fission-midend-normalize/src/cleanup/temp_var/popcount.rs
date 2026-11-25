use super::super::utils::expr_has_side_effects;
use crate::HashSet;
use crate::analysis::defuse::DefUseMap;
use crate::prelude::*;

pub fn elide_unused_popcount_assigns(func: &mut PreHirFunction) -> bool {
    if !func.body.iter().any(has_popcount) {
        return false;
    }
    let use_map = DefUseMap::build(&func.body);

    let mut changed = false;
    for _ in 0..8 {
        let round_changed = elide_popcount_round(func, &use_map);
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

fn elide_popcount_round(func: &mut PreHirFunction, use_map: &DefUseMap) -> bool {
    let mut changed = false;
    elide_popcount_in_stmts(&mut func.body, use_map, &mut changed);
    if changed {
        let remaining_names: HashSet<String> =
            func.body.iter().flat_map(collect_assigned_names).collect();
        func.locals.retain(|b| {
            remaining_names.contains(&b.name)
                || use_map.use_count.get(&b.name).copied().unwrap_or(0) > 0
        });
    }
    changed
}

fn collect_assigned_names(stmt: &PreHirStmt) -> Vec<String> {
    let mut names = Vec::new();
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            names.push(name.clone());
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter().chain(else_body.iter()) {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::For { body, .. } => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    names.extend(collect_assigned_names(s));
                }
            }
            for s in default.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        _ => {}
    }
    names
}

fn elide_popcount_in_stmts(stmts: &mut Vec<PreHirStmt>, use_map: &DefUseMap, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body) => elide_popcount_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                use_map,
                changed,
            ),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    use_map,
                    changed,
                );
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::For { body, .. } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    elide_popcount_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        use_map,
                        changed,
                    );
                }
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    use_map,
                    changed,
                );
            }
            _ => {}
        }
    }
    stmts.retain(|stmt| {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = stmt
        {
            let uses = use_map.use_count.get(name.as_str()).copied().unwrap_or(0);
            if uses == 0 && rhs_contains_popcount(rhs) && !expr_has_side_effects(rhs) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn rhs_contains_popcount(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { target, .. } if target == "__popcount" => true,
        PreHirExpr::Cast { expr: inner, .. } | PreHirExpr::Unary { expr: inner, .. } => {
            rhs_contains_popcount(inner)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rhs_contains_popcount(lhs) || rhs_contains_popcount(rhs)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(rhs_contains_popcount),
        _ => false,
    }
}

fn has_popcount(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { rhs, .. } => rhs_contains_popcount(rhs),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => rhs_contains_popcount(expr),
        PreHirStmt::Block(body) => body.iter().any(has_popcount),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rhs_contains_popcount(cond)
                || then_body.iter().any(has_popcount)
                || else_body.iter().any(has_popcount)
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
            rhs_contains_popcount(cond) || body.iter().any(has_popcount)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(has_popcount)
                || cond.as_ref().is_some_and(rhs_contains_popcount)
                || update.as_deref().is_some_and(has_popcount)
                || body.iter().any(has_popcount)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rhs_contains_popcount(expr)
                || cases.iter().any(|c| c.body.iter().any(has_popcount))
                || default.iter().any(has_popcount)
        }
        _ => false,
    }
}
