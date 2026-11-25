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
    is_integer_compare_operand: bool,
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
                PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::Gt
                    | PreHirBinaryOp::Ge
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::SGt
                    | PreHirBinaryOp::SGe
                    | PreHirBinaryOp::Div
                    | PreHirBinaryOp::Mod
                    | PreHirBinaryOp::Shr
                    | PreHirBinaryOp::Sar
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
                // Comparison and division/shift views survive intermediate
                // binding types. Later signedness or width refinement must
                // not change unsigned storage extraction into signed division.
                let is_integer_compare_boundary =
                    is_integer_compare_operand && matches!(ty, NirType::Int { .. });
                if var_ty == ty && !is_integer_compare_boundary {
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

fn is_redundant_self_assign(
    stmt: &PreHirStmt,
    binding_types: &crate::HashMap<String, NirType>,
) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return false;
    };
    if matches!(rhs, PreHirExpr::Var(var) if var == name) {
        return true;
    }
    if let Some(binding_ty) = binding_types.get(name.as_str()) {
        redundant_self_cast_assignment(name, rhs, binding_ty)
    } else {
        false
    }
}

fn elide_casts_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    binding_types: &crate::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    let mut i = 0;
    while i < stmts.len() {
        if is_redundant_self_assign(&stmts[i], binding_types) {
            stmts.remove(i);
            *changed = true;
            continue;
        }
        elide_casts_in_stmt(&mut stmts[i], binding_types, return_type, changed);
        i += 1;
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
                if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty, binding_types) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        PreHirStmt::Return(Some(expr)) => {
            if let Some(return_type) = return_type
                && let Some(stripped) =
                    try_strip_return_outer_cast(expr, return_type, binding_types)
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
                if is_redundant_self_assign(i, binding_types) {
                    *init = None;
                    *changed = true;
                } else {
                    elide_casts_in_stmt(i, binding_types, return_type, changed);
                }
            }
            if let Some(u) = update {
                if is_redundant_self_assign(u, binding_types) {
                    *update = None;
                    *changed = true;
                } else {
                    elide_casts_in_stmt(u, binding_types, return_type, changed);
                }
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

/// Make every widening integer cast say, in C, the extension the IR means.
///
/// The builder lowers `INT_SEXT` to a cast to the *signed* wider type and
/// `INT_ZEXT` to one to the *unsigned* wider type: in this IR the cast's
/// destination carries the extension kind, the way `SLt` and `Sar` carry
/// theirs. C extends by the *source* type instead, and so does everything
/// that reads the tree as C -- the printer, and the evaluator `verify`
/// checks the printed tree with. The two readings agree only while the
/// operand's binding has the destination's signedness. Type recovery sets
/// that binding from all of its uses and can settle on the other one, which
/// turns `INT_SEXT(param_1)` into `(long long)param_1` with `param_1: uint`:
/// a zero extension in the printed C of a value the machine sign-extends
/// (`movsxd`).
///
/// At the final cast boundary, a widening cast whose operand has the other
/// signedness gets a same-width view of the destination's signedness
/// underneath -- `(long long)(int)param_1` -- so both readings are one
/// extension. Constants are left alone: the printer writes a constant's
/// stored value, which already carries the extension the lowering meant.
///
/// Runs after normalize's type recovery. Passes the decompiler runs after
/// normalize (post-structuring copy propagation, call-site type propagation)
/// can still put an operand of the other signedness under a cast; the
/// sibling compare canonicalization has the same limit.
pub fn canonicalize_extension_binding_casts(func: &mut PreHirFunction) -> bool {
    let binding_types: HashMap<String, NirType> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect();
    rewrite_exprs_in_stmts(&mut func.body, &mut |expr| {
        canonicalize_extension_cast(expr, &binding_types, false)
    })
}

/// [`canonicalize_extension_binding_casts`] for after normalize, limited to
/// a widening cast applied directly to a variable.
///
/// The orchestrator's post-normalize copy propagation and call-site typing
/// change which binding sits under a cast and what type it has, so a
/// variable operand can end up with the other signedness (`return
/// (ulong)local_18` with `local_18: int`). They do not create cast pairs, and
/// a pair that exists by then (`(int)(uchar)c`) is the explicit C spelling of
/// a zero extension that later passes chose: its outer destination no longer
/// says which extension the lowering meant, so it is left alone.
pub fn canonicalize_extension_binding_casts_over_variables(func: &mut PreHirFunction) -> bool {
    let binding_types: HashMap<String, NirType> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect();
    rewrite_exprs_in_stmts(&mut func.body, &mut |expr| {
        canonicalize_extension_cast(expr, &binding_types, true)
    })
}

fn canonicalize_extension_cast(
    expr: &mut PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    variables_only: bool,
) -> bool {
    let mut changed = false;
    for_each_child_expr_mut(expr, &mut |child| {
        changed |= canonicalize_extension_cast(child, binding_types, variables_only);
    });
    let PreHirExpr::Cast {
        ty: NirType::Int {
            bits: dst_bits,
            signed: dst_signed,
        },
        expr: inner,
    } = expr
    else {
        return changed;
    };
    if matches!(inner.as_ref(), PreHirExpr::Const(..))
        || (variables_only && !matches!(inner.as_ref(), PreHirExpr::Var(_)))
    {
        return changed;
    }
    let NirType::Int {
        bits: src_bits,
        signed: src_signed,
    } = unsigned_compare_binding_type(inner, binding_types)
    else {
        return changed;
    };
    if src_bits >= *dst_bits || src_signed == *dst_signed {
        return changed;
    }
    let view = NirType::Int {
        bits: src_bits,
        signed: *dst_signed,
    };
    // `(uint)wide` read as `(int)wide` truncates to the same bits, so a
    // cast that does not extend can take the view's signedness itself
    // rather than gain another layer. One that extends keeps its own kind.
    if let PreHirExpr::Cast {
        ty: inner_ty,
        expr: deeper,
    } = inner.as_mut()
        && matches!(
            unsigned_compare_binding_type(deeper, binding_types),
            NirType::Int { bits, .. } if bits >= src_bits
        )
    {
        *inner_ty = view;
        return true;
    }
    let operand = std::mem::replace(inner.as_mut(), PreHirExpr::Const(0, NirType::Unknown));
    **inner = PreHirExpr::Cast {
        ty: view,
        expr: Box::new(operand),
    };
    true
}

/// Apply `rewrite` to every top-level expression a statement list holds,
/// including the pointer and index expressions of stored-to lvalues.
fn rewrite_exprs_in_stmts(
    stmts: &mut [PreHirStmt],
    rewrite: &mut dyn FnMut(&mut PreHirExpr) -> bool,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= rewrite_exprs_in_stmt(stmt, rewrite);
    }
    changed
}

