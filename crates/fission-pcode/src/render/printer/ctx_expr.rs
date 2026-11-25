//! Context-aware expression and lvalue printing (struct field access, cast
//! elision, compound assignment).

use super::*;

/// HIR-only: drop identity / pure widening casts that add noise without helping
/// humans. NIR profile never calls this.
pub(super) fn try_elide_hir_cast(
    ty: &NirType,
    expr: &HirExpr,
    parent_prec: u8,
    depth: usize,
    ctx: &PrintCtx<'_>,
) -> Option<String> {
    // (T)x where x already has type T (via var decl) → print x
    if let HirExpr::Var(name) = expr {
        if let Some(vt) = ctx.var_types.get(name.as_str()) {
            if *vt == ty {
                return Some(print_expr_prec_ctx(expr, parent_prec, depth + 1, ctx));
            }
        }
    }
    // (T)const when const already typed as T
    if let HirExpr::Const(_, cty) = expr {
        if cty == ty {
            return Some(print_expr_prec_ctx(expr, parent_prec, depth + 1, ctx));
        }
    }
    // (T)(T)x → peel one layer
    if let HirExpr::Cast {
        ty: inner_ty,
        expr: inner,
    } = expr
    {
        if inner_ty == ty {
            return try_elide_hir_cast(ty, inner, parent_prec, depth, ctx).or_else(|| {
                Some(print_expr_prec_ctx(
                    &HirExpr::Cast {
                        ty: ty.clone(),
                        expr: inner.clone(),
                    },
                    parent_prec,
                    depth,
                    ctx,
                ))
            });
        }
    }
    // (ulonglong)(uint)x / nested same-family integer casts → drop the inner
    // cast when outer is wider unsigned and inner is already an integer
    // cast/var. The outer cast stays: it sets the width the surrounding
    // operator works in, and `(unsigned __int128)(ulonglong)x * k` printed
    // as `x * k` is a 64-bit multiply.
    if let (
        NirType::Int {
            bits: outer_bits,
            signed: false,
        },
        HirExpr::Cast {
            ty:
                NirType::Int {
                    bits: inner_bits,
                    signed: false,
                },
            expr: inner,
        },
    ) = (ty, expr)
    {
        // Eliding both casts is safe only when the source is already
        // unsigned and no wider than the inner cast.  For a signed source,
        // `(u64)(u32)x` performs a required zero-extension at 32 bits;
        // printing just `x` would sign-extend a negative lane when it is
        // widened to 64 bits.  For a wider source the inner cast truncates.
        if *outer_bits >= *inner_bits
            && expr_unsigned_integer_width(inner, ctx).is_some_and(|bits| bits <= *inner_bits)
        {
            if *outer_bits == *inner_bits {
                return Some(print_expr_prec_ctx(inner, parent_prec, depth + 1, ctx));
            }
            return Some(print_expr_prec_ctx(
                &HirExpr::Cast {
                    ty: ty.clone(),
                    expr: inner.clone(),
                },
                parent_prec,
                depth + 1,
                ctx,
            ));
        }
    }
    None
}

/// The width of `expr` when it is an unsigned integer, `None` otherwise.
pub(super) fn expr_unsigned_integer_width(expr: &HirExpr, ctx: &PrintCtx<'_>) -> Option<u32> {
    let ty = match expr {
        HirExpr::Var(name) => ctx.var_types.get(name.as_str()).copied(),
        HirExpr::Const(_, ty)
        | HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Select { ty, .. } => Some(ty),
        HirExpr::Index { elem_ty, .. } => Some(elem_ty),
        HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::PtrOffset { .. }
        | HirExpr::AggregateCopy { .. } => None,
    }?;
    match ty {
        NirType::Int {
            bits,
            signed: false,
        } => Some(*bits),
        _ => None,
    }
}

