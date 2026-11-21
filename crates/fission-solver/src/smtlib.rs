//! SMT-LIB 2 output for the bitvector fragment.
//!
//! This exists so the solver can be checked against another one. A formula
//! built here is solved natively and also printed for an external solver --
//! z3, run offline by a test harness, never linked -- and the two answers are
//! compared. A SAT answer checks itself (evaluate the model); an UNSAT answer
//! cannot, which is what the second opinion is for.
//!
//! The printout has to mean what the AIG lowering means, not merely what the
//! node names suggest, or the comparison measures the printer:
//!
//! * Comparisons here produce a one-bit bitvector, where SMT-LIB separates
//!   `Bool` from `(_ BitVec 1)`. A comparison prints as `(ite (bvult a b) #b1
//!   #b0)`, a condition as `(= c #b1)`, and an assertion as `(= e #b1)`.
//! * Lowering pads the narrower operand with zeros; the printer
//!   zero-extends to the same width.
//! * Arrays print as `(Array (_ BitVec d) (_ BitVec r))` with `select`,
//!   `store` and an array-sorted `ite`, and switch the logic to `QF_ABV`. An
//!   index or stored value whose width differs from the array's sort is
//!   refused: SMT-LIB has no such term, so there is nothing to compare with.
//! * Shift amounts of a different width than the value and floats are refused
//!   rather than approximated.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ast::{Sort, SymExpr};

/// A construct this printer does not translate, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not expressible in QF_BV here: {}", self.0)
    }
}

impl std::error::Error for Unsupported {}

/// A complete script: declarations, one `assert` per assertion, `check-sat`.
/// The logic is `QF_BV`, or `QF_ABV` when the assertions read an array.
pub fn script(assertions: &[SymExpr]) -> Result<String, Unsupported> {
    let mut printer = Printer::default();
    let mut body = String::new();
    for assertion in assertions {
        let (text, width) = printer.bitvector(assertion)?;
        if width != 1 {
            return Err(Unsupported(format!("an assertion {width} bits wide")));
        }
        let _ = writeln!(body, "(assert (= {text} #b1))");
    }

    let mut out = String::from(if printer.declared_arrays.is_empty() {
        "(set-logic QF_BV)\n"
    } else {
        "(set-logic QF_ABV)\n"
    });
    for (symbol, width) in &printer.declared {
        let _ = writeln!(out, "(declare-const {symbol} (_ BitVec {width}))");
    }
    for (symbol, (domain, range)) in &printer.declared_arrays {
        let _ = writeln!(
            out,
            "(declare-const {symbol} (Array (_ BitVec {domain}) (_ BitVec {range})))"
        );
    }
    out.push_str(&body);
    out.push_str("(check-sat)\n");
    Ok(out)
}

/// The SMT-LIB symbol a variable is declared under. Stable, so a model
/// printed by the external solver can be matched back to the variable.
pub fn symbol_for(expr: &SymExpr) -> Option<String> {
    match expr {
        SymExpr::Var { id, .. } => Some(format!("v{id}")),
        _ => None,
    }
}

#[derive(Default)]
struct Printer {
    declared: BTreeMap<String, u32>,
    /// Array symbols with their index and element widths.
    declared_arrays: BTreeMap<String, (u32, u32)>,
}

