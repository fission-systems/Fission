//! Folding constructors, concrete evaluation, substitution and printing for
//! the [`SymExpr`] terms this crate builds.
//!
//! Every constructor here folds a node whose operands are all constants by
//! *evaluating* it with [`eval`], so folding, concrete replay of a quotient,
//! and the solver read one definition. The operators follow SMT-LIB QF_BV:
//! a shift by at least the width gives zero (or the sign fill), comparisons
//! give a one-bit value. Operands of a binary operator always share a width;
//! a mismatch is a construction bug and panics rather than letting the
//! solver's widening rule silently pick a meaning.

use std::collections::{BTreeMap, BTreeSet};

use fission_solver::{SymExpr, SymNodeId};

pub(crate) const fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else if bits == 0 {
        0
    } else {
        (1u64 << bits) - 1
    }
}

pub(crate) fn konst(value: u64, bits: u32) -> SymExpr {
    SymExpr::new_const(value, bits)
}

pub(crate) fn as_const(expr: &SymExpr) -> Option<u64> {
    match expr {
        SymExpr::Const { val, size } => Some(val & mask(*size)),
        _ => None,
    }
}

pub(crate) fn width(expr: &SymExpr) -> u32 {
    expr.get_bit_width()
}

fn signed(value: u64, bits: u32) -> i128 {
    let value = value & mask(bits);
    if bits == 0 || bits > 64 {
        return i128::from(value);
    }
    if value & (1u64 << (bits - 1)) == 0 {
        i128::from(value)
    } else {
        i128::from(value) - (1i128 << bits)
    }
}

/// Binary operators the builder emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bin {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    Lshr,
    Ashr,
    Eq,
    Neq,
    Ult,
    Ule,
    Slt,
    Sle,
}

impl Bin {
    fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Neq | Self::Ult | Self::Ule | Self::Slt | Self::Sle
        )
    }

    fn apply(self, a: u64, b: u64, bits: u32) -> u64 {
        let m = mask(bits);
        let (a, b) = (a & m, b & m);
        match self {
            Self::Add => a.wrapping_add(b) & m,
            Self::Sub => a.wrapping_sub(b) & m,
            Self::Mul => a.wrapping_mul(b) & m,
            Self::And => a & b,
            Self::Or => a | b,
            Self::Xor => a ^ b,
            Self::Shl => {
                if b >= u64::from(bits) {
                    0
                } else {
                    (a << b) & m
                }
            }
            Self::Lshr => {
                if b >= u64::from(bits) {
                    0
                } else {
                    a >> b
                }
            }
            Self::Ashr => {
                // Past the width, every bit is the sign.
                let shift = if b >= u64::from(bits) {
                    bits - 1
                } else {
                    b as u32
                };
                ((signed(a, bits) >> shift) as u64) & m
            }
            Self::Eq => u64::from(a == b),
            Self::Neq => u64::from(a != b),
            Self::Ult => u64::from(a < b),
            Self::Ule => u64::from(a <= b),
            Self::Slt => u64::from(signed(a, bits) < signed(b, bits)),
            Self::Sle => u64::from(signed(a, bits) <= signed(b, bits)),
        }
    }

    fn node(self, a: SymExpr, b: SymExpr) -> SymExpr {
        let (a, b) = (Box::new(a), Box::new(b));
        match self {
            Self::Add => SymExpr::Add(a, b),
            Self::Sub => SymExpr::Sub(a, b),
            Self::Mul => SymExpr::Mul(a, b),
            Self::And => SymExpr::And(a, b),
            Self::Or => SymExpr::Or(a, b),
            Self::Xor => SymExpr::Xor(a, b),
            Self::Shl => SymExpr::Shl(a, b),
            Self::Lshr => SymExpr::Lshr(a, b),
            Self::Ashr => SymExpr::Ashr(a, b),
            Self::Eq => SymExpr::Eq(a, b),
            Self::Neq => SymExpr::Neq(a, b),
            Self::Ult => SymExpr::Ult(a, b),
            Self::Ule => SymExpr::Ule(a, b),
            Self::Slt => SymExpr::Slt(a, b),
            Self::Sle => SymExpr::Sle(a, b),
        }
    }
}