pub(super) fn print_expr_with_ctx(expr: &HirExpr, ctx: &PrintCtx<'_>) -> String {
    print_expr_prec_ctx(expr, 0, 0, ctx)
}

pub(super) fn print_expr_prec_ctx(
    expr: &HirExpr,
    parent_prec: u8,
    depth: usize,
    ctx: &PrintCtx<'_>,
) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "0 /* [FISSION] RECURSION TOO DEEP (expression printer guard) */".to_string();
    }
    let (text, prec) = match expr {
        HirExpr::AddressOfLocal(name) => (format!("&{name}"), 110),
        HirExpr::PtrOffset { base, offset } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let text = if *offset == 0 {
                inner
            } else if *offset > 0 {
                format!("(uint8_t *)({inner}) + {offset}")
            } else {
                format!("(uint8_t *)({inner}) - {}", offset.unsigned_abs())
            };
            (text, 60)
        }
        HirExpr::FieldAccess {
            base, field_name, ..
        } => {
            let mut inner = print_expr_prec_ctx(base, 110, depth + 1, ctx);
            if matches!(base.as_ref(), HirExpr::Cast { .. }) {
                inner = format!("({inner})");
            }
            let is_ptr = ctx.expr_is_pointer(base);
            let op = if is_ptr { "->" } else { "." };
            (format!("{inner}{op}{field_name}"), 110)
        }
        HirExpr::AddressOfGlobal(name) => {
            if name.starts_with('"') {
                (name.clone(), 120)
            } else {
                (format!("&{name}"), 110)
            }
        }
        HirExpr::Var(name) => (name.clone(), 120),
        HirExpr::Const(value, ty) => {
            if let Some(name) = ctx.constant_name(*value) {
                (name.to_string(), 120)
            } else {
                (print_integer_const(*value, ty), 120)
            }
        }
        HirExpr::Cast { ty, expr } => {
            if let Some(pointer_diff) = print_pointer_diff_cast(ty, expr, depth, ctx) {
                return pointer_diff;
            }
            // HIR presentation: elide no-op / same-family widening casts when safe.
            if matches!(ctx.profile, PrintProfile::Hir) {
                if let Some(elided) = try_elide_hir_cast(ty, expr, parent_prec, depth, ctx) {
                    return elided;
                }
            }
            if matches!(ty, NirType::Ptr(_))
                && let HirExpr::Const(value, source_ty) = expr.as_ref()
                && source_ty == ty
            {
                let inner = ctx
                    .global_names
                    .and_then(|names| names.get(&(*value as u64)).cloned())
                    .unwrap_or_else(|| format!("0x{:x}", *value as u64));
                (format!("({}){inner}", print_type(ty)), 110)
            } else {
                // If casting from a pointer type to a non-pointer integer type (e.g. (uint)ptr),
                // insert an intermediate (ulonglong) cast to avoid invalid C (pointer-to-int truncation
                // without going through a pointer-sized integer is UB/error in strict C11).
                let expr_is_ptr = ctx.expr_is_pointer(expr);
                let target_is_int = !matches!(ty, NirType::Ptr(_));
                let inner = print_expr_prec_ctx(expr, 110, depth + 1, ctx);
                if expr_is_ptr && target_is_int {
                    (
                        format!("({})(unsigned long long){}", print_type(ty), inner),
                        110,
                    )
                } else {
                    (format!("({}){}", print_type(ty), inner), 110)
                }
            }
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner =
                separate_unary_minus(symbol, print_expr_prec_ctx(expr, 110, depth + 1, ctx));
            if matches!(op, HirUnaryOp::Neg | HirUnaryOp::BitNot) && ctx.expr_is_pointer(expr) {
                (format!("{symbol}(unsigned long long){inner}"), 110)
            } else {
                (format!("{symbol}{inner}"), 110)
            }
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let prec = binary_precedence(*op);
            let mut lhs_str = print_expr_prec_ctx(lhs, prec, depth + 1, ctx);
            let rhs_parent_prec = binary_rhs_parent_precedence(*op, rhs, prec + 1);
            let mut rhs_str = print_expr_prec_ctx(rhs, rhs_parent_prec, depth + 1, ctx);

            // `if (local_4 == 258)` where local_4 holds WaitForSingleObject's
            // result: name the literal after the group that return carries.
            // Only equality, because a group member is a distinct value rather
            // than a threshold -- `< WAIT_TIMEOUT` would read as an ordering
            // the constants do not have.
            if matches!(*op, HirBinaryOp::Eq | HirBinaryOp::Ne) {
                if let Some(named) = ctx.name_compared_constant(lhs, rhs) {
                    rhs_str = format!("{rhs_str} /* {named} */");
                } else if let Some(named) = ctx.name_compared_constant(rhs, lhs) {
                    lhs_str = format!("{lhs_str} /* {named} */");
                }
            }

            // Pointer plus pointer, and pointer minus pointer of unrelated
            // types, are both things C will not do. One side becomes an
            // integer and the operation keeps the meaning the machine gave it.
            // C subtracts an integer from a pointer and one pointer from
            // another, but never a pointer from an integer -- so a pointer on
            // the right of `-` always has to become one. Addition is
            // commutative and only pointer *plus pointer* is out of bounds.
            if ctx.operand_prints_as_pointer(rhs)
                && (matches!(*op, HirBinaryOp::Sub)
                    || (matches!(*op, HirBinaryOp::Add) && ctx.operand_prints_as_pointer(lhs)))
            {
                rhs_str = format!("(unsigned long long){rhs_str}");
            }
            if rejects_pointer_operands(*op) {
                if ctx.operand_prints_as_pointer(lhs)
                    || printed_operand_is_pointer_var(&lhs_str, ctx)
                {
                    lhs_str = format!("(unsigned long long){lhs_str}");
                }
                if ctx.operand_prints_as_pointer(rhs)
                    || printed_operand_is_pointer_var(&rhs_str, ctx)
                {
                    rhs_str = format!("(unsigned long long){rhs_str}");
                }
            }

            if *op == HirBinaryOp::Shr {
                if let Some(count) = width_minus_shift_count(rhs) {
                    let count_str = print_expr_prec_ctx(count, 60, depth + 1, ctx);
                    (
                        format!(
                            "{count_str} == 0 ? 0 : {lhs_str} {} {rhs_str}",
                            print_binary_op(*op)
                        ),
                        20,
                    )
                } else {
                    (
                        format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                        prec,
                    )
                }
            } else {
                (
                    format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                    prec,
                )
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let prec = 20;
            let cond = print_expr_prec_ctx(cond, prec, depth + 1, ctx);
            let then_expr = print_expr_prec_ctx(then_expr, prec, depth + 1, ctx);
            let else_expr = print_expr_prec_ctx(else_expr, prec, depth + 1, ctx);
            (format!("{cond} ? {then_expr} : {else_expr}"), prec)
        }
        HirExpr::Call {
            target, args, ty, ..
        } => {
            if target == "__fission_callind_opaque" && !args.is_empty() {
                let fn_ptr = print_expr_prec_ctx(&args[0], 0, depth + 1, ctx);
                // `args[0]` is the callee, so the real arguments start at 1 and
                // their parameter positions are shifted by one.
                let callee = callind_target_symbol(&args[0]);
                let remaining_args = args[1..]
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let printed = print_expr_prec_ctx(arg, 0, depth + 1, ctx);
                        match callee.as_deref() {
                            Some(callee) => annotate_enum_argument(callee, index, arg, printed),
                            None => printed,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret_ty = match ty {
                    NirType::Unknown => "unsigned long long".to_string(),
                    other => {
                        let printed = print_type(other);
                        if printed.is_empty() || printed == "void" {
                            "unsigned long long".to_string()
                        } else {
                            printed
                        }
                    }
                };
                (
                    // An empty function-pointer parameter list is a C11
                    // non-prototype type, so the call keeps its observed args
                    // without asserting a signature that was not recovered.
                    format!("(({ret_ty} (*)())({fn_ptr}))({remaining_args})"),
                    120,
                )
            } else {
                let callee = target.clone();
                let target = print_callable_target(target, args, ty, Some(ctx));
                let args = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let printed = print_expr_prec_ctx(arg, 0, depth + 1, ctx);
                        annotate_enum_argument(&callee, index, arg, printed)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                (format!("{target}({args})"), 120)
            }
        }
        HirExpr::Load { ptr, ty } => {
            if ctx.simple_deref_target_matches_access(ptr, ty)
                && let Some(target) = peel_simple_deref_target(ptr)
            {
                (format!("*{target}"), 110)
            } else {
                let inner = print_expr_prec_ctx(ptr, 0, depth + 1, ctx);
                (format!("*({} *)({inner})", print_type(ty)), 110)
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let mut index_str = print_expr_prec_ctx(index, 0, depth + 1, ctx);
            // A subscript has to be an integer, whatever the value was
            // recovered as.
            if ctx.operand_prints_as_pointer(index) {
                index_str = format!("(unsigned long long){index_str}");
            }
            let index = index_str;
            let text = if matches!(elem_ty, NirType::Aggregate { .. }) {
                format!("(({} *)({inner}))[{index}]", print_type(elem_ty))
            } else {
                match base.as_ref() {
                    HirExpr::Var(name)
                        if !ctx.direct_var_has_aggregate_access_mismatch(name, elem_ty)
                            && !ctx.direct_var_has_scalar_access_width_mismatch(name, elem_ty) =>
                    {
                        format!("{name}[{index}]")
                    }
                    HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                    _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
                }
            };
            (text, 120)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr_prec_ctx(src, 0, depth + 1, ctx);
            (format!("*(fission_agg{} *)({inner})", size), 110)
        }
    };
    if prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

pub(super) fn print_pointer_diff_cast(
    cast_ty: &NirType,
    expr: &HirExpr,
    depth: usize,
    ctx: &PrintCtx<'_>,
) -> Option<String> {
    if !matches!(cast_ty, NirType::Int { .. } | NirType::Bool) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if !ctx.expr_is_pointer(lhs) || !ctx.expr_is_pointer(rhs) {
        return None;
    }
    let lhs = print_expr_prec_ctx(lhs, 0, depth + 1, ctx);
    let rhs = print_expr_prec_ctx(rhs, 0, depth + 1, ctx);
    Some(format!(
        "({})((uint8_t *)({lhs}) - (uint8_t *)({rhs}))",
        print_type(cast_ty)
    ))
}

pub(super) fn print_lvalue_ctx(lhs: &HirLValue, depth: usize, ctx: &PrintCtx<'_>) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "/* [FISSION] RECURSION TOO DEEP */".to_string();
    }
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::FieldAccess {
            base, field_name, ..
        } => {
            let mut inner = print_expr_prec_ctx(base, 110, depth + 1, ctx);
            if matches!(base.as_ref(), HirExpr::Cast { .. }) {
                inner = format!("({inner})");
            }
            let is_ptr = ctx.expr_is_pointer(base);
            let op = if is_ptr { "->" } else { "." };
            format!("{inner}{op}{field_name}")
        }
        HirLValue::Deref { ptr, ty } => {
            if ctx.simple_deref_target_matches_access(ptr, ty)
                && let Some(target) = peel_simple_deref_target(ptr)
            {
                format!("*{target}")
            } else {
                format!(
                    "*({} *)({})",
                    print_type(ty),
                    print_expr_prec_ctx(ptr, 0, depth + 1, ctx)
                )
            }
        }
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let index = print_expr_prec_ctx(index, 0, depth + 1, ctx);
            if matches!(elem_ty, NirType::Aggregate { .. }) {
                format!("(({} *)({inner}))[{index}]", print_type(elem_ty))
            } else {
                match base.as_ref() {
                    HirExpr::Var(name)
                        if !ctx.direct_var_has_aggregate_access_mismatch(name, elem_ty)
                            && !ctx.direct_var_has_scalar_access_width_mismatch(name, elem_ty) =>
                    {
                        format!("{name}[{index}]")
                    }
                    HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                    _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
                }
            }
        }
    }
}

