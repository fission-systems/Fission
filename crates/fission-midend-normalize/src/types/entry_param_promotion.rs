//! Promote entry-block spills `tmp = <abi param reg>` to canonical `param_k` names.
//!
//! Uses the same provider-backed ABI carrier assignment as preview building.
//! Conservatively only renames when the RHS is a plain (or cast-wrapped) hardware register
//! for a parameter slot and the assignment appears in the leading linear prefix of the body.

use fission_midend_core::ir::{NirBindingOrigin, NirType};
use fission_midend_prehir::{PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};
use fission_midend_prehir::util::rename_vars_in_stmts;
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

fn peel_var_name<'a>(expr: &'a PreHirExpr) -> Option<&'a str> {
    match expr {
        PreHirExpr::Var(s) => Some(s.as_str()),
        PreHirExpr::Cast { expr, .. } => peel_var_name(expr),
        _ => None,
    }
}

fn collect_entry_linear_prefix<'a>(stmts: &'a [PreHirStmt], out: &mut Vec<&'a PreHirStmt>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(_) => continue,
            PreHirStmt::Block(inner) if out.is_empty() => {
                collect_entry_linear_prefix(inner, out);
                return;
            }
            PreHirStmt::Assign { .. } => out.push(stmt),
            _ => break,
        }
    }
}

fn stmt_contains_rhs_var(stmt: &PreHirStmt, target: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { rhs, .. } | PreHirStmt::Expr(rhs) | PreHirStmt::Return(Some(rhs)) => {
            expr_contains_var(rhs, target)
        }
        PreHirStmt::VaStart { va_list, .. } => expr_contains_var(va_list, target),
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_contains_rhs_var(stmt, target))
        }
        PreHirStmt::If {
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
        PreHirStmt::Switch {
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
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

/// True if every appearance of `target` as a read anywhere in `stmts` is a
/// bare, direct store of the whole register straight to memory (`*ptr =
/// target;`, with `target` as nothing more than the stored value), and it's
/// never read in any other form. That's the ARM32 `push {rN, lr}` prologue
/// idiom -- an alignment-padding register spilled once and never read back,
/// as itself or through the memory it was spilled to -- which shouldn't
/// count as evidence of a genuine incoming argument the way a real
/// parameter (later read back from its home slot, or used directly) would.
///
/// The read-back half of that is what separates the two, and it is a
/// question about the *slot*, not the register: at `-O0` an argument
/// register is spilled to its home slot in the prologue and every later use
/// reads the slot, so the register itself appears exactly once, as a store
/// value. `usart_set_databits(int, int)` arrives that way --
///
/// ```text
/// *sp = r1;        // spilled
/// uVar5 = *sp;     // and read straight back
/// ```
///
/// -- and testing only the register said "padding", losing the second of two
/// parameters and, with it, every match the metric's argument pass could
/// have made for it.
fn only_used_as_bare_store_value(stmts: &[PreHirStmt], target: &str) -> bool {
    let mut found_other_use = false;
    let mut found_store_use = false;
    walk_rhs_var_uses(stmts, target, &mut found_other_use, &mut found_store_use);
    if !found_store_use || found_other_use {
        return false;
    }
    let mut spilled_to = Vec::new();
    collect_spill_addresses(stmts, target, &mut spilled_to);
    !spilled_to
        .iter()
        .any(|addr| stmts_read_through_address(stmts, addr))
}

/// Addresses `target` was stored to as a bare value (`*addr = target;`).
fn collect_spill_addresses<'a>(
    stmts: &'a [PreHirStmt],
    target: &str,
    out: &mut Vec<&'a PreHirExpr>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, .. },
                rhs: PreHirExpr::Var(name),
            } if name == target => out.push(ptr.as_ref()),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_spill_addresses(body, target, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_spill_addresses(then_body, target, out);
                collect_spill_addresses(else_body, target, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_spill_addresses(&case.body, target, out);
                }
                collect_spill_addresses(default, target, out);
            }
            _ => {}
        }
    }
}

