//! RuleExpandLoad — Fission HIR equivalent of Ghidra's `RuleExpandLoad`.
//!
//! Ghidra source: `ruleaction.hh` L1595–1606, `ruleaction.cc` L10909–10985.
//!
//! ## Algorithm
//!
//! Ghidra's `RuleExpandLoad` is triggered on a LOAD op and checks whether the
//! LOAD's output size is *smaller* than what the pointer's data-type implies.
//! When the difference is a natural prefix/truncation (little-endian LSB read)
//! it expands the LOAD size to match the pointer type, then either:
//!
//! - **Pattern A (AND-comparison form)**: `(LOAD_small & mask) == const`
//!   → expand LOAD to larger type, adjust mask/const by shifting LSBs.
//!
//! - **Pattern B (natural truncation form)**: direct use of the narrow LOAD
//!   where the pointer type is wider integer → shrink the cast to just a
//!   re-typed Load of the same pointer.
//!
//! ## Fission HIR mapping
//!
//! In Fission the equivalent pattern is:
//!
//! ```text
//! Cast<small_int>(Load<large_int>(ptr))
//! ```
//!
//! When `small_int` is simply the lower bytes of `large_int` (natural unsigned
//! truncation, little-endian), the `Cast` is redundant: we can narrow the
//! `Load` type directly to `small_int`.
//!
//! This is the *same* simplification Ghidra performs in the natural-truncation
//! branch of `applyOp` (lines 10963–10984) — replace the LOAD with a narrower
//! LOAD and connect the original smaller output through a SUBPIECE (which
//! ultimately collapses into a simple type change in HIR).

use super::super::*;

/// Apply the `RuleExpandLoad` simplification to an entire HIR function.
///
/// Returns `true` if any transformation was made.
pub(crate) fn apply_expand_load_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= expand_load_in_stmt(stmt);
    }
    changed
}

fn expand_load_in_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { rhs, .. } => changed |= expand_load_in_expr(rhs),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => changed |= expand_load_in_expr(expr),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for s in body.iter_mut() {
                changed |= expand_load_in_stmt(s);
            }
        }
        HirStmt::For {
            init,
            update,
            cond,
            body,
        } => {
            if let Some(s) = init {
                changed |= expand_load_in_stmt(s);
            }
            if let Some(s) = update {
                changed |= expand_load_in_stmt(s);
            }
            if let Some(c) = cond {
                changed |= expand_load_in_expr(c);
            }
            for s in body.iter_mut() {
                changed |= expand_load_in_stmt(s);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= expand_load_in_expr(cond);
            for s in then_body.iter_mut() {
                changed |= expand_load_in_stmt(s);
            }
            for s in else_body.iter_mut() {
                changed |= expand_load_in_stmt(s);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= expand_load_in_expr(expr);
            for case in cases.iter_mut() {
                for s in case.body.iter_mut() {
                    changed |= expand_load_in_stmt(s);
                }
            }
            for s in default.iter_mut() {
                changed |= expand_load_in_stmt(s);
            }
        }
        _ => {}
    }
    changed
}

/// Returns the byte-width of an integer type, or `None` for non-integer types.
fn int_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Bool => Some(1),
        _ => None,
    }
}

/// Returns `true` when `outer` is an unsigned-integer type that is a strict
/// prefix of `inner` (i.e. `outer.bits < inner.bits` and both are integers).
///
/// This corresponds to Ghidra's "natural truncation" guard:
/// > little-endian: offset == 0 means we grab the LSBs, which is a natural
/// > truncation of the wider load.
fn is_natural_narrowing(outer: &NirType, inner: &NirType) -> bool {
    match (outer, inner) {
        (
            NirType::Int {
                bits: outer_bits,
                signed: false,
            },
            NirType::Int {
                bits: inner_bits, ..
            },
        ) => outer_bits < inner_bits && *outer_bits >= 8 && *inner_bits >= 8,
        _ => false,
    }
}