pub(super) fn is_integer_bitop(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::And
            | HirBinaryOp::Or
            | HirBinaryOp::Xor
            | HirBinaryOp::Shl
            | HirBinaryOp::Shr
            | HirBinaryOp::Sar
    )
}

/// Whether `expr` prints as a pointer using only what the node itself says.
///
/// The ctx-free printer has no declarations to consult, so this sees less than
/// [`PrintCtx::operand_prints_as_pointer`] -- but `&global` and a cast to a
/// pointer type are unambiguous without one, and those are what reach a bitop
/// or a subtraction in practice.
pub(super) fn expr_prints_as_pointer_ctx_free(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) => true,
        HirExpr::Cast { ty, .. } => matches!(ty, NirType::Ptr(_)),
        HirExpr::Load { ty, .. } | HirExpr::Index { elem_ty: ty, .. } => {
            matches!(ty, NirType::Ptr(_))
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => expr_prints_as_pointer_ctx_free(lhs) || expr_prints_as_pointer_ctx_free(rhs),
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => expr_prints_as_pointer_ctx_free(lhs) && !expr_prints_as_pointer_ctx_free(rhs),
        _ => false,
    }
}

/// Operators C refuses a pointer operand for outright.
///
/// `+` and `-` are not here: pointer plus integer is exactly what they are
/// for. Only pointer *and* pointer is the illegal shape, handled separately.
pub(super) fn rejects_pointer_operands(op: HirBinaryOp) -> bool {
    is_integer_bitop(op) || matches!(op, HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod)
}

