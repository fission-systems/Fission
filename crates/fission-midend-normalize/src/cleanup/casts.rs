use super::utils::*;
use crate::HashMap;
use crate::prelude::*;

pub fn strip_redundant_assign_casts(func: &mut PreHirFunction) -> bool {
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
    stmts: &mut [PreHirStmt],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= strip_redundant_casts_in_stmt(stmt, type_map);
    }
    changed
}

fn strip_redundant_casts_in_stmt(
    stmt: &mut PreHirStmt,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { rhs, .. } => {
            changed |= strip_redundant_casts_in_expr(rhs, type_map);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= strip_redundant_casts_in_expr(expr, type_map);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            changed |= strip_redundant_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
            );
        }
        PreHirStmt::For {
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
            changed |= strip_redundant_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
            );
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= strip_redundant_casts_in_expr(cond, type_map);
            changed |= strip_redundant_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                type_map,
            );
            changed |= strip_redundant_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                type_map,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= strip_redundant_casts_in_expr(expr, type_map);
            for case in cases {
                changed |= strip_redundant_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    type_map,
                );
            }
            changed |= strip_redundant_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                type_map,
            );
        }
        _ => {}
    }
    changed
}

fn strip_redundant_casts_in_expr(
    expr: &mut PreHirExpr,
    type_map: &HashMap<String, NirType>,
) -> bool {
    strip_redundant_casts_in_expr_with_context(expr, type_map, false)
}

fn strip_redundant_casts_in_expr_with_context(
    expr: &mut PreHirExpr,
    type_map: &HashMap<String, NirType>,
    is_unsigned_compare_operand: bool,
) -> bool {
    let mut changed = false;
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => {
            changed |= strip_redundant_casts_in_expr_with_context(inner, type_map, false);
        }
        PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            changed |= strip_redundant_casts_in_expr_with_context(inner, type_map, false);
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            let preserve_operands = matches!(
                op,
                PreHirBinaryOp::Lt | PreHirBinaryOp::Le | PreHirBinaryOp::Gt | PreHirBinaryOp::Ge
            );
            changed |= strip_redundant_casts_in_expr_with_context(lhs, type_map, preserve_operands);
            changed |= strip_redundant_casts_in_expr_with_context(rhs, type_map, preserve_operands);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= strip_redundant_casts_in_expr_with_context(arg, type_map, false);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= strip_redundant_casts_in_expr_with_context(base, type_map, false);
            changed |= strip_redundant_casts_in_expr_with_context(index, type_map, false);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= strip_redundant_casts_in_expr_with_context(cond, type_map, false);
            changed |= strip_redundant_casts_in_expr_with_context(then_expr, type_map, false);
            changed |= strip_redundant_casts_in_expr_with_context(else_expr, type_map, false);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    if let PreHirExpr::Cast { ty, expr: inner } = expr {
        if let PreHirExpr::Var(name) = inner.as_ref() {
            if let Some(var_ty) = type_map.get(name) {
                let is_unsigned_compare_boundary =
                    is_unsigned_compare_operand && matches!(ty, NirType::Int { signed: false, .. });
                if var_ty == ty && !is_unsigned_compare_boundary {
                    *expr = (**inner).clone();
                    changed = true;
                }
            }
        } else if let PreHirExpr::Cast {
            ty: inner_ty,
            expr: innermost,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                *expr = PreHirExpr::Cast {
                    ty: ty.clone(),
                    expr: innermost.clone(),
                };
                changed = true;
            }
        }
    }
    changed
}