fn rewrite_exprs_in_stmt(
    stmt: &mut PreHirStmt,
    rewrite: &mut dyn FnMut(&mut PreHirExpr) -> bool,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Var(_) => {}
                PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
                    changed |= rewrite(ptr);
                }
                PreHirLValue::Index { base, index, .. } => {
                    changed |= rewrite(base);
                    changed |= rewrite(index);
                }
            }
            changed |= rewrite(rhs);
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::VaStart { va_list: expr, .. }
        | PreHirStmt::Return(Some(expr)) => changed |= rewrite(expr),
        PreHirStmt::Block(body) => {
            changed |=
                rewrite_exprs_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rewrite);
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            changed |= rewrite(cond);
            changed |=
                rewrite_exprs_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rewrite);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= rewrite(cond);
            changed |= rewrite_exprs_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                rewrite,
            );
            changed |= rewrite_exprs_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                rewrite,
            );
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= rewrite_exprs_in_stmt(init, rewrite);
            }
            if let Some(cond) = cond {
                changed |= rewrite(cond);
            }
            if let Some(update) = update {
                changed |= rewrite_exprs_in_stmt(update, rewrite);
            }
            changed |=
                rewrite_exprs_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), rewrite);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= rewrite(expr);
            for case in cases {
                changed |= rewrite_exprs_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    rewrite,
                );
            }
            changed |=
                rewrite_exprs_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), rewrite);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
    changed
}

