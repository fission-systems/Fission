//! A folding constructor and the circuit for the same operation must agree.
//!
//! `SymExpr::new_add` and its siblings answer for constant operands without
//! building a node; a caller that writes the node itself gets the circuit.
//! Both are "the value of `a + b`", and `fission-dir` reaches the solver
//! through the constructors, so a fold that disagrees with lowering is a
//! wrong verdict that no test of lowering alone can see.
//!
//! The check needs no second solver: assert `raw != folded` and expect UNSAT.

use fission_solver::{SatResult, Solver, SymExpr};

type Fold = fn(SymExpr, SymExpr) -> SymExpr;
type Raw = fn(Box<SymExpr>, Box<SymExpr>) -> SymExpr;

fn differs(raw: SymExpr, folded: SymExpr) -> SatResult {
    let mut solver = Solver::new();
    solver.assert(SymExpr::Neq(Box::new(raw), Box::new(folded)));
    solver.check_sat().expect("check_sat")
}

const BINARY: [(&str, Fold, Raw); 8] = [
    ("add", SymExpr::new_add, SymExpr::Add),
    ("sub", SymExpr::new_sub, SymExpr::Sub),
    ("and", SymExpr::new_and, SymExpr::And),
    ("xor", SymExpr::new_xor, SymExpr::Xor),
    ("eq", SymExpr::new_eq, SymExpr::Eq),
    ("neq", SymExpr::new_neq, SymExpr::Neq),
    ("ult", SymExpr::new_ult, SymExpr::Ult),
    ("slt", SymExpr::new_slt, SymExpr::Slt),
];

/// Constants as the masking constructor makes them, at one width and at two.
/// This is the control for the test below: it runs the same comparison on
/// inputs every fold has always handled, so a harness that could not tell
/// agreement from disagreement would show here.
#[test]
fn folds_agree_with_lowering_on_masked_constants() {
    let values = [0u64, 1, 2, 0x7f, 0x80, 0xfe, 0xff];
    for (name, fold, raw) in BINARY {
        for (wa, wb) in [(8, 8), (8, 4), (4, 8)] {
            if name == "slt" && wa != wb {
                continue; // signed comparison of unequal widths is unsupported
            }
            for &x in &values {
                for &y in &values {
                    let (a, b) = (SymExpr::new_const(x, wa), SymExpr::new_const(y, wb));
                    let folded = fold(a.clone(), b.clone());
                    assert_eq!(
                        differs(raw(Box::new(a), Box::new(b)), folded.clone()),
                        SatResult::Unsat,
                        "{name}({x:#x}:{wa}, {y:#x}:{wb}) folded to {folded:?}"
                    );
                }
            }
        }
    }
}

/// `SymExpr::Const` has public fields, so a payload can carry bits above its
/// width. Lowering reads `size` bits and ignores the rest; the folds read
/// the whole payload. `new_eq` of `0x1ff` and `0xff`, both eight bits wide,
/// said "different" where the circuit says "equal".
#[test]
fn folds_agree_with_lowering_on_payloads_wider_than_the_width() {
    let payloads = [0x1ffu64, 0x100, 0xff, 0x01, 0xffff_ffff_ffff_ff80, 0x180];
    for (name, fold, raw) in BINARY {
        for &x in &payloads {
            for &y in &payloads {
                let a = SymExpr::Const { val: x, size: 8 };
                let b = SymExpr::Const { val: y, size: 8 };
                let folded = fold(a.clone(), b.clone());
                assert_eq!(
                    differs(raw(Box::new(a), Box::new(b)), folded.clone()),
                    SatResult::Unsat,
                    "{name}({x:#x}:8, {y:#x}:8) folded to {folded:?}"
                );
            }
        }
    }
}

/// Negation and absolute value are operations on the sign bit, and lowering
/// does exactly that. The folds went through `f32 -> f64 -> f32`, and that
/// conversion quiets a signalling NaN: the folded constant had a different
/// payload from the one the circuit produces.
#[test]
fn float_sign_folds_keep_the_payload() {
    for (bits, width) in [
        (0x7fa0_0000u64, 32),        // signalling NaN
        (0xffa0_0001, 32),           // signalling NaN, negative
        (0x7fc0_0000, 32),           // quiet NaN
        (0x3f80_0000, 32),           // 1.0
        (0x8000_0000, 32),           // -0.0
        (0x7ff4_0000_0000_0000, 64), // signalling NaN
        (0xbff0_0000_0000_0000, 64), // -1.0
    ] {
        let c = SymExpr::Const {
            val: bits,
            size: width,
        };
        assert_eq!(
            differs(
                SymExpr::FNeg(Box::new(c.clone())),
                SymExpr::new_fneg(c.clone())
            ),
            SatResult::Unsat,
            "fneg({bits:#x}) folded to {:?}",
            SymExpr::new_fneg(c.clone())
        );
        assert_eq!(
            differs(
                SymExpr::FAbs(Box::new(c.clone())),
                SymExpr::new_fabs(c.clone())
            ),
            SatResult::Unsat,
            "fabs({bits:#x}) folded to {:?}",
            SymExpr::new_fabs(c)
        );
    }
}

/// The same reading applies to a constant used as a shift amount, which
/// lowering handles without building a shifter: the amount `0x101` in eight
/// bits is `1`, not 257. Read whole, it shifted everything out.
#[test]
fn a_constant_shift_amount_is_read_at_its_width() {
    type Shift = fn(Box<SymExpr>, Box<SymExpr>) -> SymExpr;
    let shifts: [(&str, Shift); 3] = [
        ("shl", SymExpr::Shl),
        ("lshr", SymExpr::Lshr),
        ("ashr", SymExpr::Ashr),
    ];
    for (name, shift) in shifts {
        let x = SymExpr::new_var("x", 8);
        let wide_payload = SymExpr::Const {
            val: 0x101,
            size: 8,
        };
        assert_eq!(
            differs(
                shift(Box::new(x.clone()), Box::new(wide_payload)),
                shift(Box::new(x.clone()), Box::new(SymExpr::new_const(1, 8))),
            ),
            SatResult::Unsat,
            "{name} by the 8-bit payload 0x101 is {name} by 1"
        );
        // The control: an amount that really is past the width still shifts
        // everything out, so the two sides below do differ.
        assert_eq!(
            differs(
                shift(Box::new(x.clone()), Box::new(SymExpr::new_const(9, 8))),
                shift(Box::new(x), Box::new(SymExpr::new_const(1, 8))),
            ),
            SatResult::Sat,
            "{name} by 9 is not {name} by 1"
        );
    }
}

/// `is_true` and `is_false` answer for a constant without a query. The
/// one-bit payload `2` is the condition `0` -- asserting it is a
/// contradiction -- so it is false, not true.
#[test]
fn a_constant_condition_is_read_at_its_width() {
    let two_in_one_bit = SymExpr::Const { val: 2, size: 1 };
    let mut asserting = Solver::new();
    asserting.assert(two_in_one_bit.clone());
    assert_eq!(
        asserting.check_sat().expect("check_sat"),
        SatResult::Unsat,
        "the control: lowering reads this constant as false"
    );

    let mut solver = Solver::new();
    assert!(!solver.is_true(&two_in_one_bit));
    assert!(solver.is_false(&two_in_one_bit));
    let three_in_one_bit = SymExpr::Const { val: 3, size: 1 };
    assert!(solver.is_true(&three_in_one_bit));
    assert!(!solver.is_false(&three_in_one_bit));
}