impl Printer {
    /// The term and its width in bits.
    fn bitvector(&mut self, expr: &SymExpr) -> Result<(String, u32), Unsupported> {
        use SymExpr::*;
        Ok(match expr {
            Const { val, size } => {
                let width = (*size).max(1);
                let masked = if width >= 64 {
                    *val
                } else {
                    val & ((1u64 << width) - 1)
                };
                (format!("(_ bv{masked} {width})"), width)
            }
            Var { sort, .. } => {
                let Sort::BitVector(width) = sort else {
                    return Err(Unsupported(format!("a variable of sort {sort:?}")));
                };
                let symbol = symbol_for(expr).expect("a variable has a symbol");
                self.declared.insert(symbol.clone(), *width);
                (symbol, *width)
            }

            Add(a, b) => self.binary("bvadd", a, b)?,
            Sub(a, b) => self.binary("bvsub", a, b)?,
            Mul(a, b) => self.binary("bvmul", a, b)?,
            Udiv(a, b) => self.binary("bvudiv", a, b)?,
            Urem(a, b) => self.binary("bvurem", a, b)?,
            Sdiv(a, b) => self.signed_binary("bvsdiv", a, b)?,
            Srem(a, b) => self.signed_binary("bvsrem", a, b)?,
            Smod(a, b) => self.signed_binary("bvsmod", a, b)?,
            And(a, b) => self.binary("bvand", a, b)?,
            Or(a, b) => self.binary("bvor", a, b)?,
            Xor(a, b) => self.binary("bvxor", a, b)?,

            Shl(a, b) => self.shift("bvshl", a, b)?,
            Lshr(a, b) => self.shift("bvlshr", a, b)?,
            Ashr(a, b) => self.shift("bvashr", a, b)?,

            Eq(a, b) => self.comparison("=", a, b, false)?,
            Neq(a, b) => {
                let (inner, _) = self.comparison("=", a, b, false)?;
                (format!("(bvnot {inner})"), 1)
            }
            Ult(a, b) => self.comparison("bvult", a, b, false)?,
            Ule(a, b) => self.comparison("bvule", a, b, false)?,
            Slt(a, b) => self.comparison("bvslt", a, b, true)?,
            Sle(a, b) => self.comparison("bvsle", a, b, true)?,
            Sgt(a, b) => self.comparison("bvsgt", a, b, true)?,

            Ite { cond, t, f } => {
                let (c, c_width) = self.bitvector(cond)?;
                if c_width != 1 {
                    return Err(Unsupported(format!("a {c_width}-bit condition")));
                }
                let ((t, f), width) = self.aligned(t, f)?;
                (format!("(ite (= {c} #b1) {t} {f})"), width)
            }
            Extract {
                expr: inner,
                lsb,
                size,
            } => {
                let (text, width) = self.bitvector(inner)?;
                if *size == 0 {
                    return Err(Unsupported("zero-width extract".to_string()));
                }
                let Some(high) = lsb.checked_add(*size - 1) else {
                    return Err(Unsupported(format!(
                        "extract index overflow for lsb {lsb} and size {size}"
                    )));
                };
                if high >= width {
                    return Err(Unsupported(format!(
                        "extract [{high}:{lsb}] from {width} bits"
                    )));
                }
                (format!("((_ extract {high} {lsb}) {text})"), *size)
            }
            Concat(a, b) => {
                let (a, a_width) = self.bitvector(a)?;
                let (b, b_width) = self.bitvector(b)?;
                (format!("(concat {a} {b})"), a_width + b_width)
            }
            ArraySelect { array, index } => {
                let (array, domain, range) = self.array(array)?;
                let (index, index_width) = self.bitvector(index)?;
                if index_width != domain {
                    return Err(Unsupported(format!(
                        "a {index_width}-bit index into an array indexed by {domain} bits"
                    )));
                }
                (format!("(select {array} {index})"), range)
            }

            other => {
                let debug = format!("{other:?}");
                let name = debug
                    .split(|c: char| !c.is_alphanumeric())
                    .next()
                    .unwrap_or("?");
                return Err(Unsupported(name.to_string()));
            }
        })
    }

    /// An array-sorted term, with its index and element widths.
    fn array(&mut self, expr: &SymExpr) -> Result<(String, u32, u32), Unsupported> {
        use SymExpr::*;
        Ok(match expr {
            Var { sort, .. } => {
                let Sort::Array { domain, range } = sort else {
                    return Err(Unsupported(format!("a {sort:?} used as an array")));
                };
                let (Sort::BitVector(domain), Sort::BitVector(range)) =
                    (domain.as_ref(), range.as_ref())
                else {
                    return Err(Unsupported(format!("an array of sort {sort:?}")));
                };
                let symbol = symbol_for(expr).expect("a variable has a symbol");
                self.declared_arrays
                    .insert(symbol.clone(), (*domain, *range));
                (symbol, *domain, *range)
            }
            ArrayStore {
                array,
                index,
                value,
            } => {
                let (array, domain, range) = self.array(array)?;
                let (index, index_width) = self.bitvector(index)?;
                let (value, value_width) = self.bitvector(value)?;
                if index_width != domain || value_width != range {
                    return Err(Unsupported(format!(
                        "a store of {value_width} bits at a {index_width}-bit index into \
                         an array from {domain} to {range} bits"
                    )));
                }
                (format!("(store {array} {index} {value})"), domain, range)
            }
            Ite { cond, t, f } => {
                let (c, c_width) = self.bitvector(cond)?;
                if c_width != 1 {
                    return Err(Unsupported(format!("a {c_width}-bit condition")));
                }
                let (t, t_domain, t_range) = self.array(t)?;
                let (f, f_domain, f_range) = self.array(f)?;
                if (t_domain, t_range) != (f_domain, f_range) {
                    return Err(Unsupported("an ite of two array sorts".to_string()));
                }
                (format!("(ite (= {c} #b1) {t} {f})"), t_domain, t_range)
            }
            other => {
                let debug = format!("{other:?}");
                let name = debug
                    .split(|c: char| !c.is_alphanumeric())
                    .next()
                    .unwrap_or("?");
                return Err(Unsupported(format!("{name} used as an array")));
            }
        })
    }

