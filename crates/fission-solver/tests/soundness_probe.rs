//! Does the solver answer operations it has no circuit for?
//!
//! `AigManager::lower_expr` has no arm for `Mul`, `Udiv` or `Ite`, and its
//! fallback lowers them to all-false bits: the solver believes `a * b`,
//! `a / b` and `ite(c, x, y)` are always zero. These are the questions whose
//! right answer that belief gets wrong. Not committed until they pass.

use fission_solver::{SatResult, Solver, SymExpr};

fn sat(expr: SymExpr) -> SatResult {
    let mut solver = Solver::new();
    solver.assert(expr);
    solver.check_sat().expect("check_sat")
}

/// `x * 3 == 6` over 8 bits holds for x = 2 (and x = 0x57, which wraps).
/// Lowered as zero it reads `0 == 6`: UNSAT.
#[test]
fn a_product_can_equal_a_non_zero_constant() {
    let x = SymExpr::new_var("x", 8);
    let product = SymExpr::Mul(Box::new(x), Box::new(SymExpr::new_const(3, 8)));
    let eq = SymExpr::Eq(Box::new(product), Box::new(SymExpr::new_const(6, 8)));
    assert!(
        matches!(sat(eq), SatResult::Sat),
        "x * 3 == 6 has a solution"
    );
}

/// `x * 2 == 7` has no solution over 8 bits: every product of 2 is even.
/// Lowered as zero this happens to be right; the pair of tests is what shows
/// the circuit rather than luck.
#[test]
fn an_odd_product_of_two_is_impossible() {
    let x = SymExpr::new_var("x", 8);
    let product = SymExpr::Mul(Box::new(x), Box::new(SymExpr::new_const(2, 8)));
    let eq = SymExpr::Eq(Box::new(product), Box::new(SymExpr::new_const(7, 8)));
    assert!(matches!(sat(eq), SatResult::Unsat), "x * 2 is always even");
}

/// `ite(x == 1, 5, 7) == 7` holds for any x other than 1.
#[test]
fn an_if_then_else_takes_its_else_branch() {
    let x = SymExpr::new_var("x", 8);
    let cond = SymExpr::Eq(Box::new(x), Box::new(SymExpr::new_const(1, 8)));
    let ite = SymExpr::Ite {
        cond: Box::new(cond),
        t: Box::new(SymExpr::new_const(5, 8)),
        f: Box::new(SymExpr::new_const(7, 8)),
    };
    let eq = SymExpr::Eq(Box::new(ite), Box::new(SymExpr::new_const(7, 8)));
    assert!(
        matches!(sat(eq), SatResult::Sat),
        "ite(x==1, 5, 7) == 7 for x != 1"
    );
}

/// `x / 4 == 3` over 8 bits holds for x in 12..=15.
#[test]
fn a_quotient_can_equal_a_non_zero_constant() {
    let x = SymExpr::new_var("x", 8);
    let quotient = SymExpr::Udiv(Box::new(x), Box::new(SymExpr::new_const(4, 8)));
    let eq = SymExpr::Eq(Box::new(quotient), Box::new(SymExpr::new_const(3, 8)));
    assert!(
        matches!(sat(eq), SatResult::Sat),
        "x / 4 == 3 has a solution"
    );
}

/// The equivalence-proof shape: two *different* functions must not be proven
/// equal. `x * 3` and `x * 5` differ for x = 1; asking for a counterexample
/// (`x*3 != x*5`) must be SAT. With both products lowered to zero it is
/// UNSAT -- a false proof of equivalence.
#[test]
fn two_different_products_are_not_proven_equal() {
    let x = SymExpr::new_var("x", 8);
    let a = SymExpr::Mul(Box::new(x.clone()), Box::new(SymExpr::new_const(3, 8)));
    let b = SymExpr::Mul(Box::new(x), Box::new(SymExpr::new_const(5, 8)));
    let differ = SymExpr::Neq(Box::new(a), Box::new(b));
    assert!(
        matches!(sat(differ), SatResult::Sat),
        "x*3 and x*5 differ at x = 1; UNSAT here is a false equivalence proof"
    );
}

