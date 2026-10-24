/// Transfer functions for Value Set Analysis over HIR expressions.
///
/// Each `PreHirExpr` operation is mapped to an abstract transformer over
/// `CircleRange` values.  The functions are monotone and conservative:
/// when an exact result cannot be computed they return `top`.
use super::circle_range::CircleRange;
use crate::ir::{NirType, PreHirBinaryOp, PreHirExpr, PreHirUnaryOp};
use std::collections::HashMap;

/// Map from variable name → abstract range at the current program point.
pub(crate) type RangeEnv = HashMap<String, CircleRange>;

/// Evaluate the abstract range of a `PreHirExpr` given the current environment.
pub fn eval_expr(expr: &PreHirExpr, env: &RangeEnv) -> CircleRange {
    match expr {
        PreHirExpr::Const(value, ty) => {
            let bits = nir_bits(ty).unwrap_or(64);
            CircleRange::singleton(*value as u64, bits)
        }
        PreHirExpr::Var(name) => env
            .get(name.as_str())
            .copied()
            .unwrap_or_else(|| CircleRange::top(64)),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => CircleRange::top(64),
        PreHirExpr::Cast { expr: inner, ty } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let src = eval_expr(inner, env);
            src.cast_unsigned(bits)
        }
        PreHirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let src = eval_expr(inner, env);
            eval_unary(*op, src, bits)
        }
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let a = eval_expr(lhs, env);
            let b = eval_expr(rhs, env);
            eval_binary(*op, a, b, bits)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let cond_range = eval_expr(cond, env);
            if let Some(c) = cond_range.singleton_value() {
                if c != 0 {
                    eval_expr(then_expr, env).cast_unsigned(bits)
                } else {
                    eval_expr(else_expr, env).cast_unsigned(bits)
                }
            } else {
                let t = eval_expr(then_expr, env).cast_unsigned(bits);
                let e = eval_expr(else_expr, env).cast_unsigned(bits);
                t.join(&e)
            }
        }
        PreHirExpr::PtrOffset { base, offset } => {
            let base_range = eval_expr(base, env).cast_unsigned(64);
            let offset_range = CircleRange::singleton(*offset as u64, 64);
            base_range.add(&offset_range)
        }
        // For complex expressions (loads, calls, etc.) we conservatively
        // return top.
        _ => CircleRange::top(64),
    }
}

