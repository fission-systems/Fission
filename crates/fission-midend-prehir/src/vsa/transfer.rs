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
            let c = eval_expr(cond, env);
            if c.singleton_value() == Some(1) {
                eval_expr(then_expr, env).cast_unsigned(bits)
            } else if c.singleton_value() == Some(0) {
                eval_expr(else_expr, env).cast_unsigned(bits)
            } else {
                let t = eval_expr(then_expr, env).cast_unsigned(bits);
                let e = eval_expr(else_expr, env).cast_unsigned(bits);
                t.join(&e)
            }
        }
        PreHirExpr::PtrOffset { base, offset } => {
            let b = eval_expr(base, env).cast_unsigned(64);
            let off = CircleRange::singleton(*offset as u64, 64);
            b.add(&off)
        }
        PreHirExpr::FieldAccess {
            base, offset, ty, ..
        } => {
            let bits = nir_bits(ty).unwrap_or(64);
            let b = eval_expr(base, env).cast_unsigned(64);
            let off = CircleRange::singleton(*offset as u64, 64);
            b.add(&off).cast_unsigned(bits)
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
            if let Some(bv) = b.singleton_value() {
                if bv == 0 {
                    return CircleRange::top(bits);
                }
                if let Some(av) = a.singleton_value() {
                    CircleRange::singleton(av.wrapping_div(bv), bits)
                } else if !a.is_top() && !a.is_bottom() && a.lo() < a.hi() {
                    let lo = a.lo() / bv;
                    let hi = ((a.hi() - 1) / bv) + 1;
                    CircleRange::interval(lo, hi, bits)
                } else {
                    let mask = if bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << bits) - 1
                    };
                    CircleRange::interval(0, (mask / bv).saturating_add(1), bits)
                }
            } else {
                CircleRange::top(bits)
            }
        }
        Mod => {
            if let Some(bv) = b.singleton_value() {
                if bv == 0 {
                    return CircleRange::top(bits);
                }
                if let Some(av) = a.singleton_value() {
                    CircleRange::singleton(av.wrapping_rem(bv), bits)
                } else if !a.is_top() && !a.is_bottom() && a.lo() < a.hi() && a.hi() <= bv {
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
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                if let Some(av) = a.singleton_value() {
                    CircleRange::singleton((av << bv) & mask, bits)
                } else if !a.is_top() && !a.is_bottom() && a.lo() < a.hi() {
                    let max_unshifted = mask >> bv;
                    if (a.hi() - 1) <= max_unshifted {
                        CircleRange::interval(a.lo() << bv, a.hi() << bv, bits)
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
        Eq | Ne | Lt | Le | Gt | Ge | SLt | SLe | SGt | SGe => eval_cmp_ranges(op, a, b, bits),
    }
}

fn eval_cmp_ranges(op: PreHirBinaryOp, a: CircleRange, b: CircleRange, bits: u32) -> CircleRange {
    use PreHirBinaryOp::*;
    if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
        return CircleRange::singleton(eval_cmp(op, av, bv, bits), 1);
    }

    if a.is_bottom() || b.is_bottom() {
        return CircleRange::bottom(1);
    }

    let a_non_wrapping = !a.is_top() && a.lo() < a.hi();
    let b_non_wrapping = !b.is_top() && b.lo() < b.hi();

    if a_non_wrapping && b_non_wrapping {
        // Disjoint: a entirely below b (a.hi() <= b.lo())
        if a.hi() <= b.lo() {
            match op {
                Lt | Le | Ne => return CircleRange::singleton(1, 1),
                Gt | Ge | Eq => return CircleRange::singleton(0, 1),
                _ => {}
            }
        }
        // Disjoint: b entirely below a (b.hi() <= a.lo())
        if b.hi() <= a.lo() {
            match op {
                Gt | Ge | Ne => return CircleRange::singleton(1, 1),
                Lt | Le | Eq => return CircleRange::singleton(0, 1),
                _ => {}
            }
        }

        // Signed comparison handling:
        if bits > 0 && bits <= 64 {
            let sign_bit = if bits == 64 {
                1u64 << 63
            } else {
                1u64 << (bits - 1)
            };
            let a_non_neg = a.hi() <= sign_bit;
            let b_non_neg = b.hi() <= sign_bit;
            let a_neg = a.lo() >= sign_bit;
            let b_neg = b.lo() >= sign_bit;

            if a_non_neg && b_non_neg {
                if a.hi() <= b.lo() {
                    match op {
                        SLt | SLe => return CircleRange::singleton(1, 1),
                        SGt | SGe => return CircleRange::singleton(0, 1),
                        _ => {}
                    }
                }
                if b.hi() <= a.lo() {
                    match op {
                        SGt | SGe => return CircleRange::singleton(1, 1),
                        SLt | SLe => return CircleRange::singleton(0, 1),
                        _ => {}
                    }
                }
            } else if a_neg && b_neg {
                if a.hi() <= b.lo() {
                    match op {
                        SLt | SLe => return CircleRange::singleton(1, 1),
                        SGt | SGe => return CircleRange::singleton(0, 1),
                        _ => {}
                    }
                }
                if b.hi() <= a.lo() {
                    match op {
                        SGt | SGe => return CircleRange::singleton(1, 1),
                        SLt | SLe => return CircleRange::singleton(0, 1),
                        _ => {}
                    }
                }
            } else if a_neg && b_non_neg {
                match op {
                    SLt | SLe => return CircleRange::singleton(1, 1),
                    SGt | SGe => return CircleRange::singleton(0, 1),
                    _ => {}
                }
            } else if a_non_neg && b_neg {
                match op {
                    SGt | SGe => return CircleRange::singleton(1, 1),
                    SLt | SLe => return CircleRange::singleton(0, 1),
                    _ => {}
                }
            }
        }
    }

    // Disjoint equality check for any circular range (including wrapping arcs):
    if a.meet(&b).is_bottom() {
        if op == Eq {
            return CircleRange::singleton(0, 1);
        }
        if op == Ne {
            return CircleRange::singleton(1, 1);
        }
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
    fn address_of_does_not_conflate_with_var_value() {
        let mut env = RangeEnv::new();
        env.insert("x".to_string(), CircleRange::singleton(0, 32));

        let var_expr = PreHirExpr::Var("x".to_string());
        assert_eq!(eval_expr(&var_expr, &env), CircleRange::singleton(0, 32));

        let addr_local = PreHirExpr::AddressOfLocal("x".to_string());
        assert_eq!(eval_expr(&addr_local, &env), CircleRange::top(64));

        let addr_global = PreHirExpr::AddressOfGlobal("x".to_string());
        assert_eq!(eval_expr(&addr_global, &env), CircleRange::top(64));
    }

    #[test]
    fn select_expression_evaluates_branches() {
        let env = RangeEnv::new();
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };

        // cond = 1 => evaluates to then_expr
        let sel_then = PreHirExpr::Select {
            cond: Box::new(PreHirExpr::Const(1, NirType::Bool)),
            then_expr: Box::new(PreHirExpr::Const(10, ty.clone())),
            else_expr: Box::new(PreHirExpr::Const(20, ty.clone())),
            ty: ty.clone(),
        };
        assert_eq!(eval_expr(&sel_then, &env), CircleRange::singleton(10, 32));

        // cond = 0 => evaluates to else_expr
        let sel_else = PreHirExpr::Select {
            cond: Box::new(PreHirExpr::Const(0, NirType::Bool)),
            then_expr: Box::new(PreHirExpr::Const(10, ty.clone())),
            else_expr: Box::new(PreHirExpr::Const(20, ty.clone())),
            ty: ty.clone(),
        };
        assert_eq!(eval_expr(&sel_else, &env), CircleRange::singleton(20, 32));

        // unknown cond => joins branches
        let mut env_x = RangeEnv::new();
        env_x.insert("c".to_string(), CircleRange::top(1));
        let sel_join = PreHirExpr::Select {
            cond: Box::new(PreHirExpr::Var("c".to_string())),
            then_expr: Box::new(PreHirExpr::Const(10, ty.clone())),
            else_expr: Box::new(PreHirExpr::Const(12, ty.clone())),
            ty,
        };
        assert_eq!(
            eval_expr(&sel_join, &env_x),
            CircleRange::singleton(10, 32).join(&CircleRange::singleton(12, 32))
        );
    }

    #[test]
    fn ptr_offset_adds_offset_to_base() {
        let mut env = RangeEnv::new();
        env.insert("base".to_string(), CircleRange::singleton(0x1000, 64));

        let ptr_off = PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("base".to_string())),
            offset: 0x20,
        };
        assert_eq!(
            eval_expr(&ptr_off, &env),
            CircleRange::singleton(0x1020, 64)
        );
    }

    #[test]
    fn mod_bounds_interval_by_divisor() {
        // [0, 100) % 4 => [0, 4)
        let a = CircleRange::interval(0, 100, 32);
        let b = CircleRange::singleton(4, 32);
        assert_eq!(
            eval_binary(PreHirBinaryOp::Mod, a, b, 32),
            CircleRange::interval(0, 4, 32)
        );

        // [0, 3) % 4 => [0, 3) (already within divisor)
        let a_small = CircleRange::interval(0, 3, 32);
        assert_eq!(
            eval_binary(PreHirBinaryOp::Mod, a_small, b, 32),
            CircleRange::interval(0, 3, 32)
        );
    }

    #[test]
    fn div_bounds_interval_quotient() {
        // [10, 40) / 4 => [10/4, 39/4 + 1) = [2, 10)
        let a = CircleRange::interval(10, 40, 32);
        let b = CircleRange::singleton(4, 32);
        assert_eq!(
            eval_binary(PreHirBinaryOp::Div, a, b, 32),
            CircleRange::interval(2, 10, 32)
        );
    }

    #[test]
    fn shl_scales_non_overflowing_interval() {
        // [0, 8) << 2 => [0, 32)
        let a = CircleRange::interval(0, 8, 32);
        let b = CircleRange::singleton(2, 32);
        assert_eq!(
            eval_binary(PreHirBinaryOp::Shl, a, b, 32),
            CircleRange::interval(0, 32, 32)
        );
    }

    #[test]
    fn disjoint_relational_comparisons_produce_booleans() {
        let a = CircleRange::interval(0, 5, 32);
        let b = CircleRange::interval(10, 20, 32);

        // a < b is true
        assert_eq!(
            eval_binary(PreHirBinaryOp::Lt, a, b, 32),
            CircleRange::singleton(1, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Le, a, b, 32),
            CircleRange::singleton(1, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Gt, a, b, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Ge, a, b, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Eq, a, b, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Ne, a, b, 32),
            CircleRange::singleton(1, 1)
        );

        // b > a is true
        assert_eq!(
            eval_binary(PreHirBinaryOp::Lt, b, a, 32),
            CircleRange::singleton(0, 1)
        );
        assert_eq!(
            eval_binary(PreHirBinaryOp::Gt, b, a, 32),
            CircleRange::singleton(1, 1)
        );
    }
}
