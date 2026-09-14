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