    /// Both operands at the same width, zero-extending the narrower one --
    /// which is what lowering does when it pads with `FALSE`.
    fn aligned(
        &mut self,
        a: &SymExpr,
        b: &SymExpr,
    ) -> Result<((String, String), u32), Unsupported> {
        let (a, a_width) = self.bitvector(a)?;
        let (b, b_width) = self.bitvector(b)?;
        let width = a_width.max(b_width);
        let extend = |text: String, from: u32| {
            if from == width {
                text
            } else {
                format!("((_ zero_extend {}) {text})", width - from)
            }
        };
        Ok(((extend(a, a_width), extend(b, b_width)), width))
    }

    fn binary(&mut self, op: &str, a: &SymExpr, b: &SymExpr) -> Result<(String, u32), Unsupported> {
        let ((a, b), width) = self.aligned(a, b)?;
        Ok((format!("({op} {a} {b})"), width))
    }

    /// Signed operations read the top bit as the sign, so zero-extending a
    /// narrower operand would change its value. Refused rather than guessed.
    fn signed_binary(
        &mut self,
        op: &str,
        a: &SymExpr,
        b: &SymExpr,
    ) -> Result<(String, u32), Unsupported> {
        let (a, a_width) = self.bitvector(a)?;
        let (b, b_width) = self.bitvector(b)?;
        if a_width != b_width {
            return Err(Unsupported(format!("{op} of {a_width} and {b_width} bits")));
        }
        Ok((format!("({op} {a} {b})"), a_width))
    }

    fn shift(&mut self, op: &str, a: &SymExpr, b: &SymExpr) -> Result<(String, u32), Unsupported> {
        let (a, a_width) = self.bitvector(a)?;
        let (b, b_width) = self.bitvector(b)?;
        if a_width != b_width {
            return Err(Unsupported(format!(
                "{op} of a {a_width}-bit value by a {b_width}-bit amount"
            )));
        }
        Ok((format!("({op} {a} {b})"), a_width))
    }

