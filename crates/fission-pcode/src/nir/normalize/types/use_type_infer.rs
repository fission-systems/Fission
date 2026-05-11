/// Use-driven backward type propagation pass.
///
/// `apply_type_inference_pass` (type_infer.rs) propagates types forward from
/// *definition* sites: if `x = (int32)...` then `x.ty = Int32`.  It cannot
/// infer types from *use* sites because `expr_type(HirExpr::Var(_)) = Unknown`.
///
/// This pass performs the complementary backward-direction inference:
///
/// 1. Walk every expression and statement to collect **use-site constraints**:
///    - `Load { ptr: Var(x), ty }` → x must be a pointer to ty
///    - `Deref { ptr: Var(x), ty }` (lvalue store destination) → same
///    - `Index { base: Var(x), elem_ty }` (array lvalue) → x is `Ptr(elem_ty)`
///    - `Binary { op: SLt|SLe, lhs: Var(x), ty }` → x is a signed integer
///    - `Binary { op: Lt|Le, lhs: Var(x), ty }` → x is an unsigned integer
///    - `Return(Var(x))` + known function return type → x gets the return type
///    - `Assign rhs = Cast(T, Var(x))` → x gets type T (the cast source)
///
/// 2. Merge all collected constraints into `NirBinding.ty` for locals and
///    params that are still `Unknown`.  Constraints are only *strengthened*
///    (Unknown → Ptr → Int with signedness), never weakened.
///
/// 3. Iterate until convergence (usually 1–2 rounds via the var-chain alias
///    mechanism).
///
/// This pass is binary-independent and heuristic-free.  It is placed right
/// after `apply_type_inference_pass` so that the def-driven types it computed
/// can serve as additional seeds for backward propagation.
use super::super::*;
use std::collections::HashMap;

/// A type constraint derived from the context in which a variable is used.
#[derive(Debug, Clone, PartialEq, Eq)]
enum UseConstraint {
    /// Variable is used as a memory address (Load/Store/Deref); must be a pointer.
    Ptr(NirType),
    /// Variable is used in a signed comparison; must be a signed integer.
    Signed { bits: u32 },
    /// Variable is used in an unsigned comparison; must be an unsigned integer.
    Unsigned { bits: u32 },
    /// Variable is used in a context that requires exactly this type.
    Exact(NirType),
}

/// Accumulate use-site constraints for all named variables in `stmts`.
fn collect_constraints(
    stmts: &[HirStmt],
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, known_binding_types, out);
    }
}

fn collect_constraints_stmt(
    stmt: &HirStmt,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // Use-site on the lhs: Deref/Index require the base to be a pointer.
            collect_constraints_lvalue(lhs, out);
            collect_assignment_copy_constraints(lhs, rhs, known_binding_types, out);
            // Use-site on the rhs: look for Cast(T, Var(x)) → x: T.
            collect_constraints_cast_source(rhs, out);
            // Recurse into rhs for nested uses.
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        HirStmt::Expr(expr) => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        HirStmt::Block(body) => {
            collect_constraints(body, return_type, known_binding_types, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(then_body, return_type, known_binding_types, out);
            collect_constraints(else_body, return_type, known_binding_types, out);
        }
        HirStmt::While { cond, body } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(body, return_type, known_binding_types, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_constraints(body, return_type, known_binding_types, out);
            collect_constraints_expr(cond, return_type, known_binding_types, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_constraints_stmt(i, return_type, known_binding_types, out);
            }
            if let Some(c) = cond {
                collect_constraints_expr(c, return_type, known_binding_types, out);
            }
            if let Some(u) = update {
                collect_constraints_stmt(u, return_type, known_binding_types, out);
            }
            collect_constraints(body, return_type, known_binding_types, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
            for case in cases {
                collect_constraints(&case.body, return_type, known_binding_types, out);
            }
            collect_constraints(default, return_type, known_binding_types, out);
        }
        HirStmt::Return(Some(expr)) => {
            // If the function's return type is already known and the expression
            // is a bare variable, constrain that variable to the return type.
            if *return_type != NirType::Unknown {
                if let HirExpr::Var(name) = expr {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Exact(return_type.clone()));
                }
            }
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        _ => {}
    }
}

fn collect_assignment_copy_constraints(
    lhs: &HirLValue,
    rhs: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let HirLValue::Var(lhs_name) = lhs else {
        return;
    };

    if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
        if let HirExpr::Var(rhs_name) = rhs {
            out.entry(rhs_name.clone())
                .or_default()
                .push(copy_constraint_from_type(lhs_ty));
        }
    }
    if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
        if matches!(lhs_ty, NirType::Ptr(_)) {
            collect_pointer_assignment_base_constraints(rhs, lhs_ty, out);
        }
    }

    if let HirExpr::Var(rhs_name) = rhs {
        if let Some(rhs_ty) = known_binding_types.get(rhs_name) {
            out.entry(lhs_name.clone())
                .or_default()
                .push(copy_constraint_from_type(rhs_ty));
        }
    }
}

