//! Promote entry-block spills `tmp = <abi param reg>` to canonical `param_k` names.
//!
//! Uses the same provider-backed ABI carrier assignment as preview building.
//! Conservatively only renames when the RHS is a plain (or cast-wrapped) hardware register
//! for a parameter slot and the assignment appears in the leading linear prefix of the body.

use fission_midend_core::ir::{
    DirExpr, DirFunction, DirLValue, DirStmt, DirBinding, NirBindingOrigin, NirType,
};
use fission_midend_core::util_dir::rename_vars_in_stmts;
use fission_midend_core::{AbiState, CallingConvention};
use crate::HashSet;
use std::collections::BTreeSet;

use fission_midend_core::wave_stats::add_entry_param_promotions;

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

fn peel_var_name<'a>(expr: &'a DirExpr) -> Option<&'a str> {
    match expr {
        DirExpr::Var(s) => Some(s.as_str()),
        DirExpr::Cast { expr, .. } => peel_var_name(expr),
        _ => None,
    }
}

fn collect_entry_linear_prefix<'a>(stmts: &'a [DirStmt], out: &mut Vec<&'a DirStmt>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Label(_) => continue,
            DirStmt::Block(inner) if out.is_empty() => {
                collect_entry_linear_prefix(inner, out);
                return;
            }
            DirStmt::Assign { .. } => out.push(stmt),
            _ => break,
        }
    }
}

fn stmt_contains_rhs_var(stmt: &DirStmt, target: &str) -> bool {
    match stmt {
        DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
            expr_contains_var(rhs, target)
        }
        DirStmt::VaStart { va_list, .. } => expr_contains_var(va_list, target),
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_contains_rhs_var(stmt, target))
        }
        DirStmt::If {
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
        DirStmt::Switch {
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
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

fn expr_contains_var(expr: &DirExpr, target: &str) -> bool {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => name == target,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, target),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, target) || expr_contains_var(rhs, target)
        }
        DirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, target)),
        DirExpr::PtrOffset { base, .. } => expr_contains_var(base, target),
        DirExpr::Index { base, index, .. } => {
            expr_contains_var(base, target) || expr_contains_var(index, target)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, target)
                || expr_contains_var(then_expr, target)
                || expr_contains_var(else_expr, target)
        }
        DirExpr::Const(_, _) => false,
    }
}

fn stmt_assigns_var(stmt: &DirStmt, target: &str) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } => name == target,
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
                || else_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| case.body.iter().any(|stmt| stmt_assigns_var(stmt, target)))
                || default.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        _ => false,
    }
}