/// Build `a op b`, folding constants and a few identities that keep guards
/// readable. Every rewrite is an identity of the operator, not a heuristic.
pub(crate) fn bin(op: Bin, a: SymExpr, b: SymExpr) -> SymExpr {
    let bits = width(&a);
    assert_eq!(
        bits,
        width(&b),
        "operands of {op:?} disagree on width: {a:?} vs {b:?}"
    );
    if let (Some(x), Some(y)) = (as_const(&a), as_const(&b)) {
        let out_bits = if op.is_comparison() { 1 } else { bits };
        return konst(op.apply(x, y, bits), out_bits);
    }
    let all = mask(bits);
    match op {
        Bin::Add | Bin::Or | Bin::Xor if as_const(&b) == Some(0) => return a,
        Bin::Add | Bin::Or | Bin::Xor if as_const(&a) == Some(0) => return b,
        Bin::Sub | Bin::Shl | Bin::Lshr | Bin::Ashr if as_const(&b) == Some(0) => return a,
        Bin::And if as_const(&b) == Some(all) => return a,
        Bin::And if as_const(&a) == Some(all) => return b,
        Bin::And if as_const(&a) == Some(0) || as_const(&b) == Some(0) => return konst(0, bits),
        Bin::Or if as_const(&a) == Some(all) || as_const(&b) == Some(all) => {
            return konst(all, bits);
        }
        Bin::Mul if as_const(&b) == Some(1) => return a,
        Bin::Mul if as_const(&a) == Some(1) => return b,
        Bin::Mul if as_const(&a) == Some(0) || as_const(&b) == Some(0) => return konst(0, bits),
        Bin::And | Bin::Or if a == b => return a,
        Bin::Xor | Bin::Sub if a == b => return konst(0, bits),
        Bin::Eq | Bin::Ule | Bin::Sle if a == b => return konst(1, 1),
        Bin::Neq | Bin::Ult | Bin::Slt if a == b => return konst(0, 1),
        _ => {}
    }
    if bits == 1 && matches!(op, Bin::Eq | Bin::Neq) {
        // A one-bit comparison against a constant is the operand or its
        // negation.
        let (value, constant) = match (as_const(&a), as_const(&b)) {
            (Some(c), None) => (b.clone(), Some(c)),
            (None, Some(c)) => (a.clone(), Some(c)),
            _ => (a.clone(), None),
        };
        if let Some(c) = constant {
            let same = (c == 1) == (op == Bin::Eq);
            return if same { value } else { not(value) };
        }
    }
    if matches!(op, Bin::Eq | Bin::Neq) {
        // `(x - y) == 0` is `x == y`; flag lowerings test differences.
        if as_const(&b) == Some(0) {
            if let SymExpr::Sub(x, y) | SymExpr::Xor(x, y) = &a {
                return bin(op, (**x).clone(), (**y).clone());
            }
        }
        // `ite(c, 1, 0) == 0` is `!c`, and `!= 0` is `c`.
        if let (Some(k), SymExpr::Ite { cond, t, f }) = (as_const(&b), &a)
            && let (Some(tv), Some(fv)) = (as_const(t), as_const(f))
            && tv != fv
        {
            let c = (**cond).clone();
            if k == tv {
                return if op == Bin::Eq { c } else { not(c) };
            }
            if k == fv {
                return if op == Bin::Eq { not(c) } else { c };
            }
            return konst(u64::from(op == Bin::Neq), 1);
        }
    }
    op.node(a, b)
}

/// Logical negation of a one-bit value.
pub(crate) fn not(value: SymExpr) -> SymExpr {
    assert_eq!(width(&value), 1, "not() takes a one-bit value");
    if let Some(c) = as_const(&value) {
        return konst(c ^ 1, 1);
    }
    match value {
        SymExpr::Eq(a, b) => SymExpr::Neq(a, b),
        SymExpr::Neq(a, b) => SymExpr::Eq(a, b),
        SymExpr::Xor(a, b) if as_const(&b) == Some(1) => *a,
        other => SymExpr::Xor(Box::new(other), Box::new(konst(1, 1))),
    }
}

pub(crate) fn and_all(terms: impl IntoIterator<Item = SymExpr>) -> SymExpr {
    terms
        .into_iter()
        .fold(konst(1, 1), |acc, term| bin(Bin::And, acc, term))
}

pub(crate) fn or_all(terms: impl IntoIterator<Item = SymExpr>) -> SymExpr {
    terms
        .into_iter()
        .fold(konst(0, 1), |acc, term| bin(Bin::Or, acc, term))
}

pub(crate) fn ite(cond: SymExpr, t: SymExpr, f: SymExpr) -> SymExpr {
    assert_eq!(width(&cond), 1, "ite condition must be one bit");
    assert_eq!(width(&t), width(&f), "ite arms disagree on width");
    match as_const(&cond) {
        Some(1) => t,
        Some(_) => f,
        None if t == f => t,
        None => SymExpr::Ite {
            cond: Box::new(cond),
            t: Box::new(t),
            f: Box::new(f),
        },
    }
}

