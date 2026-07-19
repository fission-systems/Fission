use crate::prelude::*;
use super::utils::*;
use crate::HashMap;

pub fn strip_redundant_assign_casts(func: &mut HirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }
    if type_map.is_empty() {
        return false;
    }
    strip_redundant_casts_in_stmts(&mut func.body, &type_map)
}

fn strip_redundant_casts_in_stmts(
    stmts: &mut [HirStmt],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= strip_redundant_casts_in_stmt(stmt, type_map);
    }
    changed
}

fn strip_redundant_casts_in_stmt(stmt: &mut HirStmt, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { rhs, .. } => {
            changed |= strip_redundant_casts_in_expr(rhs, type_map);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= strip_redundant_casts_in_expr(expr, type_map);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= strip_redundant_casts_in_stmts(body, type_map);
        }
        HirStmt::For {
            init,
            update,
            body,
            cond,
        } => {
            if let Some(i) = init {
                changed |= strip_redundant_casts_in_stmt(i, type_map);
            }
            if let Some(c) = cond {
                changed |= strip_redundant_casts_in_expr(c, type_map);
            }
            if let Some(u) = update {
                changed |= strip_redundant_casts_in_stmt(u, type_map);
            }
            changed |= strip_redundant_casts_in_stmts(body, type_map);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= strip_redundant_casts_in_expr(cond, type_map);
            changed |= strip_redundant_casts_in_stmts(then_body, type_map);
            changed |= strip_redundant_casts_in_stmts(else_body, type_map);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= strip_redundant_casts_in_expr(expr, type_map);
            for case in cases {
                changed |= strip_redundant_casts_in_stmts(&mut case.body, type_map);
            }
            changed |= strip_redundant_casts_in_stmts(default, type_map);
        }
        _ => {}
    }
    changed
}

fn strip_redundant_casts_in_expr(expr: &mut HirExpr, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Cast { expr: inner, .. } => {
            changed |= strip_redundant_casts_in_expr(inner, type_map);
        }
        HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= strip_redundant_casts_in_expr(inner, type_map);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= strip_redundant_casts_in_expr(lhs, type_map);
            changed |= strip_redundant_casts_in_expr(rhs, type_map);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= strip_redundant_casts_in_expr(arg, type_map);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= strip_redundant_casts_in_expr(base, type_map);
            changed |= strip_redundant_casts_in_expr(index, type_map);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= strip_redundant_casts_in_expr(cond, type_map);
            changed |= strip_redundant_casts_in_expr(then_expr, type_map);
            changed |= strip_redundant_casts_in_expr(else_expr, type_map);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    if let HirExpr::Cast { ty, expr: inner } = expr {
        if let HirExpr::Var(name) = inner.as_ref() {
            if let Some(var_ty) = type_map.get(name) {
                if var_ty == ty {
                    *expr = (**inner).clone();
                    changed = true;
                }
            }
        } else if let HirExpr::Cast {
            ty: inner_ty,
            expr: innermost,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                *expr = HirExpr::Cast {
                    ty: ty.clone(),
                    expr: innermost.clone(),
                };
                changed = true;
            }
        }
    }
    changed
}

pub fn collapse_trivial_pointer_alias_bindings(func: &mut HirFunction) -> bool {
    let mut aliases = HashMap::<String, HirExpr>::default();
    for binding in &func.locals {
        if !matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        if binding.name.starts_with("slot_") && should_preserve_slot_alias_binding(func, binding) {
            continue;
        }
        let Some(initializer) = binding.initializer.as_ref() else {
            continue;
        };
        let Some(replacement) = pointer_alias_replacement(initializer) else {
            continue;
        };
        if expr_mentions_var(&replacement, &binding.name)
            || expr_has_side_effects(&replacement)
            || var_is_assigned_in_stmts(&func.body, &binding.name)
        {
            continue;
        }
        let use_count = count_uses_in_stmt_list(&func.body, &binding.name)
            + count_uses_in_bindings(&func.locals, &binding.name);
        if use_count > 0 {
            aliases.insert(binding.name.clone(), replacement);
        }
    }
    if aliases.is_empty() {
        return false;
    }

    for (name, replacement) in &aliases {
        for stmt in &mut func.body {
            replace_var_in_stmt(stmt, name, replacement);
        }
        for binding in &mut func.locals {
            if binding.name != *name
                && let Some(initializer) = &mut binding.initializer
            {
                replace_var_in_expr(initializer, name, replacement);
            }
        }
    }

    let before = func.locals.len();
    func.locals
        .retain(|binding| !aliases.contains_key(&binding.name));
    before != func.locals.len()
}

