//! Promote entry-block spills `tmp = <abi param reg>` to canonical `param_k` names.
//!
//! Uses the same provider-backed ABI carrier assignment as preview building.
//! Conservatively only renames when the RHS is a plain (or cast-wrapped) hardware register
//! for a parameter slot and the assignment appears in the leading linear prefix of the body.

use crate::nir::types::{
    HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType,
};
use crate::nir::var_rename::rename_vars_in_stmts;
use crate::nir::{AbiState, CallingConvention};
use std::collections::HashSet;

use super::super::wave_stats::add_entry_param_promotions;

fn param_slot_for_hw_register(reg: &str, abi: CallingConvention) -> Option<usize> {
    AbiState::new(abi, true, 8, 0).param_slot_for_name(reg)
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

fn stmt_contains_rhs_var(stmt: &HirStmt, target: &str) -> bool {
    match stmt {
        HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) | HirStmt::Return(Some(rhs)) => {
            expr_contains_var(rhs, target)
        }
        HirStmt::VaStart { va_list, .. } => expr_contains_var(va_list, target),
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_contains_rhs_var(stmt, target))
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_contains_var(cond, target)
                || then_body
                    .iter()
                    .any(|stmt| stmt_contains_rhs_var(stmt, target))
                || else_body
                    .iter()
                    .any(|stmt| stmt_contains_rhs_var(stmt, target))
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_var(expr, target)
                || cases.iter().any(|case| {
                    case.body
                        .iter()
                        .any(|stmt| stmt_contains_rhs_var(stmt, target))
                })
                || default
                    .iter()
                    .any(|stmt| stmt_contains_rhs_var(stmt, target))
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn expr_contains_var(expr: &HirExpr, target: &str) -> bool {
    match expr {
        HirExpr::Var(name) => name == target,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, target),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, target) || expr_contains_var(rhs, target)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, target)),
        HirExpr::PtrOffset { base, .. } => expr_contains_var(base, target),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, target) || expr_contains_var(index, target)
        }
        HirExpr::Const(_, _) => false,
    }
}

fn stmt_assigns_var(stmt: &HirStmt, target: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => name == target,
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
                || else_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| case.body.iter().any(|stmt| stmt_assigns_var(stmt, target)))
                || default.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        _ => false,
    }
}

fn detect_variadic_register_save(func: &HirFunction) -> bool {
    fn stmt_has_variadic_shape(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Assign {
                rhs: HirExpr::Call { args, .. },
                ..
            }
            | HirStmt::Expr(HirExpr::Call { args, .. }) => args.len() > 4,
            HirStmt::VaStart { .. } => true,
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => stmts.iter().any(stmt_has_variadic_shape),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.iter().any(stmt_has_variadic_shape)
                    || else_body.iter().any(stmt_has_variadic_shape)
            }
            HirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|case| case.body.iter().any(stmt_has_variadic_shape))
                    || default.iter().any(stmt_has_variadic_shape)
            }
            _ => stmt_contains_rhs_var(stmt, "r8") || stmt_contains_rhs_var(stmt, "r9"),
        }
    }

    func.body.iter().any(stmt_has_variadic_shape)
}

fn promote_direct_param_register_reads(func: &mut HirFunction) -> usize {
    let abi = func.calling_convention;
    let variadic_evidence =
        abi == CallingConvention::WindowsX64 && detect_variadic_register_save(func);
    let max_fixed_slot = if variadic_evidence {
        2
    } else {
        abi.param_offsets().len()
    };
    let mut renames = Vec::new();
    let mut promotions = 0usize;
    for slot in 0..max_fixed_slot {
        let Some(hw) = hw_name_for_slot(abi, slot) else {
            continue;
        };
        if stmt_assigns_var(&HirStmt::Block(func.body.clone()), hw) {
            continue;
        }
        if !func.body.iter().any(|stmt| stmt_contains_rhs_var(stmt, hw)) {
            continue;
        }
        let param_name = format!("param_{}", slot + 1);
        ensure_param_binding(
            func,
            slot,
            NirType::Int {
                bits: 64,
                signed: true,
            },
        );
        renames.push((hw.to_string(), param_name));
        promotions += 1;
    }
    if !renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &renames);
    }
    promotions
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

fn trim_unused_variadic_tail_params(func: &mut HirFunction) -> bool {
    if func.calling_convention != CallingConvention::WindowsX64
        || !detect_variadic_register_save(func)
        || func.params.len() <= 2
    {
        return false;
    }

    let removable = func.params.iter().skip(2).all(|param| {
        !func
            .body
            .iter()
            .any(|stmt| stmt_contains_rhs_var(stmt, &param.name))
    });
    if !removable {
        return false;
    }
    func.params.truncate(2);
    true
}

fn hw_name_for_slot(abi: CallingConvention, slot: usize) -> Option<&'static str> {
    AbiState::new(abi, true, 8, 0).param_hw_name(slot)
}

/// Remove `param_k = <hw>` copies where `<hw>` is the incoming register for slot `k`.
fn remove_redundant_param_hw_copies(body: &mut Vec<HirStmt>, abi: CallingConvention) {
    body.retain_mut(|stmt| match stmt {
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
                    && param_slot_for_hw_register(hw, abi) == Some(slot)
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
        HirStmt::Switch { cases, default, .. } => {
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
        let promotions = promote_direct_param_register_reads(func);
        if promotions == 0 {
            return trim_unused_variadic_tail_params(func);
        }
        let _ = trim_unused_variadic_tail_params(func);
        add_entry_param_promotions(promotions);
        return true;
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
    promotions += promote_direct_param_register_reads(func);
    let _ = trim_unused_variadic_tail_params(func);
    add_entry_param_promotions(promotions);
    true
}
