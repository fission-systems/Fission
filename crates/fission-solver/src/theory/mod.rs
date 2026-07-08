pub mod bitvector;
pub mod array;

use crate::cnf::Lit;

/// Represents a conflict discovered by a Theory solver.
/// The `core` is the set of literals that caused the conflict.
/// If assignment {L1, L2, L3} is inconsistent, the conflict clause (lemma) is {!L1, !L2, !L3}.
/// The `core` should contain exactly that clause: `vec![L1.not(), L2.not(), L3.not()]`.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub core: Vec<Lit>,
}

/// Action returned by a theory after checking the assignments.
#[derive(Debug, Clone)]
pub enum TheoryStatus {
    /// The theory is satisfied (or has nothing to say).
    Satisfied,
    /// The theory discovered new constraints (lemmas) that must hold.
    /// Each inner Vec<Lit> is a clause. 
    /// E.g. an unsat core `vec![!A, !B]` is returned as `Lemmas(vec![vec![!A, !B]])`.
    Lemmas(Vec<Vec<Lit>>),
}

pub trait Theory {
    /// Evaluate the current boolean assignments.
    /// Returns `TheoryStatus::Lemmas` if it generates new clauses (like bit-blasting constraints or unsat cores).
    fn check(&mut self, assignments: &[Lit]) -> TheoryStatus;
}
