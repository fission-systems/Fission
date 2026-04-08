/// Pointer arithmetic HIR recovery pass.
///
/// After use-driven type propagation (`use_type_infer.rs`) has marked pointer
/// variables, many remaining `IntAdd`/`IntSub` expressions of the form
///
/// ```text
/// IntAdd(Var(ptr), Const(k))
/// IntAdd(Var(ptr), Mul(idx, Const(stride)))
/// ```
///
/// still have purely-integer semantics in the HIR even though `ptr` is now
/// known to be a pointer.  This pass converts them to the higher-level
/// `PtrOffset` and `Index` HIR nodes, which the printer renders as:
///
/// ```c
/// ptr + k          (PtrOffset with stride 1)
/// ptr[idx]         (Index)
/// ```
///
/// rather than the verbose `(uint8_t *)(ptr) + k` produced by the generic
/// integer-arithmetic printer.
///
/// ### Algorithm
///
/// For every expression tree:
///
/// 1. `Add(Var(x), Mul(idx, Const(stride)))` where `x.ty == Ptr(T)` and
///    `stride == sizeof(T)` (or T is Unknown/byte) → `Index { base, index, elem_ty: T }`
///
/// 2. `Add(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: k }`
///
/// 3. `Cast { ty: Ptr(Int8), expr: Add(Var(x), ...) }` where `x` is already
///    known to be a pointer → strip the `Ptr(Int8)` cast (it was added as a
///    `uint8_t *` cast by the builder; now that the base is typed we don't need it).
///
/// 4. `Sub(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: -k }`
///
/// All transformations are conservative: we only act when the pointer type is
/// concretely known (`Ptr(_)`), never for `Unknown`.
///
/// Reference: Ghidra `RulePtrArith` (ruleaction.cc:6496),
///            RetDec `DerefToArrayIndexOptimizer`.
use super::*;
use std::collections::HashMap;

/// Build a map from variable name → NirType for all locals and params.
fn build_binding_type_map(func: &HirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| b.ty != NirType::Unknown)
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect()
}

/// Return the pointee type if `ty` is `Ptr(pointee)`.
fn pointee_ty(ty: &NirType) -> Option<&NirType> {
    match ty {
        NirType::Ptr(inner) => Some(inner),
        _ => None,
    }
}

/// Return the byte size of a type, if known.
fn type_byte_size(ty: &NirType) -> Option<u64> {
    match ty {
        NirType::Int { bits, .. } | NirType::Float { bits } => Some(u64::from(*bits / 8)),
        NirType::Bool => Some(1),
        NirType::Ptr(_) => Some(8), // assume 64-bit
        _ => None,
    }
}

/// Try to recognise `Mul(idx, Const(stride))` or `Mul(Const(stride), idx)`,
/// returning `(idx_expr, stride)`.
fn try_extract_index_mul(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let HirExpr::Binary { op: HirBinaryOp::Mul, lhs, rhs, .. } = expr else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (_, HirExpr::Const(stride, _)) => Some((*lhs.clone(), *stride)),
        (HirExpr::Const(stride, _), _) => Some((*rhs.clone(), *stride)),
        _ => None,
    }
}

/// Attempt to convert a pointer-arithmetic expression to a PtrOffset or Index
/// node.  Returns `Some(new_expr)` on success; `None` means leave unchanged.
fn try_recover_ptr_arith(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary { op, lhs, rhs, .. } = expr else {
        return None;
    };

    let (ptr_expr, rhs_expr, neg) = match op {
        HirBinaryOp::Add => (lhs.as_ref(), rhs.as_ref(), false),
        HirBinaryOp::Sub => (lhs.as_ref(), rhs.as_ref(), true),
        _ => return None,
    };

    // Determine if the LHS is a pointer-typed variable.
    let ptr_ty = match ptr_expr {
        HirExpr::Var(name) => binding_types.get(name.as_str()).and_then(|t| {
            if matches!(t, NirType::Ptr(_)) { Some(t) } else { None }
        })?,
        _ => return None,
    };
    let elem_ty = pointee_ty(ptr_ty).cloned().unwrap_or(NirType::Unknown);

    // Pattern 1: Add(ptr, Mul(idx, Const(stride))) → Index when stride matches.
    if !neg {
        if let Some((idx_expr, stride)) = try_extract_index_mul(rhs_expr) {
            let stride_matches = match type_byte_size(&elem_ty) {
                Some(sz) => stride as u64 == sz,
                None => stride == 1, // unknown elem_ty → only allow stride-1
            };
            if stride_matches && stride > 0 {
                return Some(HirExpr::Index {
                    base: Box::new(ptr_expr.clone()),
                    index: Box::new(idx_expr),
                    elem_ty,
                });
            }
            // stride == 1 with byte pointer is also a valid index
            if stride == 1 && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
                return Some(HirExpr::Index {
                    base: Box::new(ptr_expr.clone()),
                    index: Box::new(idx_expr),
                    elem_ty,
                });
            }
        }
    }

    // Pattern 2: Add(ptr, Const(k)) / Sub(ptr, Const(k)) → PtrOffset.
    if let HirExpr::Const(k, _) = rhs_expr {
        let offset = if neg { -k } else { *k };
        // Only emit PtrOffset when offset != 0 (offset 0 is a no-op).
        if offset != 0 {
            return Some(HirExpr::PtrOffset {
                base: Box::new(ptr_expr.clone()),
                offset,
            });
        }
        // offset == 0: the expression equals ptr — return bare Var.
        if offset == 0 {
            return Some(ptr_expr.clone());
        }
    }

    // Pattern 3: Add(ptr, non-const-index) → Index with stride 1 for byte pointers.
    if !neg && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
        return Some(HirExpr::Index {
            base: Box::new(ptr_expr.clone()),
            index: Box::new(rhs_expr.clone()),
            elem_ty,
        });
    }

    None
}

