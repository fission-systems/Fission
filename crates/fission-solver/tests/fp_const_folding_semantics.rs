//! Independent reference-vector and soundness tests for issue #1278:
//! - Target-native Binary32 and Binary64 IEEE 754 RNE constant folding without
//!   `f32 -> f64 -> f32` double-rounding.
//! - NaN bit-pattern uncertainty preserved (`PreserveUncertainty` default) vs
//!   explicit `CanonicalQuiet` opt-in.
//! - Exact bit-pattern sign manipulation on `FNeg`/`FAbs`.
//! - IEEE numeric comparison vs raw bitvector equality, unsupported widths,
//!   conditional `NO_ARITHMETIC_FOLD` profile, and solver query soundness.

use fission_solver::ast::{checked_float_format, FpFoldProfile, NanFoldPolicy, Sort, SymExpr};
use fission_solver::solver::{SatResult, Solver};

fn c32(bits: u32) -> SymExpr {
    SymExpr::Const {
        val: u64::from(bits),
        size: 32,
    }
}

fn c64(bits: u64) -> SymExpr {
    SymExpr::Const {
        val: bits,
        size: 64,
    }
}

#[test]
fn f32_folding_uses_native_single_precision_without_double_rounding() {
    // Classic f32 halfway / rounding boundary vectors + division where
    // computing in f64 and casting back would differ or lose precision semantics;
    // compare against native IEEE binary32 arithmetic directly.
    let pairs_f32: &[(u32, u32)] = &[
        (1.5f32.to_bits(), 2.5f32.to_bits()),
        (0x3f80_0001, 0x3f80_0003), // adjacent 1+2^-23 operands
        (0x4b80_0001, 0x3f00_0000), // ties-to-even near exact integer boundary
        (0x0000_0001, 0x0000_0001), // smallest positive subnormal + itself
        (0x007f_ffff, 0x0000_0001), // max subnormal + min subnormal -> normal boundary
        (0x0080_0000, 0x8000_0001), // min normal - smallest subnormal
        (0x3f80_0000, 0xbf80_0000), // exact cancellation: +1.0 + (-1.0) = +0.0
        (0x8000_0000, 0x8000_0000), // (-0.0) + (-0.0) = -0.0
        (0x8000_0000, 0x0000_0000), // (-0.0) + (+0.0) = +0.0
        (0x3f80_0000, 0x8000_0000), // +1.0 / (-0.0) = -Inf
        (0x7f80_0000, 0x7f80_0000), // +Inf + +Inf = +Inf
    ];

    for &(abits, bbits) in pairs_f32 {
        let af = f32::from_bits(abits);
        let bf = f32::from_bits(bbits);

        let check = |got: SymExpr, expected: f32, op_name: &str| {
            if expected.is_nan() {
                assert!(
                    !matches!(got, SymExpr::Const { .. }),
                    "{op_name} produced NaN and must stay unfolded under STRICT_IEEE754_RNE"
                );
            } else {
                assert_eq!(
                    got,
                    c32(expected.to_bits()),
                    "{op_name}({abits:#010x}, {bbits:#010x}) bits mismatch: expected {:#010x}",
                    expected.to_bits()
                );
            }
        };

        check(SymExpr::new_fadd(c32(abits), c32(bbits)), af + bf, "fadd");
        check(SymExpr::new_fsub(c32(abits), c32(bbits)), af - bf, "fsub");
        check(SymExpr::new_fmul(c32(abits), c32(bbits)), af * bf, "fmul");
        check(SymExpr::new_fdiv(c32(abits), c32(bbits)), af / bf, "fdiv");
    }

    // Specific double-rounding divergence case for f32 addition:
    // x = 16777216.0 (2^24, LSB = 2.0), y = 1.0 + 2^-23 = 0x3f80_0001.
    // Exact sum = 2^24 + 1 + 2^-23 (strictly > midpoint 2^24 + 1), which rounds UP
    // in single precision to 2^24 + 2 (0x4b80_0001) and also here matches native f32;
    // contrast `a * b` or chained ops that only native f32 preserves accurately.
    let subnormal_sqrt = SymExpr::new_fsqrt(c32(0x8000_0000)); // sqrt(-0.0) == -0.0
    assert_eq!(subnormal_sqrt, c32(0x8000_0000));

    let neg_sqrt = SymExpr::new_fsqrt(c32((-1.0f32).to_bits()));
    assert!(matches!(neg_sqrt, SymExpr::FSqrt(_)));
}

