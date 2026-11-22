use super::PointerLayout;
use super::address_terms::{try_extract_index_mul, typed_pointer_base};
use super::pointer_types::{pointee_ty, type_byte_size};
use crate::HashMap;
use crate::prelude::*;

fn pointer_const_expr(
    value: i64,
    sample: &PreHirExpr,
    pointer_layout: PointerLayout,
) -> PreHirExpr {
    match sample {
        PreHirExpr::Const(_, ty) => PreHirExpr::Const(value, ty.clone()),
        _ => PreHirExpr::Const(
            value,
            NirType::Int {
                bits: pointer_layout.bits,
                signed: value < 0,
            },
        ),
    }
}

fn pointer_sized_uint_ty(pointer_layout: PointerLayout) -> NirType {
    NirType::Int {
        bits: pointer_layout.bits,
        signed: false,
    }
}

fn cast_pointer_operand_to_uint(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    typed_pointer_base(expr, binding_types)?;
    Some(PreHirExpr::Cast {
        ty: pointer_sized_uint_ty(pointer_layout),
        expr: Box::new(expr.clone()),
    })
}

pub(super) fn cast_pointer_operands_for_integer_arith(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let PreHirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };
    if !matches!(ty, NirType::Int { .. })
        || !matches!(op, PreHirBinaryOp::Add | PreHirBinaryOp::Sub)
    {
        return None;
    }
    let new_lhs = cast_pointer_operand_to_uint(lhs, binding_types, pointer_layout);
    let new_rhs = cast_pointer_operand_to_uint(rhs, binding_types, pointer_layout);
    if new_lhs.is_none() && new_rhs.is_none() {
        return None;
    }
    Some(PreHirExpr::Binary {
        op: *op,
        lhs: Box::new(new_lhs.unwrap_or_else(|| lhs.as_ref().clone())),
        rhs: Box::new(new_rhs.unwrap_or_else(|| rhs.as_ref().clone())),
        ty: ty.clone(),
    })
}

fn recover_const_offset_as_typed_pointer_add(
    ptr_expr: &PreHirExpr,
    ptr_ty: &NirType,
    elem_ty: &NirType,
    offset: i64,
    rhs_expr: &PreHirExpr,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    if matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. }) {
        return None;
    }
    let elem_size = type_byte_size(elem_ty, pointer_layout)?;
    if elem_size == 0 {
        return None;
    }
    let elem_size = i64::try_from(elem_size).ok()?;
    if offset % elem_size != 0 {
        return None;
    }
    let elem_offset = offset / elem_size;
    if elem_offset == 0 {
        return Some(ptr_expr.clone());
    }
    let op = if elem_offset > 0 {
        PreHirBinaryOp::Add
    } else {
        PreHirBinaryOp::Sub
    };
    Some(PreHirExpr::Binary {
        op,
        lhs: Box::new(ptr_expr.clone()),
        rhs: Box::new(pointer_const_expr(
            elem_offset.abs(),
            rhs_expr,
            pointer_layout,
        )),
        ty: ptr_ty.clone(),
    })
}

/// Fold `PtrOffset(PtrOffset(base, a), b)` → `PtrOffset(base, a+b)`.
/// Also folds `PtrOffset(base, 0)` → `base`.
/// Returns `true` if any rewrite occurred.
pub(super) fn fold_ptr_offset_chains(expr: &mut PreHirExpr) -> bool {
    let mut changed = false;
    // Recurse children first.
    match expr {
        PreHirExpr::PtrOffset { base, .. } => {
            changed |= fold_ptr_offset_chains(base);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= fold_ptr_offset_chains(lhs);
            changed |= fold_ptr_offset_chains(rhs);
        }
        PreHirExpr::Cast { expr: inner, .. } | PreHirExpr::Unary { expr: inner, .. } => {
            changed |= fold_ptr_offset_chains(inner);
        }
        PreHirExpr::Load { ptr, .. } => {
            changed |= fold_ptr_offset_chains(ptr);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= fold_ptr_offset_chains(base);
            changed |= fold_ptr_offset_chains(index);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= fold_ptr_offset_chains(a);
            }
        }
        _ => {}
    }
    // Now try to fold this node.
    if let PreHirExpr::PtrOffset { base, offset } = expr {
        // PtrOffset(base, 0) → base
        if *offset == 0 {
            let inner = *base.clone();
            *expr = inner;
            return true;
        }
        // PtrOffset(PtrOffset(inner, a), b) → PtrOffset(inner, a+b)
        if let PreHirExpr::PtrOffset {
            base: inner,
            offset: inner_offset,
        } = base.as_mut()
        {
            let combined = inner_offset.saturating_add(*offset);
            let new_inner = *inner.clone();
            *expr = PreHirExpr::PtrOffset {
                base: Box::new(new_inner),
                offset: combined,
            };
            changed = true;
        }
    }
    changed
}

/// Structural equality helper used to avoid pointless rewrites.
trait StructuralEq {
    fn structural_eq(&self, other: &Self) -> bool;
}

impl StructuralEq for PreHirExpr {
    fn structural_eq(&self, other: &Self) -> bool {
        // Simple pointer-identity check is fine here; this only avoids a no-op path.
        std::ptr::eq(self as *const _, other as *const _)
    }
}