pub(super) fn printed_operand_is_pointer_var(operand: &str, ctx: &PrintCtx<'_>) -> bool {
    ctx.var_types
        .get(operand)
        .is_some_and(|ty| matches!(ty, NirType::Ptr(_)))
}

pub(super) fn pointer_scalar_compound_op(op: HirBinaryOp) -> Option<&'static str> {
    match op {
        HirBinaryOp::Mul => Some("*"),
        HirBinaryOp::Div => Some("/"),
        HirBinaryOp::Mod => Some("%"),
        HirBinaryOp::And => Some("&"),
        HirBinaryOp::Or => Some("|"),
        HirBinaryOp::Xor => Some("^"),
        HirBinaryOp::Shl => Some("<<"),
        HirBinaryOp::Shr | HirBinaryOp::Sar => Some(">>"),
        _ => None,
    }
}

pub(super) fn try_compound_assignment(
    lhs: &HirLValue,
    rhs: &HirExpr,
    ctx: &PrintCtx<'_>,
) -> Option<String> {
    let HirLValue::Var(var_name) = lhs else {
        return None;
    };
    let HirExpr::Binary {
        op,
        lhs: lhs_expr,
        rhs: rhs_expr,
        ..
    } = rhs
    else {
        return None;
    };
    let HirExpr::Var(lhs_name) = lhs_expr.as_ref() else {
        return None;
    };
    if var_name != lhs_name {
        return None;
    }

    // If the LHS variable is a pointer type and the operation is scalar-only,
    // we cannot emit `ptr *= val` / `ptr &= val` (invalid C). Keep the same
    // surface variable but force the arithmetic through a pointer-sized scalar.
    let var_is_ptr = ctx.pointer_decl_names.contains(var_name.as_str());
    if var_is_ptr && let Some(op_str) = pointer_scalar_compound_op(*op) {
        let ptr_ty = ctx
            .decl_type_text
            .get(var_name.as_str())
            .cloned()
            .unwrap_or_else(|| "void *".to_string());
        let mut rhs_str = print_expr_with_ctx(rhs_expr, ctx);
        if ctx.operand_prints_as_pointer(rhs_expr) || printed_operand_is_pointer_var(&rhs_str, ctx)
        {
            rhs_str = format!("(unsigned long long){rhs_str}");
        }
        return Some(format!(
            "{var_name} = ({ptr_ty})((unsigned long long){var_name} {op_str} {rhs_str});"
        ));
    }

    if matches!(op, HirBinaryOp::Add) && matches!(rhs_expr.as_ref(), HirExpr::Const(1, _)) {
        return Some(format!("{}++;", var_name));
    }
    if matches!(op, HirBinaryOp::Sub) && matches!(rhs_expr.as_ref(), HirExpr::Const(1, _)) {
        return Some(format!("{}--;", var_name));
    }
    let op_str = match op {
        HirBinaryOp::Add => "+=",
        HirBinaryOp::Sub => "-=",
        HirBinaryOp::Mul => "*=",
        HirBinaryOp::Div => "/=",
        HirBinaryOp::Mod => "%=",
        HirBinaryOp::And => "&=",
        HirBinaryOp::Or => "|=",
        HirBinaryOp::Xor => "^=",
        HirBinaryOp::Shl => "<<=",
        HirBinaryOp::Shr => ">>=",
        _ => return None,
    };

    let mut rhs_str = print_expr_with_ctx(rhs_expr, ctx);
    // The compound form hides an operand, but C still reads it: `x &= p` is
    // `x & p`, and `x += p` is pointer plus pointer when x is one too.
    if ctx.operand_prints_as_pointer(rhs_expr)
        && (rejects_pointer_operands(*op)
            || (matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub)
                && ctx.operand_prints_as_pointer(lhs_expr)))
    {
        rhs_str = format!("(unsigned long long){rhs_str}");
    }

    Some(format!("{} {} {};", var_name, op_str, rhs_str))
}