fn for_each_child_expr_mut(expr: &mut PreHirExpr, visit: &mut dyn FnMut(&mut PreHirExpr)) {
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => visit(inner),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            visit(lhs);
            visit(rhs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                visit(arg);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            visit(base);
            visit(index);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            visit(cond);
            visit(then_expr);
            visit(else_expr);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn canonicalize_unsigned_compare_binding_casts_in_stmts(
    stmts: &mut [PreHirStmt],
    binding_types: &HashMap<String, NirType>,
) -> bool {
    // Not `.any()`: it stops at the first statement that changed, and every
    // statement after it kept its unsigned compare printed as a signed one.
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= canonicalize_unsigned_compare_binding_casts_in_stmt(stmt, binding_types);
    }
    changed
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
        PreHirStmt::Block(body) => canonicalize_unsigned_compare_binding_casts_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            binding_types,
        ),
        // A loop condition is a comparison boundary exactly like an `if`
        // condition. Loop-condition inlining moves a compare out of the body
        // and into `cond` after every other cast pass has run, so skipping it
        // here printed an unsigned `IntLess` as a signed C `<`.
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            let mut changed =
                canonicalize_unsigned_compare_binding_casts_in_expr(cond, binding_types);
            changed |= canonicalize_unsigned_compare_binding_casts_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
            );
            changed
        }
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
        PreHirExpr::Call { args, .. } => {
            let mut changed = false;
            for arg in args.iter_mut() {
                changed |= canonicalize_unsigned_compare_binding_casts_in_expr(arg, binding_types);
            }
            changed
        }
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

fn resolved_expr_type(expr: &PreHirExpr, binding_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        PreHirExpr::Var(name) => binding_types.get(name).cloned().unwrap_or(NirType::Unknown),
        PreHirExpr::Binary { lhs, rhs, ty, .. } => {
            let lhs_ty = resolved_expr_type(lhs, binding_types);
            let rhs_ty = resolved_expr_type(rhs, binding_types);
            if let (Some(l_bits), Some(r_bits)) =
                (scalar_bit_width(&lhs_ty), scalar_bit_width(&rhs_ty))
            {
                let op_bits = l_bits.max(r_bits);
                if let Some(bin_bits) = scalar_bit_width(ty) {
                    if op_bits < bin_bits {
                        return NirType::Int {
                            bits: op_bits,
                            signed: false,
                        };
                    }
                }
            }
            ty.clone()
        }
        _ => expr_type(expr),
    }
}

fn try_strip_outer_cast(
    expr: &PreHirExpr,
    binding_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    try_strip_outer_cast_for_target(expr, binding_ty, binding_types)
}

fn try_strip_return_outer_cast(
    expr: &PreHirExpr,
    return_type: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    try_strip_outer_cast_for_target(expr, return_type, binding_types)
}

