use super::super::utils::{
    is_abi_return_register_name, is_pure_return_collapse_rhs, is_trivial_temp_name,
    retain_unmarked_stmts,
};
use crate::HashSet;
use crate::analysis::preservation::should_block_trivial_return_collapse;
use crate::prelude::*;
use fission_midend_core::wave_stats;

pub fn collapse_trivial_assign_returns(
    stmts: &mut Vec<PreHirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    let mut changed = false;
    let mut blocked = 0usize;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let replacement = match (&stmts[idx], &stmts[idx + 1]) {
            (
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs,
                },
                PreHirStmt::Return(Some(PreHirExpr::Var(ret_name))),
            ) if name == ret_name => {
                // Collapse candidates:
                // - ABI return regs with pure RHS (`rax = param+5; return rax`)
                // - trivial temps (subject to preservation)
                // - any local with pure RHS, especially const (`w8 = 3; return w8`)
                let pure_rhs = is_pure_return_collapse_rhs(rhs);
                let is_temp = is_trivial_temp_name(name);
                let is_abi = is_abi_return_register_name(name);
                if is_abi {
                    if pure_rhs { Some(rhs.clone()) } else { None }
                } else if is_temp || pure_rhs {
                    if should_block_trivial_return_collapse(name, preserved_temps)
                        && !matches!(rhs, PreHirExpr::Const(_, _))
                    {
                        blocked += 1;
                        None
                    } else {
                        Some(rhs.clone())
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[idx + 1] = PreHirStmt::Return(Some(expr));
            to_remove[idx] = true;
            changed = true;
        }
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    // Recurse into nested structured regions so Block/if arms also fold.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    preserved_temps,
                ) {
                    changed = true;
                }
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    if collapse_trivial_assign_returns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        preserved_temps,
                    ) {
                        changed = true;
                    }
                }
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            _ => {}
        }
    }

    wave_stats::add_preserved_temp_prune_blocked(blocked);
    changed
}
