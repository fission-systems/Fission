//! The scalar semantic contract every DIR evaluator implements.
//!
//! Three evaluators read the same IR: the concrete interpreter
//! ([`crate::eval`]), the symbolic lowering ([`crate::lower_sym`]) and the
//! native P-code reconstruction ([`crate::native`]). A symbolic `Unsat` is a
//! proof about the lowered formula; it says something about the IR only when
//! the lowering computes what the concrete interpreter computes. This module
//! is where the operations the two used to disagree on are defined once, and
//! where the rules every evaluator follows are written down.
//!
//! # Values
//!
//! A scalar is a bit pattern of its declared width `w` (`Bool` is 1 bit,
//! `Int { bits }` is `bits`, 1..=64). Signedness is not part of the pattern;
//! it decides only how the pattern is *extended* and how it is *compared*.
//!
//! # Conversions (casts, assignments, initializers, select arms, operands)
//!
//! Converting a `from`-bit value to `to` bits truncates to the low `to` bits
//! when narrowing and extends by the **source** type's signedness when
//! widening. A narrowing conversion of a multi-bit value **into `Bool`** is
//! ambiguous -- p-code truncates (`SUBPIECE`), the C the printer writes
//! (`(bool)x`) tests `x != 0` -- so the symbolic tier refuses it rather than
//! certify either reading. Widening from `Bool` is unambiguous.
//!
//! # Arithmetic and bitwise operators (`Add Sub Mul And Or Xor`, `Neg BitNot`)
//!
//! Each operand is converted to the result width, then the operation runs
//! modulo `2^result_bits`. (The low result bits of these operations depend
//! only on the low result bits of the operands, so this equals computing at
//! any wider width and truncating.)
//!
//! # Comparisons and logical operators
//!
//! Comparisons convert both operands to the wider of the two operand widths
//! (each extended by its own signedness) and compare there: `Lt..Ge`
//! unsigned, `SLt..SGe` two's-complement signed. `LogicalAnd`/`LogicalOr`/
//! `Not` and every condition (`If`, `Select`) test `value != 0` at the
//! value's own width. The 0/1 result is zero-extended to the result type.
//!
//! # Shifts (`Shl`, `Shr`, `Sar`) -- [`shift`] / [`shift_sym`]
//!
//! P-code's definition (`INT_LEFT`, `INT_RIGHT`, `INT_SRIGHT`), which a
//! machine's own count masking (x86 `& 63`) is already explicit in:
//!
//! - The **count** is the right operand's full value read as **unsigned at
//!   its own width** -- never masked, never truncated to the shifted width. A
//!   negative signed count is therefore a large count.
//! - The shift runs at `W = max(operand_bits, result_bits)`. The operand is
//!   brought to `W` first: `Shl` extends it by its declared signedness (as
//!   every other arithmetic operand), `Shr` zero-extends its own bits and
//!   `Sar` sign-extends its own bits (the operator decides, not the type).
//! - A count `>= W` yields 0 for `Shl`/`Shr` and the replicated sign bit for
//!   `Sar`.
//! - The `W`-bit result is then truncated to the result width.
//!
//! So 64-bit `1 << 64` and `1 << 65` are 0, and 16-bit `0x0100 >> 1` with an
//! 8-bit result is `0x80`.
//!
//! # Returns
//!
//! A `return e` yields `e`'s value at `e`'s own type. The symbolic tier
//! refuses a function whose returned expression width differs from its
//! declared return width instead of inventing a conversion.
//!
//! # Not modelled (refused explicitly at the symbolic tier)
//!
//! `Div`/`Mod` (the concrete tier traps on a zero divisor; the solver's
//! division is total), floats, pointers, aggregates, widths outside 1..=64,
//! memory, calls and loops.

use fission_solver::ast::SymExpr;

/// Which shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShiftKind {
    Left,
    LogicalRight,
    ArithmeticRight,
}

/// All ones in the low `bits` bits (`bits` in 0..=64).
#[must_use]
pub(crate) fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

/// `value`'s low `bits` bits extended to 64 by the sign bit `bits - 1`.
#[must_use]
pub(crate) fn sign_extend(value: u64, bits: u32) -> u64 {
    if bits == 0 || bits >= 64 {
        return value;
    }
    let shift = 64 - bits;
    (((value << shift) as i64) >> shift) as u64
}

/// The shift contract on concrete bits: `value` is a `width`-bit pattern
/// already brought to the shift width (see the module docs), `count` is the
/// count's full unsigned value. Returns a `width`-bit pattern.
#[must_use]
pub(crate) fn shift(kind: ShiftKind, value: u64, width: u32, count: u64) -> u64 {
    debug_assert!((1..=64).contains(&width), "shift width {width}");
    let m = mask(width);
    let v = value & m;
    if count >= u64::from(width) {
        return match kind {
            ShiftKind::Left | ShiftKind::LogicalRight => 0,
            ShiftKind::ArithmeticRight => {
                if v >> (width - 1) & 1 == 1 {
                    m
                } else {
                    0
                }
            }
        };
    }
    let c = count as u32;
    let out = match kind {
        ShiftKind::Left => v << c,
        ShiftKind::LogicalRight => v >> c,
        ShiftKind::ArithmeticRight => ((sign_extend(v, width) as i64) >> c) as u64,
    };
    out & m
}