pub fn collapse_trivial_pointer_alias_bindings(func: &mut PreHirFunction) -> bool {
    let mut aliases = HashMap::<String, PreHirExpr>::default();
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

fn should_preserve_slot_alias_binding(func: &PreHirFunction, binding: &PreHirBinding) -> bool {
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

fn ptr_offset_const(expr: &PreHirExpr) -> Option<i64> {
    match expr {
        PreHirExpr::PtrOffset { offset, .. } => Some(*offset),
        PreHirExpr::Cast { expr, .. } => ptr_offset_const(expr),
        _ => Some(0),
    }
}

fn stmt_list_uses_var_as_index_base(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_uses_var_as_index_base(stmt, name))
}

fn stmt_uses_var_as_index_base(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::Return(Some(expr))
        | PreHirStmt::VaStart { va_list: expr, .. } => expr_uses_var_as_index_base(expr, name),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => stmt_list_uses_var_as_index_base(body, name),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var_as_index_base(cond, name)
                || stmt_list_uses_var_as_index_base(then_body, name)
                || stmt_list_uses_var_as_index_base(else_body, name)
        }
        PreHirStmt::Switch {
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
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn lvalue_uses_var_as_index_base(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Index { base, index, .. } => {
            matches!(base.as_ref(), PreHirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        PreHirLValue::Deref { ptr, .. } => expr_uses_var_as_index_base(ptr, name),
        PreHirLValue::Var(_) => false,
        PreHirLValue::FieldAccess { base, .. } => expr_uses_var_as_index_base(base, name),
    }
}

fn expr_uses_var_as_index_base(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Index { base, index, .. } => {
            matches!(base.as_ref(), PreHirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_uses_var_as_index_base(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_uses_var_as_index_base(arg, name)),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var_as_index_base(cond, name)
                || expr_uses_var_as_index_base(then_expr, name)
                || expr_uses_var_as_index_base(else_expr, name)
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn pointer_alias_replacement(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
            Some(expr.clone())
        }
        PreHirExpr::Cast {
            ty: NirType::Ptr(_),
            expr,
        } => match expr.as_ref() {
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
                Some((**expr).clone())
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn cast_elision_pass(func: &mut PreHirFunction) -> bool {
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

fn redundant_self_cast_assignment(name: &str, rhs: &PreHirExpr, binding_ty: &NirType) -> bool {
    let PreHirExpr::Cast { ty: cast_ty, expr } = rhs else {
        return false;
    };
    let PreHirExpr::Var(var) = expr.as_ref() else {
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
    stmts: &mut Vec<PreHirStmt>,
    binding_types: &crate::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        elide_casts_in_stmt(stmt, binding_types, return_type, changed);
    }
}

fn elide_casts_in_stmt(
    stmt: &mut PreHirStmt,
    binding_types: &crate::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            if let Some(binding_ty) = binding_types.get(name.as_str()) {
                if redundant_self_cast_assignment(name, rhs, binding_ty) {
                    *rhs = PreHirExpr::Var(name.clone());
                    *changed = true;
                } else if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        PreHirStmt::Return(Some(expr)) => {
            if let Some(return_type) = return_type
                && let Some(stripped) = try_strip_return_outer_cast(expr, return_type)
            {
                *expr = stripped;
                *changed = true;
            }
        }
        PreHirStmt::Block(stmts) => elide_casts_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            binding_types,
            return_type,
            changed,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            elide_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                binding_types,
                return_type,
                changed,
            );
            elide_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                binding_types,
                return_type,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => elide_casts_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            binding_types,
            return_type,
            changed,
        ),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                elide_casts_in_stmt(i, binding_types, return_type, changed);
            }
            if let Some(u) = update {
                elide_casts_in_stmt(u, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
                return_type,
                changed,
            );
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                elide_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    binding_types,
                    return_type,
                    changed,
                );
            }
            elide_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                binding_types,
                return_type,
                changed,
            );
        }
        _ => {}
    }
}

/// Restore the C-level unsigned interpretation at the final comparison
/// boundary after alias/copy cleanup.
///
/// P-code `IntLess`/`IntLessEqual` are unsigned comparisons, while generic
/// arithmetic such as `IntSub` has no signedness in its opcode.  Earlier
/// lowering can therefore preserve the distinction as an explicit cast or as
/// an unsigned temporary.  A later pure-copy cleanup may replace that
/// temporary with a signed binding and remove the only visible cast.  The
/// binding table is the canonical type fact available at this stage, so add
/// the cast back only when a comparison operand is known to be signed.  An
/// untyped atomic value is deliberately left alone.
pub fn canonicalize_unsigned_compare_binding_casts(func: &mut PreHirFunction) -> bool {
    let binding_types: HashMap<String, NirType> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect();
    if binding_types.is_empty() {
        return false;
    }
    canonicalize_unsigned_compare_binding_casts_in_stmts(&mut func.body, &binding_types)
}

fn canonicalize_unsigned_compare_binding_casts_in_stmts(
    stmts: &mut [PreHirStmt],
    binding_types: &HashMap<String, NirType>,
) -> bool {
    stmts
        .iter_mut()
        .map(|stmt| canonicalize_unsigned_compare_binding_casts_in_stmt(stmt, binding_types))
        .any(|changed| changed)
}

fn canonicalize_unsigned_compare_binding_casts_in_stmt(
    stmt: &mut PreHirStmt,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_lvalue(lhs, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_expr(rhs, binding_types);
            changed
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::VaStart { va_list: expr, .. }
        | PreHirStmt::Return(Some(expr)) => {
            canonicalize_unsigned_compare_binding_casts_in_expr(expr, binding_types)
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => canonicalize_unsigned_compare_binding_casts_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            binding_types,
        ),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(cond, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                binding_types,
            );
            changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                binding_types,
            );
            changed
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut changed = false;
            if let Some(init) = init {
                changed |= canonicalize_unsigned_compare_binding_casts_in_stmt(init, binding_types);
            }
            if let Some(cond) = cond {
                changed |= canonicalize_unsigned_compare_binding_casts_in_expr(cond, binding_types);
            }
            if let Some(update) = update {
                changed |=
                    canonicalize_unsigned_compare_binding_casts_in_stmt(update, binding_types);
            }
            changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
            );
            changed
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(expr, binding_types);
            for case in cases {
                changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    binding_types,
                );
            }
            changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                binding_types,
            );
            changed
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn canonicalize_unsigned_compare_binding_casts_in_lvalue(
    lhs: &mut PreHirLValue,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
            canonicalize_unsigned_compare_binding_casts_in_expr(ptr, binding_types)
        }
        PreHirLValue::Index { base, index, .. } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(base, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_expr(index, binding_types);
            changed
        }
    }
}