fn should_preserve_slot_alias_binding(func: &HirFunction, binding: &NirBinding) -> bool {
    binding.surface_type_name.is_some()
        || matches!(
            binding.origin,
            Some(NirBindingOrigin::StackOffset(_))
                | Some(NirBindingOrigin::DerivedFromStackOffset(_))
        )
        || binding
            .initializer
            .as_ref()
            .and_then(ptr_offset_const)
            .is_some_and(|offset| offset != 0)
        || stmt_list_uses_var_as_index_base(&func.body, &binding.name)
}

fn ptr_offset_const(expr: &HirExpr) -> Option<i64> {
    match expr {
        HirExpr::PtrOffset { offset, .. } => Some(*offset),
        HirExpr::Cast { expr, .. } => ptr_offset_const(expr),
        _ => Some(0),
    }
}

fn stmt_list_uses_var_as_index_base(stmts: &[HirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_uses_var_as_index_base(stmt, name))
}

fn stmt_uses_var_as_index_base(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        HirStmt::Expr(expr)
        | HirStmt::Return(Some(expr))
        | HirStmt::VaStart { va_list: expr, .. } => expr_uses_var_as_index_base(expr, name),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => stmt_list_uses_var_as_index_base(body, name),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var_as_index_base(cond, name)
                || stmt_list_uses_var_as_index_base(then_body, name)
                || stmt_list_uses_var_as_index_base(else_body, name)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_var_as_index_base(expr, name)
                || cases
                    .iter()
                    .any(|case| stmt_list_uses_var_as_index_base(&case.body, name))
                || stmt_list_uses_var_as_index_base(default, name)
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn lvalue_uses_var_as_index_base(lhs: &HirLValue, name: &str) -> bool {
    match lhs {
        HirLValue::Index { base, index, .. } => {
            matches!(base.as_ref(), HirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        HirLValue::Deref { ptr, .. } => expr_uses_var_as_index_base(ptr, name),
        HirLValue::Var(_) => false,
        HirLValue::FieldAccess { base, .. } => expr_uses_var_as_index_base(base, name),
    }
}

fn expr_uses_var_as_index_base(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Index { base, index, .. } => {
            matches!(base.as_ref(), HirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => expr_uses_var_as_index_base(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        HirExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_uses_var_as_index_base(arg, name)),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var_as_index_base(cond, name)
                || expr_uses_var_as_index_base(then_expr, name)
                || expr_uses_var_as_index_base(else_expr, name)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn pointer_alias_replacement(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => Some(expr.clone()),
        HirExpr::Cast {
            ty: NirType::Ptr(_),
            expr,
        } => match expr.as_ref() {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => Some((**expr).clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn cast_elision_pass(func: &mut HirFunction) -> bool {
    let binding_types: std::collections::HashMap<String, NirType> = func
        .locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| is_scalar_non_unknown(&b.ty))
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect();

    let return_type = is_scalar_non_unknown(&func.return_type).then(|| func.return_type.clone());

    if binding_types.is_empty() && return_type.is_none() {
        return false;
    }

    let mut changed = false;
    elide_casts_in_stmts(
        &mut func.body,
        &binding_types,
        return_type.as_ref(),
        &mut changed,
    );
    changed
}

fn is_scalar_non_unknown(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

fn scalar_bit_width(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn redundant_self_cast_assignment(name: &str, rhs: &HirExpr, binding_ty: &NirType) -> bool {
    let HirExpr::Cast { ty: cast_ty, expr } = rhs else {
        return false;
    };
    let HirExpr::Var(var) = expr.as_ref() else {
        return false;
    };
    if var != name {
        return false;
    }
    let Some(binding_bits) = scalar_bit_width(binding_ty) else {
        return false;
    };
    let Some(cast_bits) = scalar_bit_width(cast_ty) else {
        return false;
    };
    cast_bits >= binding_bits
}

fn elide_casts_in_stmts(
    stmts: &mut Vec<HirStmt>,
    binding_types: &std::collections::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        elide_casts_in_stmt(stmt, binding_types, return_type, changed);
    }
}

fn elide_casts_in_stmt(
    stmt: &mut HirStmt,
    binding_types: &std::collections::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if let Some(binding_ty) = binding_types.get(name.as_str()) {
                if redundant_self_cast_assignment(name, rhs, binding_ty) {
                    *rhs = HirExpr::Var(name.clone());
                    *changed = true;
                } else if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        HirStmt::Return(Some(expr)) => {
            if let Some(return_type) = return_type
                && let Some(stripped) = try_strip_return_outer_cast(expr, return_type)
            {
                *expr = stripped;
                *changed = true;
            }
        }
        HirStmt::Block(stmts) => elide_casts_in_stmts(stmts, binding_types, return_type, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            elide_casts_in_stmts(then_body, binding_types, return_type, changed);
            elide_casts_in_stmts(else_body, binding_types, return_type, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            elide_casts_in_stmts(body, binding_types, return_type, changed)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                elide_casts_in_stmt(i, binding_types, return_type, changed);
            }
            if let Some(u) = update {
                elide_casts_in_stmt(u, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(body, binding_types, return_type, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                elide_casts_in_stmts(&mut case.body, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(default, binding_types, return_type, changed);
        }
        _ => {}
    }
}

fn try_strip_outer_cast(expr: &HirExpr, binding_ty: &NirType) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    if cast_ty == binding_ty {
        let inner_ty = expr_type(inner);
        let compatible = match (&inner_ty, binding_ty) {
            (NirType::Unknown, _) => true,
            (a, b) if a == b => true,
            (NirType::Bool, NirType::Int { .. }) => true,
            (
                NirType::Int {
                    bits: inner_bits, ..
                },
                NirType::Int {
                    bits: outer_bits, ..
                },
            ) => inner_bits <= outer_bits,
            _ => false,
        };
        if compatible {
            return Some((**inner).clone());
        }
    } else if is_scalar_non_unknown(cast_ty) && is_scalar_non_unknown(binding_ty) {
        if let (Some(cast_bits), Some(binding_bits)) =
            (scalar_bit_width(cast_ty), scalar_bit_width(binding_ty))
        {
            if cast_bits >= binding_bits {
                return Some((**inner).clone());
            }
        }
    }
    None
}

fn try_strip_return_outer_cast(expr: &HirExpr, return_type: &NirType) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    if cast_ty == return_type && is_scalar_non_unknown(cast_ty) {
        Some((**inner).clone())
    } else if is_scalar_non_unknown(cast_ty) && is_scalar_non_unknown(return_type) {
        if let (Some(cast_bits), Some(return_bits)) =
            (scalar_bit_width(cast_ty), scalar_bit_width(return_type))
        {
            if cast_bits >= return_bits {
                return Some((**inner).clone());
            }
        }
        None
    } else {
        None
    }
}

/// ActionSetCasts / RulePushPtr / RuleStructOffset0-style cleanups at expression level.
pub fn normalize_pointer_and_struct_casts(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::FieldAccess {
            offset: 0, base, ..
        } => Some((**base).clone()),
        HirExpr::PtrOffset { base, offset: 0 } => Some((**base).clone()),
        _ => None,
    }
}