fn expand_load_in_expr(expr: &mut HirExpr) -> bool {
    // -----------------------------------------------------------------------
    // Pattern A (AND-comparison form) — checked BEFORE recursion so that
    // Pattern B (below) does not eagerly collapse Cast<small>(Load<large>)
    // inside the And expression before Pattern A gets a chance to see it.
    //
    //   ((Cast<u{small}>(Load<u{large}>(ptr))) & mask) == cmp_val
    //   → (Load<u{large}>(ptr) & (mask widened to u{large})) == (cmp_val widened)
    // -----------------------------------------------------------------------
    if let HirExpr::Binary {
        op: HirBinaryOp::Eq | HirBinaryOp::Ne,
        lhs: cmp_lhs,
        rhs: cmp_rhs,
        ty: cmp_ty,
    } = expr
    {
        if let HirExpr::Const(cmp_val, _) = cmp_rhs.as_ref() {
            let cmp_val = *cmp_val;
            if let HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: and_lhs,
                rhs: and_rhs,
                ty: and_ty,
            } = cmp_lhs.as_mut()
            {
                if let HirExpr::Const(mask_val, _) = and_rhs.as_ref() {
                    let mask_val = *mask_val;
                    if let HirExpr::Cast {
                        ty: cast_ty,
                        expr: cast_inner,
                    } = and_lhs.as_mut()
                    {
                        if let HirExpr::Load {
                            ptr: load_ptr,
                            ty: load_ty,
                        } = cast_inner.as_mut()
                        {
                            let sb = int_bits(cast_ty);
                            let lb = int_bits(load_ty);
                            if let (Some(sb_bits), Some(lb_bits)) = (sb, lb) {
                                if sb_bits < lb_bits {
                                    let wide_ty = load_ty.clone();
                                    let wide_ptr = load_ptr.clone();
                                    *and_lhs = Box::new(HirExpr::Load {
                                        ptr: wide_ptr,
                                        ty: wide_ty.clone(),
                                    });
                                    *and_ty = wide_ty.clone();
                                    *and_rhs = Box::new(HirExpr::Const(mask_val, wide_ty.clone()));
                                    *cmp_rhs = Box::new(HirExpr::Const(cmp_val, wide_ty));
                                    *cmp_ty = NirType::Bool;
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pattern B (natural truncation):
    //   Cast<u{small}>(Load<u{large}>(ptr))  →  Load<u{small}>(ptr)
    // -----------------------------------------------------------------------
    if let HirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    {
        if let HirExpr::Load {
            ptr: load_ptr,
            ty: load_ty,
        } = inner.as_mut()
        {
            if is_natural_narrowing(cast_ty, load_ty) {
                *expr = HirExpr::Load {
                    ptr: load_ptr.clone(),
                    ty: cast_ty.clone(),
                };
                return true;
            }
        }
    }

    // Recurse into sub-expressions.
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= expand_load_in_expr(lhs);
            changed |= expand_load_in_expr(rhs);
        }
        HirExpr::Unary { expr: inner, .. }
        | HirExpr::Cast { expr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= expand_load_in_expr(inner);
        }
        HirExpr::Load { ptr, .. } => {
            changed |= expand_load_in_expr(ptr);
        }
        HirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= expand_load_in_expr(arg);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= expand_load_in_expr(base);
            changed |= expand_load_in_expr(index);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= expand_load_in_expr(cond);
            changed |= expand_load_in_expr(then_expr);
            changed |= expand_load_in_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_int(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn u8() -> NirType {
        make_int(8, false)
    }
    fn u16() -> NirType {
        make_int(16, false)
    }
    fn u32() -> NirType {
        make_int(32, false)
    }

    fn load_expr(ty: NirType) -> HirExpr {
        HirExpr::Load {
            ptr: Box::new(HirExpr::Var("ptr".to_string())),
            ty,
        }
    }

    fn cast_expr(ty: NirType, inner: HirExpr) -> HirExpr {
        HirExpr::Cast {
            ty,
            expr: Box::new(inner),
        }
    }

    fn empty_func_with_body(body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_string(),
            int_param_offsets: Vec::new(),
            body,
            ..HirFunction::default()
        }
    }

    #[test]
    fn narrow_cast_of_load_is_collapsed() {
        // Cast<u8>(Load<u32>(ptr))  →  Load<u8>(ptr)
        let stmt = HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: cast_expr(u8(), load_expr(u32())),
        };
        let mut func = empty_func_with_body(vec![stmt]);
        let changed = apply_expand_load_pass(&mut func);
        assert!(changed, "expected transformation");
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(
                matches!(
                    rhs,
                    HirExpr::Load {
                        ty: NirType::Int {
                            bits: 8,
                            signed: false
                        },
                        ..
                    }
                ),
                "expected Load<u8> after transformation, got: {rhs:?}"
            );
        } else {
            panic!("expected Assign stmt");
        }
    }

    #[test]
    fn no_transform_when_load_is_already_narrow() {
        // Cast<u8>(Load<u8>(ptr)) — no change because bits are equal
        let stmt = HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: cast_expr(u8(), load_expr(u8())),
        };
        let mut func = empty_func_with_body(vec![stmt]);
        let changed = apply_expand_load_pass(&mut func);
        assert!(!changed, "should not transform when sizes match");
    }

    #[test]
    fn no_transform_when_cast_widens() {
        // Cast<u32>(Load<u8>(ptr)) — this would widen, not narrow; no change
        let stmt = HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: cast_expr(u32(), load_expr(u8())),
        };
        let mut func = empty_func_with_body(vec![stmt]);
        let changed = apply_expand_load_pass(&mut func);
        assert!(!changed, "should not transform widening cast");
    }

    #[test]
    fn and_comparison_load_is_widened() {
        // (Cast<u8>(Load<u32>(ptr)) & 0xFF) == 5
        // → (Load<u32>(ptr) & 0xFF_u32) == 5_u32
        let and_expr = HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(cast_expr(u8(), load_expr(u32()))),
            rhs: Box::new(HirExpr::Const(0xFF, u8())),
            ty: u8(),
        };
        let cmp_expr = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(and_expr),
            rhs: Box::new(HirExpr::Const(5, u8())),
            ty: NirType::Bool,
        };
        let stmt = HirStmt::Expr(cmp_expr);
        let mut func = empty_func_with_body(vec![stmt]);
        let changed = apply_expand_load_pass(&mut func);
        assert!(changed, "expected transformation in AND-comparison form");
        if let HirStmt::Expr(HirExpr::Binary { lhs, rhs, .. }) = &func.body[0] {
            // rhs (cmp const) should now have wider type
            assert!(
                matches!(
                    rhs.as_ref(),
                    HirExpr::Const(5, NirType::Int { bits: 32, .. })
                ),
                "expected cmp_rhs widened to u32, got: {rhs:?}"
            );
            // lhs should be And(Load<u32>, Const<u32>)
            if let HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: and_lhs,
                rhs: and_rhs,
                ..
            } = lhs.as_ref()
            {
                assert!(
                    matches!(
                        and_lhs.as_ref(),
                        HirExpr::Load {
                            ty: NirType::Int { bits: 32, .. },
                            ..
                        }
                    ),
                    "expected Load<u32> as and_lhs, got: {and_lhs:?}"
                );
                assert!(
                    matches!(
                        and_rhs.as_ref(),
                        HirExpr::Const(0xFF, NirType::Int { bits: 32, .. })
                    ),
                    "expected and_rhs widened to u32, got: {and_rhs:?}"
                );
            } else {
                panic!("expected And expression as cmp_lhs");
            }
        } else {
            panic!("expected Expr stmt with Binary comparison");
        }
    }

    #[test]
    fn narrow_cast_of_u16_load_u32_collapsed() {
        // Cast<u16>(Load<u32>(ptr)) → Load<u16>(ptr)
        let stmt = HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: cast_expr(u16(), load_expr(u32())),
        };
        let mut func = empty_func_with_body(vec![stmt]);
        let changed = apply_expand_load_pass(&mut func);
        assert!(changed, "expected transformation");
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(
                matches!(
                    rhs,
                    HirExpr::Load {
                        ty: NirType::Int {
                            bits: 16,
                            signed: false
                        },
                        ..
                    }
                ),
                "expected Load<u16>, got: {rhs:?}"
            );
        }
    }
}