/// Attempt to convert a pointer-arithmetic expression to a PtrOffset or Index
/// node.  Returns `Some(new_expr)` on success; `None` means leave unchanged.
pub(super) fn try_recover_ptr_arith(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let PreHirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };

    let (ptr_expr, rhs_expr, neg) = match op {
        PreHirBinaryOp::Add if typed_pointer_base(lhs, binding_types).is_some() => {
            (lhs.as_ref(), rhs.as_ref(), false)
        }
        PreHirBinaryOp::Add if typed_pointer_base(rhs, binding_types).is_some() => {
            (rhs.as_ref(), lhs.as_ref(), false)
        }
        PreHirBinaryOp::Sub => (lhs.as_ref(), rhs.as_ref(), true),
        _ => return None,
    };

    let (typed_ptr_expr, ptr_ty, from_byte_cast) = typed_pointer_base(ptr_expr, binding_types)?;
    let mut elem_ty = pointee_ty(&ptr_ty).cloned().unwrap_or(NirType::Unknown);

    // Dynamic variable index scale recovery: extract dynamic stride if present
    let mut stride_opt = None;
    if !neg {
        if let Some((_, stride)) = try_extract_index_mul(rhs_expr) {
            stride_opt = Some(stride);
        }
    }

    // On-the-fly pointee type refinement using stride
    if matches!(elem_ty, NirType::Unknown | NirType::Int { bits: 8, .. }) {
        if let Some(stride) = stride_opt {
            if stride > 0 {
                elem_ty = match stride {
                    2 => NirType::Int {
                        bits: 16,
                        signed: false,
                    },
                    4 => NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    8 => NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    _ => NirType::Int {
                        bits: (stride * 8) as u32,
                        signed: false,
                    },
                };
            }
        }
    }

    // Pointer-typed non-const arithmetic (e.g. `p + i`) is handled elsewhere;
    // pointer-typed const offsets enter Pattern 2 only when the pointee is
    // Unknown/Aggregate (byte-oriented) or the base was an explicit byte cast.
    let pointer_typed_const_byte_offset = matches!(ty, NirType::Ptr(_))
        && !from_byte_cast
        && matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. })
        && matches!(rhs_expr, PreHirExpr::Const(_, _));
    if matches!(ty, NirType::Ptr(_)) && !from_byte_cast && !pointer_typed_const_byte_offset {
        return None;
    }

    // Pattern 1: Add(ptr, Mul(idx, Const(stride))) → pointer add when stride matches.
    if !neg {
        if let Some((idx_expr, stride)) = try_extract_index_mul(rhs_expr) {
            let stride_matches = match type_byte_size(&elem_ty, pointer_layout) {
                Some(sz) => stride as u64 == sz,
                None => stride == 1, // unknown elem_ty → only allow stride-1
            };
            if stride_matches && stride > 0 {
                let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: refined_ptr_ty,
                });
            }
            // stride == 1 with byte pointer is also a valid index
            if stride == 1 && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
                let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: refined_ptr_ty,
                });
            }
        }
    }

    // Pattern 2: Add(ptr, Const(k)) / Sub(ptr, Const(k)) → PtrOffset.
    //
    // `PreHirExpr::PtrOffset.offset` is always in **bytes** (printer emits
    // `(uint8_t *)(base) + off`). The RHS const on a typed pointer add may be
    // either already in bytes (common after raw INT_ADD) or an **element
    // count** (C `int *p; p + 1`). `recover_const_offset_as_typed_pointer_add`
    // handles the divisible-byte case. When the const does not divide the
    // pointee size, treat it as an element index unless the base came from an
    // explicit byte cast (`(uint8_t *)p + k` must stay byte-scaled).
    //
    // Without the element-count path, `*(pairs_i + 1)` for `int *` pair fields
    // became `PtrOffset(..., 1)` → `(uint8_t *)(...) + 1` (+1 byte) instead of
    // +sizeof(int), breaking Pair/KV value loads (`accumulate_pairs`-class).
    if let PreHirExpr::Const(k, _) = rhs_expr {
        let offset = if neg { -k } else { *k };
        if matches!(ty, NirType::Ptr(_)) || from_byte_cast {
            if let Some(recovered) = recover_const_offset_as_typed_pointer_add(
                &typed_ptr_expr,
                &ptr_ty,
                &elem_ty,
                offset,
                rhs_expr,
                pointer_layout,
            ) {
                return Some(recovered);
            }
        }
        if from_byte_cast && offset == 0 {
            return Some(typed_ptr_expr.clone());
        }
        // Byte-cast form `(uint8_t *)p + k` (or Cast-wrapped base + k): k is
        // in bytes. PtrOffset is also in bytes, so emit exact unscaled byte offset.
        if from_byte_cast {
            if offset != 0 {
                return Some(PreHirExpr::PtrOffset {
                    base: Box::new(typed_ptr_expr.clone()),
                    offset,
                });
            }
            return Some(typed_ptr_expr.clone());
        }
        // Pointer-typed adds:
        // - Unknown/Aggregate pointee: const is a byte offset → PtrOffset
        // - Concrete multi-byte pointee (int*, etc.): const is already a C
        //   element count; leave unchanged (avoids double-scale).
        if matches!(ty, NirType::Ptr(_)) {
            if matches!(
                elem_ty,
                NirType::Unknown | NirType::Aggregate { .. } | NirType::Int { bits: 8, .. }
            ) {
                if offset != 0 {
                    return Some(PreHirExpr::PtrOffset {
                        base: Box::new(typed_ptr_expr.clone()),
                        offset,
                    });
                }
                return Some(typed_ptr_expr.clone());
            }
            return None;
        }
        // Scalar Int-typed address arithmetic: emit byte PtrOffset.
        if offset != 0 {
            return Some(PreHirExpr::PtrOffset {
                base: Box::new(typed_ptr_expr.clone()),
                offset,
            });
        }
        return Some(typed_ptr_expr.clone());
    }

    // Pattern 3: Add(ptr, non-const-index) → pointer add with stride 1 for byte pointers.
    if !neg && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
        let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        return Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(typed_ptr_expr.clone()),
            rhs: Box::new(rhs_expr.clone()),
            ty: refined_ptr_ty,
        });
    }

    None
}
