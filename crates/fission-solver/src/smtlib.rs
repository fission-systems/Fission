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
//! * Shift amounts of a different width than the value, floats and arrays are
//!   refused rather than approximated.

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

/// A complete QF_BV script: declarations, one `assert` per assertion,
/// `check-sat`.
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

    let mut out = String::from("(set-logic QF_BV)\n");
    for (symbol, width) in &printer.declared {
        let _ = writeln!(out, "(declare-const {symbol} (_ BitVec {width}))");
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
                let high = lsb + size - 1;
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