    fn comparison(
        &mut self,
        op: &str,
        a: &SymExpr,
        b: &SymExpr,
        signed: bool,
    ) -> Result<(String, u32), Unsupported> {
        let (a, b) = if signed {
            let (a, a_width) = self.bitvector(a)?;
            let (b, b_width) = self.bitvector(b)?;
            if a_width != b_width {
                return Err(Unsupported(format!("{op} of {a_width} and {b_width} bits")));
            }
            (a, b)
        } else {
            self.aligned(a, b)?.0
        };
        Ok((format!("(ite ({op} {a} {b}) #b1 #b0)"), 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One comparison, printed the way lowering means it: a one-bit result
    /// asserted equal to `#b1`, the constant masked to its width.
    #[test]
    fn a_comparison_prints_as_a_one_bit_bitvector() {
        let x = SymExpr::new_var("x", 4);
        let symbol = symbol_for(&x).expect("symbol");
        let lt = SymExpr::Ult(Box::new(x), Box::new(SymExpr::new_const(0x1F, 4)));
        let script = script(&[lt]).expect("printable");
        assert_eq!(
            script,
            format!(
                "(set-logic QF_BV)\n\
                 (declare-const {symbol} (_ BitVec 4))\n\
                 (assert (= (ite (bvult {symbol} (_ bv15 4)) #b1 #b0) #b1))\n\
                 (check-sat)\n"
            )
        );
    }

    /// Lowering pads a narrower operand with zeros, so the printer does too.
    #[test]
    fn a_narrower_unsigned_operand_is_zero_extended() {
        let a = SymExpr::new_var("a", 8);
        let b = SymExpr::new_var("b", 4);
        let (a_sym, b_sym) = (symbol_for(&a).unwrap(), symbol_for(&b).unwrap());
        let eq = SymExpr::Eq(
            Box::new(SymExpr::Add(Box::new(a), Box::new(b))),
            Box::new(SymExpr::new_const(3, 8)),
        );
        let script = script(&[eq]).expect("printable");
        assert!(
            script.contains(&format!("(bvadd {a_sym} ((_ zero_extend 4) {b_sym}))")),
            "{script}"
        );
    }

    #[test]
    fn invalid_extract_bounds_are_refused_without_arithmetic_panics() {
        let inner = SymExpr::new_var("x", 8);

        let zero_width = SymExpr::Extract {
            expr: Box::new(inner.clone()),
            lsb: 0,
            size: 0,
        };
        assert_eq!(
            Printer::default().bitvector(&zero_width),
            Err(Unsupported("zero-width extract".to_string()))
        );

        let overflowing = SymExpr::Extract {
            expr: Box::new(inner),
            lsb: u32::MAX,
            size: 2,
        };
        assert_eq!(
            Printer::default().bitvector(&overflowing),
            Err(Unsupported(format!(
                "extract index overflow for lsb {} and size 2",
                u32::MAX
            )))
        );
    }

    /// A read through a store chain prints as `QF_ABV`, with the array
    /// declared at its own sort.
    #[test]
    fn a_read_over_a_write_prints_as_an_array_term() {
        let memory = SymExpr::new_array_var("m", 4, 8);
        let (i, j) = (SymExpr::new_var("i", 4), SymExpr::new_var("j", 4));
        let (m_sym, i_sym, j_sym) = (
            symbol_for(&memory).unwrap(),
            symbol_for(&i).unwrap(),
            symbol_for(&j).unwrap(),
        );
        let written = SymExpr::ArrayStore {
            array: Box::new(memory),
            index: Box::new(i),
            value: Box::new(SymExpr::new_const(7, 8)),
        };
        let read = SymExpr::ArraySelect {
            array: Box::new(written),
            index: Box::new(j),
        };
        let eq = SymExpr::Eq(Box::new(read), Box::new(SymExpr::new_const(7, 8)));
        let script = script(&[eq]).expect("printable");
        assert!(script.starts_with("(set-logic QF_ABV)\n"), "{script}");
        assert!(
            script.contains(&format!(
                "(declare-const {m_sym} (Array (_ BitVec 4) (_ BitVec 8)))"
            )),
            "{script}"
        );
        assert!(
            script.contains(&format!(
                "(select (store {m_sym} {i_sym} (_ bv7 8)) {j_sym})"
            )),
            "{script}"
        );
    }

    /// SMT-LIB has no select whose index is not the array's index sort, so
    /// there is no script that means the same thing.
    #[test]
    fn an_index_of_the_wrong_width_is_refused() {
        let memory = SymExpr::new_array_var("m", 4, 8);
        let read = SymExpr::ArraySelect {
            array: Box::new(memory.clone()),
            index: Box::new(SymExpr::new_var("wide", 8)),
        };
        let eq = SymExpr::Eq(Box::new(read), Box::new(SymExpr::new_const(0, 8)));
        assert!(script(&[eq]).is_err(), "8-bit index into a 4-bit domain");

        let stored = SymExpr::ArrayStore {
            array: Box::new(memory),
            index: Box::new(SymExpr::new_var("i", 4)),
            value: Box::new(SymExpr::new_var("wide_value", 16)),
        };
        let read = SymExpr::ArraySelect {
            array: Box::new(stored),
            index: Box::new(SymExpr::new_var("j", 4)),
        };
        let eq = SymExpr::Eq(Box::new(read), Box::new(SymExpr::new_const(0, 8)));
        assert!(script(&[eq]).is_err(), "16-bit value into 8-bit elements");
    }

    /// Zero-extending a signed operand would change its value, so it is
    /// refused -- as are an assertion that is not one bit and a float.
    #[test]
    fn what_cannot_be_printed_faithfully_is_refused() {
        let wide = SymExpr::new_var("w", 8);
        let narrow = SymExpr::new_var("n", 4);
        let signed = SymExpr::Slt(Box::new(wide.clone()), Box::new(narrow));
        assert!(script(&[signed]).is_err(), "mixed-width signed comparison");

        assert!(
            script(std::slice::from_ref(&wide)).is_err(),
            "an 8-bit assertion"
        );

        let float = SymExpr::FAdd(Box::new(wide.clone()), Box::new(wide));
        let eq = SymExpr::Eq(Box::new(float), Box::new(SymExpr::new_const(0, 8)));
        assert!(script(&[eq]).is_err(), "float arithmetic");
    }
}