// ── Every 4-bit input, against the SMT-LIB definition ───────────────────────
//
// Not against Z3: the division circuit is ported from Z3, so agreeing with Z3
// would only show the port is faithful, not that either is right. The
// definitions are SMT-LIB's, written out here.

const WIDTH: u32 = 4;
const MASK: u64 = (1 << WIDTH) - 1;

/// Pin `x` and `y`, then ask whether `op(x, y) == expected` (must be SAT) and
/// whether `op(x, y) != expected` (must be UNSAT).
fn check_exhaustively(
    what: &str,
    build: impl Fn(SymExpr, SymExpr) -> SymExpr,
    expected: impl Fn(u64, u64) -> u64,
) {
    for x in 0..=MASK {
        for y in 0..=MASK {
            let want = expected(x, y) & MASK;
            for (negate, verdict) in [(false, "SAT"), (true, "UNSAT")] {
                let xv = SymExpr::new_var("x", WIDTH);
                let yv = SymExpr::new_var("y", WIDTH);
                let mut solver = Solver::new();
                solver.assert(SymExpr::Eq(
                    Box::new(xv.clone()),
                    Box::new(SymExpr::new_const(x, WIDTH)),
                ));
                solver.assert(SymExpr::Eq(
                    Box::new(yv.clone()),
                    Box::new(SymExpr::new_const(y, WIDTH)),
                ));
                let result = build(xv, yv);
                let want_expr = Box::new(SymExpr::new_const(want, WIDTH));
                solver.assert(if negate {
                    SymExpr::Neq(Box::new(result), want_expr)
                } else {
                    SymExpr::Eq(Box::new(result), want_expr)
                });
                let got = solver.check_sat().expect("check_sat");
                let ok = match verdict {
                    "SAT" => matches!(got, SatResult::Sat),
                    _ => matches!(got, SatResult::Unsat),
                };
                assert!(
                    ok,
                    "{what}({x}, {y}): expected {want}, `{}` should be {verdict}, got {got:?}",
                    if negate { "!=" } else { "==" }
                );
            }
        }
    }
}

#[test]
fn multiplication_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvmul",
        |x, y| SymExpr::Mul(Box::new(x), Box::new(y)),
        |x, y| x * y,
    );
}

/// SMT-LIB: `bvudiv x 0` is all ones.
#[test]
fn unsigned_division_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvudiv",
        |x, y| SymExpr::Udiv(Box::new(x), Box::new(y)),
        |x, y| if y == 0 { MASK } else { x / y },
    );
}

/// There is no remainder node yet, so the remainder is built from the other
/// two: `x - (x / y) * y`. SMT-LIB's `bvurem x 0` is `x`, and this spelling
/// agrees because `bvudiv x 0` times zero is zero.
#[test]
fn a_remainder_built_from_divide_and_multiply_is_right_on_every_4_bit_pair() {
    check_exhaustively(
        "x - (x/y)*y",
        |x, y| {
            let quotient = SymExpr::Udiv(Box::new(x.clone()), Box::new(y.clone()));
            let back = SymExpr::Mul(Box::new(quotient), Box::new(y));
            SymExpr::Sub(Box::new(x), Box::new(back))
        },
        |x, y| if y == 0 { x } else { x % y },
    );
}

/// `ite(x < y, x, y)` is the unsigned minimum.
#[test]
fn if_then_else_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "ite(x<y, x, y)",
        |x, y| SymExpr::Ite {
            cond: Box::new(SymExpr::Ult(Box::new(x.clone()), Box::new(y.clone()))),
            t: Box::new(x),
            f: Box::new(y),
        },
        |x, y| x.min(y),
    );
}

/// An assertion that is not one bit wide used to be dropped without a word,
/// leaving a looser problem than the one asked. It must not come back with a
/// verdict.
#[test]
fn a_dropped_assertion_makes_the_answer_unknown() {
    let mut solver = Solver::new();
    solver.assert(SymExpr::new_var("wide", 8));
    assert!(
        matches!(solver.check_sat().expect("check_sat"), SatResult::Unknown),
        "an assertion the solver could not encode must not yield SAT or UNSAT"
    );
}

