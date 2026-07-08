pub mod bitvector;

use crate::cnf::Lit;

/// Represents a conflict discovered by a Theory solver.
/// The `core` is the set of literals that caused the conflict.
/// If assignment {L1, L2, L3} is inconsistent, the conflict clause (lemma) is {!L1, !L2, !L3}.
/// The `core` should contain exactly that clause: `vec![L1.not(), L2.not(), L3.not()]`.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub core: Vec<Lit>,
}

pub trait Theory {
    /// Evaluate the current boolean assignments.
    /// If an inconsistency is found, return `Err(Conflict)` containing the unsat core (lemma).
    fn check(&mut self, assignments: &[Lit]) -> Result<(), Conflict>;
}