#[test]
fn nan_inputs_and_nan_results_preserve_uncertainty_and_cannot_forge_bv_proofs() {
    let snan32 = 0x7f80_0001u32;
    let qnan32_custom = 0x7fc0_1234u32;
    let qnan64_custom = 0x7ff8_0000_0000_0042u64;

    // Under STRICT_IEEE754_RNE, NaN inputs and NaN-producing arithmetic stay unfolded.
    assert!(matches!(
        SymExpr::new_fadd(c32(snan32), c32(1.0f32.to_bits())),
        SymExpr::FAdd(_, _)
    ));
    assert!(matches!(
        SymExpr::new_fmul(c32(qnan32_custom), c32(2.0f32.to_bits())),
        SymExpr::FMul(_, _)
    ));
    assert!(matches!(
        SymExpr::new_fdiv(c32(0), c32(0)), // 0.0 / 0.0 -> NaN
        SymExpr::FDiv(_, _)
    ));
    assert!(matches!(
        SymExpr::new_fsub(c64(f64::INFINITY.to_bits()), c64(f64::INFINITY.to_bits())),
        SymExpr::FSub(_, _)
    ));
    assert!(matches!(
        SymExpr::new_fsqrt(c64(qnan64_custom)),
        SymExpr::FSqrt(_)
    ));

    // A solver consumer comparing `new_fadd(NaN, 1.0) == canonical_qnan` must NOT
    // obtain Unsat or Valid for raw BV equality under the default profile because
    // NaN payload is unspecified across ISAs/circuits.
    let via_ctor = SymExpr::new_fadd(c32(snan32), c32(1.0f32.to_bits()));
    let raw_node = SymExpr::FAdd(Box::new(c32(snan32)), Box::new(c32(1.0f32.to_bits())));
    assert_eq!(via_ctor, raw_node);

    let eq_expr = SymExpr::new_eq(via_ctor, c32(0x7fc0_0000));
    assert!(
        matches!(eq_expr, SymExpr::Eq(_, _)),
        "NaN-derived expression must not fold into a BV equality constant"
    );
    let mut solver = Solver::new();
    solver.assert(eq_expr);
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);

    // Explicit opt-in CanonicalQuiet profile folds to documented canonical qNaN bits.
    let qnan_profile = FpFoldProfile {
        nan_policy: NanFoldPolicy::CanonicalQuiet,
        ..FpFoldProfile::STRICT_IEEE754_RNE
    };
    assert_eq!(
        SymExpr::new_fdiv_with_profile(c32(0), c32(0), qnan_profile),
        c32(0x7fc0_0000)
    );
    assert_eq!(
        SymExpr::new_fsqrt_with_profile(c64((-1.0f64).to_bits()), qnan_profile),
        c64(0x7ff8_0000_0000_0000)
    );

    // NO_ARITHMETIC_FOLD keeps even finite arithmetic symbolic while retaining predicates.
    assert!(matches!(
        SymExpr::new_fadd_with_profile(
            c32(1.0f32.to_bits()),
            c32(2.0f32.to_bits()),
            FpFoldProfile::NO_ARITHMETIC_FOLD,
        ),
        SymExpr::FAdd(_, _)
    ));
    assert_eq!(
        SymExpr::new_flt_with_profile(
            c32(1.0f32.to_bits()),
            c32(2.0f32.to_bits()),
            FpFoldProfile::NO_ARITHMETIC_FOLD,
        ),
        SymExpr::Const { val: 1, size: 1 }
    );
}

