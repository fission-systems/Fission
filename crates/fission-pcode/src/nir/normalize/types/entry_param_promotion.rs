//! Promote entry-block spills `tmp = <abi param reg>` to canonical `param_k` names.
//!
//! Uses the same register↔slot ordering as [`crate::nir::support::CallingConvention::param_reg_slots_64`].
//! Conservatively only renames when the RHS is a plain (or cast-wrapped) hardware register
//! for a parameter slot and the assignment appears in the leading linear prefix of the body.

use crate::nir::support::{x64_ghidra_reg_name, CallingConvention};
use crate::nir::types::{HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType};
use crate::nir::var_rename::rename_vars_in_stmts;
use std::collections::HashSet;

use super::super::wave_stats::add_entry_param_promotions;

fn param_slot_for_hw_register(reg: &str, abi: CallingConvention) -> Option<usize> {
    abi.param_offsets().iter().position(|&off| {
        x64_ghidra_reg_name(off).is_some_and(|hw| hw.eq_ignore_ascii_case(reg))
    })
}

fn peel_var_name<'a>(expr: &'a HirExpr) -> Option<&'a str> {
    match expr {
        HirExpr::Var(s) => Some(s.as_str()),
        HirExpr::Cast { expr, .. } => peel_var_name(expr),
        _ => None,
    }
}

fn collect_entry_linear_prefix<'a>(stmts: &'a [HirStmt], out: &mut Vec<&'a HirStmt>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Label(_) => continue,
            HirStmt::Block(inner) if out.is_empty() => {
                collect_entry_linear_prefix(inner, out);
                return;
            }
            HirStmt::Assign { .. } => out.push(stmt),
            _ => break,
        }
    }
}

fn sort_params_by_index(params: &mut [crate::nir::types::NirBinding]) {
    params.sort_by_key(|b| {
        b.name
            .strip_prefix("param_")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(999)
    });
}

fn ensure_param_binding(func: &mut HirFunction, slot: usize, ty: NirType) {
    let name = format!("param_{}", slot + 1);
    if let Some(p) = func.params.iter_mut().find(|p| p.name == name) {
        if matches!(p.ty, NirType::Unknown) && !matches!(ty, NirType::Unknown) {
            p.ty = ty;
        }
        return;
    }
    func.params.push(NirBinding {
        name,
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(slot)),
        initializer: None,
    });
    sort_params_by_index(&mut func.params);
}

fn remove_local_binding(func: &mut HirFunction, name: &str) {
    if let Some(pos) = func.locals.iter().position(|b| b.name == name) {
        func.locals.remove(pos);
    }
}

fn hw_name_for_slot(abi: CallingConvention, slot: usize) -> Option<&'static str> {
    abi.param_offsets().get(slot).copied().and_then(x64_ghidra_reg_name)
}

/// Remove `param_k = <hw>` copies where `<hw>` is the incoming register for slot `k`.
fn remove_redundant_param_hw_copies(body: &mut Vec<HirStmt>, abi: CallingConvention) {
    body.retain_mut(|stmt| {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(lhs_name),
                rhs,
            } => {
                if let Some(slot) = lhs_name
                    .strip_prefix("param_")
                    .and_then(|s| s.parse::<usize>().ok())
                    .map(|n| n.saturating_sub(1))
                {
                    if let Some(hw) = peel_var_name(rhs)
                        && let Some(expected) = hw_name_for_slot(abi, slot)
                        && hw.eq_ignore_ascii_case(expected)
                    {
                        return false;
                    }
                }
                true
            }
            HirStmt::Block(stmts) => {
                remove_redundant_param_hw_copies(stmts, abi);
                true
            }
            HirStmt::While { body: stmts, .. } | HirStmt::DoWhile { body: stmts, .. } => {
                remove_redundant_param_hw_copies(stmts, abi);
                true
            }
            HirStmt::For { body: stmts, .. } => {
                remove_redundant_param_hw_copies(stmts, abi);
                true
            }
            HirStmt::Switch {
                cases,
                default,
                ..
            } => {
                for c in cases.iter_mut() {
                    remove_redundant_param_hw_copies(&mut c.body, abi);
                }
                remove_redundant_param_hw_copies(default, abi);
                true
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                remove_redundant_param_hw_copies(then_body, abi);
                remove_redundant_param_hw_copies(else_body, abi);
                true
            }
            _ => true,
        }
    });
}

pub(crate) fn apply_entry_param_promotion_pass(func: &mut HirFunction) -> bool {
    if !func.is_64bit {
        return false;
    }
    let abi = func.calling_convention;
    let mut prefix = Vec::new();
    collect_entry_linear_prefix(&func.body, &mut prefix);

    let mut seen_lhs = HashSet::new();
    let mut spill_to_slot: Vec<(String, usize, NirType)> = Vec::new();

    for stmt in &prefix {
        let HirStmt::Assign { lhs, rhs } = stmt else {
            continue;
        };
        let HirLValue::Var(lhs_name) = lhs else {
            continue;
        };
        if lhs_name.starts_with("param_") {
            continue;
        }
        let Some(rhs_name) = peel_var_name(rhs) else {
            continue;
        };
        let Some(slot) = param_slot_for_hw_register(rhs_name, abi) else {
            continue;
        };
        if !seen_lhs.insert(lhs_name.clone()) {
            continue;
        }
        let ty = match rhs {
            HirExpr::Var(_) => NirType::Int {
                bits: 64,
                signed: true,
            },
            HirExpr::Cast { ty, .. } => ty.clone(),
            _ => NirType::Unknown,
        };
        spill_to_slot.push((lhs_name.clone(), slot, ty));
    }

    // One local name per slot (first wins); drop conflicting mappings.
    let mut used_slots = HashSet::new();
    spill_to_slot.retain(|(_, slot, _)| {
        if used_slots.contains(slot) {
            return false;
        }
        used_slots.insert(*slot);
        true
    });

    if spill_to_slot.is_empty() {
        return false;
    }

    let mut renames = Vec::new();
    let mut promotions = 0usize;
    for (local_name, slot, ty) in &spill_to_slot {
        let param_name = format!("param_{}", slot + 1);
        renames.push((local_name.clone(), param_name));
        ensure_param_binding(func, *slot, ty.clone());
        remove_local_binding(func, local_name);
        promotions += 1;
    }

    rename_vars_in_stmts(&mut func.body, &renames);
    remove_redundant_param_hw_copies(&mut func.body, abi);
    add_entry_param_promotions(promotions);
    true
}