fn canonicalize_unsigned_compare_binding_casts_in_expr(
    expr: &mut PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = match expr {
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            canonicalize_unsigned_compare_binding_casts_in_expr(expr, binding_types)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(lhs, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_expr(rhs, binding_types);
            changed
        }
        PreHirExpr::Call { args, .. } => args
            .iter_mut()
            .map(|arg| canonicalize_unsigned_compare_binding_casts_in_expr(arg, binding_types))
            .any(|changed| changed),
        PreHirExpr::Index { base, index, .. } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(base, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_expr(index, binding_types);
            changed
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(cond, binding_types);
            changed |=
                canonicalize_unsigned_compare_binding_casts_in_expr(then_expr, binding_types);
            changed |=
                canonicalize_unsigned_compare_binding_casts_in_expr(else_expr, binding_types);
            changed
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    };

    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Lt | PreHirBinaryOp::Le | PreHirBinaryOp::Gt | PreHirBinaryOp::Ge,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return changed;
    };
    // A direct variable-to-variable comparison already carries the best
    // available ABI/type information.  Do not introduce casts merely because
    // an earlier type pass conservatively rewrote those bindings as signed;
    // this preserves the established flag-recovery surface.  The lost
    // unsigned provenance this pass repairs is the mixed/compound form (for
    // example a signed binding compared with a materialized range constant).
    if matches!(lhs.as_ref(), PreHirExpr::Var(_)) && matches!(rhs.as_ref(), PreHirExpr::Var(_)) {
        return changed;
    }
    let Some(bits) = unsigned_compare_binding_width(lhs, rhs, binding_types) else {
        return changed;
    };
    let new_lhs = unsigned_compare_binding_operand(lhs, bits, binding_types);
    let new_rhs = unsigned_compare_binding_operand(rhs, bits, binding_types);
    if new_lhs != **lhs {
        **lhs = new_lhs;
        changed = true;
    }
    if new_rhs != **rhs {
        **rhs = new_rhs;
        changed = true;
    }
    changed
}