fn detect_variadic_register_save(func: &DirFunction) -> bool {
    fn stmt_has_variadic_shape(stmt: &DirStmt) -> bool {
        match stmt {
            DirStmt::Assign {
                rhs: DirExpr::Call { args, .. },
                ..
            }
            | DirStmt::Expr(DirExpr::Call { args, .. }) => args.len() > 4,
            DirStmt::VaStart { .. } => true,
            DirStmt::Block(stmts)
            | DirStmt::While { body: stmts, .. }
            | DirStmt::DoWhile { body: stmts, .. }
            | DirStmt::For { body: stmts, .. } => stmts.iter().any(stmt_has_variadic_shape),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.iter().any(stmt_has_variadic_shape)
                    || else_body.iter().any(stmt_has_variadic_shape)
            }
            DirStmt::Switch { cases, default, .. } => {
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

fn param_ty_for_abi(func: &DirFunction) -> NirType {
    NirType::Int {
        bits: abi_pointer_size(func.is_64bit, func.calling_convention) * 8,
        signed: true,
    }
}

fn promote_existing_param_name_reads(func: &mut DirFunction) -> usize {
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

fn promote_direct_param_register_reads(func: &mut DirFunction) -> usize {
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
            if stmt_assigns_var(&DirStmt::Block(func.body.clone()), &hw) {
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

fn sort_params_by_index(params: &mut [fission_midend_core::ir::DirBinding]) {
    params.sort_by_key(|b| {
        b.name
            .strip_prefix("param_")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(999)
    });
}

fn ensure_param_binding(func: &mut DirFunction, slot: usize, ty: NirType) {
    let name = format!("param_{}", slot + 1);
    if let Some(p) = func.params.iter_mut().find(|p| p.name == name) {
        if matches!(p.ty, NirType::Unknown) && !matches!(ty, NirType::Unknown) {
            p.ty = ty;
        }
        return;
    }
    func.params.push(DirBinding {
        name,
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(slot)),
        initializer: None,
    });
    sort_params_by_index(&mut func.params);
}

fn remove_local_binding(func: &mut DirFunction, name: &str) {
    if let Some(pos) = func.locals.iter().position(|b| b.name == name) {
        func.locals.remove(pos);
    }
}

fn trim_unused_variadic_tail_params(func: &mut DirFunction) -> bool {
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

fn abi_state_for_func(func: &DirFunction) -> AbiState {
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

fn hw_name_for_slot(func: &DirFunction, slot: usize) -> Option<String> {
    abi_state_for_func(func).param_hw_name(slot)
}

fn hardware_names_for_slot(func: &DirFunction, slot: usize) -> Vec<String> {
    let abi = abi_state_for_func(func);
    let mut names = BTreeSet::new();
    if let Some(hw) = abi.param_hw_name(slot) {
        names.insert(hw);
    }
    let mut body_vars = HashSet::default();
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

fn collect_var_names_in_stmt(stmt: &DirStmt, vars: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign { rhs, .. } => collect_var_names_in_expr(rhs, vars),
        DirStmt::Return(Some(expr)) => collect_var_names_in_expr(expr, vars),
        DirStmt::Expr(expr) => collect_var_names_in_expr(expr, vars),
        DirStmt::If {
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
        DirStmt::Block(stmts) => {
            for s in stmts {
                collect_var_names_in_stmt(s, vars);
            }
        }
        _ => {}
    }
}

fn collect_var_names_in_expr(expr: &DirExpr, vars: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => collect_var_names_in_expr(inner, vars),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_var_names_in_expr(lhs, vars);
            collect_var_names_in_expr(rhs, vars);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_var_names_in_expr(cond, vars);
            collect_var_names_in_expr(then_expr, vars);
            collect_var_names_in_expr(else_expr, vars);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_var_names_in_expr(arg, vars);
            }
        }
        DirExpr::Index { base, index, .. } => {
            collect_var_names_in_expr(base, vars);
            collect_var_names_in_expr(index, vars);
        }
        DirExpr::Const(_, _) | DirExpr::AddressOfGlobal(_) => {}
    }
}

/// Remove `param_k = <hw>` copies where `<hw>` is the incoming register for slot `k`.
fn remove_redundant_param_hw_copies(body: &mut Vec<DirStmt>, abi: CallingConvention) {
    body.retain_mut(|stmt| match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
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
        DirStmt::Block(stmts) => {
            remove_redundant_param_hw_copies(stmts, abi);
            true
        }
        DirStmt::While { body: stmts, .. } | DirStmt::DoWhile { body: stmts, .. } => {
            remove_redundant_param_hw_copies(stmts, abi);
            true
        }
        DirStmt::For { body: stmts, .. } => {
            remove_redundant_param_hw_copies(stmts, abi);
            true
        }
        DirStmt::Switch { cases, default, .. } => {
            for c in cases.iter_mut() {
                remove_redundant_param_hw_copies(&mut c.body, abi);
            }
            remove_redundant_param_hw_copies(default, abi);
            true
        }
        DirStmt::If {
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

pub fn apply_entry_param_promotion_pass(func: &mut DirFunction) -> bool {
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

    let mut seen_lhs = HashSet::default();
    let mut spill_to_slot: Vec<(String, usize, NirType)> = Vec::new();

    for stmt in &prefix {
        let DirStmt::Assign { lhs, rhs } = stmt else {
            continue;
        };
        let DirLValue::Var(lhs_name) = lhs else {
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
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => NirType::Int {
                bits: 64,
                signed: true,
            },
            DirExpr::Cast { ty, .. } => ty.clone(),
            _ => NirType::Unknown,
        };
        spill_to_slot.push((lhs_name.clone(), slot, ty));
    }

    // One local name per slot (first wins); drop conflicting mappings.
    let mut used_slots = HashSet::default();
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
