//! Specialized use-site type promotions.
//!
//! These refinements are intentionally separate from the generic constraint
//! collector in the parent module. Each pass uses whole-function evidence from
//! return values, unknown calls, stored values, or narrowing return expressions
//! to strengthen types after the ordinary use constraints have been merged.

use super::*;

pub(super) fn promote_return_signedness_from_returns(func: &mut PreHirFunction) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits: return_bits,
        signed: false,
    } = &func.return_type
    else {
        return false;
    };
    let return_bits = *return_bits;

    let known_binding_types = collect_known_binding_types(func);
    let mut candidates = Vec::new();
    let value_return_count =
        collect_value_return_types(&func.body, &known_binding_types, &mut candidates);
    if value_return_count == 0 || candidates.len() != value_return_count {
        return false;
    }
    if candidates.iter().all(|ty| {
        matches!(
            ty,
            NirType::Int {
                bits,
                signed: true
            } if *bits == return_bits
        )
    }) {
        func.return_type = NirType::Int {
            bits: return_bits,
            signed: true,
        };
        true
    } else {
        false
    }
}

fn collect_value_return_types(
    stmts: &[PreHirStmt],
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    let mut value_return_count = 0usize;
    for stmt in stmts {
        value_return_count += collect_value_return_types_stmt(stmt, known_binding_types, out);
    }
    value_return_count
}

fn collect_value_return_types_stmt(
    stmt: &PreHirStmt,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            if let Some(ty) = return_expr_type(expr, known_binding_types) {
                out.push(ty);
            }
            1
        }
        PreHirStmt::Return(None) => 0,
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_value_return_types(stmts, known_binding_types, out)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_value_return_types(then_body, known_binding_types, out)
                + collect_value_return_types(else_body, known_binding_types, out)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut value_return_count = 0;
            for case in cases {
                value_return_count +=
                    collect_value_return_types(&case.body, known_binding_types, out);
            }
            value_return_count + collect_value_return_types(default, known_binding_types, out)
        }
        _ => 0,
    }
}

pub(super) fn promote_unknown_call_return_type(func: &mut PreHirFunction) -> bool {
    if func.surface_return_type_name.is_some() || func.return_type != NirType::Unknown {
        return false;
    }
    let mut value_return_count = 0usize;
    let mut unknown_call_return_count = 0usize;
    collect_unknown_call_returns(
        &func.body,
        &mut value_return_count,
        &mut unknown_call_return_count,
    );
    if value_return_count == 0 || value_return_count != unknown_call_return_count {
        return false;
    }
    func.return_type = native_unsigned_word_type(func);
    true
}

fn native_unsigned_word_type(func: &PreHirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

fn collect_unknown_call_returns(
    stmts: &[PreHirStmt],
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    for stmt in stmts {
        collect_unknown_call_returns_stmt(stmt, value_return_count, unknown_call_return_count);
    }
}

fn collect_unknown_call_returns_stmt(
    stmt: &PreHirStmt,
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            *value_return_count += 1;
            if is_unknown_call_result(expr) {
                *unknown_call_return_count += 1;
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_unknown_call_returns(stmts, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_unknown_call_returns(then_body, value_return_count, unknown_call_return_count);
            collect_unknown_call_returns(else_body, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_unknown_call_returns(
                    &case.body,
                    value_return_count,
                    unknown_call_return_count,
                );
            }
            collect_unknown_call_returns(default, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn is_unknown_call_result(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { ty, .. } => *ty == NirType::Unknown,
        PreHirExpr::Cast { expr, ty } if *ty == NirType::Unknown => is_unknown_call_result(expr),
        _ => false,
    }
}

fn count_var_uses_expr(expr: &PreHirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => count_var_uses_expr(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses_expr(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                count_var_uses_expr(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_expr(then_expr, out);
            count_var_uses_expr(else_expr, out);
        }
    }
}

fn count_var_uses_lvalue(lhs: &PreHirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => count_var_uses_expr(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            count_var_uses_expr(base, out);
        }
    }
}

fn count_var_uses_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_uses_lvalue(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            count_var_uses_expr(va_list, out);
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => count_var_uses_stmts(stmts, out),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_stmts(then_body, out);
            count_var_uses_stmts(else_body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                count_var_uses_stmt(init, out);
            }
            if let Some(cond) = cond {
                count_var_uses_expr(cond, out);
            }
            if let Some(update) = update {
                count_var_uses_stmt(update, out);
            }
            count_var_uses_stmts(body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses_expr(expr, out);
            for case in cases {
                count_var_uses_stmts(&case.body, out);
            }
            count_var_uses_stmts(default, out);
        }
        PreHirStmt::Return(Some(expr)) => count_var_uses_expr(expr, out),
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn count_var_uses_stmts(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_var_uses_stmt(stmt, out);
    }
}

fn store_value_var_name(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name.as_str()),
        PreHirExpr::Cast { expr, .. } => store_value_var_name(expr),
        _ => None,
    }
}

fn count_store_value_uses_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { .. } | PreHirLValue::Index { .. },
            rhs,
        } => {
            if let Some(name) = store_value_var_name(rhs) {
                *out.entry(name.to_owned()).or_default() += 1;
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => count_store_value_uses_stmts(stmts, out),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_store_value_uses_stmts(then_body, out);
            count_store_value_uses_stmts(else_body, out);
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                count_store_value_uses_stmt(init, out);
            }
            if let Some(update) = update {
                count_store_value_uses_stmt(update, out);
            }
            count_store_value_uses_stmts(body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_store_value_uses_stmts(&case.body, out);
            }
            count_store_value_uses_stmts(default, out);
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn count_store_value_uses_stmts(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_store_value_uses_stmt(stmt, out);
    }
}

pub(super) fn promote_store_value_only_unsigned_params(func: &mut PreHirFunction) -> bool {
    let mut all_uses = HashMap::default();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut store_value_uses = HashMap::default();
    count_store_value_uses_stmts(&func.body, &mut store_value_uses);

    let mut changed = false;
    for binding in &mut func.params {
        if binding.surface_type_name.is_some()
            || !matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_)))
        {
            continue;
        }
        let NirType::Int {
            bits: 32,
            signed: false,
        } = binding.ty
        else {
            continue;
        };
        let all = all_uses.get(&binding.name).copied().unwrap_or(0);
        let stores = store_value_uses.get(&binding.name).copied().unwrap_or(0);
        if all > 0 && all == stores {
            binding.ty = NirType::Int {
                bits: 32,
                signed: true,
            };
            changed = true;
        }
    }
    changed
}