/// Whether anything loads through `addr`.
///
/// Matched structurally, so an address reached by a different spelling is
/// not recognized. That errs toward leaving the register unpromoted, which
/// is the behaviour this predicate had for every register before.
fn stmts_read_through_address(stmts: &[PreHirStmt], addr: &PreHirExpr) -> bool {
    fn in_expr(expr: &PreHirExpr, addr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Load { ptr, .. } if ptr.as_ref() == addr => true,
            PreHirExpr::Load { ptr, .. } => in_expr(ptr, addr),
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => in_expr(expr, addr),
            PreHirExpr::Binary { lhs, rhs, .. } => in_expr(lhs, addr) || in_expr(rhs, addr),
            PreHirExpr::Index { base, index, .. } => in_expr(base, addr) || in_expr(index, addr),
            PreHirExpr::PtrOffset { base, .. } => in_expr(base, addr),
            PreHirExpr::FieldAccess { base, .. } => in_expr(base, addr),
            PreHirExpr::Call { args, .. } => args.iter().any(|a| in_expr(a, addr)),
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => in_expr(cond, addr) || in_expr(then_expr, addr) || in_expr(else_expr, addr),
            _ => false,
        }
    }
    fn in_stmts(stmts: &[PreHirStmt], addr: &PreHirExpr) -> bool {
        stmts.iter().any(|stmt| match stmt {
            PreHirStmt::Assign { rhs, .. } => in_expr(rhs, addr),
            PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => in_expr(e, addr),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => in_stmts(body, addr),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => in_expr(cond, addr) || in_stmts(then_body, addr) || in_stmts(else_body, addr),
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                in_expr(expr, addr)
                    || cases.iter().any(|c| in_stmts(&c.body, addr))
                    || in_stmts(default, addr)
            }
            _ => false,
        })
    }
    in_stmts(stmts, addr)
}

fn walk_rhs_var_uses(
    stmts: &[PreHirStmt],
    target: &str,
    found_other: &mut bool,
    found_store: &mut bool,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if lvalue_address_contains_var(lhs, target) {
                    *found_other = true;
                }
                if matches!(lhs, PreHirLValue::Deref { .. })
                    && matches!(rhs, PreHirExpr::Var(name) if name == target)
                {
                    *found_store = true;
                } else if expr_contains_var(rhs, target) {
                    *found_other = true;
                }
            }
            PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => {
                if expr_contains_var(e, target) {
                    *found_other = true;
                }
            }
            PreHirStmt::VaStart { va_list, .. } => {
                if expr_contains_var(va_list, target) {
                    *found_other = true;
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                walk_rhs_var_uses(body, target, found_other, found_store);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if expr_contains_var(cond, target) {
                    *found_other = true;
                }
                walk_rhs_var_uses(then_body, target, found_other, found_store);
                walk_rhs_var_uses(else_body, target, found_other, found_store);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                if expr_contains_var(expr, target) {
                    *found_other = true;
                }
                for case in cases {
                    walk_rhs_var_uses(&case.body, target, found_other, found_store);
                }
                walk_rhs_var_uses(default, target, found_other, found_store);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn lvalue_address_contains_var(lvalue: &PreHirLValue, target: &str) -> bool {
    match lvalue {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_contains_var(ptr, target),
        PreHirLValue::Index { base, index, .. } => {
            expr_contains_var(base, target) || expr_contains_var(index, target)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_contains_var(base, target),
    }
}

fn expr_contains_var(expr: &PreHirExpr, target: &str) -> bool {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => name == target,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, target),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, target) || expr_contains_var(rhs, target)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, target)),
        PreHirExpr::PtrOffset { base, .. } => expr_contains_var(base, target),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_var(base, target) || expr_contains_var(index, target)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, target)
                || expr_contains_var(then_expr, target)
                || expr_contains_var(else_expr, target)
        }
        PreHirExpr::Const(_, _) => false,
    }
}

fn stmt_assigns_var(stmt: &PreHirStmt, target: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => name == target,
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            stmts.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
                || else_body.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| case.body.iter().any(|stmt| stmt_assigns_var(stmt, target)))
                || default.iter().any(|stmt| stmt_assigns_var(stmt, target))
        }
        _ => false,
    }
}

