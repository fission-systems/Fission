//! Deciding what a guard means, rather than what it looks like.
//!
//! In shape-matching, the conditions are incidental -- a rule fires on the
//! graph. In condition-driven structuring they *are* the structure, so how much
//! structure can be recovered is bounded by how much the simplifier can prove.
//! That bound was visible: `simplify_logical_expr` does not reduce `c OR NOT c`,
//! the join of a diamond and the most common shape a reaching condition takes,
//! and every join in a region carried a tautology as its guard until
//! [`crate::reaching_conditions`] special-cased that one form syntactically.
//!
//! Special-casing forms does not scale. A guard is a boolean formula over a
//! handful of atoms, so evaluate it over every assignment of those atoms and
//! compare the results. `c OR NOT c` and `(a AND c) OR (a AND NOT c)` and
//! anything else with the same meaning all fall out of the same check, without
//! anyone having written the identity down.
//!
//! # Atoms, not variables
//!
//! An atom is any subexpression that is not one of `AND`/`OR`/`NOT` --
//! usually a guard variable, but a comparison or a call works the same way.
//! Two atoms are the same atom when they are structurally equal, which is
//! conservative in the right direction: mistaking one atom for two loses a
//! simplification, mistaking two for one would claim a formula is a tautology
//! when it is not.
//!
//! # Bounded on purpose
//!
//! The table is `2^n` bits for `n` atoms, so [`MAX_ATOMS`] caps it and every
//! query answers "don't know" past that rather than becoming expensive.
//! Callers treat "don't know" as "no simplification available", never as a
//! proof either way.

use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirUnaryOp};

/// Atom ceiling. Twelve atoms is a 4096-row table -- 64 `u64` words, evaluated
/// once per query. Reaching conditions in practice have far fewer; the
/// structuring driver caps a whole region at sixteen decisions.
pub const MAX_ATOMS: usize = 12;

/// A formula's value under every assignment of its atoms, one bit per row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthTable {
    rows: usize,
    words: Vec<u64>,
}

impl TruthTable {
    fn zeros(rows: usize) -> Self {
        Self {
            rows,
            words: vec![0u64; rows.div_ceil(64)],
        }
    }

    fn get(&self, row: usize) -> bool {
        self.words[row / 64] >> (row % 64) & 1 == 1
    }

    fn set(&mut self, row: usize) {
        self.words[row / 64] |= 1u64 << (row % 64);
    }

    /// True under every assignment.
    pub fn is_tautology(&self) -> bool {
        (0..self.rows).all(|r| self.get(r))
    }

    /// False under every assignment.
    pub fn is_contradiction(&self) -> bool {
        (0..self.rows).all(|r| !self.get(r))
    }

    /// True exactly where `other` is false.
    pub fn is_complement_of(&self, other: &Self) -> bool {
        self.rows == other.rows && (0..self.rows).all(|r| self.get(r) != other.get(r))
    }

    /// True everywhere `self` is true -- `self` implies `other`.
    pub fn implies(&self, other: &Self) -> bool {
        self.rows == other.rows && (0..self.rows).all(|r| !self.get(r) || other.get(r))
    }
}

/// Collect the atoms of `exprs`, in first-seen order. `None` past
/// [`MAX_ATOMS`].
fn atoms_of(exprs: &[&PreHirExpr]) -> Option<Vec<PreHirExpr>> {
    let mut atoms: Vec<PreHirExpr> = Vec::new();
    for e in exprs {
        collect(e, &mut atoms)?;
    }
    Some(atoms)
}

fn collect(e: &PreHirExpr, out: &mut Vec<PreHirExpr>) -> Option<()> {
    match e {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            collect(lhs, out)?;
            collect(rhs, out)
        }
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => collect(expr, out),
        // A constant is decided without an atom.
        PreHirExpr::Const(..) => Some(()),
        atom => {
            if !out.iter().any(|a| a == atom) {
                if out.len() >= MAX_ATOMS {
                    return None;
                }
                out.push(atom.clone());
            }
            Some(())
        }
    }
}

/// A formula with each atom resolved to its column once.
///
/// Evaluating the `PreHirExpr` directly looked every atom up by structural
/// equality against the atom list on every row -- up to 4,096 rows times
/// every leaf, each a deep comparison -- which made truth tables a fifth of
/// structuring's time on the full DecBench dataset. Resolved, a table is a
/// few word operations per node.
enum Formula {
    Const(bool),
    Atom(usize),
    Not(Box<Formula>),
    And(Box<Formula>, Box<Formula>),
    Or(Box<Formula>, Box<Formula>),
}

