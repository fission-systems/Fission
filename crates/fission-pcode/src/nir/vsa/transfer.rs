/// Transfer functions for Value Set Analysis over HIR expressions.
///
/// Each `HirExpr` operation is mapped to an abstract transformer over
/// `CircleRange` values.  The functions are monotone and conservative:
/// when an exact result cannot be computed they return `top`.
use super::circle_range::CircleRange;
use crate::nir::{HirBinaryOp, HirExpr, HirUnaryOp, NirType};
use std::collections::HashMap;

/// Map from variable name → abstract range at the current program point.
pub(crate) type RangeEnv = HashMap<String, CircleRange>;

/// Evaluate the abstract range of a `HirExpr` given the current environment.
pub(crate) fn eval_expr(expr: &HirExpr, env: &RangeEnv) -> CircleRange {
    match expr {
        HirExpr::Const(value, ty) => {
            let bits = nir_bits(ty).unwrap_or(64);
            CircleRange::singleton(*value as u64, bits)
        }
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => env
            .get(name.as_str())
            .copied()
            .unwrap_or_else(|| CircleRange::top(64)),
        HirExpr::Cast { expr: inner, ty } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let src = eval_expr(inner, env);
            src.cast_unsigned(bits)
        }
        HirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let src = eval_expr(inner, env);
            eval_unary(*op, src, bits)
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let a = eval_expr(lhs, env);
            let b = eval_expr(rhs, env);
            eval_binary(*op, a, b, bits)
        }
        // For complex expressions (loads, calls, etc.) we conservatively
        // return top.
        _ => CircleRange::top(64),
    }
}

fn eval_unary(op: HirUnaryOp, src: CircleRange, bits: u32) -> CircleRange {
    use HirUnaryOp::*;
    match op {
        // Boolean negation: !0 = 1, !non_zero = 0.
        Not => {
            if let Some(v) = src.singleton_value() {
                CircleRange::singleton(if v == 0 { 1 } else { 0 }, bits)
            } else {
                CircleRange::top(bits)
            }
        }
        // Bitwise NOT.
        BitNot => {
            if let Some(v) = src.singleton_value() {
                let mask = if src.bits() >= 64 {
                    u64::MAX
                } else {
                    (1u64 << src.bits()) - 1
                };
                CircleRange::singleton(!v & mask, bits)
            } else {
                CircleRange::top(bits)
            }
        }
        // Integer negation (two's complement).
        Neg => {
            if let Some(v) = src.singleton_value() {
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                CircleRange::singleton(v.wrapping_neg() & mask, bits)
            } else {
                CircleRange::top(bits)
            }
        }
    }
}

fn eval_binary(op: HirBinaryOp, a: CircleRange, b: CircleRange, bits: u32) -> CircleRange {
    use HirBinaryOp::*;
    let a = a.cast_unsigned(bits);
    let b = b.cast_unsigned(bits);
    match op {
        Add => a.add(&b),
        Sub => a.sub(&b),
        Mul => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                CircleRange::singleton(av.wrapping_mul(bv) & mask, bits)
            } else {
                CircleRange::top(bits)
            }
        }
        Div => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                if bv == 0 {
                    return CircleRange::top(bits);
                }
                CircleRange::singleton(av.wrapping_div(bv), bits)
            } else {
                CircleRange::top(bits)
            }
        }
        Mod => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                if bv == 0 {
                    return CircleRange::top(bits);
                }
                CircleRange::singleton(av.wrapping_rem(bv), bits)
            } else {
                CircleRange::top(bits)
            }
        }
        Shl => {
            if let Some(bv) = b.singleton_value() {
                if bv >= bits as u64 {
                    return CircleRange::singleton(0, bits);
                }
                if let Some(av) = a.singleton_value() {
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    CircleRange::singleton((av << bv) & mask, bits)
                } else {
                    CircleRange::top(bits)
                }
            } else {
                CircleRange::top(bits)
            }
        }
        Shr => {
            if let Some(bv) = b.singleton_value() {
                a.shr_const(bv as u32)
            } else {
                CircleRange::top(bits)
            }
        }
        Sar => {
            if let Some(bv) = b.singleton_value() {
                if let Some(av) = a.singleton_value() {
                    let shift = (bv as u32).min(bits.saturating_sub(1));
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    let signed = sign_extend(av, bits);
                    CircleRange::singleton((signed >> shift) as u64 & mask, bits)
                } else {
                    CircleRange::top(bits)
                }
            } else {
                CircleRange::top(bits)
            }
        }
        And => {
            if let Some(bv) = b.singleton_value() {
                a.and_const(bv)
            } else if let Some(av) = a.singleton_value() {
                b.and_const(av)
            } else {
                CircleRange::top(bits)
            }
        }
        Or => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(av | bv, bits)
            } else {
                CircleRange::top(bits)
            }
        }
        Xor => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(av ^ bv, bits)
            } else {
                CircleRange::top(bits)
            }
        }
        LogicalAnd => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(if av != 0 && bv != 0 { 1 } else { 0 }, 1)
            } else {
                CircleRange::top(1)
            }
        }
        LogicalOr => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(if av != 0 || bv != 0 { 1 } else { 0 }, 1)
            } else {
                CircleRange::top(1)
            }
        }
        // Comparisons produce 0 or 1.
        Eq | Ne | Lt | Le | SLt | SLe => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(eval_cmp(op, av, bv, bits), 1)
            } else {
                CircleRange::top(1)
            }
        }
    }
}

fn eval_cmp(op: HirBinaryOp, a: u64, b: u64, bits: u32) -> u64 {
    use HirBinaryOp::*;
    let result = match op {
        Eq => a == b,
        Ne => a != b,
        Lt => a < b,
        Le => a <= b,
        SLt => sign_extend(a, bits) < sign_extend(b, bits),
        SLe => sign_extend(a, bits) <= sign_extend(b, bits),
        _ => return 0,
    };
    if result { 1 } else { 0 }
}

fn sign_extend(v: u64, bits: u32) -> i64 {
    if bits == 0 {
        return 0;
    }
    if bits >= 64 {
        return v as i64;
    }
    let shift = 64 - bits;
    ((v as i64) << shift) >> shift
}

pub(crate) fn nir_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Ptr(_) => Some(64),
        NirType::Float { bits } => Some(*bits),
        _ => None,
    }
}