/// Strip `(cast_ty)inner` from a value stored into (or returned as)
/// `target_ty`, where the store performs C's implicit conversion instead.
///
/// The two agree only when the conversion cannot depend on anything the
/// tree does not establish:
///
/// - `inner` must have a known integer or boolean type. A float operand is a
///   conversion, and an operand of unknown type (`binding_types` holds scalar
///   bindings only, so a float, pointer or untyped binding reads as unknown)
///   has a width and kind nothing here can vouch for.
/// - No bit the cast extends may survive into `target_ty`. A widening cast is
///   the only place this IR records sign- versus zero-extension (the builder
///   lowers `INT_SEXT`/`INT_ZEXT` to a cast to the signed/unsigned wider
///   type), and the operand's binding type is not final here: the type fixed
///   point runs again after this pass, and C extends by that type. So the
///   operand must already be `target_ty`'s width, which leaves the cast either
///   a same-width reinterpretation or an extension the store truncates away.
///   A boolean (0 or 1) extends to the same value either way.
fn try_strip_outer_cast_for_target(
    expr: &PreHirExpr,
    target_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    let PreHirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    if !is_scalar_non_unknown(cast_ty) || !is_scalar_non_unknown(target_ty) {
        return None;
    }
    let cast_bits = scalar_bit_width(cast_ty)?;
    let target_bits = scalar_bit_width(target_ty)?;
    if cast_bits < target_bits {
        // `(uint)x` into a `long`: the store extends the cast's result, not `x`.
        return None;
    }
    let preserved = match resolved_expr_type(inner, binding_types) {
        NirType::Bool => true,
        NirType::Int { bits, .. } => bits == target_bits,
        NirType::Unknown | NirType::Float { .. } | NirType::Ptr(_) | NirType::Aggregate { .. } => {
            false
        }
    };
    preserved.then(|| (**inner).clone())
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

    fn signed_local(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: int(32, true),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    fn unsigned_lt_const(name: &str, bound: i64) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Lt,
            lhs: Box::new(PreHirExpr::Var(name.to_string())),
            rhs: Box::new(PreHirExpr::Const(bound, int(32, false))),
            ty: NirType::Bool,
        }
    }

    fn compares_through_unsigned_cast(cond: &PreHirExpr, name: &str) -> bool {
        matches!(cond, PreHirExpr::Binary { lhs, .. }
            if matches!(lhs.as_ref(), PreHirExpr::Cast { ty, expr }
                if *ty == int(32, false)
                    && matches!(expr.as_ref(), PreHirExpr::Var(var) if var == name)))
    }

    #[test]
    fn restores_unsigned_cast_in_a_while_condition() {
        let mut func = PreHirFunction {
            name: "unsigned_compare_while".to_string(),
            locals: vec![signed_local("i")],
            body: vec![PreHirStmt::While {
                cond: unsigned_lt_const("i", 10),
                body: std::rc::Rc::new(vec![PreHirStmt::Break]),
            }],
            ..Default::default()
        };

        assert!(canonicalize_unsigned_compare_binding_casts(&mut func));
        let PreHirStmt::While { cond, .. } = &func.body[0] else {
            panic!("loop kept");
        };
        assert!(compares_through_unsigned_cast(cond, "i"), "{cond:?}");
    }

    #[test]
    fn restores_unsigned_cast_in_a_do_while_condition() {
        let mut func = PreHirFunction {
            name: "unsigned_compare_do_while".to_string(),
            locals: vec![signed_local("i")],
            body: vec![PreHirStmt::DoWhile {
                body: std::rc::Rc::new(vec![PreHirStmt::Continue]),
                cond: unsigned_lt_const("i", 10),
            }],
            ..Default::default()
        };

        assert!(canonicalize_unsigned_compare_binding_casts(&mut func));
        let PreHirStmt::DoWhile { cond, .. } = &func.body[0] else {
            panic!("loop kept");
        };
        assert!(compares_through_unsigned_cast(cond, "i"), "{cond:?}");
    }

    #[test]
    fn restores_unsigned_cast_in_every_statement_not_only_the_first() {
        let guard = |name: &str| PreHirStmt::If {
            cond: unsigned_lt_const(name, 10),
            then_body: std::rc::Rc::new(vec![PreHirStmt::Break]),
            else_body: std::rc::Rc::new(vec![]),
        };
        let mut func = PreHirFunction {
            name: "unsigned_compare_twice".to_string(),
            locals: vec![signed_local("i"), signed_local("j")],
            body: vec![guard("i"), guard("j")],
            ..Default::default()
        };

        assert!(canonicalize_unsigned_compare_binding_casts(&mut func));
        for (stmt, name) in func.body.iter().zip(["i", "j"]) {
            let PreHirStmt::If { cond, .. } = stmt else {
                panic!("guard kept");
            };
            assert!(compares_through_unsigned_cast(cond, name), "{cond:?}");
        }
    }

    #[test]
    fn restores_unsigned_cast_in_every_call_argument_not_only_the_first() {
        let mut func = PreHirFunction {
            name: "unsigned_compare_in_arguments".to_string(),
            locals: vec![signed_local("i"), signed_local("j")],
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "observe".to_string(),
                args: vec![unsigned_lt_const("i", 10), unsigned_lt_const("j", 10)],
                ty: NirType::Unknown,
            })],
            ..Default::default()
        };

        assert!(canonicalize_unsigned_compare_binding_casts(&mut func));
        let PreHirStmt::Expr(PreHirExpr::Call { args, .. }) = &func.body[0] else {
            panic!("call kept");
        };
        for (arg, name) in args.iter().zip(["i", "j"]) {
            assert!(compares_through_unsigned_cast(arg, name), "{arg:?}");
        }
    }
}