/// A shift by a *symbolic* amount was lowered as the identity: `x << y` was
/// `x`. SMT-LIB: a shift by the width or more gives zero.
#[test]
fn left_shift_by_a_symbolic_amount_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvshl",
        |x, y| SymExpr::Shl(Box::new(x), Box::new(y)),
        |x, y| if y >= WIDTH as u64 { 0 } else { x << y },
    );
}

#[test]
fn logical_right_shift_by_a_symbolic_amount_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvlshr",
        |x, y| SymExpr::Lshr(Box::new(x), Box::new(y)),
        |x, y| if y >= WIDTH as u64 { 0 } else { x >> y },
    );
}

// ── Remainder, signed division and arithmetic shift ─────────────────────────

/// A 4-bit value read as two's complement.
fn signed(value: u64) -> i64 {
    let value = value & MASK;
    if value & (1 << (WIDTH - 1)) != 0 {
        value as i64 - (1 << WIDTH)
    } else {
        value as i64
    }
}

/// SMT-LIB `bvurem`: `x % 0` is `x`.
#[test]
fn unsigned_remainder_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvurem",
        |x, y| SymExpr::Urem(Box::new(x), Box::new(y)),
        |x, y| if y == 0 { x } else { x % y },
    );
}

/// SMT-LIB `bvsdiv`: truncates toward zero; `s / 0` is 1 for negative `s` and
/// all ones otherwise; `-8 / -1` wraps back to -8.
#[test]
fn signed_division_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvsdiv",
        |x, y| SymExpr::Sdiv(Box::new(x), Box::new(y)),
        |x, y| {
            let (s, t) = (signed(x), signed(y));
            if t == 0 {
                if s < 0 {
                    1
                } else {
                    MASK
                }
            } else {
                (s / t) as u64
            }
        },
    );
}

/// SMT-LIB `bvsrem`: the sign follows the dividend; `s rem 0` is `s`.
#[test]
fn signed_remainder_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvsrem",
        |x, y| SymExpr::Srem(Box::new(x), Box::new(y)),
        |x, y| {
            let (s, t) = (signed(x), signed(y));
            if t == 0 {
                s as u64
            } else {
                (s % t) as u64
            }
        },
    );
}

/// SMT-LIB `bvsmod`: the sign follows the divisor, a zero remainder stays
/// zero, and `s mod 0` is `s`.
#[test]
fn signed_modulus_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvsmod",
        |x, y| SymExpr::Smod(Box::new(x), Box::new(y)),
        |x, y| {
            let (s, t) = (signed(x), signed(y));
            if t == 0 {
                return s as u64;
            }
            let r = s % t;
            (if r != 0 && ((r < 0) != (t < 0)) {
                r + t
            } else {
                r
            }) as u64
        },
    );
}

/// SMT-LIB `bvashr`: vacated bits copy the sign; a shift by the width or more
/// leaves only sign bits.
#[test]
fn arithmetic_right_shift_matches_its_definition_on_every_4_bit_pair() {
    check_exhaustively(
        "bvashr",
        |x, y| SymExpr::Ashr(Box::new(x), Box::new(y)),
        |x, y| {
            let s = signed(x);
            if y >= WIDTH as u64 {
                if s < 0 {
                    MASK
                } else {
                    0
                }
            } else {
                (s >> y) as u64
            }
        },
    );
}

/// A shift by a huge constant used to allocate the shift amount in bits before
/// truncating. It is zero, and answering that must not cost memory.
#[test]
fn a_huge_constant_shift_is_zero_without_allocating_it() {
    let x = SymExpr::new_var("x", 8);
    let shifted = SymExpr::Shl(Box::new(x), Box::new(SymExpr::new_const(1 << 40, 8)));
    let non_zero = SymExpr::Neq(Box::new(shifted), Box::new(SymExpr::new_const(0, 8)));
    assert!(
        matches!(sat(non_zero), SatResult::Unsat),
        "x << 2^40 is zero for every x"
    );
}

