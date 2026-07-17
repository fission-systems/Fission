//! Promote entry-block spills `tmp = <abi param reg>` to canonical `param_k` names.
//!
//! Uses the same provider-backed ABI carrier assignment as preview building.
//! Conservatively only renames when the RHS is a plain (or cast-wrapped) hardware register
//! for a parameter slot and the assignment appears in the leading linear prefix of the body.

use crate::midend::ir::{
    HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType,
};
use crate::midend::var_rename::rename_vars_in_stmts;
use crate::midend::{AbiState, CallingConvention};
use std::collections::{BTreeSet, HashSet};

use crate::midend::wave_stats::add_entry_param_promotions;

fn abi_pointer_size(is_64bit: bool, abi: CallingConvention) -> u32 {
    if is_64bit
        || matches!(
            abi,
            CallingConvention::LoongArch64
                | CallingConvention::Mips64
                | CallingConvention::PowerPc64
        )
    {
        8
    } else {
        4
    }
}

fn abi_is_32bit_register_set(abi: CallingConvention) -> bool {
    matches!(
        abi,
        CallingConvention::Arm32
            | CallingConvention::LoongArch32
            | CallingConvention::Mips32
            | CallingConvention::PowerPc32
    )
}

fn param_slot_for_hw_register(reg: &str, abi: CallingConvention, is_64bit: bool) -> Option<usize> {
    AbiState::new(abi, is_64bit, abi_pointer_size(is_64bit, abi), 0).param_slot_for_name(reg)
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
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => name == target,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, target),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, target) || expr_contains_var(rhs, target)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, target)),
        HirExpr::PtrOffset { base, .. } => expr_contains_var(base, target),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, target) || expr_contains_var(index, target)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, target)
                || expr_contains_var(then_expr, target)
                || expr_contains_var(else_expr, target)
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

fn param_ty_for_abi(func: &HirFunction) -> NirType {
    NirType::Int {
        bits: abi_pointer_size(func.is_64bit, func.calling_convention) * 8,
        signed: true,
    }
}

fn promote_existing_param_name_reads(func: &mut HirFunction) -> usize {
    let mut promotions = 0usize;
    for slot in 0..func.int_param_offsets.len() {
        let param_name = format!("param_{}", slot + 1);
        if !func
            .body
            .iter()
            .any(|stmt| stmt_contains_rhs_var(stmt, &param_name))
        {
            continue;
        }
        let had_param = func.params.iter().any(|p| p.name == param_name);
        let had_local = func.locals.iter().any(|b| b.name == param_name);
        ensure_param_binding(func, slot, param_ty_for_abi(func));
        remove_local_binding(func, &param_name);
        if !had_param || had_local {
            promotions += 1;
        }
    }
    promotions
}

fn promote_direct_param_register_reads(func: &mut HirFunction) -> usize {
    let abi = func.calling_convention;
    let variadic_evidence =
        abi == CallingConvention::WindowsX64 && detect_variadic_register_save(func);
    let max_fixed_slot = if variadic_evidence {
        2
    } else {
        func.int_param_offsets.len()
    };
    let mut renames = Vec::new();
    let mut promotions = 0usize;
    for slot in 0..max_fixed_slot {
        let hw_names = hardware_names_for_slot(func, slot);
        if hw_names.is_empty() {
            continue;
        }
        let param_name = format!("param_{}", slot + 1);
        let mut promoted = false;
        for hw in hw_names {
            if stmt_assigns_var(&HirStmt::Block(func.body.clone()), &hw) {
                continue;
            }
            if !func
                .body
                .iter()
                .any(|stmt| stmt_contains_rhs_var(stmt, &hw))
            {
                continue;
            }
            ensure_param_binding(func, slot, param_ty_for_abi(func));
            renames.push((hw, param_name.clone()));
            promoted = true;
        }
        if promoted {
            promotions += 1;
        }
    }
    if !renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &renames);
    }
    promotions
}

fn sort_params_by_index(params: &mut [crate::midend::ir::NirBinding]) {
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

fn abi_state_for_func(func: &HirFunction) -> AbiState {
    AbiState::new_with_cspec(
        func.calling_convention,
        func.is_64bit,
        abi_pointer_size(func.is_64bit, func.calling_convention),
        0,
        Some(func.int_param_offsets.clone()),
        None,
        None,
    )
}

fn hw_name_for_slot(func: &HirFunction, slot: usize) -> Option<String> {
    abi_state_for_func(func).param_hw_name(slot)
}

fn hardware_names_for_slot(func: &HirFunction, slot: usize) -> Vec<String> {
    let abi = abi_state_for_func(func);
    let mut names = BTreeSet::new();
    if let Some(hw) = abi.param_hw_name(slot) {
        names.insert(hw);
    }
    let mut body_vars = HashSet::new();
    for stmt in &func.body {
        collect_var_names_in_stmt(stmt, &mut body_vars);
    }
    for name in body_vars {
        if abi.param_slot_for_name(&name) == Some(slot) {
            names.insert(name);
        }
    }
    names.into_iter().collect()
}

fn collect_var_names_in_stmt(stmt: &HirStmt, vars: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { rhs, .. } => collect_var_names_in_expr(rhs, vars),
        HirStmt::Return(Some(expr)) => collect_var_names_in_expr(expr, vars),
        HirStmt::Expr(expr) => collect_var_names_in_expr(expr, vars),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_var_names_in_expr(cond, vars);
            for s in then_body {
                collect_var_names_in_stmt(s, vars);
            }
            for s in else_body {
                collect_var_names_in_stmt(s, vars);
            }
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                collect_var_names_in_stmt(s, vars);
            }
        }
        _ => {}
    }
}

fn collect_var_names_in_expr(expr: &HirExpr, vars: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => collect_var_names_in_expr(inner, vars),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_var_names_in_expr(lhs, vars);
            collect_var_names_in_expr(rhs, vars);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_var_names_in_expr(cond, vars);
            collect_var_names_in_expr(then_expr, vars);
            collect_var_names_in_expr(else_expr, vars);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_var_names_in_expr(arg, vars);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_var_names_in_expr(base, vars);
            collect_var_names_in_expr(index, vars);
        }
        HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
    }
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
                    && param_slot_for_hw_register(hw, abi, !abi_is_32bit_register_set(abi))
                        == Some(slot)
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
    if (!func.is_64bit
        && !matches!(
            func.calling_convention,
            CallingConvention::Arm32
                | CallingConvention::PowerPc32
                | CallingConvention::LoongArch32
                | CallingConvention::Mips32
        ))
        || func.suppress_entry_register_params
    {
        return false;
    }
    let abi = func.calling_convention;
    let mut promotions = promote_existing_param_name_reads(func);
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
        let Some(slot) = param_slot_for_hw_register(rhs_name, abi, func.is_64bit) else {
            continue;
        };
        if !seen_lhs.insert(lhs_name.clone()) {
            continue;
        }
        let ty = match rhs {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => NirType::Int {
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
        promotions += promote_direct_param_register_reads(func);
        if promotions == 0 {
            return trim_unused_variadic_tail_params(func);
        }
        let _ = trim_unused_variadic_tail_params(func);
        add_entry_param_promotions(promotions);
        return true;
    }

    let mut renames = Vec::new();
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