#[cfg(test)]
mod comparison_view_tests {
    use super::*;

    #[test]
    fn redundant_cast_cleanup_preserves_division_and_shift_views_before_type_refinement() {
        let unsigned = NirType::Int {
            bits: 32,
            signed: false,
        };
        let signed = NirType::Int {
            bits: 32,
            signed: true,
        };
        for op in [
            PreHirBinaryOp::Div,
            PreHirBinaryOp::Mod,
            PreHirBinaryOp::Shr,
            PreHirBinaryOp::Sar,
        ] {
            for binding_ty in [unsigned.clone(), signed.clone()] {
                let types = HashMap::from_iter([("carrier".into(), binding_ty)]);
                let mut expr = PreHirExpr::Binary {
                    op,
                    lhs: Box::new(PreHirExpr::Cast {
                        ty: unsigned.clone(),
                        expr: Box::new(PreHirExpr::Var("carrier".into())),
                    }),
                    rhs: Box::new(PreHirExpr::Const(8, unsigned.clone())),
                    ty: unsigned.clone(),
                };
                let original = expr.clone();
                assert!(!strip_redundant_casts_in_expr(&mut expr, &types));
                assert_eq!(
                    expr, original,
                    "the storage view must survive a later signed binding"
                );
            }
        }
    }