fn eval_unary(op: PreHirUnaryOp, src: CircleRange, bits: u32) -> CircleRange {
    use PreHirUnaryOp::*;
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

fn eval_binary(op: PreHirBinaryOp, a: CircleRange, b: CircleRange, bits: u32) -> CircleRange {
    use PreHirBinaryOp::*;
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
            } else if let Some(bv) = b.singleton_value() {
                if bv == 0 {
                    CircleRange::top(bits)
                } else if !a.is_top() && !a.is_bottom() && a.lo() < a.hi() {
                    let lo = a.lo() / bv;
                    let hi = (a.hi().wrapping_sub(1) / bv).wrapping_add(1);
                    CircleRange::interval(lo, hi, bits)
                } else {
                    CircleRange::top(bits)
                }
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
            } else if let Some(bv) = b.singleton_value() {
                if bv == 0 || a.is_bottom() {
                    CircleRange::top(bits)
                } else if !a.is_top() && a.lo() < a.hi() && a.hi().wrapping_sub(1) < bv {
                    a
                } else {
                    CircleRange::interval(0, bv, bits)
                }
            } else {
                CircleRange::top(bits)
            }
        }
        Shl => {
            if let Some(bv) = b.singleton_value() {
                if bv >= bits as u64 {
                    return CircleRange::singleton(0, bits);
                }
                if bv == 0 {
                    return a;
                }
                if let Some(av) = a.singleton_value() {
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    CircleRange::singleton((av << bv) & mask, bits)
                } else if !a.is_top() && !a.is_bottom() && a.lo() < a.hi() {
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    let max_val = a.hi().wrapping_sub(1);
                    if bv < 64 && max_val <= (mask >> bv) {
                        let new_lo = a.lo() << bv;
                        let new_hi = (max_val << bv).wrapping_add(1) & mask;
                        CircleRange::interval(new_lo, new_hi, bits)
                    } else {
                        CircleRange::top(bits)
                    }
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
        Eq | Ne | Lt | Le | Gt | Ge | SLt | SLe | SGt | SGe => eval_relational(op, &a, &b, bits),
    }
}

fn eval_relational(op: PreHirBinaryOp, a: &CircleRange, b: &CircleRange, bits: u32) -> CircleRange {
    use PreHirBinaryOp::*;
    if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
        return CircleRange::singleton(eval_cmp(op, av, bv, bits), 1);
    }
    if a.is_bottom() || b.is_bottom() {
        return CircleRange::bottom(1);
    }

    let is_disjoint = a.meet(b).is_bottom();
    match op {
        Eq => {
            if is_disjoint {
                return CircleRange::singleton(0, 1);
            }
        }
        Ne => {
            if is_disjoint {
                return CircleRange::singleton(1, 1);
            }
        }
        Lt | Le | Gt | Ge => {
            let a_non_wrapping = !a.is_top() && a.lo() < a.hi();
            let b_non_wrapping = !b.is_top() && b.lo() < b.hi();
            if a_non_wrapping && b_non_wrapping {
                if a.hi() <= b.lo() {
                    return match op {
                        Lt | Le => CircleRange::singleton(1, 1),
                        Gt | Ge => CircleRange::singleton(0, 1),
                        _ => unreachable!(),
                    };
                } else if b.hi() <= a.lo() {
                    return match op {
                        Gt | Ge => CircleRange::singleton(1, 1),
                        Lt | Le => CircleRange::singleton(0, 1),
                        _ => unreachable!(),
                    };
                }
            }
        }
        SLt | SLe | SGt | SGe => {
            let a_non_wrapping = !a.is_top() && a.lo() < a.hi();
            let b_non_wrapping = !b.is_top() && b.lo() < b.hi();
            if a_non_wrapping && b_non_wrapping {
                let a_smin = sign_extend(a.lo(), bits);
                let a_smax = sign_extend(a.hi().wrapping_sub(1), bits);
                let b_smin = sign_extend(b.lo(), bits);
                let b_smax = sign_extend(b.hi().wrapping_sub(1), bits);

                let a_signed_ok = a_smin <= a_smax
                    && (a.count() as i128 == (a_smax as i128 - a_smin as i128 + 1));
                let b_signed_ok = b_smin <= b_smax
                    && (b.count() as i128 == (b_smax as i128 - b_smin as i128 + 1));

                if a_signed_ok && b_signed_ok {
                    if a_smax < b_smin {
                        return match op {
                            SLt | SLe => CircleRange::singleton(1, 1),
                            SGt | SGe => CircleRange::singleton(0, 1),
                            _ => unreachable!(),
                        };
                    } else if b_smax < a_smin {
                        return match op {
                            SGt | SGe => CircleRange::singleton(1, 1),
                            SLt | SLe => CircleRange::singleton(0, 1),
                            _ => unreachable!(),
                        };
                    }
                }
            }
        }
        _ => {}
    }

    CircleRange::top(1)
}