fn collect_pointer_assignment_base_constraints(
    rhs: &HirExpr,
    ptr_ty: &NirType,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let NirType::Ptr(pointee) = ptr_ty else {
        return;
    };
    match rhs {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        HirExpr::Cast { expr, .. } => {
            collect_pointer_assignment_base_constraints(expr, ptr_ty, out);
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if expr_is_pointer_offset_like(rhs.as_ref()) {
                if let HirExpr::Var(name) = lhs.as_ref() {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
            if expr_is_pointer_offset_like(lhs.as_ref()) {
                if let HirExpr::Var(name) = rhs.as_ref() {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            ..
        } => {
            if let HirExpr::Var(name) = lhs.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        _ => {}
    }
}

fn expr_is_pointer_offset_like(expr: &HirExpr) -> bool {
    !matches!(expr, HirExpr::Var(_))
}

fn copy_constraint_from_type(ty: &NirType) -> UseConstraint {
    match ty {
        NirType::Ptr(pointee) => UseConstraint::Ptr(pointee.as_ref().clone()),
        _ => UseConstraint::Exact(ty.clone()),
    }
}

/// Collect pointer constraints from lvalue use sites.
fn collect_constraints_lvalue(lhs: &HirLValue, out: &mut HashMap<String, Vec<UseConstraint>>) {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            // Storing through *ptr → ptr must be Ptr(ty).
            if let HirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
        HirLValue::Index { base, elem_ty, .. } => {
            // base[idx] → base is Ptr(elem_ty).
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
        }
        HirLValue::Var(_) => {}
    }
}

/// Collect `Cast(T, Var(x))` → x: T constraints.
fn collect_constraints_cast_source(expr: &HirExpr, out: &mut HashMap<String, Vec<UseConstraint>>) {
    if let HirExpr::Cast { ty, expr: inner } = expr {
        if let HirExpr::Var(name) = inner.as_ref() {
            // The variable is being cast; constrain it to the source type of the
            // cast.  Only scalar types — do not propagate Ptr here (the cast might
            // be an explicit reinterpretation).
            match ty {
                NirType::Int { .. } | NirType::Bool => {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Exact(ty.clone()));
                }
                _ => {}
            }
        }
    }
}

/// Recurse into an expression and collect use-site constraints.
fn collect_constraints_expr(
    expr: &HirExpr,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            // Loading through *ptr → ptr is Ptr(ty).
            if let HirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
            // Recurse into the pointer expression itself.
            collect_constraints_expr(ptr, return_type, known_binding_types, out);
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            match op {
                // Signed comparison → operands are signed integers.  The
                // comparison expression itself is Bool, so operand width must
                // come from an actual operand or an existing binding type.
                HirBinaryOp::SLt | HirBinaryOp::SLe => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, true, out)
                }
                // Unsigned comparison → operands are unsigned integers.
                HirBinaryOp::Lt | HirBinaryOp::Le => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, false, out)
                }
                // Arithmetic right-shift: the left operand must be a signed integer.
                // `x >> k` where `>>` is Sar (arithmetic) means x is signed.
                HirBinaryOp::Sar => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let HirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Signed { bits });
                    }
                }
                _ => {}
            }
            collect_constraints_expr(lhs, return_type, known_binding_types, out);
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        HirExpr::Unary { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        HirExpr::Cast { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_constraints_expr(arg, return_type, known_binding_types, out);
            }
        }
        HirExpr::PtrOffset { base, .. } | HirExpr::AggregateCopy { src: base, .. } => {
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            // base[index] → base is Ptr(elem_ty).
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
            collect_constraints_expr(base, return_type, known_binding_types, out);
            collect_constraints_expr(index, return_type, known_binding_types, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints_expr(then_expr, return_type, known_binding_types, out);
            collect_constraints_expr(else_expr, return_type, known_binding_types, out);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn collect_compare_constraints(
    lhs: &HirExpr,
    rhs: &HirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    signed: bool,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let lhs_bits = expr_int_bits(lhs, known_binding_types)
        .or_else(|| expr_int_bits(rhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));
    let rhs_bits = expr_int_bits(rhs, known_binding_types)
        .or_else(|| expr_int_bits(lhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));

    if let (HirExpr::Var(name), Some(bits)) = (lhs, lhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
    if let (HirExpr::Var(name), Some(bits)) = (rhs, rhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
}

fn compare_constraint(bits: u32, signed: bool) -> UseConstraint {
    if signed {
        UseConstraint::Signed { bits }
    } else {
        UseConstraint::Unsigned { bits }
    }
}

fn expr_int_bits(expr: &HirExpr, known_binding_types: &HashMap<String, NirType>) -> Option<u32> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).and_then(nir_type_bits)
        }
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. }
        | HirExpr::Cast { ty, .. }
        | HirExpr::Select { ty, .. } => nir_type_bits(ty),
        HirExpr::Binary { ty, .. } => nir_type_bits(ty),
        HirExpr::PtrOffset { .. } | HirExpr::AggregateCopy { .. } => None,
    }
}