/// `value != 0` as one bit, recovering the condition of a widened boolean.
pub(crate) fn truthy(value: SymExpr) -> SymExpr {
    if width(&value) == 1 {
        return value;
    }
    let bits = width(&value);
    bin(Bin::Neq, value, konst(0, bits))
}

/// A one-bit condition widened to `bits` as 0/1.
pub(crate) fn widen_bool(cond: SymExpr, bits: u32) -> SymExpr {
    if bits == 1 {
        return cond;
    }
    ite(cond, konst(1, bits), konst(0, bits))
}

pub(crate) fn extract(value: SymExpr, lsb: u32, size: u32) -> SymExpr {
    let bits = width(&value);
    assert!(
        size > 0 && lsb + size <= bits,
        "extract [{lsb}, +{size}) outside a {bits}-bit value"
    );
    if lsb == 0 && size == bits {
        return value;
    }
    if let Some(c) = as_const(&value) {
        return konst((c >> lsb) & mask(size), size);
    }
    match value {
        SymExpr::Extract {
            expr,
            lsb: inner,
            size: _,
        } => extract(*expr, inner + lsb, size),
        SymExpr::Concat(hi, lo) => {
            let lo_bits = width(&lo);
            if lsb + size <= lo_bits {
                extract(*lo, lsb, size)
            } else if lsb >= lo_bits {
                extract(*hi, lsb - lo_bits, size)
            } else {
                let low = extract((*lo).clone(), lsb, lo_bits - lsb);
                let high = extract(*hi, 0, lsb + size - lo_bits);
                concat(high, low)
            }
        }
        SymExpr::Ite { cond, t, f } => ite(*cond, extract(*t, lsb, size), extract(*f, lsb, size)),
        // Bitwise operators act on each bit alone, and the low bits of a
        // sum, difference or product depend only on the operands' low bits.
        // Narrowing first is what keeps a 32-bit operation on a 64-bit
        // register from reading the register's upper half.
        SymExpr::And(a, b) => bin(Bin::And, extract(*a, lsb, size), extract(*b, lsb, size)),
        SymExpr::Or(a, b) => bin(Bin::Or, extract(*a, lsb, size), extract(*b, lsb, size)),
        SymExpr::Xor(a, b) => bin(Bin::Xor, extract(*a, lsb, size), extract(*b, lsb, size)),
        SymExpr::Add(a, b) if lsb == 0 => bin(Bin::Add, extract(*a, 0, size), extract(*b, 0, size)),
        SymExpr::Sub(a, b) if lsb == 0 => bin(Bin::Sub, extract(*a, 0, size), extract(*b, 0, size)),
        SymExpr::Mul(a, b) if lsb == 0 => bin(Bin::Mul, extract(*a, 0, size), extract(*b, 0, size)),
        other => SymExpr::Extract {
            expr: Box::new(other),
            lsb,
            size,
        },
    }
}

pub(crate) fn concat(hi: SymExpr, lo: SymExpr) -> SymExpr {
    let lo_bits = width(&lo);
    let hi_bits = width(&hi);
    if let (Some(h), Some(l)) = (as_const(&hi), as_const(&lo)) {
        return konst((h << lo_bits) | l, hi_bits + lo_bits);
    }
    if let (
        SymExpr::Extract {
            expr: he,
            lsb: hl,
            size: hs,
        },
        SymExpr::Extract {
            expr: le,
            lsb: ll,
            size: ls,
        },
    ) = (&hi, &lo)
        && he == le
        && *hl == ll + ls
    {
        return extract((**le).clone(), *ll, ls + hs);
    }
    // Re-associate when two neighbouring pieces merge: a new piece adjoining
    // the near end of an existing run.
    if let SymExpr::Concat(run_hi, run_lo) = &hi
        && !matches!(lo, SymExpr::Concat(..))
    {
        let merged = concat((**run_lo).clone(), lo.clone());
        if !matches!(merged, SymExpr::Concat(..)) {
            return concat((**run_hi).clone(), merged);
        }
    }
    if let SymExpr::Concat(run_hi, run_lo) = &lo
        && !matches!(hi, SymExpr::Concat(..))
    {
        let merged = concat(hi.clone(), (**run_hi).clone());
        if !matches!(merged, SymExpr::Concat(..)) {
            return concat(merged, (**run_lo).clone());
        }
    }
    SymExpr::Concat(Box::new(hi), Box::new(lo))
}