// ── Constants as direct operands ────────────────────────────────────────────
//
// Every check above pins variables with `x == c` and hands the operation
// variables. A constant written straight into the operation takes a different
// lowering path, and a random differential run against z3 disagreed on 42 of
// 500 formulas that did exactly that.

fn check_constant_operands(
    what: &str,
    build: impl Fn(SymExpr, SymExpr) -> SymExpr,
    expected: impl Fn(u64, u64) -> u64,
) {
    for x in 0..=MASK {
        for y in 0..=MASK {
            let want = expected(x, y) & MASK;
            let result = || build(SymExpr::new_const(x, WIDTH), SymExpr::new_const(y, WIDTH));
            let eq = SymExpr::Eq(
                Box::new(result()),
                Box::new(SymExpr::new_const(want, WIDTH)),
            );
            let neq = SymExpr::Neq(
                Box::new(result()),
                Box::new(SymExpr::new_const(want, WIDTH)),
            );
            let got_eq = sat(eq);
            let got_neq = sat(neq);
            assert!(
                matches!(got_eq, SatResult::Sat) && matches!(got_neq, SatResult::Unsat),
                "{what}(#{x}, #{y}) with constant operands: expected {want}; \
                 `==` gave {got_eq:?}, `!=` gave {got_neq:?}"
            );
        }
    }
}

#[test]
fn every_operation_is_right_with_constant_operands() {
    let s = |v: u64| signed(v);
    check_constant_operands(
        "bvadd",
        |a, b| SymExpr::Add(Box::new(a), Box::new(b)),
        |x, y| x + y,
    );
    check_constant_operands(
        "bvsub",
        |a, b| SymExpr::Sub(Box::new(a), Box::new(b)),
        |x, y| x.wrapping_sub(y),
    );
    check_constant_operands(
        "bvmul",
        |a, b| SymExpr::Mul(Box::new(a), Box::new(b)),
        |x, y| x * y,
    );
    check_constant_operands(
        "bvudiv",
        |a, b| SymExpr::Udiv(Box::new(a), Box::new(b)),
        |x, y| if y == 0 { MASK } else { x / y },
    );
    check_constant_operands(
        "bvurem",
        |a, b| SymExpr::Urem(Box::new(a), Box::new(b)),
        |x, y| if y == 0 { x } else { x % y },
    );
    check_constant_operands(
        "bvsdiv",
        |a, b| SymExpr::Sdiv(Box::new(a), Box::new(b)),
        |x, y| {
            let (a, b) = (s(x), s(y));
            if b == 0 {
                if a < 0 {
                    1
                } else {
                    MASK
                }
            } else {
                (a / b) as u64
            }
        },
    );
    check_constant_operands(
        "bvsrem",
        |a, b| SymExpr::Srem(Box::new(a), Box::new(b)),
        |x, y| {
            let (a, b) = (s(x), s(y));
            if b == 0 {
                a as u64
            } else {
                (a % b) as u64
            }
        },
    );
    check_constant_operands(
        "bvsmod",
        |a, b| SymExpr::Smod(Box::new(a), Box::new(b)),
        |x, y| {
            let (a, b) = (s(x), s(y));
            if b == 0 {
                return a as u64;
            }
            let r = a % b;
            (if r != 0 && ((r < 0) != (b < 0)) {
                r + b
            } else {
                r
            }) as u64
        },
    );
    check_constant_operands(
        "bvand",
        |a, b| SymExpr::And(Box::new(a), Box::new(b)),
        |x, y| x & y,
    );
    check_constant_operands(
        "bvor",
        |a, b| SymExpr::Or(Box::new(a), Box::new(b)),
        |x, y| x | y,
    );
    check_constant_operands(
        "bvxor",
        |a, b| SymExpr::Xor(Box::new(a), Box::new(b)),
        |x, y| x ^ y,
    );
    check_constant_operands(
        "bvshl",
        |a, b| SymExpr::Shl(Box::new(a), Box::new(b)),
        |x, y| if y >= WIDTH as u64 { 0 } else { x << y },
    );
    check_constant_operands(
        "bvlshr",
        |a, b| SymExpr::Lshr(Box::new(a), Box::new(b)),
        |x, y| if y >= WIDTH as u64 { 0 } else { x >> y },
    );
    check_constant_operands(
        "bvashr",
        |a, b| SymExpr::Ashr(Box::new(a), Box::new(b)),
        |x, y| {
            let a = s(x);
            if y >= WIDTH as u64 {
                if a < 0 {
                    MASK
                } else {
                    0
                }
            } else {
                (a >> y) as u64
            }
        },
    );
}

