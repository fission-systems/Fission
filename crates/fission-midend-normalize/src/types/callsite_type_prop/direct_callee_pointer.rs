//! Direct-callee pointer contract propagation.
//!
//! This module applies admitted concrete pointer parameters from an isolated
//! callee to stable caller argument chains, while rejecting conflicts with
//! caller-side pointer evidence.

use super::*;

fn tighten_binding_from_direct_callee_pointer(
    binding: &mut PreHirBinding,
    candidate: &NirType,
    pointer_bits: u32,
) -> bool {
    if tighten_binding_ty(binding, candidate) {
        return true;
    }
    if binding.surface_type_name.is_some() || !matches!(candidate, NirType::Ptr(_)) {
        return false;
    }
    match binding.ty {
        NirType::Int { bits, .. } if bits == pointer_bits => {
            binding.ty = candidate.clone();
            true
        }
        _ => false,
    }
}

fn binding_accepts_direct_callee_pointer(
    binding: &PreHirBinding,
    candidate: &NirType,
    pointer_bits: u32,
) -> bool {
    match (&binding.ty, candidate) {
        (NirType::Unknown, NirType::Ptr(_)) => true,
        (existing, candidate) if existing == candidate => true,
        (NirType::Ptr(existing), NirType::Ptr(candidate)) => {
            **candidate == NirType::Unknown || **existing == NirType::Unknown
        }
        (NirType::Int { bits, .. }, NirType::Ptr(_)) => {
            *bits == pointer_bits && binding.surface_type_name.is_none()
        }
        _ => false,
    }
}

fn expr_root_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr, .. } => expr_root_var(expr),
        _ => None,
    }
}

fn expr_uses_pointer_base(expr: &PreHirExpr, names: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Load { ptr, .. } => {
            expr_root_var(ptr).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(ptr, names)
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
                || expr_uses_pointer_base(index, names)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_pointer_base(lhs, names) || expr_uses_pointer_base(rhs, names)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_uses_pointer_base(expr, names)
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_uses_pointer_base(src, names),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_pointer_base(cond, names)
                || expr_uses_pointer_base(then_expr, names)
                || expr_uses_pointer_base(else_expr, names)
        }
        PreHirExpr::Call { args, .. } => args
            .iter()
            .any(|argument| expr_uses_pointer_base(argument, names)),
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn lvalue_uses_pointer_base(lvalue: &PreHirLValue, names: &HashSet<String>) -> bool {
    match lvalue {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => {
            expr_root_var(ptr).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(ptr, names)
        }
        PreHirLValue::Index { base, index, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
                || expr_uses_pointer_base(index, names)
        }
        PreHirLValue::FieldAccess { base, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
        }
    }
}

fn stmts_use_pointer_base(stmts: &[PreHirStmt], names: &HashSet<String>) -> bool {
    stmts.iter().any(|stmt| match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_uses_pointer_base(lhs, names) || expr_uses_pointer_base(rhs, names)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            expr_uses_pointer_base(expr, names)
        }
        PreHirStmt::VaStart { va_list, .. } => expr_uses_pointer_base(va_list, names),
        PreHirStmt::Block(body) => stmts_use_pointer_base(body, names),
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            expr_uses_pointer_base(cond, names) || stmts_use_pointer_base(body, names)
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_pointer_base(cond, names)
                || stmts_use_pointer_base(then_body, names)
                || stmts_use_pointer_base(else_body, names)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmts_use_pointer_base(std::slice::from_ref(stmt), names))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_uses_pointer_base(expr, names))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmts_use_pointer_base(std::slice::from_ref(stmt), names))
                || stmts_use_pointer_base(body, names)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_pointer_base(expr, names)
                || cases
                    .iter()
                    .any(|case| stmts_use_pointer_base(&case.body, names))
                || stmts_use_pointer_base(default, names)
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    })
}

/// Apply an isolated direct callee's admitted pointer parameter contract to
/// the matching caller argument. The callee fact producer has already rejected
/// generic `void*` and pointer types without a concrete pointee or informative
/// surface declaration. Backward transit uses the same stable-copy proof as
/// API prototype propagation.
pub(super) fn apply_direct_callee_pointer_transitively(
    func: &mut PreHirFunction,
    copy_sources: &HashMap<String, String>,
    definition_counts: &HashMap<String, usize>,
    self_referential: &HashSet<String>,
    arg_var: &str,
    param_ty: &NirType,
    surface_type_name: Option<&str>,
) -> bool {
    let concrete_pointee = matches!(param_ty, NirType::Ptr(inner) if **inner != NirType::Unknown);
    let informative_surface =
        surface_type_name.is_some_and(fission_signatures::pointer_surface_type_name_is_specific);
    if !matches!(param_ty, NirType::Ptr(_)) || (!concrete_pointee && !informative_surface) {
        return false;
    }
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    let mut chain = Vec::new();
    while visited.insert(current.clone()) {
        chain.push(current.clone());
        if !super::super::type_flow::binding_is_safe_for_backward_refine(
            &current,
            definition_counts,
            self_referential,
        ) {
            break;
        }
        match copy_sources.get(&current) {
            Some(source)
                if super::super::type_flow::binding_is_safe_for_backward_refine(
                    source,
                    definition_counts,
                    self_referential,
                ) =>
            {
                current = source.clone();
            }
            None | Some(_) => break,
        }
    }

    if !chain
        .iter()
        .any(|name| func.params.iter().any(|param| param.name == *name))
    {
        return false;
    }

    let has_surface_conflict = chain.iter().any(|name| {
        func.locals
            .iter()
            .chain(func.params.iter())
            .find(|binding| binding.name == *name)
            .and_then(|binding| binding.surface_type_name.as_deref())
            .is_some_and(|existing| surface_type_name != Some(existing))
    });
    if has_surface_conflict {
        return false;
    }

    let chain_names = chain.iter().cloned().collect::<HashSet<_>>();
    if surface_type_name.is_none() && stmts_use_pointer_base(&func.body, &chain_names) {
        // A concrete pointee observed only through the callee must not be
        // stacked on top of the caller's independent dereference/index
        // evidence. The caller-side solver owns pointer depth in that case;
        // this interprocedural rule is for otherwise scalar forwarding chains.
        return false;
    }

    let has_type_conflict = chain.iter().any(|name| {
        func.locals
            .iter()
            .chain(func.params.iter())
            .find(|binding| binding.name == *name)
            .is_some_and(|binding| {
                !binding_accepts_direct_callee_pointer(binding, param_ty, pointer_bits)
            })
    });
    if has_type_conflict {
        return false;
    }

    let mut changed = false;
    for name in &chain {
        if let Some(binding) = binding_by_name_mut(&mut func.locals, name)
            .or_else(|| binding_by_name_mut(&mut func.params, name))
        {
            changed |= tighten_binding_from_direct_callee_pointer(binding, param_ty, pointer_bits);
            if binding.surface_type_name.is_none()
                && let Some(surface) = surface_type_name
            {
                binding.surface_type_name = Some(surface.to_string());
                changed = true;
            }
        }
    }
    if changed && std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[DIRECT-CALLEE-TYPE-DIAG] function={} arg={} candidate={:?} surface={:?} chain={:?}",
            func.name, arg_var, param_ty, surface_type_name, chain
        );
    }
    changed
}