pub(crate) fn zext(value: SymExpr, bits: u32) -> SymExpr {
    let from = width(&value);
    assert!(bits >= from, "zext to a narrower width");
    if bits == from {
        return value;
    }
    concat(konst(0, bits - from), value)
}

pub(crate) fn sext(value: SymExpr, bits: u32) -> SymExpr {
    let from = width(&value);
    assert!(bits >= from, "sext to a narrower width");
    if bits == from {
        return value;
    }
    if let Some(c) = as_const(&value) {
        return konst(signed(c, from) as u64, bits);
    }
    let sign = extract(value.clone(), from - 1, 1);
    let fill = ite(
        sign,
        konst(mask(bits - from), bits - from),
        konst(0, bits - from),
    );
    concat(fill, value)
}

/// Width-adjust `value` to `bits`, zero-extending or keeping the low bits.
pub(crate) fn resize(value: SymExpr, bits: u32) -> SymExpr {
    let from = width(&value);
    if from >= bits {
        extract(value, 0, bits)
    } else {
        zext(value, bits)
    }
}

/// Evaluate `expr` with each variable's value taken from `env`. `None` when a
/// variable is unbound or the term uses an operator this crate never builds.
pub(crate) fn eval(expr: &SymExpr, env: &BTreeMap<SymNodeId, u64>) -> Option<u64> {
    let bits = width(expr);
    let binary =
        |op: Bin, a: &SymExpr, b: &SymExpr| Some(op.apply(eval(a, env)?, eval(b, env)?, width(a)));
    let value = match expr {
        SymExpr::Const { val, size } => val & mask(*size),
        SymExpr::Var { id, .. } => *env.get(id)? & mask(bits),
        SymExpr::Add(a, b) => binary(Bin::Add, a, b)?,
        SymExpr::Sub(a, b) => binary(Bin::Sub, a, b)?,
        SymExpr::Mul(a, b) => binary(Bin::Mul, a, b)?,
        SymExpr::And(a, b) => binary(Bin::And, a, b)?,
        SymExpr::Or(a, b) => binary(Bin::Or, a, b)?,
        SymExpr::Xor(a, b) => binary(Bin::Xor, a, b)?,
        SymExpr::Shl(a, b) => binary(Bin::Shl, a, b)?,
        SymExpr::Lshr(a, b) => binary(Bin::Lshr, a, b)?,
        SymExpr::Ashr(a, b) => binary(Bin::Ashr, a, b)?,
        SymExpr::Eq(a, b) => binary(Bin::Eq, a, b)?,
        SymExpr::Neq(a, b) => binary(Bin::Neq, a, b)?,
        SymExpr::Ult(a, b) => binary(Bin::Ult, a, b)?,
        SymExpr::Ule(a, b) => binary(Bin::Ule, a, b)?,
        SymExpr::Slt(a, b) => binary(Bin::Slt, a, b)?,
        SymExpr::Sle(a, b) => binary(Bin::Sle, a, b)?,
        SymExpr::Ite { cond, t, f } => {
            if eval(cond, env)? & 1 == 1 {
                eval(t, env)?
            } else {
                eval(f, env)?
            }
        }
        SymExpr::Extract { expr, lsb, size } => (eval(expr, env)? >> lsb) & mask(*size),
        SymExpr::Concat(hi, lo) => {
            let lo_bits = width(lo);
            let low = eval(lo, env)?;
            let high = eval(hi, env)?;
            if lo_bits >= 64 {
                low
            } else {
                (high << lo_bits) | low
            }
        }
        _ => return None,
    };
    Some(value & mask(bits))
}

