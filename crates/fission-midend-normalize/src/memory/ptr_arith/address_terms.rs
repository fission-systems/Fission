use crate::HashMap;
use crate::prelude::*;

/// Try to recognise `Mul(idx, Const(stride))` or `Mul(Const(stride), idx)`,
/// returning `(idx_expr, stride)`.
pub(super) fn try_extract_index_mul(expr: &PreHirExpr) -> Option<(PreHirExpr, i64)> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Mul,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (_, PreHirExpr::Const(stride, _)) => Some((*lhs.clone(), *stride)),
        (PreHirExpr::Const(stride, _), _) => Some((*rhs.clone(), *stride)),
        _ => None,
    }
}

/// Like [`try_extract_index_mul`], but a left shift by a constant also scales:
/// `Shl(idx, k)` is `idx * (1 << k)`. Returns `(idx_expr, stride, is_shift)`.
///
/// Only the load/store rewrite uses this. That rewrite names its element type
/// from the access itself (`Index { elem_ty: access_ty }`), so it does not
/// depend on the base's declared pointee. The element-unit rewrite of a bare
/// `base + idx * stride` does -- it divides the stride by the *current*
/// pointee size, and a later pass can change that pointee -- so it keeps
/// matching `Mul` only.
pub(super) fn try_extract_index_scale(expr: &PreHirExpr) -> Option<(PreHirExpr, i64, bool)> {
    if let Some((idx, stride)) = try_extract_index_mul(expr) {
        return Some((idx, stride, false));
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    match rhs.as_ref() {
        PreHirExpr::Const(shift, _) if (1..=6).contains(shift) => {
            Some((*lhs.clone(), 1i64 << *shift, true))
        }
        _ => None,
    }
}

/// Return a typed pointer base expression.  The builder may preserve byte-level
/// address arithmetic as `Cast(Ptr(Int8), p)` even after use-driven inference
/// has proven that `p` is a more specific pointer type.  Keep track of that
/// byte cast so constant byte offsets can be rescaled to element offsets.
pub(super) fn typed_pointer_base(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<(PreHirExpr, NirType, bool)> {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => binding_types.get(name.as_str()).and_then(|ty| {
            if matches!(ty, NirType::Ptr(_)) {
                Some((expr.clone(), ty.clone(), false))
            } else {
                None
            }
        }),
        PreHirExpr::Cast {
            ty: NirType::Ptr(pointee),
            expr: inner,
        } if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) => {
            let (base, ty, _) = typed_pointer_base(inner, binding_types)?;
            Some((base, ty, true))
        }
        _ => {
            let ty = expr_type(expr);
            if matches!(ty, NirType::Ptr(_)) {
                Some((expr.clone(), ty, false))
            } else {
                None
            }
        }
    }
}