/// The shift contract on bit-vectors: `value` is `width` bits wide (already
/// brought to the shift width), `count` is `count_bits` wide and read as an
/// unsigned value. Built so every node has SMT-LIB sorts (the solver refuses
/// a shift whose amount and value widths differ) and the count is never
/// truncated: a count wider than `width` is compared at its own width first.
#[must_use]
pub(crate) fn shift_sym(
    kind: ShiftKind,
    value: SymExpr,
    width: u32,
    count: SymExpr,
    count_bits: u32,
) -> SymExpr {
    let build = |value: SymExpr, count: SymExpr| match kind {
        ShiftKind::Left => SymExpr::Shl(Box::new(value), Box::new(count)),
        ShiftKind::LogicalRight => SymExpr::Lshr(Box::new(value), Box::new(count)),
        ShiftKind::ArithmeticRight => SymExpr::Ashr(Box::new(value), Box::new(count)),
    };
    match count_bits.cmp(&width) {
        std::cmp::Ordering::Equal => build(value, count),
        // Zero-extending the count keeps its value.
        std::cmp::Ordering::Less => build(
            value,
            SymExpr::Concat(
                Box::new(SymExpr::new_const(0, width - count_bits)),
                Box::new(count),
            ),
        ),
        // `count < width` fits in `width` bits (width < 2^width), so the
        // low bits are the count itself; otherwise the shift saturates.
        std::cmp::Ordering::Greater => {
            let in_range = SymExpr::Ult(
                Box::new(count.clone()),
                Box::new(SymExpr::new_const(u64::from(width), count_bits)),
            );
            let saturated = match kind {
                ShiftKind::Left | ShiftKind::LogicalRight => SymExpr::new_const(0, width),
                ShiftKind::ArithmeticRight => SymExpr::Ashr(
                    Box::new(value.clone()),
                    Box::new(SymExpr::new_const(u64::from(width - 1), width)),
                ),
            };
            let narrowed = SymExpr::Extract {
                expr: Box::new(count),
                lsb: 0,
                size: width,
            };
            SymExpr::Ite {
                cond: Box::new(in_range),
                t: Box::new(build(value, narrowed)),
                f: Box::new(saturated),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ShiftKind, mask, shift, shift_sym};
    use fission_solver::ast::{SymExpr, SymNodeId};
    use std::collections::HashMap;

    /// The definition, written independently: arbitrary precision, then
    /// truncate. `i128` holds every 64-bit value and every in-range shift.
    fn reference(kind: ShiftKind, value: u64, width: u32, count: u64) -> u64 {
        let v = value & mask(width);
        let signed = if width < 64 && v >> (width - 1) & 1 == 1 {
            i128::from(v) - (1i128 << width)
        } else if width == 64 {
            i128::from(v as i64)
        } else {
            i128::from(v)
        };
        let out: i128 = match kind {
            ShiftKind::Left => {
                if count >= 128 {
                    0
                } else {
                    i128::from(v) << count
                }
            }
            ShiftKind::LogicalRight => {
                if count >= 128 {
                    0
                } else {
                    i128::from(v) >> count
                }
            }
            ShiftKind::ArithmeticRight => signed >> count.min(127),
        };
        (out as u64) & mask(width)
    }

    #[test]
    fn concrete_shift_matches_the_arbitrary_precision_definition() {
        let widths = [1u32, 2, 4, 7, 8, 16, 31, 32, 63, 64];
        for kind in [
            ShiftKind::Left,
            ShiftKind::LogicalRight,
            ShiftKind::ArithmeticRight,
        ] {
            for &w in &widths {
                let m = mask(w);
                let values = [0, 1, m, m >> 1, (m >> 1) + 1, 0x5a5a_5a5a_5a5a_5a5a & m];
                let counts = [
                    0,
                    1,
                    u64::from(w) - 1,
                    u64::from(w),
                    u64::from(w) + 1,
                    63,
                    64,
                    65,
                    255,
                    u64::MAX,
                ];
                for &v in &values {
                    for &c in &counts {
                        assert_eq!(
                            shift(kind, v, w, c),
                            reference(kind, v, w, c),
                            "{kind:?} w={w} v=0x{v:x} c={c}"
                        );
                    }
                }
            }
        }
    }

    /// Exhaustive at small widths: every value, every count at a narrower,
    /// equal and wider count width, through the witness evaluator.
    #[test]
    fn symbolic_shift_matches_concrete_shift_exhaustively_at_small_widths() {
        for kind in [
            ShiftKind::Left,
            ShiftKind::LogicalRight,
            ShiftKind::ArithmeticRight,
        ] {
            for w in 1u32..=6 {
                for cw in 1u32..=8 {
                    let x = SymExpr::new_var("x", w);
                    let c = SymExpr::new_var("c", cw);
                    let (SymExpr::Var { id: xi, .. }, SymExpr::Var { id: ci, .. }) = (&x, &c)
                    else {
                        unreachable!()
                    };
                    let (xi, ci): (SymNodeId, SymNodeId) = (*xi, *ci);
                    let e = shift_sym(kind, x.clone(), w, c.clone(), cw);
                    assert_eq!(e.get_bit_width(), w);
                    for v in 0..=mask(w) {
                        for cv in 0..=mask(cw) {
                            let env = HashMap::from([(xi, v), (ci, cv)]);
                            let got = crate::symbolic::eval_sym_bv(&e, &env).expect("eval");
                            assert_eq!(
                                got,
                                shift(kind, v, w, cv),
                                "{kind:?} w={w} cw={cw} v={v} c={cv}"
                            );
                        }
                    }
                }
            }
        }
    }
}