pub(super) fn promote_store_value_only_aggregate_bindings(
    func: &mut PreHirFunction,
    constraints: &HashMap<String, Vec<UseConstraint>>,
    roles: &HashMap<String, BindingUseRole>,
) -> bool {
    let mut all_uses = HashMap::default();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut store_value_uses = HashMap::default();
    count_store_value_uses_stmts(&func.body, &mut store_value_uses);

    let mut changed = false;
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if binding.surface_type_name.is_some() {
            continue;
        }
        let all = all_uses.get(&binding.name).copied().unwrap_or(0);
        let stores = store_value_uses.get(&binding.name).copied().unwrap_or(0);
        let has_conflicting_role = roles
            .get(&binding.name)
            .is_some_and(|role| role.scalar_use || role.address_use);
        if all == 0 || (all != stores && has_conflicting_role) {
            continue;
        }
        let Some(expected) = constraints.get(&binding.name).and_then(|items| {
            let mut aggregates = items.iter().filter_map(|item| match item {
                UseConstraint::Exact(ty @ NirType::Aggregate { fields, .. })
                    if fields.is_empty() =>
                {
                    Some(ty)
                }
                _ => None,
            });
            let first = aggregates.next()?;
            aggregates.all(|ty| ty == first).then(|| first.clone())
        }) else {
            continue;
        };
        if binding.ty != expected {
            binding.ty = expected;
            changed = true;
        }
    }
    changed
}

fn wrapping_narrow_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Add
            | PreHirBinaryOp::Sub
            | PreHirBinaryOp::Mul
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
    )
}

/// Single-statement-level definitions of plain variables, for
/// [`collect_wrapping_narrow_return_vars`] to see through.
///
/// A name maps to its defining expression only if the whole body assigns it
/// exactly once; a second assignment removes it, so the map never claims a
/// definition that some other path overwrites.
fn collect_single_var_defs<'a>(
    stmts: &'a [PreHirStmt],
    defs: &mut HashMap<String, Option<&'a PreHirExpr>>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } => match defs.entry(name.clone()) {
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    slot.insert(None);
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(Some(rhs));
                }
            },
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_single_var_defs(body, defs),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_single_var_defs(then_body, defs);
                collect_single_var_defs(else_body, defs);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_single_var_defs(&case.body, defs);
                }
                collect_single_var_defs(default, defs);
            }
            _ => {}
        }
    }
}

/// What [`collect_wrapping_narrow_return_vars`] needs to see through a
/// returned temporary: the single definition of each name, and how many times
/// the whole body uses it.
struct WrappingNarrowCtx<'a> {
    single_defs: &'a HashMap<String, Option<&'a PreHirExpr>>,
    all_uses: &'a HashMap<String, usize>,
}