/// Replace variables by `map[id]`, folding as the tree is rebuilt.
pub(crate) fn subst(expr: &SymExpr, map: &BTreeMap<SymNodeId, SymExpr>) -> SymExpr {
    let s = |e: &SymExpr| subst(e, map);
    match expr {
        SymExpr::Const { .. } => expr.clone(),
        SymExpr::Var { id, .. } => map.get(id).cloned().unwrap_or_else(|| expr.clone()),
        SymExpr::Add(a, b) => bin(Bin::Add, s(a), s(b)),
        SymExpr::Sub(a, b) => bin(Bin::Sub, s(a), s(b)),
        SymExpr::Mul(a, b) => bin(Bin::Mul, s(a), s(b)),
        SymExpr::And(a, b) => bin(Bin::And, s(a), s(b)),
        SymExpr::Or(a, b) => bin(Bin::Or, s(a), s(b)),
        SymExpr::Xor(a, b) if width(a) == 1 && as_const(b) == Some(1) => not(s(a)),
        SymExpr::Xor(a, b) => bin(Bin::Xor, s(a), s(b)),
        SymExpr::Shl(a, b) => bin(Bin::Shl, s(a), s(b)),
        SymExpr::Lshr(a, b) => bin(Bin::Lshr, s(a), s(b)),
        SymExpr::Ashr(a, b) => bin(Bin::Ashr, s(a), s(b)),
        SymExpr::Eq(a, b) => bin(Bin::Eq, s(a), s(b)),
        SymExpr::Neq(a, b) => bin(Bin::Neq, s(a), s(b)),
        SymExpr::Ult(a, b) => bin(Bin::Ult, s(a), s(b)),
        SymExpr::Ule(a, b) => bin(Bin::Ule, s(a), s(b)),
        SymExpr::Slt(a, b) => bin(Bin::Slt, s(a), s(b)),
        SymExpr::Sle(a, b) => bin(Bin::Sle, s(a), s(b)),
        SymExpr::Ite { cond, t, f } => ite(s(cond), s(t), s(f)),
        SymExpr::Extract { expr, lsb, size } => extract(s(expr), *lsb, *size),
        SymExpr::Concat(hi, lo) => concat(s(hi), s(lo)),
        other => other.clone(),
    }
}

/// The variables `expr` mentions.
pub(crate) fn free_vars(expr: &SymExpr) -> BTreeSet<SymNodeId> {
    let mut out = BTreeSet::new();
    expr.visit_preorder(&mut |node| {
        if let SymExpr::Var { id, .. } = node {
            out.insert(*id);
        }
    });
    out
}

