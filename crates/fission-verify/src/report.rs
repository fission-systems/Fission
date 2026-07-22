//! Shared result types for every verification tier (concrete eval, emulator
//! ground truth, solver-backed symbolic equivalence). Kept independent of
//! any one tier's internals so a caller can report all tiers uniformly.

use std::fmt;

/// Why a function (or a specific tier) couldn't be checked at all -- always
/// distinct from a checked-and-passed/failed result. Never coerced into a
/// silent pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// The body (or a sub-expression) uses a construct this tier's evaluator
    /// doesn't model yet -- see each tier's own scope doc comment.
    Construct(&'static str),
    /// A parameter or return value has a type this tier doesn't model
    /// (e.g. `Ptr`/`Aggregate`/`Float` -- see the crate's scope notes).
    UnsupportedType(&'static str),
    /// Symbolic-tier only: the function contains a loop (`While`/`DoWhile`/
    /// `For`), which v1's acyclic equivalence encoding doesn't cover.
    ContainsLoop,
    /// Emulator tier only: couldn't resolve the calling convention / build
    /// the emulator for this binary.
    EmulatorSetup(String),
}

impl fmt::Display for UnsupportedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnsupportedReason::Construct(what) => write!(f, "unsupported construct: {what}"),
            UnsupportedReason::UnsupportedType(what) => write!(f, "unsupported type: {what}"),
            UnsupportedReason::ContainsLoop => write!(f, "contains a loop (symbolic tier)"),
            UnsupportedReason::EmulatorSetup(msg) => write!(f, "emulator setup failed: {msg}"),
        }
    }
}

/// One concrete input tuple where two sides' evaluated results differed --
/// or where one side errored and the other didn't (asymmetric failure is
/// itself reported, never silently dropped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub args: Vec<i64>,
    pub dir_result: Option<i64>,
    pub hir_result: Option<i64>,
    /// Ground-truth tier only: what the real emulator actually returned for
    /// these same args. `None` for tiers that don't run the emulator.
    pub emulator_result: Option<i64>,
}

/// Outcome of checking one function at one verification tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Concrete tier: every sample agreed. Symbolic tier: `Unsat` -- proved
    /// equivalent for the modeled subset, not just "no counterexample found
    /// among a few samples."
    Equivalent { checked: usize },
    /// At least one concrete divergence (or a solver-found counterexample,
    /// reported as a single-element `Vec` after ground-truth replay).
    Diverged(Vec<Divergence>),
    /// Not checked at this tier, with the specific reason -- distinct from
    /// both `Equivalent` and `Diverged` so callers can't mistake "we didn't
    /// look" for "we looked and it passed."
    Unsupported(UnsupportedReason),
    /// Symbolic tier only: solver returned `Unknown` (should be rare/never
    /// in v1 since no memory theory is engaged yet) -- reported inconclusive,
    /// never coerced into a pass.
    Inconclusive { reason: String },
}

impl VerifyOutcome {
    pub fn is_equivalent(&self) -> bool {
        matches!(self, VerifyOutcome::Equivalent { .. })
    }

    pub fn is_diverged(&self) -> bool {
        matches!(self, VerifyOutcome::Diverged(_))
    }
}