// ── Comparisons ─────────────────────────────────────────────────────────────
//
// Nothing above compared symbolic operands exhaustively. A random run against
// z3 disagreed on 175 of 2000 shallow formulas, nearly all with a signed
// comparison in them; the smallest was `27 <s 62` at six bits.

/// Pin `x` and `y`, then ask whether the one-bit comparison equals the
/// expected truth value (SAT) and whether it differs (UNSAT).
fn check_comparison_exhaustively(
    what: &str,
    build: impl Fn(SymExpr, SymExpr) -> SymExpr,
    expected: impl Fn(u64, u64) -> bool,
) {
    for x in 0..=MASK {
        for y in 0..=MASK {
            let want = u64::from(expected(x, y));
            for negate in [false, true] {
                let xv = SymExpr::new_var("x", WIDTH);
                let yv = SymExpr::new_var("y", WIDTH);
                let mut solver = Solver::new();
                solver.assert(SymExpr::Eq(
                    Box::new(xv.clone()),
                    Box::new(SymExpr::new_const(x, WIDTH)),
                ));
                solver.assert(SymExpr::Eq(
                    Box::new(yv.clone()),
                    Box::new(SymExpr::new_const(y, WIDTH)),
                ));
                let truth = Box::new(SymExpr::new_const(want, 1));
                let result = Box::new(build(xv, yv));
                solver.assert(if negate {
                    SymExpr::Neq(result, truth)
                } else {
                    SymExpr::Eq(result, truth)
                });
                let got = solver.check_sat().expect("check_sat");
                let ok = if negate {
                    matches!(got, SatResult::Unsat)
                } else {
                    matches!(got, SatResult::Sat)
                };
                assert!(
                    ok,
                    "{what}({x}, {y}) should be {}, got {got:?} for `{}`",
                    want == 1,
                    if negate { "!=" } else { "==" }
                );
            }
        }
    }
}

#[test]
fn unsigned_comparisons_match_their_definitions_on_every_4_bit_pair() {
    check_comparison_exhaustively(
        "bvult",
        |a, b| SymExpr::Ult(Box::new(a), Box::new(b)),
        |x, y| x < y,
    );
    check_comparison_exhaustively(
        "bvule",
        |a, b| SymExpr::Ule(Box::new(a), Box::new(b)),
        |x, y| x <= y,
    );
}

#[test]
fn signed_comparisons_match_their_definitions_on_every_4_bit_pair() {
    check_comparison_exhaustively(
        "bvslt",
        |a, b| SymExpr::Slt(Box::new(a), Box::new(b)),
        |x, y| signed(x) < signed(y),
    );
    check_comparison_exhaustively(
        "bvsle",
        |a, b| SymExpr::Sle(Box::new(a), Box::new(b)),
        |x, y| signed(x) <= signed(y),
    );
    check_comparison_exhaustively(
        "bvsgt",
        |a, b| SymExpr::Sgt(Box::new(a), Box::new(b)),
        |x, y| signed(x) > signed(y),
    );
}

/// The sign mask used to be `1 << (size * 8 - 1)`, which at 64 bits is a
/// shift by 511. The most negative 64-bit value is less than zero.
#[test]
fn a_64_bit_signed_comparison_does_not_overflow_and_is_right() {
    let min = SymExpr::new_const(1u64 << 63, 64);
    let zero = SymExpr::new_const(0, 64);
    let lt = SymExpr::Slt(Box::new(min.clone()), Box::new(zero.clone()));
    assert!(matches!(sat(lt), SatResult::Sat), "-2^63 <s 0");
    let gt = SymExpr::Slt(Box::new(zero), Box::new(min));
    assert!(matches!(sat(gt), SatResult::Unsat), "0 <s -2^63 is false");
}

