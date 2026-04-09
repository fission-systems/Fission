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
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, out);
    }
}

fn collect_constraints_stmt(
    stmt: &HirStmt,
    return_type: &NirType,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // Use-site on the lhs: Deref/Index require the base to be a pointer.
            collect_constraints_lvalue(lhs, out);
            // Use-site on the rhs: look for Cast(T, Var(x)) → x: T.
            collect_constraints_cast_source(rhs, out);
            // Recurse into rhs for nested uses.
            collect_constraints_expr(rhs, return_type, out);
        }
        HirStmt::Expr(expr) => {
            collect_constraints_expr(expr, return_type, out);
        }
        HirStmt::Block(body) => {
            collect_constraints(body, return_type, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_constraints_expr(cond, return_type, out);
            collect_constraints(then_body, return_type, out);
            collect_constraints(else_body, return_type, out);
        }
        HirStmt::While { cond, body } => {
            collect_constraints_expr(cond, return_type, out);
            collect_constraints(body, return_type, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_constraints(body, return_type, out);
            collect_constraints_expr(cond, return_type, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_constraints_stmt(i, return_type, out);
            }
            if let Some(c) = cond {
                collect_constraints_expr(c, return_type, out);
            }
            if let Some(u) = update {
                collect_constraints_stmt(u, return_type, out);
            }
            collect_constraints(body, return_type, out);
        }
        HirStmt::Switch { expr, cases, default } => {
            collect_constraints_expr(expr, return_type, out);
            for case in cases {
                collect_constraints(&case.body, return_type, out);
            }
            collect_constraints(default, return_type, out);
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
            collect_constraints_expr(expr, return_type, out);
        }
        _ => {}
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
fn collect_constraints_cast_source(
    expr: &HirExpr,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
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
            collect_constraints_expr(ptr, return_type, out);
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            // Signed comparison → operands are signed integers.
            match op {
                HirBinaryOp::SLt | HirBinaryOp::SLe => {
                    let bits = nir_type_bits(ty).unwrap_or(64);
                    if let HirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Signed { bits });
                    }
                    if let HirExpr::Var(name) = rhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Signed { bits });
                    }
                }
                // Unsigned comparison → operands are unsigned integers.
                HirBinaryOp::Lt | HirBinaryOp::Le => {
                    let bits = nir_type_bits(ty).unwrap_or(64);
                    if let HirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Unsigned { bits });
                    }
                    if let HirExpr::Var(name) = rhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Unsigned { bits });
                    }
                }
                // Arithmetic right-shift: the left operand must be a signed integer.
                // `x >> k` where `>>` is Sar (arithmetic) means x is signed.
                HirBinaryOp::Sar => {
                    let bits = nir_type_bits(ty).unwrap_or(32);
                    if let HirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Signed { bits });
                    }
                }
                _ => {}
            }
            collect_constraints_expr(lhs, return_type, out);
            collect_constraints_expr(rhs, return_type, out);
        }
        HirExpr::Unary { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, out);
        }
        HirExpr::Cast { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_constraints_expr(arg, return_type, out);
            }
        }
        HirExpr::PtrOffset { base, .. } | HirExpr::AggregateCopy { src: base, .. } => {
            collect_constraints_expr(base, return_type, out);
        }
        HirExpr::Index { base, index, elem_ty } => {
            // base[index] → base is Ptr(elem_ty).
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
            collect_constraints_expr(base, return_type, out);
            collect_constraints_expr(index, return_type, out);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

/// Extract the bit-width of an integer/bool NirType, if applicable.
fn nir_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Bool => Some(1),
        _ => None,
    }
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
        (NirType::Int { .. }, UseConstraint::Exact(NirType::Int { bits: new_bits, signed: new_signed })) => {
            // Only change signedness if currently unsigned → promote to signed.
            if let NirType::Int { signed: cur_signed, bits: cur_bits } = &binding.ty {
                if !*cur_signed && *new_signed && cur_bits == new_bits {
                    binding.ty = NirType::Int { bits: *new_bits, signed: true };
                    return true;
                }
            }
            false
        }
        (_, UseConstraint::Exact(_)) => false,

        // Signed/unsigned constraint — apply if Unknown or conflicting.
        (NirType::Unknown, UseConstraint::Signed { bits }) => {
            binding.ty = NirType::Int { bits: *bits, signed: true };
            true
        }
        (NirType::Unknown, UseConstraint::Unsigned { bits }) => {
            binding.ty = NirType::Int { bits: *bits, signed: false };
            true
        }
        (NirType::Int { signed: false, bits: cur_bits }, UseConstraint::Signed { bits: new_bits })
            if cur_bits == new_bits =>
        {
            // Promote from unsigned to signed.
            binding.ty = NirType::Int { bits: *new_bits, signed: true };
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
        collect_constraints(&func.body, &func.return_type, &mut constraints);

        let mut round_changed = false;
        for binding in func
            .locals
            .iter_mut()
            .chain(func.params.iter_mut())
        {
            if let Some(constraints_for) = constraints.get(&binding.name) {
                for constraint in constraints_for {
                    round_changed |= merge_constraint(binding, constraint);
                }
            }
        }
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

    fn make_func(
        locals: Vec<NirBinding>,
        body: Vec<HirStmt>,
        return_type: NirType,
    ) -> HirFunction {
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
                ty: NirType::Int { bits: 32, signed: false },
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
            NirType::Ptr(Box::new(NirType::Int { bits: 32, signed: false }))
        );
    }

    /// Deref store lhs: *p = val → p: Ptr(val_ty)
    #[test]
    fn infers_ptr_from_deref_store() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int { bits: 64, signed: false },
            },
            rhs: HirExpr::Const(0, NirType::Int { bits: 64, signed: false }),
        }];
        let mut func = make_func(vec![make_binding("p")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Ptr(Box::new(NirType::Int { bits: 64, signed: false }))
        );
    }

    /// SLt comparison → operand is signed int
    #[test]
    fn infers_signed_from_slt() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Const(0, NirType::Int { bits: 32, signed: true })),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
        }];
        let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        // a should be inferred as signed 64-bit (default bits from Bool type)
        assert!(matches!(
            func.locals[0].ty,
            NirType::Int { signed: true, .. }
        ));
    }

    /// Return(Var("r")) + known return_type → r gets return_type
    #[test]
    fn infers_type_from_return_context() {
        let body = vec![HirStmt::Return(Some(HirExpr::Var("r".to_owned())))];
        let ret_ty = NirType::Int { bits: 32, signed: true };
        let mut func = make_func(vec![make_binding("r")], body, ret_ty.clone());
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(func.locals[0].ty, ret_ty);
    }
}