/// C-like rendering with variables named through `names`.
pub(crate) fn render(expr: &SymExpr, names: &BTreeMap<SymNodeId, String>) -> String {
    let r = |e: &SymExpr| render(e, names);
    let infix = |op: &str, a: &SymExpr, b: &SymExpr| format!("({} {op} {})", r(a), r(b));
    match expr {
        SymExpr::Const { val, size } => {
            if *size == 1 {
                if *val & 1 == 1 { "true" } else { "false" }.to_string()
            } else if *val < 10 {
                format!("{val}")
            } else {
                format!("0x{val:x}")
            }
        }
        SymExpr::Var { id, name, .. } => names.get(id).cloned().unwrap_or_else(|| name.clone()),
        SymExpr::Add(a, b) => infix("+", a, b),
        SymExpr::Sub(a, b) => infix("-", a, b),
        SymExpr::Mul(a, b) => infix("*", a, b),
        SymExpr::And(a, b) if width(a) == 1 => infix("&&", a, b),
        SymExpr::Or(a, b) if width(a) == 1 => infix("||", a, b),
        SymExpr::Xor(a, b) if width(a) == 1 && as_const(b) == Some(1) => format!("!{}", r(a)),
        SymExpr::And(a, b) => infix("&", a, b),
        SymExpr::Or(a, b) => infix("|", a, b),
        SymExpr::Xor(a, b) => infix("^", a, b),
        SymExpr::Shl(a, b) => infix("<<", a, b),
        SymExpr::Lshr(a, b) => infix(">>", a, b),
        SymExpr::Ashr(a, b) => infix(">>s", a, b),
        SymExpr::Eq(a, b) => infix("==", a, b),
        SymExpr::Neq(a, b) => infix("!=", a, b),
        SymExpr::Ult(a, b) => infix("<", a, b),
        SymExpr::Ule(a, b) => infix("<=", a, b),
        SymExpr::Slt(a, b) => infix("<s", a, b),
        SymExpr::Sle(a, b) => infix("<=s", a, b),
        SymExpr::Ite { cond, t, f } => format!("({} ? {} : {})", r(cond), r(t), r(f)),
        SymExpr::Extract { expr, lsb, size } => {
            if *size == 1 {
                format!("{}[{lsb}]", r(expr))
            } else {
                format!("{}[{}:{lsb}]", r(expr), lsb + size - 1)
            }
        }
        SymExpr::Concat(hi, lo) => {
            if as_const(hi) == Some(0) {
                format!("zext{}({})", width(expr), r(lo))
            } else {
                format!("concat({}, {})", r(hi), r(lo))
            }
        }
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use fission_solver::{SatResult, Solver};

    use super::*;

    fn var(name: &str, bits: u32) -> (SymExpr, SymNodeId) {
        let v = SymExpr::new_var(name, bits);
        let SymExpr::Var { id, .. } = v else {
            unreachable!()
        };
        (v, id)
    }

    /// The folding evaluator and the solver must read every operator the
    /// same way, or a folded guard and a solved one disagree.
    #[test]
    fn concrete_evaluation_agrees_with_the_solver() {
        let samples: [(u64, u64); 6] = [
            (0, 0),
            (1, 0xff),
            (0x80, 0x7f),
            (0xff, 7),
            (0x5a, 0xa5),
            (3, 9),
        ];
        let ops = [
            Bin::Add,
            Bin::Sub,
            Bin::Mul,
            Bin::And,
            Bin::Or,
            Bin::Xor,
            Bin::Shl,
            Bin::Lshr,
            Bin::Ashr,
            Bin::Eq,
            Bin::Neq,
            Bin::Ult,
            Bin::Ule,
            Bin::Slt,
            Bin::Sle,
        ];
        for op in ops {
            for (x, y) in samples {
                let (a, a_id) = var("a", 8);
                let (b, b_id) = var("b", 8);
                let term = op.node(a.clone(), b.clone());
                let env = BTreeMap::from([(a_id, x), (b_id, y)]);
                let want = eval(&term, &env).expect("evaluable");
                let mut solver = Solver::new();
                solver.assert(SymExpr::new_eq(a, konst(x, 8)));
                solver.assert(SymExpr::new_eq(b, konst(y, 8)));
                let out_bits = width(&term);
                solver.assert(SymExpr::new_neq(term, konst(want, out_bits)));
                assert_eq!(
                    solver.check_sat().unwrap(),
                    SatResult::Unsat,
                    "{op:?} on {x:#x}, {y:#x}: the solver disagrees with {want:#x}"
                );
            }
        }
    }

    /// Narrowing is pushed into sums, products and bitwise operators; the
    /// rewritten term must keep the original's value.
    #[test]
    fn narrowing_into_an_operation_keeps_its_value() {
        let (x, x_id) = var("x", 64);
        let (y, y_id) = var("y", 64);
        let terms = [
            SymExpr::Add(Box::new(x.clone()), Box::new(y.clone())),
            SymExpr::Sub(Box::new(x.clone()), Box::new(y.clone())),
            SymExpr::Mul(Box::new(x.clone()), Box::new(y.clone())),
            SymExpr::And(Box::new(x.clone()), Box::new(y.clone())),
            SymExpr::Or(Box::new(x.clone()), Box::new(y.clone())),
            SymExpr::Xor(Box::new(x.clone()), Box::new(y.clone())),
        ];
        let values = [
            (0u64, 0u64),
            (u64::MAX, 1),
            (0x1234_5678_9abc_def0, 0x0fed_cba9_8765_4321),
            (0x8000_0000_0000_0001, 0xffff_ffff_0000_0003),
        ];
        for term in terms {
            for (lsb, size) in [(0, 8), (0, 32), (8, 8), (16, 32), (32, 32)] {
                let narrowed = extract(term.clone(), lsb, size);
                for (a, b) in values {
                    let env = BTreeMap::from([(x_id, a), (y_id, b)]);
                    let whole = eval(&term, &env).unwrap();
                    assert_eq!(
                        eval(&narrowed, &env),
                        Some((whole >> lsb) & mask(size)),
                        "{term:?}[{lsb}+{size}] at {a:#x}, {b:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn adjacent_extracts_reassemble_their_source() {
        let (x, _) = var("x", 32);
        let bytes: Vec<_> = (0..4).map(|i| extract(x.clone(), i * 8, 8)).collect();
        let joined = bytes
            .into_iter()
            .reduce(|low, byte| concat(byte, low))
            .unwrap();
        assert_eq!(joined, x);
    }

    #[test]
    fn flag_shapes_fold_to_their_comparison() {
        let (x, _) = var("x", 8);
        let zf = widen_bool(
            bin(
                Bin::Eq,
                bin(Bin::Sub, x.clone(), konst(0x5a, 8)),
                konst(0, 8),
            ),
            8,
        );
        assert_eq!(
            truthy(zf.clone()),
            SymExpr::new_eq(x.clone(), konst(0x5a, 8))
        );
        let not_zf = bin(Bin::Eq, zf, konst(0, 8));
        assert_eq!(not_zf, SymExpr::new_neq(x, konst(0x5a, 8)));
    }
}