// ── The same variable on both sides ─────────────────────────────────────────
//
// Every exhaustive check above used two *different* variables pinned to equal
// values. With one variable on both sides the two operands are the same AIG
// literals, and structural simplification takes over. The disagreements left
// after fixing the signed comparisons were all of this shape --
// `(bvurem x x)` against `(bvsmod x x)`.

fn check_same_operand(
    what: &str,
    build: impl Fn(SymExpr, SymExpr) -> SymExpr,
    expected: impl Fn(u64) -> u64,
) {
    for x in 0..=MASK {
        let want = expected(x) & MASK;
        for negate in [false, true] {
            let xv = SymExpr::new_var("x", WIDTH);
            let mut solver = Solver::new();
            solver.assert(SymExpr::Eq(
                Box::new(xv.clone()),
                Box::new(SymExpr::new_const(x, WIDTH)),
            ));
            let result = Box::new(build(xv.clone(), xv));
            let target = Box::new(SymExpr::new_const(want, WIDTH));
            solver.assert(if negate {
                SymExpr::Neq(result, target)
            } else {
                SymExpr::Eq(result, target)
            });
            let got = solver.check_sat().expect("check_sat");
            let ok = if negate {
                matches!(got, SatResult::Unsat)
            } else {
                matches!(got, SatResult::Sat)
            };
            assert!(
                ok,
                "{what}(x, x) at x = {x}: expected {want}, got {got:?} for `{}`",
                if negate { "!=" } else { "==" }
            );
        }
    }
}

#[test]
fn every_operation_is_right_with_the_same_variable_on_both_sides() {
    let s = |v: u64| signed(v);
    check_same_operand(
        "bvadd",
        |a, b| SymExpr::Add(Box::new(a), Box::new(b)),
        |x| x + x,
    );
    check_same_operand(
        "bvsub",
        |a, b| SymExpr::Sub(Box::new(a), Box::new(b)),
        |_| 0,
    );
    check_same_operand(
        "bvmul",
        |a, b| SymExpr::Mul(Box::new(a), Box::new(b)),
        |x| x * x,
    );
    check_same_operand(
        "bvudiv",
        |a, b| SymExpr::Udiv(Box::new(a), Box::new(b)),
        |x| if x == 0 { MASK } else { 1 },
    );
    check_same_operand(
        "bvurem",
        |a, b| SymExpr::Urem(Box::new(a), Box::new(b)),
        |x| if x == 0 { 0 } else { 0 },
    );
    check_same_operand(
        "bvsdiv",
        |a, b| SymExpr::Sdiv(Box::new(a), Box::new(b)),
        |x| if x == 0 { MASK } else { 1 },
    );
    check_same_operand(
        "bvsrem",
        |a, b| SymExpr::Srem(Box::new(a), Box::new(b)),
        |x| if s(x) == 0 { x } else { 0 },
    );
    check_same_operand(
        "bvsmod",
        |a, b| SymExpr::Smod(Box::new(a), Box::new(b)),
        |x| if s(x) == 0 { x } else { 0 },
    );
    check_same_operand(
        "bvand",
        |a, b| SymExpr::And(Box::new(a), Box::new(b)),
        |x| x,
    );
    check_same_operand("bvor", |a, b| SymExpr::Or(Box::new(a), Box::new(b)), |x| x);
    check_same_operand(
        "bvxor",
        |a, b| SymExpr::Xor(Box::new(a), Box::new(b)),
        |_| 0,
    );
    check_same_operand(
        "bvshl",
        |a, b| SymExpr::Shl(Box::new(a), Box::new(b)),
        |x| if x >= WIDTH as u64 { 0 } else { x << x },
    );
    check_same_operand(
        "bvlshr",
        |a, b| SymExpr::Lshr(Box::new(a), Box::new(b)),
        |x| if x >= WIDTH as u64 { 0 } else { x >> x },
    );
    check_same_operand(
        "bvashr",
        |a, b| SymExpr::Ashr(Box::new(a), Box::new(b)),
        |x| {
            if x >= WIDTH as u64 {
                if s(x) < 0 {
                    MASK
                } else {
                    0
                }
            } else {
                (s(x) >> x) as u64
            }
        },
    );
}