fn compile(e: &PreHirExpr, atoms: &[PreHirExpr]) -> Formula {
    match e {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => Formula::And(Box::new(compile(lhs, atoms)), Box::new(compile(rhs, atoms))),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => Formula::Or(Box::new(compile(lhs, atoms)), Box::new(compile(rhs, atoms))),
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => Formula::Not(Box::new(compile(expr, atoms))),
        PreHirExpr::Const(v, _) => Formula::Const(*v != 0),
        // An atom missing from the list reads as false, as it always has.
        atom => atoms
            .iter()
            .position(|a| a == atom)
            .map_or(Formula::Const(false), Formula::Atom),
    }
}

/// Rows `64 * word ..` of atom `index`'s column: row `r` holds bit `index`
/// of `r`.
fn atom_column(index: usize, word: usize) -> u64 {
    const LOW: [u64; 6] = [
        0xAAAA_AAAA_AAAA_AAAA,
        0xCCCC_CCCC_CCCC_CCCC,
        0xF0F0_F0F0_F0F0_F0F0,
        0xFF00_FF00_FF00_FF00,
        0xFFFF_0000_FFFF_0000,
        0xFFFF_FFFF_0000_0000,
    ];
    if index < 6 {
        LOW[index]
    } else if (word >> (index - 6)) & 1 == 1 {
        u64::MAX
    } else {
        0
    }
}

fn eval_word(f: &Formula, word: usize) -> u64 {
    match f {
        Formula::Const(true) => u64::MAX,
        Formula::Const(false) => 0,
        Formula::Atom(index) => atom_column(*index, word),
        Formula::Not(inner) => !eval_word(inner, word),
        Formula::And(lhs, rhs) => eval_word(lhs, word) & eval_word(rhs, word),
        Formula::Or(lhs, rhs) => eval_word(lhs, word) | eval_word(rhs, word),
    }
}

/// Tabulate `expr` over its own atoms.
pub fn tabulate(expr: &PreHirExpr) -> Option<TruthTable> {
    let atoms = atoms_of(&[expr])?;
    Some(tabulate_over(expr, &atoms))
}

fn tabulate_over(expr: &PreHirExpr, atoms: &[PreHirExpr]) -> TruthTable {
    let rows = 1usize << atoms.len();
    let mut table = TruthTable::zeros(rows);
    let formula = compile(expr, atoms);
    let valid = if rows < 64 {
        (1u64 << rows) - 1
    } else {
        u64::MAX
    };
    for (word, slot) in table.words.iter_mut().enumerate() {
        *slot = eval_word(&formula, word) & valid;
    }
    table
}

/// Tabulate two formulas over one shared atom ordering.
///
/// Two tables only mean anything against each other when their rows stand for
/// the same assignments, which means one atom list built from both formulas.
/// Building it per formula silently indexes them differently: `a` and `NOT b`
/// came out as complements, because each was tabulated with its own variable
/// in position zero.
fn tabulate_pair(a: &PreHirExpr, b: &PreHirExpr) -> Option<(TruthTable, TruthTable)> {
    let atoms = atoms_of(&[a, b])?;
    Some((tabulate_over(a, &atoms), tabulate_over(b, &atoms)))
}

/// Whether `expr` holds under every assignment of its atoms.
///
/// `false` also covers "could not decide" -- a caller must never read this as
/// a proof that the formula is *not* a tautology.
pub fn is_tautology(expr: &PreHirExpr) -> bool {
    tabulate(expr).is_some_and(|t| t.is_tautology())
}

/// Whether `expr` is false under every assignment. Same caveat as
/// [`is_tautology`].
pub fn is_contradiction(expr: &PreHirExpr) -> bool {
    tabulate(expr).is_some_and(|t| t.is_contradiction())
}

/// Whether `a` and `b` are exact complements -- one holds precisely when the
/// other does not, so what they guard are the two arms of one decision.
pub fn are_complementary(a: &PreHirExpr, b: &PreHirExpr) -> bool {
    let Some((ta, tb)) = tabulate_pair(a, b) else {
        return false;
    };
    ta.is_complement_of(&tb)
}

/// Whether `a` holding forces `b` to hold.
pub fn implies(a: &PreHirExpr, b: &PreHirExpr) -> bool {
    let Some((ta, tb)) = tabulate_pair(a, b) else {
        return false;
    };
    ta.implies(&tb)
}

