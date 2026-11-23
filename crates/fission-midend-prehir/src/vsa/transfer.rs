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
            integer_result(ty, |bits| CircleRange::singleton(*value as u64, bits))
        }
        PreHirExpr::Var(name) => env
            .get(name.as_str())
            .copied()
            .unwrap_or_else(|| CircleRange::top(64)),
        // Where a variable lives, not what it holds: the environment maps names
        // to stored values, so reading it here made `&x` equal to `x`.
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => CircleRange::top(64),
        PreHirExpr::PtrOffset { base, offset } => {
            let base = eval_expr(base, env);
            let bits = base.bits();
            base.add(&CircleRange::singleton(*offset as u64, bits))
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => integer_result(ty, |bits| match eval_expr(cond, env).singleton_value() {
            Some(0) => eval_expr(else_expr, env),
            Some(_) => eval_expr(then_expr, env),
            None => {
                let then_range = eval_expr(then_expr, env);
                let else_range = eval_expr(else_expr, env);
                if then_range.bits() == else_range.bits() {
                    then_range.join(&else_range)
                } else {
                    CircleRange::top(bits)
                }
            }
        }),
        PreHirExpr::Cast { expr: inner, ty } => {
            integer_result(ty, |bits| eval_expr(inner, env).cast_unsigned(bits))
        }
        PreHirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => integer_result(ty, |bits| eval_unary(*op, eval_expr(inner, env), bits)),
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            let a = eval_expr(lhs, env);
            let b = eval_expr(rhs, env);
            if is_comparison(*op) {
                // The result is a Bool, but the operands live in their own domain:
                // reading them at the 1-bit result width truncates both sides and
                // makes `1 <s 0` come out true.
                return if a.bits() == b.bits() && CircleRange::supports_intervals(a.bits()) {
                    eval_binary(*op, a, b, a.bits())
                } else {
                    // Mixed or unsupported widths cannot be compared safely in this domain.
                    CircleRange::top(1)
                };
            }
            integer_result(ty, |bits| eval_binary(*op, a, b, bits))
        }
        // For complex expressions (loads, calls, etc.) we conservatively
        // return top.
        _ => CircleRange::top(64),
    }
}

fn integer_result(ty: &NirType, eval: impl FnOnce(u32) -> CircleRange) -> CircleRange {
    match nir_bits(ty) {
        Some(bits) if CircleRange::supports_intervals(bits) => eval(bits),
        Some(bits) => CircleRange::top(bits),
        None => CircleRange::top(64),
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
    if !CircleRange::supports_intervals(bits) {
        return if is_comparison(op) {
            CircleRange::top(1)
        } else {
            CircleRange::top(bits)
        };
    }
    let a = a.cast_unsigned(bits);
    // The shift count has its own operand width. Narrowing it to the result
    // width can turn a large shift into a small one before the bounds check.
    let b = match op {
        Shl | Shr | Sar => b,
        _ => b.cast_unsigned(bits),
    };
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
        // PreHIR uses the same operators for signed, unsigned, and float division/mod.
        // Abstain conservatively so signed or float values are never folded as unsigned.
        Div | Mod => CircleRange::top(bits),
        Shl => match b.singleton_value() {
            Some(shift) if shift >= u64::from(bits) => CircleRange::singleton(0, bits),
            Some(shift) if shift < u64::from(u64::BITS) => a.singleton_value().map_or_else(
                || CircleRange::top(bits),
                |v| CircleRange::singleton(v << shift, bits),
            ),
            _ => CircleRange::top(bits),
        },
        Shr => match b.singleton_value() {
            Some(shift) => a.shr_const(shift.min(u64::from(bits)) as u32),
            None => CircleRange::top(bits),
        },
        Sar => match (a.singleton_value(), b.singleton_value()) {
            (Some(value), Some(shift)) => {
                let shift = shift.min(u64::from(bits.saturating_sub(1)));
                CircleRange::singleton((sign_extend(value, bits) >> shift) as u64, bits)
            }
            _ => CircleRange::top(bits),
        },
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
        Eq | Ne | Lt | Le | Gt | Ge | SLt | SLe | SGt | SGe => {
            if let (Some(av), Some(bv)) = (a.singleton_value(), b.singleton_value()) {
                CircleRange::singleton(eval_cmp(op, av, bv, bits), 1)
            } else {
                CircleRange::top(1)
            }
        }
    }
}

fn is_comparison(op: PreHirBinaryOp) -> bool {
    use PreHirBinaryOp::*;
    matches!(op, Eq | Ne | Lt | Le | Gt | Ge | SLt | SLe | SGt | SGe)
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
        _ => None,
    }
}