    #[test]
    fn assignment_cast_elision_resolves_variable_width_before_removing_conversion() {
        let narrow = NirType::Int {
            bits: 32,
            signed: false,
        };
        let wide = NirType::Int {
            bits: 64,
            signed: false,
        };
        let expr = PreHirExpr::Cast {
            ty: narrow.clone(),
            expr: Box::new(PreHirExpr::Var("carrier".into())),
        };
        let mut types = HashMap::from_iter([("carrier".to_string(), wide)]);
        assert!(try_strip_outer_cast(&expr, &narrow, &types).is_none());
        types.insert("carrier".into(), NirType::Ptr(Box::new(NirType::Unknown)));
        assert!(try_strip_outer_cast(&expr, &narrow, &types).is_none());
        types.insert("carrier".into(), narrow.clone());
        assert_eq!(
            try_strip_outer_cast(&expr, &narrow, &types),
            Some(PreHirExpr::Var("carrier".into()))
        );
        let widening = PreHirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
            expr: Box::new(PreHirExpr::Var("carrier".into())),
        };
        // A widening cast is where the IR records zero- versus sign-extension,
        // and `carrier`'s type may still change after cast elision: keep it.
        assert!(
            try_strip_outer_cast(
                &widening,
                &NirType::Int {
                    bits: 64,
                    signed: false
                },
                &types
            )
            .is_none()
        );
    }

    #[test]
    fn redundant_cast_cleanup_preserves_integer_comparison_views() {
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let type_map = HashMap::from_iter([("carrier".to_string(), ty.clone())]);
        for op in [
            PreHirBinaryOp::Eq,
            PreHirBinaryOp::Ne,
            PreHirBinaryOp::SLt,
            PreHirBinaryOp::SLe,
            PreHirBinaryOp::SGt,
            PreHirBinaryOp::SGe,
            PreHirBinaryOp::Lt,
            PreHirBinaryOp::Le,
        ] {
            let mut comparison = PreHirExpr::Binary {
                op,
                lhs: Box::new(PreHirExpr::Cast {
                    ty: ty.clone(),
                    expr: Box::new(PreHirExpr::Var("carrier".into())),
                }),
                rhs: Box::new(PreHirExpr::Const(-1, ty.clone())),
                ty: NirType::Bool,
            };
            let original = comparison.clone();
            assert!(!strip_redundant_casts_in_expr(&mut comparison, &type_map));
            assert_eq!(comparison, original);
        }
        let mut plain = PreHirExpr::Cast {
            ty,
            expr: Box::new(PreHirExpr::Var("carrier".into())),
        };
        assert!(strip_redundant_casts_in_expr(&mut plain, &type_map));
        assert!(matches!(plain, PreHirExpr::Var(_)));
    }

    #[test]
    fn try_strip_return_outer_cast_preserves_truncating_cast() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let types = HashMap::from_iter([("val".to_string(), u64_ty.clone())]);
        // return (uint32_t)(val >> 32);
        let trunc_expr = PreHirExpr::Cast {
            ty: u32_ty.clone(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(PreHirExpr::Var("val".into())),
                rhs: Box::new(PreHirExpr::Const(32, u64_ty.clone())),
                ty: u64_ty.clone(),
            }),
        };
        assert!(try_strip_return_outer_cast(&trunc_expr, &u32_ty, &types).is_none());

        // Direct var of 64-bit: return (uint32_t)val;
        let direct_trunc = PreHirExpr::Cast {
            ty: u32_ty.clone(),
            expr: Box::new(PreHirExpr::Var("val".into())),
        };
        assert!(try_strip_return_outer_cast(&direct_trunc, &u32_ty, &types).is_none());

        // Safe when inner expression is 32-bit: return (uint32_t)val32;
        let types32 = HashMap::from_iter([("val32".to_string(), u32_ty.clone())]);
        let safe_cast = PreHirExpr::Cast {
            ty: u32_ty.clone(),
            expr: Box::new(PreHirExpr::Var("val32".into())),
        };
        assert_eq!(
            try_strip_return_outer_cast(&safe_cast, &u32_ty, &types32),
            Some(PreHirExpr::Var("val32".into()))
        );
    }

    #[test]
    fn preserves_float_to_int_and_signedness_changing_widening_outer_casts() {
        let int = |bits: u32, signed: bool| NirType::Int { bits, signed };
        let f32_ty = NirType::Float { bits: 32 };
        let i8_ty = int(8, true);
        let i32_ty = int(32, true);
        let u32_ty = int(32, false);
        let u64_ty = int(64, false);
        let types = HashMap::from_iter([
            ("fval".to_string(), f32_ty),
            ("s8".to_string(), i8_ty),
            ("x32".to_string(), i32_ty.clone()),
        ]);

        // Float -> Int conversion must not be stripped even across different integer target widths.
        let float_to_int = PreHirExpr::Cast {
            ty: i32_ty.clone(),
            expr: Box::new(PreHirExpr::Var("fval".into())),
        };
        assert!(try_strip_outer_cast(&float_to_int, &u32_ty, &types).is_none());
        assert!(try_strip_return_outer_cast(&float_to_int, &u32_ty, &types).is_none());

        // Signedness-changing widening (u64)(i8) assigned to i32 must not be stripped.
        let sign_widen = PreHirExpr::Cast {
            ty: u64_ty,
            expr: Box::new(PreHirExpr::Var("s8".into())),
        };
        assert!(try_strip_outer_cast(&sign_widen, &i32_ty, &types).is_none());
        assert!(try_strip_return_outer_cast(&sign_widen, &i32_ty, &types).is_none());
    }
}