/// Whether `a` and `b` mean the same thing, however they are written.
pub fn are_equivalent(a: &PreHirExpr, b: &PreHirExpr) -> bool {
    let Some((ta, tb)) = tabulate_pair(a, b) else {
        return false;
    };
    ta == tb
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::util::negate_expr;

    fn var(n: &str) -> PreHirExpr {
        PreHirExpr::Var(n.to_string())
    }
    fn and(a: PreHirExpr, b: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs: Box::new(a),
            rhs: Box::new(b),
            ty: NirType::Bool,
        }
    }
    fn or(a: PreHirExpr, b: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
            lhs: Box::new(a),
            rhs: Box::new(b),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn the_join_of_a_diamond_is_a_tautology_however_it_is_written() {
        let c = var("c");
        assert!(is_tautology(&or(c.clone(), negate_expr(c.clone()))));

        // The same with a guard in front, which the syntactic fold in
        // `reaching_conditions` had to be told about separately.
        let a = var("a");
        let guarded = or(
            and(a.clone(), c.clone()),
            and(a.clone(), negate_expr(c.clone())),
        );
        assert!(!is_tautology(&guarded), "it still depends on a");
        assert!(are_equivalent(&guarded, &a), "but it is exactly a");
    }

    #[test]
    fn complements_are_recognised_through_rewriting() {
        let (a, b) = (var("a"), var("b"));
        // De Morgan: NOT(a AND b) against (NOT a) OR (NOT b) are the same, so
        // `a AND b` and `(NOT a) OR (NOT b)` are complements -- which no
        // syntactic check catches.
        let lhs = and(a.clone(), b.clone());
        let rhs = or(negate_expr(a), negate_expr(b));
        assert!(are_complementary(&lhs, &rhs));
    }

    #[test]
    fn unrelated_conditions_are_not_complementary() {
        // The failure that matters: claiming two arms are exclusive when they
        // are not would merge code that can both run.
        assert!(!are_complementary(&var("a"), &negate_expr(var("b"))));
        assert!(!are_complementary(&var("a"), &var("b")));
    }

    #[test]
    fn a_narrower_guard_implies_the_wider_one() {
        let (a, b) = (var("a"), var("b"));
        assert!(implies(&and(a.clone(), b.clone()), &a));
        assert!(!implies(&a, &and(a.clone(), b)));
    }

    #[test]
    fn contradictions_are_decided_too() {
        let c = var("c");
        assert!(is_contradiction(&and(c.clone(), negate_expr(c))));
        assert!(!is_contradiction(&var("a")));
    }

    #[test]
    fn an_atom_is_whatever_is_not_a_connective() {
        // A comparison is an atom like any other, so this is the same
        // tautology as `c OR NOT c` even though there is no variable in it.
        let cmp = PreHirExpr::Binary {
            op: PreHirBinaryOp::Lt,
            lhs: Box::new(var("x")),
            rhs: Box::new(PreHirExpr::Const(10, NirType::Bool)),
            ty: NirType::Bool,
        };
        assert!(is_tautology(&or(cmp.clone(), negate_expr(cmp))));
    }

    /// The row-at-a-time evaluation the word-parallel one replaced, kept as
    /// the oracle it must agree with.
    fn eval_row(e: &PreHirExpr, atoms: &[PreHirExpr], row: usize) -> bool {
        match e {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => eval_row(lhs, atoms, row) && eval_row(rhs, atoms, row),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => eval_row(lhs, atoms, row) || eval_row(rhs, atoms, row),
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                expr,
                ..
            } => !eval_row(expr, atoms, row),
            PreHirExpr::Const(v, _) => *v != 0,
            atom => atoms
                .iter()
                .position(|a| a == atom)
                .is_some_and(|i| row >> i & 1 == 1),
        }
    }

    #[test]
    fn word_parallel_tables_agree_with_row_evaluation() {
        // A deterministic pseudo-random formula generator over up to twelve
        // atoms, so every column shape -- inside one word and across words --
        // and both constants are exercised.
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        fn build(next: &mut dyn FnMut() -> u64, depth: u32, atoms: u64) -> PreHirExpr {
            let pick = next() % if depth == 0 { 2 } else { 5 };
            match pick {
                0 => var(&format!("a{}", next() % atoms)),
                1 if next() % 7 == 0 => PreHirExpr::Const((next() % 2) as i64, NirType::Bool),
                1 => var(&format!("a{}", next() % atoms)),
                2 => negate_expr(build(next, depth - 1, atoms)),
                3 => and(build(next, depth - 1, atoms), build(next, depth - 1, atoms)),
                _ => or(build(next, depth - 1, atoms), build(next, depth - 1, atoms)),
            }
        }
        for case in 0..400 {
            let atom_count = 1 + (case % MAX_ATOMS) as u64;
            let expr = build(&mut next, 6, atom_count);
            let Some(table) = tabulate(&expr) else {
                continue;
            };
            let atoms = atoms_of(&[&expr]).expect("tabulated");
            for row in 0..table.rows {
                assert_eq!(
                    table.get(row),
                    eval_row(&expr, &atoms, row),
                    "row {row} of {expr:?}"
                );
            }
        }
    }

    #[test]
    fn too_many_atoms_decides_nothing_rather_than_guessing() {
        let mut e = var("a0");
        for i in 1..=MAX_ATOMS {
            e = or(e, var(&format!("a{i}")));
        }
        assert!(!is_tautology(&e), "undecided reads as no simplification");
        assert!(tabulate(&e).is_none());
    }
}