fn detect_variadic_register_save(func: &PreHirFunction) -> bool {
    fn stmt_has_variadic_shape(stmt: &PreHirStmt) -> bool {
        match stmt {
            PreHirStmt::Assign {
                rhs: PreHirExpr::Call { args, .. },
                ..
            }
            | PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => args.len() > 4,
            PreHirStmt::VaStart { .. } => true,
            PreHirStmt::Block(stmts)
            | PreHirStmt::While { body: stmts, .. }
            | PreHirStmt::DoWhile { body: stmts, .. }
            | PreHirStmt::For { body: stmts, .. } => stmts.iter().any(stmt_has_variadic_shape),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.iter().any(stmt_has_variadic_shape)
                    || else_body.iter().any(stmt_has_variadic_shape)
            }
            PreHirStmt::Switch { cases, default, .. } => {
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

fn param_ty_for_abi(func: &PreHirFunction) -> NirType {
    NirType::Int {
        bits: abi_pointer_size(func.is_64bit, func.calling_convention) * 8,
        signed: true,
    }
}

fn promote_existing_param_name_reads(func: &mut PreHirFunction) -> usize {
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

fn promote_direct_param_register_reads(func: &mut PreHirFunction) -> usize {
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
            if stmt_assigns_var(&PreHirStmt::Block(func.body.clone().into()), &hw) {
                continue;
            }
            if !func
                .body
                .iter()
                .any(|stmt| stmt_contains_rhs_var(stmt, &hw))
            {
                continue;
            }
            if only_used_as_bare_store_value(&func.body, &hw) {
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

fn sort_params_by_index(params: &mut [fission_midend_prehir::PreHirBinding]) {
    params.sort_by_key(|b| {
        b.name
            .strip_prefix("param_")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(999)
    });
}

fn ensure_param_binding(func: &mut PreHirFunction, slot: usize, ty: NirType) {
    let name = format!("param_{}", slot + 1);
    if let Some(p) = func.params.iter_mut().find(|p| p.name == name) {
        if matches!(p.ty, NirType::Unknown) && !matches!(ty, NirType::Unknown) {
            p.ty = ty;
        }
        return;
    }
    func.params.push(PreHirBinding {
        name,
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(slot)),
        initializer: None,
    });
    sort_params_by_index(&mut func.params);
}

fn remove_local_binding(func: &mut PreHirFunction, name: &str) {
    if let Some(pos) = func.locals.iter().position(|b| b.name == name) {
        func.locals.remove(pos);
    }
}

fn trim_unused_variadic_tail_params(func: &mut PreHirFunction) -> bool {
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

fn abi_state_for_func(func: &PreHirFunction) -> AbiState {
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

fn hw_name_for_slot(func: &PreHirFunction, slot: usize) -> Option<String> {
    abi_state_for_func(func).param_hw_name(slot)
}

fn hardware_names_for_slot(func: &PreHirFunction, slot: usize) -> Vec<String> {
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

fn collect_var_names_in_stmt(stmt: &PreHirStmt, vars: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { rhs, .. } => collect_var_names_in_expr(rhs, vars),
        PreHirStmt::Return(Some(expr)) => collect_var_names_in_expr(expr, vars),
        PreHirStmt::Expr(expr) => collect_var_names_in_expr(expr, vars),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_var_names_in_expr(cond, vars);
            for s in then_body.iter() {
                collect_var_names_in_stmt(s, vars);
            }
            for s in else_body.iter() {
                collect_var_names_in_stmt(s, vars);
            }
        }
        PreHirStmt::Block(stmts) => {
            for s in stmts.iter() {
                collect_var_names_in_stmt(s, vars);
            }
        }
        _ => {}
    }
}

fn collect_var_names_in_expr(expr: &PreHirExpr, vars: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => collect_var_names_in_expr(inner, vars),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_var_names_in_expr(lhs, vars);
            collect_var_names_in_expr(rhs, vars);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_var_names_in_expr(cond, vars);
            collect_var_names_in_expr(then_expr, vars);
            collect_var_names_in_expr(else_expr, vars);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_var_names_in_expr(arg, vars);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_var_names_in_expr(base, vars);
            collect_var_names_in_expr(index, vars);
        }
        PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
    }
}

/// Remove `param_k = <hw>` copies where `<hw>` is the incoming register for slot `k`.
fn remove_redundant_param_hw_copies(body: &mut Vec<PreHirStmt>, abi: CallingConvention) {
    body.retain_mut(|stmt| match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
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
        PreHirStmt::Block(stmts) => {
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts), abi);
            true
        }
        PreHirStmt::While { body: stmts, .. } | PreHirStmt::DoWhile { body: stmts, .. } => {
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts), abi);
            true
        }
        PreHirStmt::For { body: stmts, .. } => {
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts), abi);
            true
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for c in cases.iter_mut() {
                remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut c.body), abi);
            }
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), abi);
            true
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), abi);
            remove_redundant_param_hw_copies(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), abi);
            true
        }
        _ => true,
    });
}

pub fn apply_entry_param_promotion_pass(func: &mut PreHirFunction) -> bool {
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
        let PreHirStmt::Assign { lhs, rhs } = stmt else {
            continue;
        };
        let PreHirLValue::Var(lhs_name) = lhs else {
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
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) => NirType::Int {
                bits: 64,
                signed: true,
            },
            PreHirExpr::Cast { ty, .. } => ty.clone(),
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