pub(super) fn print_assignment_rhs(lhs: &HirLValue, rhs: &HirExpr, ctx: &PrintCtx<'_>) -> String {
    let rhs_str = print_expr_with_ctx(rhs, ctx);
    let HirLValue::Var(var_name) = lhs else {
        return rhs_str;
    };
    let Some(lhs_ty) = ctx.var_types.get(var_name.as_str()) else {
        return rhs_str;
    };
    match lhs_ty {
        NirType::Ptr(_) => {
            if !ctx.expr_has_pointer_like_result(rhs) {
                format!("({})({rhs_str})", print_type(lhs_ty))
            } else if ctx.pointer_assignment_needs_decl_cast(var_name, rhs) {
                if let Some(declaration) = ctx.decl_type_text.get(var_name.as_str()) {
                    format!("({declaration})({rhs_str})")
                } else {
                    rhs_str
                }
            } else {
                rhs_str
            }
        }
        NirType::Int { .. } if ctx.expr_has_pointer_like_result(rhs) => {
            format!("({})(unsigned long long)({rhs_str})", print_type(lhs_ty))
        }
        _ => rhs_str,
    }
}

pub(super) fn print_return_expr(expr: &HirExpr, ctx: &PrintCtx<'_>) -> String {
    let expr_str = print_expr_with_ctx(expr, ctx);
    match ctx.return_type {
        NirType::Ptr(_) if !ctx.expr_has_pointer_like_result(expr) => {
            format!("({})({expr_str})", print_type(ctx.return_type))
        }
        NirType::Int { .. } if ctx.expr_has_pointer_like_result(expr) => {
            format!(
                "({})(unsigned long long)({expr_str})",
                print_type(ctx.return_type)
            )
        }
        _ => expr_str,
    }
}