#[test]
fn fneg_and_fabs_preserve_exact_bit_patterns_including_nan_payloads() {
    let snan32 = 0x7f80_0001u32;
    assert_eq!(SymExpr::new_fneg(c32(snan32)), c32(snan32 | 0x8000_0000));
    assert_eq!(SymExpr::new_fabs(c32(snan32 | 0x8000_0000)), c32(snan32));

    let qnan64_neg = 0xfff8_0000_1234_5678u64;
    assert_eq!(
        SymExpr::new_fabs(c64(qnan64_neg)),
        c64(qnan64_neg & 0x7fff_ffff_ffff_ffff)
    );
    assert_eq!(SymExpr::new_fneg(c32(0)), c32(0x8000_0000)); // +0.0 -> -0.0
    assert_eq!(SymExpr::new_fabs(c64(0x8000_0000_0000_0000)), c64(0)); // -0.0 -> +0.0
}

#[test]
fn ieee_numeric_predicates_distinguish_signed_zero_and_nan_from_raw_bv_equality() {
    let pos_zero = c32(0x0000_0000);
    let neg_zero = c32(0x8000_0000);
    assert_eq!(
        SymExpr::new_feq(pos_zero.clone(), neg_zero.clone()),
        SymExpr::Const { val: 1, size: 1 }
    );
    assert_eq!(
        SymExpr::new_eq(pos_zero, neg_zero),
        SymExpr::Const { val: 0, size: 1 }
    );

    let nan_a = c64(0x7ff8_0000_0000_0001);
    let nan_b = c64(0x7ff8_0000_0000_0001);
    assert_eq!(
        SymExpr::new_feq(nan_a.clone(), nan_b.clone()),
        SymExpr::Const { val: 0, size: 1 }
    );
    assert_eq!(
        SymExpr::new_fneq(nan_a.clone(), nan_b.clone()),
        SymExpr::Const { val: 1, size: 1 }
    );
    assert_eq!(
        SymExpr::new_eq(nan_a.clone(), nan_b),
        SymExpr::Const { val: 1, size: 1 }
    );
    assert_eq!(
        SymExpr::new_fisnan(nan_a),
        SymExpr::Const { val: 1, size: 1 }
    );
    assert_eq!(
        SymExpr::new_fisnan(c32(0x7f80_0000)), // +Inf is not NaN
        SymExpr::Const { val: 0, size: 1 }
    );
}

#[test]
fn unsupported_and_mixed_widths_are_rejected_without_inferring_binary64() {
    for width in [8u32, 16, 80, 128] {
        let var = SymExpr::new_float_var("w", width);
        assert_eq!(checked_float_format(&var), None);
        let added = SymExpr::new_fadd(var.clone(), var);
        assert!(matches!(added, SymExpr::FAdd(_, _)));
        assert_eq!(
            added.get_sort(),
            Sort::Float(width),
            "unsupported width {width} must not silently become Float(64)"
        );
    }

    // 16-bit concrete constant stays unfolded and reports Float(16), not Float(64).
    let f16_a = SymExpr::Const {
        val: 0x3c00,
        size: 16,
    };
    let f16_b = SymExpr::Const {
        val: 0x4000,
        size: 16,
    };
    let f16_sum = SymExpr::new_fadd(f16_a.clone(), f16_b.clone());
    assert!(matches!(f16_sum, SymExpr::FAdd(_, _)));
    assert_eq!(f16_sum.get_sort(), Sort::Float(16));
    assert!(matches!(SymExpr::new_feq(f16_a, f16_b), SymExpr::FEq(_, _)));

    // Mixed 32-bit and 64-bit concrete operands stay unfolded.
    assert!(matches!(
        SymExpr::new_fadd(c32(1.0f32.to_bits()), c64(1.0f64.to_bits())),
        SymExpr::FAdd(_, _)
    ));
    assert!(matches!(
        SymExpr::new_flt(c32(1.0f32.to_bits()), c64(2.0f64.to_bits())),
        SymExpr::FLt(_, _)
    ));
}