fn eval_cmp(op: PreHirBinaryOp, a: u64, b: u64, bits: u32) -> u64 {
    use PreHirBinaryOp::*;
    let result = match op {
        Eq => a == b,
        Ne => a != b,
        Lt => a < b,
        Le => a <= b,
        Gt => a > b,
        Ge => a >= b,
        SLt => sign_extend(a, bits) < sign_extend(b, bits),
        SLe => sign_extend(a, bits) <= sign_extend(b, bits),
        SGt => sign_extend(a, bits) > sign_extend(b, bits),
        SGe => sign_extend(a, bits) >= sign_extend(b, bits),
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

pub fn nir_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Ptr(_) => Some(64),
        NirType::Float { bits } => Some(*bits),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_of_does_not_return_var_value() {
        let mut env = RangeEnv::new();
        env.insert("x".into(), CircleRange::singleton(0, 32));

        let addr_local = PreHirExpr::AddressOfLocal("x".into());
        let addr_global = PreHirExpr::AddressOfGlobal("x".into());
        let var_x = PreHirExpr::Var("x".into());

        assert_eq!(eval_expr(&var_x, &env), CircleRange::singleton(0, 32));
        assert_eq!(eval_expr(&addr_local, &env), CircleRange::top(64));
        assert_eq!(eval_expr(&addr_global, &env), CircleRange::top(64));
    }

    #[test]
    fn select_and_ptr_offset_evaluated() {
        let mut env = RangeEnv::new();
        env.insert("p".into(), CircleRange::singleton(0x1000, 64));

        let ptr_offset = PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("p".into())),
            offset: 16,
        };
        assert_eq!(
            eval_expr(&ptr_offset, &env),
            CircleRange::singleton(0x1010, 64)
        );

        let select_true = PreHirExpr::Select {
            cond: Box::new(PreHirExpr::Const(1, NirType::Bool)),
            then_expr: Box::new(PreHirExpr::Const(
                42,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            else_expr: Box::new(PreHirExpr::Const(
                100,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        assert_eq!(
            eval_expr(&select_true, &env),
            CircleRange::singleton(42, 32)
        );

        let select_false = PreHirExpr::Select {
            cond: Box::new(PreHirExpr::Const(0, NirType::Bool)),
            then_expr: Box::new(PreHirExpr::Const(
                42,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            else_expr: Box::new(PreHirExpr::Const(
                100,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        assert_eq!(
            eval_expr(&select_false, &env),
            CircleRange::singleton(100, 32)
        );
    }

    #[test]
    fn mod_shl_div_interval_bounds() {
        let top32 = CircleRange::top(32);
        let rem4 = eval_binary(
            PreHirBinaryOp::Mod,
            top32,
            CircleRange::singleton(4, 32),
            32,
        );
        assert_eq!(rem4, CircleRange::interval(0, 4, 32));

        let small = CircleRange::interval(0, 3, 32);
        let rem_small = eval_binary(
            PreHirBinaryOp::Mod,
            small,
            CircleRange::singleton(4, 32),
            32,
        );
        assert_eq!(rem_small, small);

        let div_range = CircleRange::interval(10, 21, 32);
        let div5 = eval_binary(
            PreHirBinaryOp::Div,
            div_range,
            CircleRange::singleton(5, 32),
            32,
        );
        assert_eq!(div5, CircleRange::interval(2, 5, 32));

        let shl_range = CircleRange::interval(0, 4, 32);
        let shl2 = eval_binary(
            PreHirBinaryOp::Shl,
            shl_range,
            CircleRange::singleton(2, 32),
            32,
        );
        assert_eq!(shl2, CircleRange::interval(0, 13, 32));
    }

    #[test]
    fn relational_disjoint_intervals() {
        let r1 = CircleRange::interval(0, 5, 32);
        let r2 = CircleRange::interval(10, 20, 32);

        assert_eq!(
            eval_binary(PreHirBinaryOp::Lt, r1, r2, 32),
            CircleRange::singleton(1, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Le, r1, r2, 32),
            CircleRange::singleton(1, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Gt, r1, r2, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Ge, r1, r2, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Eq, r1, r2, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Ne, r1, r2, 32),
            CircleRange::singleton(1, 1)
        );

        assert_eq!(
            eval_binary(PreHirBinaryOp::Gt, r2, r1, 32),
            CircleRange::singleton(1, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Lt, r2, r1, 32),
            CircleRange::singleton(0, 1)
        );
    }
}