// ── Clauses that arrive after their literals are fixed ──────────────────────
//
// The solver loads each assertion's unit clause before the Tseitin clauses
// that connect assertions to each other. `add_clause` used to attach watches
// without reading the clause against the level-0 assignment, so a connecting
// clause whose literals were already all false was never looked at again.
// Found as "the same variable on both sides": `x + x != 0` with `x == 0` was
// SAT, while the same two constraints joined into one assertion were UNSAT.

/// At the SAT core: `a`, then `!b`, then `a -> b`.
#[test]
fn a_clause_falsified_before_it_arrives_is_still_a_conflict() {
    use fission_solver::cnf::Lit;
    use fission_solver::sat::SatSolver;

    let mut sat = SatSolver::new();
    let (a, b) = (Lit::new(1, false), Lit::new(2, false));
    assert!(sat.add_clause(vec![a]));
    assert!(sat.add_clause(vec![b.not()]));
    let accepted = sat.add_clause(vec![a.not(), b]);
    assert!(
        !accepted || !sat.solve(),
        "a, !b and (a -> b) together are unsatisfiable"
    );
}

fn verdict(assertions: Vec<SymExpr>) -> SatResult {
    let mut solver = Solver::new();
    for assertion in assertions {
        solver.assert(assertion);
    }
    solver.check_sat().expect("check_sat")
}

/// The shapes that went wrong, as assertions that share AIG nodes. Joined
/// into one assertion each of these was already right, and so was the same
/// problem over two different variables -- which is why no earlier test
/// could see it.
#[test]
fn assertions_that_share_structure_are_all_enforced() {
    let x = SymExpr::new_var("x", 4);
    let c = |v| SymExpr::new_const(v, 4);
    let pin = |v| SymExpr::Eq(Box::new(x.clone()), Box::new(c(v)));
    let doubled = || SymExpr::Add(Box::new(x.clone()), Box::new(x.clone()));

    let cases: Vec<(&str, Vec<SymExpr>)> = vec![
        (
            "x == 0 ; x + x != 0",
            vec![pin(0), SymExpr::Neq(Box::new(doubled()), Box::new(c(0)))],
        ),
        (
            "x + x != 0 ; x == 0",
            vec![SymExpr::Neq(Box::new(doubled()), Box::new(c(0))), pin(0)],
        ),
        (
            "x == 0 ; (x << 1) != 0",
            vec![
                pin(0),
                SymExpr::Neq(
                    Box::new(SymExpr::Shl(Box::new(x.clone()), Box::new(c(1)))),
                    Box::new(c(0)),
                ),
            ],
        ),
        (
            "x == 1 ; x + x != 2",
            vec![pin(1), SymExpr::Neq(Box::new(doubled()), Box::new(c(2)))],
        ),
    ];
    for (label, assertions) in cases {
        assert!(
            matches!(verdict(assertions), SatResult::Unsat),
            "{label} is unsatisfiable"
        );
    }
}

// ── Problems big enough to collect learned clauses ──────────────────────────

/// `urem(x, x)` and `smod(x, x)` are equal for every `x`, so neither is greater.
/// From five bits up the search learns enough clauses to trigger collection,
/// which used to delete input clauses -- and this came back SAT. Every value
/// pinned individually was right, because pinning ends the search at level 0.
#[test]
fn a_free_variable_problem_stays_right_after_clause_collection() {
    for width in 1..=8u32 {
        let x = SymExpr::new_var("x", width);
        let greater = SymExpr::Sgt(
            Box::new(SymExpr::Urem(Box::new(x.clone()), Box::new(x.clone()))),
            Box::new(SymExpr::Smod(Box::new(x.clone()), Box::new(x))),
        );
        assert!(
            matches!(sat(greater), SatResult::Unsat),
            "urem(x,x) >s smod(x,x) has no solution at {width} bits"
        );
    }
}