/// Recursively rewrite all pointer-arithmetic sub-expressions in `expr`.
fn recover_in_expr(expr: &mut HirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    // Try the top-level pattern first.
    if let Some(new_expr) = try_recover_ptr_arith(expr, binding_types) {
        *expr = new_expr;
        return true;
    }
    // Recurse into children.
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= recover_in_expr(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirExpr::Unary { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
        }
        HirExpr::Cast { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
            // After recursing: if we now have Cast(Ptr(Int8), PtrOffset { base, .. })
            // and base is already pointer-typed, strip the cast.
            if let HirExpr::Cast {
                ty: NirType::Ptr(pointee),
                expr: inner2,
            } = expr
            {
                if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) {
                    if matches!(
                        inner2.as_ref(),
                        HirExpr::PtrOffset { .. } | HirExpr::Index { .. }
                    ) {
                        let new_inner = *inner2.clone();
                        *expr = new_inner;
                        changed = true;
                    }
                }
            }
        }
        HirExpr::Load { ptr, .. } => {
            changed |= recover_in_expr(ptr, binding_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= recover_in_expr(arg, binding_types);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= recover_in_expr(base, binding_types);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= recover_in_expr(base, binding_types);
            changed |= recover_in_expr(index, binding_types);
        }
        HirExpr::AggregateCopy { src, .. } => {
            changed |= recover_in_expr(src, binding_types);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn recover_in_lvalue(lhs: &mut HirLValue, binding_types: &HashMap<String, NirType>) -> bool {
    match lhs {
        HirLValue::Deref { ptr, .. } => recover_in_expr(ptr, binding_types),
        HirLValue::Index { base, index, .. } => {
            let a = recover_in_expr(base, binding_types);
            let b = recover_in_expr(index, binding_types);
            a || b
        }
        HirLValue::Var(_) => false,
    }
}

fn recover_in_stmts(stmts: &mut Vec<HirStmt>, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_in_stmt(stmt, binding_types);
    }
    changed
}

fn recover_in_stmt(stmt: &mut HirStmt, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= recover_in_lvalue(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirStmt::Expr(expr) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        HirStmt::Block(body) => {
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_stmts(then_body, binding_types);
            changed |= recover_in_stmts(else_body, binding_types);
        }
        HirStmt::While { cond, body } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::DoWhile { body, cond } => {
            changed |= recover_in_stmts(body, binding_types);
            changed |= recover_in_expr(cond, binding_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= recover_in_stmt(i, binding_types);
            }
            if let Some(c) = cond {
                changed |= recover_in_expr(c, binding_types);
            }
            if let Some(u) = update {
                changed |= recover_in_stmt(u, binding_types);
            }
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::Switch { expr, cases, default } => {
            changed |= recover_in_expr(expr, binding_types);
            for case in cases.iter_mut() {
                changed |= recover_in_stmts(&mut case.body, binding_types);
            }
            changed |= recover_in_stmts(default, binding_types);
        }
        HirStmt::Return(Some(expr)) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        _ => {}
    }
    changed
}

/// Apply the pointer arithmetic recovery pass to a function.
///
/// Returns `true` if any expression was rewritten.
pub(super) fn apply_ptr_arith_recovery_pass(func: &mut HirFunction) -> bool {
    let binding_types = build_binding_type_map(func);
    if binding_types.is_empty() {
        return false;
    }
    recover_in_stmts(&mut func.body, &binding_types)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_binding_with_ty(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_owned(),
            params: vec![],
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
        }
    }

    /// Add(Var("p"), Const(4)) where p: Ptr(uint32) → PtrOffset { base: p, offset: 4 }
    #[test]
    fn converts_add_const_to_ptr_offset() {
        let elem_ty = NirType::Int { bits: 32, signed: false };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(4, NirType::Int { bits: 64, signed: false })),
                ty: NirType::Int { bits: 64, signed: false },
            },
        }];
        let mut func = make_func(
            vec![make_binding_with_ty("p", p_ty)],
            body,
        );
        let changed = apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::PtrOffset { offset: 4, .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Add(Var("p"), Mul(Var("i"), Const(4))) where p: Ptr(uint32) → Index
    #[test]
    fn converts_add_stride_to_index() {
        let elem_ty = NirType::Int { bits: 32, signed: false };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(HirExpr::Var("i".to_owned())),
                    rhs: Box::new(HirExpr::Const(4, NirType::Int { bits: 64, signed: false })),
                    ty: NirType::Int { bits: 64, signed: false },
                }),
                ty: NirType::Int { bits: 64, signed: false },
            },
        }];
        let mut func = make_func(
            vec![make_binding_with_ty("p", p_ty)],
            body,
        );
        let changed = apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::Index { .. }));
        } else {
            panic!("expected assign");
        }
    }
}