#[cfg(test)]
mod review_precision_regressions {
    use super::*;
    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }
    fn comparison(lhs: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(lhs),
            rhs: Box::new(PreHirExpr::Const(0, u32_ty())),
            ty: NirType::Bool,
        }
    }
    #[test]
    fn review_select_precision_must_not_expose_comparison_width_bug() {
        let expr = PreHirExpr::Select {
            cond: Box::new(comparison(PreHirExpr::Const(1, u32_ty()))),
            then_expr: Box::new(PreHirExpr::Const(22, u32_ty())),
            else_expr: Box::new(PreHirExpr::Const(11, u32_ty())),
            ty: u32_ty(),
        };
        let result = eval_expr(&expr, &RangeEnv::new());
        assert_ne!(
            result.singleton_value(),
            Some(22),
            "signed 1 < 0 is false; the abstract result must not exclude 11"
        );
    }
    #[test]
    fn review_add_precision_must_not_expose_comparison_width_bug() {
        let sum = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Const(1, u32_ty())),
            rhs: Box::new(PreHirExpr::Const(0, u32_ty())),
            ty: u32_ty(),
        };
        let result = eval_expr(&comparison(sum), &RangeEnv::new());
        assert_eq!(
            result.singleton_value(),
            Some(0),
            "signed (1 + 0) < 0 is false"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shift amount below the domain width but past the width of the `u64`
    /// that carries a singleton is not representable; it must widen to top
    /// instead of shifting a `u64` by 64 or more.
    #[test]
    fn shl_by_more_than_a_word_in_a_wide_domain_is_top() {
        let a = CircleRange::singleton(1, 128);
        let b = CircleRange::singleton(70, 128);
        let out = eval_binary(PreHirBinaryOp::Shl, a, b, 128);
        assert_eq!(out, CircleRange::top(128));
    }

    #[test]
    fn shl_within_a_word_still_folds() {
        let a = CircleRange::singleton(1, 64);
        let b = CircleRange::singleton(8, 64);
        let out = eval_binary(PreHirBinaryOp::Shl, a, b, 64);
        assert_eq!(out.singleton_value(), Some(0x100));
    }

    #[test]
    fn wide_128bit_arithmetic_and_comparisons_return_top_without_panicking() {
        let ty128 = NirType::Int {
            bits: 128,
            signed: false,
        };
        let c1 = PreHirExpr::Const(1, ty128.clone());
        let c_neg1 = PreHirExpr::Const(-1, ty128.clone());
        for op in [
            PreHirBinaryOp::Add,
            PreHirBinaryOp::Sub,
            PreHirBinaryOp::Mul,
            PreHirBinaryOp::Shl,
            PreHirBinaryOp::Shr,
            PreHirBinaryOp::Sar,
        ] {
            let expr = PreHirExpr::Binary {
                op,
                lhs: Box::new(c1.clone()),
                rhs: Box::new(c_neg1.clone()),
                ty: ty128.clone(),
            };
            let range = eval_expr(&expr, &RangeEnv::new());
            assert_eq!(range, CircleRange::top(128));
            assert_eq!(range.count(), u128::MAX);
        }
        let cmp = PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs: Box::new(c1),
            rhs: Box::new(c_neg1),
            ty: NirType::Bool,
        };
        assert_eq!(eval_expr(&cmp, &RangeEnv::new()), CircleRange::top(1));
    }

    #[test]
    fn large_shift_counts_do_not_truncate_to_u32_on_shr_and_sar() {
        let count_hi_zero_lo = CircleRange::singleton(1u64 << 32, 64);
        let count_hi_one_lo = CircleRange::singleton((1u64 << 32) | 1, 64);
        for count in [count_hi_zero_lo, count_hi_one_lo] {
            for bits in [32, 64] {
                let all_ones = CircleRange::singleton(u64::MAX, bits);
                for op in [PreHirBinaryOp::Shl, PreHirBinaryOp::Shr] {
                    assert_eq!(
                        eval_binary(op, all_ones, count, bits).singleton_value(),
                        Some(0),
                        "op={op:?}, bits={bits}, count={count:?}"
                    );
                }
                assert_eq!(
                    eval_binary(PreHirBinaryOp::Sar, all_ones, count, bits),
                    all_ones,
                    "bits={bits}, count={count:?}"
                );
                let positive = CircleRange::singleton(8, bits);
                assert_eq!(
                    eval_binary(PreHirBinaryOp::Sar, positive, count, bits).singleton_value(),
                    Some(0),
                    "bits={bits}, count={count:?}"
                );
            }
        }
    }

    #[test]
    fn eval_expr_shift_counts_above_u32_max_preserve_high_bits() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let shift_count = PreHirExpr::Const(1i64 << 32, u64_ty.clone());
        let val64 = PreHirExpr::Const(-1, u64_ty.clone());
        let shr64 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs: Box::new(val64.clone()),
            rhs: Box::new(shift_count.clone()),
            ty: u64_ty,
        };
        assert_eq!(
            eval_expr(&shr64, &RangeEnv::new()).singleton_value(),
            Some(0)
        );

        let val32 = PreHirExpr::Const(-1, u32_ty.clone());
        let sar32 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Sar,
            lhs: Box::new(val32),
            rhs: Box::new(shift_count),
            ty: u32_ty,
        };
        assert_eq!(
            eval_expr(&sar32, &RangeEnv::new()).singleton_value(),
            Some(0xffff_ffff)
        );
    }

    #[test]
    fn div_and_mod_abstain_for_signed_and_unsigned_operands() {
        for op in [PreHirBinaryOp::Div, PreHirBinaryOp::Mod] {
            let neg4 = CircleRange::singleton(0xffff_fffc, 32);
            let two = CircleRange::singleton(2, 32);
            assert!(eval_binary(op, neg4, two, 32).is_top());

            let pos10 = CircleRange::singleton(10, 32);
            assert!(eval_binary(op, pos10, two, 32).is_top());
        }
    }

    #[test]
    fn float_values_and_operations_remain_top() {
        for bits in [32, 64] {
            let ty = NirType::Float { bits };
            assert_eq!(nir_bits(&ty), None);

            let one_bits = if bits == 32 {
                i64::from(1.0_f32.to_bits())
            } else {
                1.0_f64.to_bits() as i64
            };
            let f_const = || Box::new(PreHirExpr::Const(one_bits, ty.clone()));
            let add = PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: f_const(),
                rhs: f_const(),
                ty: ty.clone(),
            };
            let div = PreHirExpr::Binary {
                op: PreHirBinaryOp::Div,
                lhs: f_const(),
                rhs: f_const(),
                ty: ty.clone(),
            };
            let cmp = PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs: Box::new(add.clone()),
                rhs: f_const(),
                ty: NirType::Bool,
            };
            let cast = PreHirExpr::Cast {
                expr: Box::new(add.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            };
            let neg = PreHirExpr::Unary {
                op: PreHirUnaryOp::Neg,
                expr: f_const(),
                ty: ty.clone(),
            };
            let select = PreHirExpr::Select {
                cond: Box::new(PreHirExpr::Const(1, NirType::Bool)),
                then_expr: Box::new(PreHirExpr::Const(
                    42,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                else_expr: f_const(),
                ty: ty.clone(),
            };
            let env = RangeEnv::new();
            for expr in [add, div, cmp, cast, neg, select] {
                assert!(
                    eval_expr(&expr, &env).is_top(),
                    "bits={bits}, expr={expr:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod address_of_tests {
    use super::*;

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    /// `&x` is where `x` lives, not what it holds: with `x == 0` the address of
    /// `x` is not the number zero.
    #[test]
    fn the_address_of_a_variable_is_not_its_value() {
        let mut env = RangeEnv::new();
        env.insert("x".to_string(), CircleRange::singleton(0, 64));
        for expr in [
            PreHirExpr::AddressOfLocal("x".to_string()),
            PreHirExpr::AddressOfGlobal("x".to_string()),
        ] {
            assert!(eval_expr(&expr, &env).is_top(), "{expr:?}");
        }
        // The variable itself is still read from the environment.
        assert_eq!(
            eval_expr(&PreHirExpr::Var("x".to_string()), &env).singleton_value(),
            Some(0)
        );
    }

    #[test]
    fn a_pointer_offset_adds_to_the_base_range() {
        let mut env = RangeEnv::new();
        env.insert("p".to_string(), CircleRange::singleton(100, 64));
        let expr = PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("p".to_string())),
            offset: 8,
        };
        assert_eq!(eval_expr(&expr, &env).singleton_value(), Some(108));
        let _ = u64_ty();
    }

    #[test]
    fn a_select_on_a_known_condition_takes_that_arm() {
        let one = |v: i64| PreHirExpr::Const(v, u64_ty());
        let select = |cond: PreHirExpr| PreHirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(one(22)),
            else_expr: Box::new(one(11)),
            ty: u64_ty(),
        };
        let truth = PreHirExpr::Const(1, NirType::Bool);
        let falsity = PreHirExpr::Const(0, NirType::Bool);
        assert_eq!(
            eval_expr(&select(truth), &RangeEnv::new()).singleton_value(),
            Some(22)
        );
        assert_eq!(
            eval_expr(&select(falsity), &RangeEnv::new()).singleton_value(),
            Some(11)
        );
        // An unknown condition may take either arm.
        let unknown = select(PreHirExpr::Var("c".to_string()));
        let joined = eval_expr(&unknown, &RangeEnv::new());
        assert_eq!(joined.singleton_value(), None);
    }
}