/// Extract the bit-width of an integer/bool NirType, if applicable.
fn nir_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Bool => None,
        _ => None,
    }
}

fn collect_known_binding_types(func: &HirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::new();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if binding.ty != NirType::Unknown {
            known.insert(binding.name.clone(), binding.ty.clone());
        }
    }
    known
}

fn return_expr_type(
    expr: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).cloned()
        }
        other => {
            let ty = expr_type(other);
            (ty != NirType::Unknown).then_some(ty)
        }
    }
}

fn collect_value_return_types(
    stmts: &[HirStmt],
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
    stmt: &HirStmt,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            if let Some(ty) = return_expr_type(expr, known_binding_types) {
                out.push(ty);
            }
            1
        }
        HirStmt::Return(None) => 0,
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_value_return_types(stmts, known_binding_types, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_value_return_types(then_body, known_binding_types, out)
                + collect_value_return_types(else_body, known_binding_types, out)
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut value_return_count = 0usize;
            for case in cases {
                value_return_count +=
                    collect_value_return_types(&case.body, known_binding_types, out);
            }
            value_return_count + collect_value_return_types(default, known_binding_types, out)
        }
        _ => 0,
    }
}

fn promote_return_signedness_from_returns(func: &mut HirFunction) -> bool {
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

fn count_var_uses_expr(expr: &HirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_var_uses_expr(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses_expr(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                count_var_uses_expr(arg, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        HirExpr::Select {
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

fn count_var_uses_lvalue(lhs: &HirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => count_var_uses_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
    }
}

fn count_var_uses_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_var_uses_lvalue(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => {
            count_var_uses_expr(va_list, out);
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => count_var_uses_stmts(stmts, out),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_stmts(then_body, out);
            count_var_uses_stmts(else_body, out);
        }
        HirStmt::For {
            init, cond, update, ..
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
        }
        HirStmt::Switch {
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
        HirStmt::Return(Some(expr)) => count_var_uses_expr(expr, out),
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn count_var_uses_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_var_uses_stmt(stmt, out);
    }
}

fn wrapping_narrow_op(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Add
            | HirBinaryOp::Sub
            | HirBinaryOp::Mul
            | HirBinaryOp::And
            | HirBinaryOp::Or
            | HirBinaryOp::Xor
    )
}

fn collect_wrapping_narrow_return_vars(
    expr: &HirExpr,
    context_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirExpr::Cast { ty, expr } => {
            let bits = nir_type_bits(ty).unwrap_or(context_bits).min(context_bits);
            collect_wrapping_narrow_return_vars(expr, bits, out);
        }
        HirExpr::Unary {
            op: HirUnaryOp::Neg,
            expr,
            ..
        } => collect_wrapping_narrow_return_vars(expr, context_bits, out),
        HirExpr::Binary { op, lhs, rhs, .. } if wrapping_narrow_op(*op) => {
            collect_wrapping_narrow_return_vars(lhs, context_bits, out);
            collect_wrapping_narrow_return_vars(rhs, context_bits, out);
        }
        HirExpr::Const(_, _)
        | HirExpr::Unary { .. }
        | HirExpr::Binary { .. }
        | HirExpr::Call { .. }
        | HirExpr::Load { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::Select { .. }
        | HirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmt(
    stmt: &HirStmt,
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match stmt {
        HirStmt::Return(Some(expr)) => collect_wrapping_narrow_return_vars(expr, return_bits, out),
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_wrapping_narrow_return_vars_stmts(stmts, return_bits, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_wrapping_narrow_return_vars_stmts(then_body, return_bits, out);
            collect_wrapping_narrow_return_vars_stmts(else_body, return_bits, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_wrapping_narrow_return_vars_stmts(&case.body, return_bits, out);
            }
            collect_wrapping_narrow_return_vars_stmts(default, return_bits, out);
        }
        _ => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmts(
    stmts: &[HirStmt],
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    for stmt in stmts {
        collect_wrapping_narrow_return_vars_stmt(stmt, return_bits, out);
    }
}

fn narrow_integer_params_from_wrapping_return_uses(func: &mut HirFunction) -> bool {
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

    let mut all_uses = HashMap::new();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut constrained_uses = HashMap::new();
    collect_wrapping_narrow_return_vars_stmts(&func.body, return_bits, &mut constrained_uses);

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

/// Merge a `UseConstraint` into a binding, returning `true` if the type changed.
///
/// The merge is monotone: types only move from weaker to stronger:
/// `Unknown < Int(unsigned) < Int(signed) < Ptr(Unknown) < Ptr(known)`.
/// An existing `Known` type is NEVER overwritten.
fn merge_constraint(binding: &mut NirBinding, constraint: &UseConstraint) -> bool {
    if binding.surface_type_name.is_some() {
        return false;
    }
    match (&binding.ty, constraint) {
        // Already has a non-Unknown type — don't overwrite.
        (NirType::Ptr(_), _) => false,
        (NirType::Float { .. }, _) => false,
        (NirType::Aggregate { .. }, _) => false,
        (NirType::Bool, _) => false,

        // Pointer constraint — always upgrade when current is Unknown or Int.
        (_, UseConstraint::Ptr(pointee)) => {
            let new_ty = NirType::Ptr(Box::new(pointee.clone()));
            if binding.ty != new_ty {
                binding.ty = new_ty;
                true
            } else {
                false
            }
        }

        // Exact type from Cast context — only upgrade Unknown.
        (NirType::Unknown, UseConstraint::Exact(ty)) => {
            binding.ty = ty.clone();
            true
        }
        (
            NirType::Int { .. },
            UseConstraint::Exact(NirType::Int {
                bits: new_bits,
                signed: new_signed,
            }),
        ) => {
            // Only change signedness if currently unsigned → promote to signed.
            if let NirType::Int {
                signed: cur_signed,
                bits: cur_bits,
            } = &binding.ty
            {
                if !*cur_signed && *new_signed && cur_bits == new_bits {
                    binding.ty = NirType::Int {
                        bits: *new_bits,
                        signed: true,
                    };
                    return true;
                }
            }
            false
        }
        (_, UseConstraint::Exact(_)) => false,

        // Signed/unsigned constraint — apply if Unknown or conflicting.
        (NirType::Unknown, UseConstraint::Signed { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: true,
            };
            true
        }
        (NirType::Unknown, UseConstraint::Unsigned { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: false,
            };
            true
        }
        (
            NirType::Int {
                signed: false,
                bits: cur_bits,
            },
            UseConstraint::Signed { bits: new_bits },
        ) if cur_bits == new_bits => {
            // Promote from unsigned to signed.
            binding.ty = NirType::Int {
                bits: *new_bits,
                signed: true,
            };
            true
        }
        _ => false,
    }
}

/// Apply the use-driven backward type inference pass.
///
/// Iterates to convergence (typically 1–2 rounds).  Returns `true` if any
/// binding type changed.
pub(crate) fn apply_use_driven_type_infer_pass(func: &mut HirFunction) -> bool {
    let mut any_changed = false;
    // Iterate to convergence (alias chains may require multiple rounds).
    for _ in 0..4 {
        let mut constraints: HashMap<String, Vec<UseConstraint>> = HashMap::new();
        let known_binding_types = collect_known_binding_types(func);
        collect_constraints(
            &func.body,
            &func.return_type,
            &known_binding_types,
            &mut constraints,
        );

        let mut round_changed = false;
        for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
            if let Some(constraints_for) = constraints.get(&binding.name) {
                for constraint in constraints_for {
                    round_changed |= merge_constraint(binding, constraint);
                }
            }
        }
        round_changed |= promote_return_signedness_from_returns(func);
        round_changed |= narrow_integer_params_from_wrapping_return_uses(func);
        if !round_changed {
            break;
        }
        any_changed = true;
    }
    any_changed
}

#[cfg(test)]
mod tests {
    use super::super::super::*;

    fn make_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_typed_binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>, return_type: NirType) -> HirFunction {
        HirFunction {
            name: "test".to_owned(),
            params: vec![],
            locals,
            return_type,
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    /// Load { ptr: Var("p"), ty: uint32 } → p: Ptr(uint32)
    #[test]
    fn infers_ptr_from_load() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("x".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![make_binding("p"), make_binding("x")],
            body,
            NirType::Unknown,
        );
        let changed = super::apply_use_driven_type_infer_pass(&mut func);
        assert!(changed);
        assert_eq!(
            func.locals[0].ty,
            NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false
            }))
        );
    }

    /// Deref store lhs: *p = val → p: Ptr(val_ty)
    #[test]
    fn infers_ptr_from_deref_store() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
            rhs: HirExpr::Const(
                0,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        }];
        let mut func = make_func(vec![make_binding("p")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Ptr(Box::new(NirType::Int {
                bits: 64,
                signed: false
            }))
        );
    }

    /// SLt comparison → operand is signed int
    #[test]
    fn infers_signed_from_slt() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
        }];
        let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 32,
                signed: true
            }
        );
    }

    #[test]
    fn signed_compare_promotes_unsigned_params_and_return() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(Some(HirExpr::Var("b".to_owned())))],
            else_body: vec![HirStmt::Return(Some(HirExpr::Var("a".to_owned())))],
        }];
        let mut func = HirFunction {
            name: "signed_max".to_owned(),
            params: vec![
                make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("b", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let signed_i32 = NirType::Int {
            bits: 32,
            signed: true,
        };
        assert_eq!(func.params[0].ty, signed_i32);
        assert_eq!(func.params[1].ty, signed_i32);
        assert_eq!(func.return_type, signed_i32);
    }

    #[test]
    fn wrapping_return_use_narrows_wide_integer_params() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = HirFunction {
            name: "add32".to_owned(),
            params: vec![
                make_typed_binding("param_1", u64_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("param_2", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty.clone(),
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                rhs: Box::new(HirExpr::Var("param_2".to_owned())),
                ty: u64_ty,
            }))],
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, u32_ty);
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn wrapping_return_use_does_not_narrow_param_with_unconstrained_use() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = HirFunction {
            name: "add32_with_call".to_owned(),
            params: vec![make_typed_binding(
                "param_1",
                u64_ty.clone(),
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Expr(HirExpr::Call {
                    target: "observe64".to_owned(),
                    args: vec![HirExpr::Var("param_1".to_owned())],
                    ty: NirType::Unknown,
                }),
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, u64_ty);
    }

    #[test]
    fn signed_compare_without_width_evidence_does_not_invent_type() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
        }];
        let mut func = make_func(
            vec![make_binding("a"), make_binding("b")],
            body,
            NirType::Unknown,
        );
        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.locals[0].ty, NirType::Unknown);
        assert_eq!(func.locals[1].ty, NirType::Unknown);
    }

    #[test]
    fn signed_compare_uses_constant_width_evidence() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 16,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
        }];
        let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 16,
                signed: true
            }
        );
    }

    /// Return(Var("r")) + known return_type → r gets return_type
    #[test]
    fn infers_type_from_return_context() {
        let body = vec![HirStmt::Return(Some(HirExpr::Var("r".to_owned())))];
        let ret_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = make_func(vec![make_binding("r")], body, ret_ty.clone());
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(func.locals[0].ty, ret_ty);
    }

    #[test]
    fn propagates_pointer_use_back_through_copy_edge() {
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_owned()),
                rhs: HirExpr::Var("param_1".to_owned()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: HirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = HirFunction {
            name: "copy_ptr".to_owned(),
            params: vec![make_typed_binding(
                "param_1",
                NirType::Unknown,
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![make_binding("p")],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let expected = NirType::Ptr(Box::new(uint_ty));
        assert_eq!(func.locals[0].ty, expected);
        assert_eq!(func.params[0].ty, func.locals[0].ty);
    }

    #[test]
    fn propagates_pointer_use_back_through_scaled_pointer_assignment() {
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_owned()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: Box::new(HirExpr::Var("idx".to_owned())),
                        rhs: Box::new(HirExpr::Const(4, u64_ty.clone())),
                        ty: u64_ty.clone(),
                    }),
                    ty: u64_ty,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: HirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = HirFunction {
            name: "scaled_ptr".to_owned(),
            params: vec![make_typed_binding(
                "param_1",
                NirType::Unknown,
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![make_binding("p"), make_binding("idx")],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let expected = NirType::Ptr(Box::new(uint_ty));
        assert_eq!(func.locals[0].ty, expected);
        assert_eq!(func.params[0].ty, func.locals[0].ty);
        assert_eq!(func.locals[1].ty, NirType::Unknown);
    }
}
