use crate::prelude::*;
use super::utils::*;
use crate::HashMap;

pub fn strip_redundant_assign_casts(func: &mut DirFunction) -> bool {
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
    stmts: &mut [DirStmt],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= strip_redundant_casts_in_stmt(stmt, type_map);
    }
    changed
}

fn strip_redundant_casts_in_stmt(stmt: &mut DirStmt, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { rhs, .. } => {
            changed |= strip_redundant_casts_in_expr(rhs, type_map);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= strip_redundant_casts_in_expr(expr, type_map);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= strip_redundant_casts_in_stmts(body, type_map);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= strip_redundant_casts_in_expr(cond, type_map);
            changed |= strip_redundant_casts_in_stmts(then_body, type_map);
            changed |= strip_redundant_casts_in_stmts(else_body, type_map);
        }
        DirStmt::Switch {
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

fn strip_redundant_casts_in_expr(expr: &mut DirExpr, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match expr {
        DirExpr::Cast { expr: inner, .. } => {
            changed |= strip_redundant_casts_in_expr(inner, type_map);
        }
        DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            changed |= strip_redundant_casts_in_expr(inner, type_map);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= strip_redundant_casts_in_expr(lhs, type_map);
            changed |= strip_redundant_casts_in_expr(rhs, type_map);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= strip_redundant_casts_in_expr(arg, type_map);
            }
        }
        DirExpr::Index { base, index, .. } => {
            changed |= strip_redundant_casts_in_expr(base, type_map);
            changed |= strip_redundant_casts_in_expr(index, type_map);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= strip_redundant_casts_in_expr(cond, type_map);
            changed |= strip_redundant_casts_in_expr(then_expr, type_map);
            changed |= strip_redundant_casts_in_expr(else_expr, type_map);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    if let DirExpr::Cast { ty, expr: inner } = expr {
        if let DirExpr::Var(name) = inner.as_ref() {
            if let Some(var_ty) = type_map.get(name) {
                if var_ty == ty {
                    *expr = (**inner).clone();
                    changed = true;
                }
            }
        } else if let DirExpr::Cast {
            ty: inner_ty,
            expr: innermost,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                *expr = DirExpr::Cast {
                    ty: ty.clone(),
                    expr: innermost.clone(),
                };
                changed = true;
            }
        }
    }
    changed
}

pub fn collapse_trivial_pointer_alias_bindings(func: &mut DirFunction) -> bool {
    let mut aliases = HashMap::<String, DirExpr>::default();
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

fn should_preserve_slot_alias_binding(func: &DirFunction, binding: &DirBinding) -> bool {
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

fn ptr_offset_const(expr: &DirExpr) -> Option<i64> {
    match expr {
        DirExpr::PtrOffset { offset, .. } => Some(*offset),
        DirExpr::Cast { expr, .. } => ptr_offset_const(expr),
        _ => Some(0),
    }
}

fn stmt_list_uses_var_as_index_base(stmts: &[DirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_uses_var_as_index_base(stmt, name))
}

fn stmt_uses_var_as_index_base(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        DirStmt::Expr(expr)
        | DirStmt::Return(Some(expr))
        | DirStmt::VaStart { va_list: expr, .. } => expr_uses_var_as_index_base(expr, name),
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => stmt_list_uses_var_as_index_base(body, name),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var_as_index_base(cond, name)
                || stmt_list_uses_var_as_index_base(then_body, name)
                || stmt_list_uses_var_as_index_base(else_body, name)
        }
        DirStmt::Switch {
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
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

fn lvalue_uses_var_as_index_base(lhs: &DirLValue, name: &str) -> bool {
    match lhs {
        DirLValue::Index { base, index, .. } => {
            matches!(base.as_ref(), DirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        DirLValue::Deref { ptr, .. } => expr_uses_var_as_index_base(ptr, name),
        DirLValue::Var(_) => false,
        DirLValue::FieldAccess { base, .. } => expr_uses_var_as_index_base(base, name),
    }
}

fn expr_uses_var_as_index_base(expr: &DirExpr, name: &str) -> bool {
    match expr {
        DirExpr::Index { base, index, .. } => {
            matches!(base.as_ref(), DirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => expr_uses_var_as_index_base(expr, name),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        DirExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_uses_var_as_index_base(arg, name)),
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var_as_index_base(cond, name)
                || expr_uses_var_as_index_base(then_expr, name)
                || expr_uses_var_as_index_base(else_expr, name)
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => false,
    }
}

fn pointer_alias_replacement(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => Some(expr.clone()),
        DirExpr::Cast {
            ty: NirType::Ptr(_),
            expr,
        } => match expr.as_ref() {
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => Some((**expr).clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn cast_elision_pass(func: &mut DirFunction) -> bool {
    let binding_types: crate::HashMap<String, NirType> = func
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

fn redundant_self_cast_assignment(name: &str, rhs: &DirExpr, binding_ty: &NirType) -> bool {
    let DirExpr::Cast { ty: cast_ty, expr } = rhs else {
        return false;
    };
    let DirExpr::Var(var) = expr.as_ref() else {
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
    stmts: &mut Vec<DirStmt>,
    binding_types: &crate::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        elide_casts_in_stmt(stmt, binding_types, return_type, changed);
    }
}

fn elide_casts_in_stmt(
    stmt: &mut DirStmt,
    binding_types: &crate::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            if let Some(binding_ty) = binding_types.get(name.as_str()) {
                if redundant_self_cast_assignment(name, rhs, binding_ty) {
                    *rhs = DirExpr::Var(name.clone());
                    *changed = true;
                } else if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        DirStmt::Return(Some(expr)) => {
            if let Some(return_type) = return_type
                && let Some(stripped) = try_strip_return_outer_cast(expr, return_type)
            {
                *expr = stripped;
                *changed = true;
            }
        }
        DirStmt::Block(stmts) => elide_casts_in_stmts(stmts, binding_types, return_type, changed),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            elide_casts_in_stmts(then_body, binding_types, return_type, changed);
            elide_casts_in_stmts(else_body, binding_types, return_type, changed);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            elide_casts_in_stmts(body, binding_types, return_type, changed)
        }
        DirStmt::For {
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
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                elide_casts_in_stmts(&mut case.body, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(default, binding_types, return_type, changed);
        }
        _ => {}
    }
}

fn try_strip_outer_cast(expr: &DirExpr, binding_ty: &NirType) -> Option<DirExpr> {
    let DirExpr::Cast {
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

fn try_strip_return_outer_cast(expr: &DirExpr, return_type: &NirType) -> Option<DirExpr> {
    let DirExpr::Cast {
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
pub fn normalize_pointer_and_struct_casts(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::FieldAccess {
            offset: 0, base, ..
        } => Some((**base).clone()),
        DirExpr::PtrOffset { base, offset: 0 } => Some((**base).clone()),
        _ => None,
    }
}