fn unsigned_compare_binding_type(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> NirType {
    match expr {
        PreHirExpr::Var(name) => binding_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| expr_type(expr)),
        _ => expr_type(expr),
    }
}

fn unsigned_compare_binding_width(
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<u32> {
    let mut width = 0;
    for expr in [lhs, rhs] {
        match unsigned_compare_binding_type(expr, binding_types) {
            NirType::Bool => width = width.max(1),
            NirType::Int { bits, .. } => width = width.max(bits),
            NirType::Unknown => {}
            _ => return None,
        }
    }
    (width > 0).then_some(width)
}

fn unsigned_compare_binding_operand(
    expr: &PreHirExpr,
    bits: u32,
    binding_types: &HashMap<String, NirType>,
) -> PreHirExpr {
    let NirType::Int {
        bits: source_bits,
        signed: true,
    } = unsigned_compare_binding_type(expr, binding_types)
    else {
        return expr.clone();
    };
    if source_bits != bits {
        return expr.clone();
    }
    if matches!(expr, PreHirExpr::Const(value, _) if *value >= 0) {
        return expr.clone();
    }
    PreHirExpr::Cast {
        ty: NirType::Int {
            bits: bits.max(1),
            signed: false,
        },
        expr: Box::new(expr.clone()),
    }
}

fn try_strip_outer_cast(expr: &PreHirExpr, binding_ty: &NirType) -> Option<PreHirExpr> {
    let PreHirExpr::Cast {
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

fn try_strip_return_outer_cast(expr: &PreHirExpr, return_type: &NirType) -> Option<PreHirExpr> {
    let PreHirExpr::Cast {
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
pub fn normalize_pointer_and_struct_casts(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::FieldAccess {
            offset: 0, base, ..
        } => Some((**base).clone()),
        PreHirExpr::PtrOffset { base, offset: 0 } => Some((**base).clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    #[test]
    fn keeps_unsigned_cast_at_unsigned_compare_boundary() {
        let mut type_map = HashMap::default();
        type_map.insert("value".to_string(), int(32, false));
        let mut expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Lt,
            lhs: Box::new(PreHirExpr::Cast {
                ty: int(32, false),
                expr: Box::new(PreHirExpr::Var("value".to_string())),
            }),
            rhs: Box::new(PreHirExpr::Const(98, int(32, false))),
            ty: NirType::Bool,
        };

        assert!(!strip_redundant_casts_in_expr(&mut expr, &type_map));
        assert!(matches!(
            expr,
            PreHirExpr::Binary { lhs, .. }
                if matches!(lhs.as_ref(), PreHirExpr::Cast { ty, .. } if *ty == int(32, false))
        ));
    }

    #[test]
    fn strips_same_type_cast_outside_unsigned_compare() {
        let mut type_map = HashMap::default();
        type_map.insert("value".to_string(), int(32, false));
        let mut expr = PreHirExpr::Cast {
            ty: int(32, false),
            expr: Box::new(PreHirExpr::Var("value".to_string())),
        };

        assert!(strip_redundant_casts_in_expr(&mut expr, &type_map));
        assert!(matches!(expr, PreHirExpr::Var(name) if name == "value"));
    }

    #[test]
    fn restores_unsigned_cast_for_signed_binding_after_alias_cleanup() {
        let mut func = PreHirFunction {
            name: "unsigned_compare_binding".to_string(),
            locals: vec![PreHirBinding {
                name: "iVar18".to_string(),
                ty: int(32, true),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("cf".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(PreHirExpr::Var("iVar18".to_string())),
                    rhs: Box::new(PreHirExpr::Const(100, int(32, false))),
                    ty: NirType::Bool,
                },
            }],
            ..Default::default()
        };

        assert!(canonicalize_unsigned_compare_binding_casts(&mut func));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary { lhs, .. },
                ..
            } if matches!(lhs.as_ref(), PreHirExpr::Cast { ty, expr }
                if *ty == int(32, false)
                    && matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "iVar18"))
        ));
    }
}
