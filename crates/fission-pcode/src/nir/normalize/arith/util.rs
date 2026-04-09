//! Shared helpers for integer width, masks, and boolean sentinels.

use super::super::*;

pub(crate) fn int_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

pub(crate) fn full_mask_for_bits(bits: u32) -> Option<i64> {
    match bits {
        0 => None,
        1..=62 => Some((1_i64 << bits) - 1),
        63 => Some(i64::MAX),
        _ => None,
    }
}

pub(crate) fn wrap_negated_const(value: i64, bits: u32) -> Option<i64> {
    if bits == 0 || bits > 64 {
        return None;
    }
    let mask = if bits == 64 {
        u128::from(u64::MAX)
    } else {
        (1_u128 << bits) - 1
    };
    let unsigned = (value as i128 as u128) & mask;
    let negated = unsigned.wrapping_neg() & mask;
    Some(negated as i64)
}

pub(crate) fn full_mask_for_type(ty: &NirType) -> Option<i64> {
    int_type_bits(ty).and_then(full_mask_for_bits)
}

pub(crate) fn is_full_mask_const(expr: &HirExpr, ty: &NirType) -> bool {
    let HirExpr::Const(value, _) = expr else {
        return false;
    };
    full_mask_for_type(ty).is_some_and(|mask| mask == *value)
}

pub(crate) fn is_zero_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(0, _))
}

pub(crate) fn is_one_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(1, _))
}

pub(crate) fn is_negative_one_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(-1, _))
}

pub(crate) fn bool_false_expr() -> HirExpr {
    HirExpr::Const(0, NirType::Bool)
}

pub(crate) fn bool_true_expr() -> HirExpr {
    HirExpr::Const(1, NirType::Bool)
}

pub(crate) fn is_bool_false_expr(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(0, NirType::Bool))
}

pub(crate) fn is_bool_true_expr(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(1, NirType::Bool))
}

pub(crate) fn is_integer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

pub(crate) fn is_self_comparable_non_float_type(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_)
    )
}

pub(crate) fn source_is_scalarish(ty: &NirType) -> bool {
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. })
}