fn collect_wrapping_narrow_return_vars(
    expr: &PreHirExpr,
    context_bits: u32,
    out: &mut HashMap<String, usize>,
    ctx: Option<&WrappingNarrowCtx<'_>>,
    seen: &mut Vec<String>,
) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            // `return rax` after `rax = (int)(param_1 + param_2)` constrains the
            // params exactly as `return (int)(param_1 + param_2)` does, but the
            // params are not in the return expression, so without following the
            // definition they look unconstrained and stay 64-bit.
            //
            // Only a name the body uses ONCE may be followed. A second use is a
            // second consumer that this walk does not see, and it could observe
            // the operands at full width -- the truncation guarding this one
            // read is no evidence about that one.
            if let Some(ctx) = ctx {
                if ctx.all_uses.get(name).copied().unwrap_or(0) == 1
                    && !seen.iter().any(|s| s == name)
                {
                    if let Some(Some(def)) = ctx.single_defs.get(name) {
                        seen.push(name.clone());
                        collect_wrapping_narrow_return_vars(
                            def,
                            context_bits,
                            out,
                            Some(ctx),
                            seen,
                        );
                        seen.pop();
                        return;
                    }
                }
            }
            *out.entry(name.clone()).or_default() += 1;
        }
        PreHirExpr::Cast { ty, expr } => {
            let bits = nir_type_bits(ty).unwrap_or(context_bits).min(context_bits);
            collect_wrapping_narrow_return_vars(expr, bits, out, ctx, seen);
        }
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Neg,
            expr,
            ..
        } => collect_wrapping_narrow_return_vars(expr, context_bits, out, ctx, seen),
        PreHirExpr::Binary { op, lhs, rhs, .. } if wrapping_narrow_op(*op) => {
            collect_wrapping_narrow_return_vars(lhs, context_bits, out, ctx, seen);
            collect_wrapping_narrow_return_vars(rhs, context_bits, out, ctx, seen);
        }
        PreHirExpr::Const(_, _)
        | PreHirExpr::Unary { .. }
        | PreHirExpr::Binary { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::Load { .. }
        | PreHirExpr::PtrOffset { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::FieldAccess { .. }
        | PreHirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmt(
    stmt: &PreHirStmt,
    return_bits: u32,
    out: &mut HashMap<String, usize>,
    ctx: Option<&WrappingNarrowCtx<'_>>,
) {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            collect_wrapping_narrow_return_vars(expr, return_bits, out, ctx, &mut Vec::new())
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_wrapping_narrow_return_vars_stmts(stmts, return_bits, out, ctx)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_wrapping_narrow_return_vars_stmts(then_body, return_bits, out, ctx);
            collect_wrapping_narrow_return_vars_stmts(else_body, return_bits, out, ctx);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_wrapping_narrow_return_vars_stmts(&case.body, return_bits, out, ctx);
            }
            collect_wrapping_narrow_return_vars_stmts(default, return_bits, out, ctx);
        }
        _ => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmts(
    stmts: &[PreHirStmt],
    return_bits: u32,
    out: &mut HashMap<String, usize>,
    ctx: Option<&WrappingNarrowCtx<'_>>,
) {
    for stmt in stmts {
        collect_wrapping_narrow_return_vars_stmt(stmt, return_bits, out, ctx);
    }
}

pub(super) fn narrow_integer_params_from_wrapping_return_uses(func: &mut PreHirFunction) -> bool {
    let NirType::Int {
        bits: return_bits,
        signed: return_signed,
    } = &func.return_type
    else {
        return false;
    };
    let return_bits = *return_bits;
    let return_signed = *return_signed;
    if return_bits >= 64 {
        return false;
    }

    let mut all_uses = HashMap::default();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut constrained_uses = HashMap::default();
    {
        // Borrows of the body end before `func.params` is written below.
        let mut single_defs = HashMap::default();
        collect_single_var_defs(&func.body, &mut single_defs);
        let ctx = WrappingNarrowCtx {
            single_defs: &single_defs,
            all_uses: &all_uses,
        };
        collect_wrapping_narrow_return_vars_stmts(
            &func.body,
            return_bits,
            &mut constrained_uses,
            Some(&ctx),
        );
    }

    let mut changed = false;
    for binding in &mut func.params {
        if binding.surface_type_name.is_some() {
            continue;
        }
        if !matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_))) {
            continue;
        }
        let NirType::Int { bits, .. } = binding.ty else {
            continue;
        };
        if bits <= return_bits {
            continue;
        }
        let all = all_uses.get(&binding.name).copied().unwrap_or(0);
        let constrained = constrained_uses.get(&binding.name).copied().unwrap_or(0);
        if all > 0 && all == constrained {
            binding.ty = NirType::Int {
                bits: return_bits,
                signed: return_signed,
            };
            changed = true;
        }
    }
    changed
}
