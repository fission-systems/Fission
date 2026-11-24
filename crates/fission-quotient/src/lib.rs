//! White-box coalgebraic bisimulation quotients (WBCQ, RFC #1093).
//!
//! **Experimental.** Nothing in the production pipeline depends on this
//! crate; it sits beside `fission-dir` as an isolated prototype (the RFC's
//! P0/P1 stage).
//!
//! Instead of structuring a control-flow graph, the engine computes the
//! smallest state machine that is observably equivalent to a P-code slice:
//!
//! 1. [`summary`] executes one step of the slice symbolically, yielding every
//!    feasible path as `(guard, result, context')`. Branch sides the solver
//!    refutes for every input and state -- opaque predicates -- are pruned.
//! 2. [`srqa::factor`] splits the persistent context into control slots,
//!    which reach a branch or the result, and data slots, which do not.
//! 3. [`learn`] learns the observable machine with an apartness tree, the
//!    emulator answering membership queries. Distinct states always come
//!    with a concrete distinguishing word.
//! 4. [`srqa::build`] turns the hypothesis into a symbolic automaton whose
//!    guards and data updates come from the path summaries.
//! 5. [`certify`] asks the solver for a word of length up to `H` on which
//!    the slice and the automaton differ. `Unsat` certifies bounded
//!    equivalence under the memory model; a candidate word is replayed on
//!    the emulator before it refines anything; `Unknown` is never a proof.
//!
//! The result is a [`report::CertifiedQuotient`] (`quotient.v1`), whose
//! status says exactly how far its claim reaches.

pub mod boundary;
pub mod calls;
pub mod certify;
mod engine;
mod expr;
pub mod learn;
pub mod oracle;
pub mod report;
pub mod srqa;
pub mod summary;

pub use boundary::{
    AbiModel, ContextRegion, InputPort, MemoryLayout, PointerRegion, QuotientBoundary,
    QuotientBudget,
};
pub use calls::{CallFacts, CallModel};
pub use engine::{QuotientEngine, QuotientModel, ReplayVerdict, Wbcq};
pub use oracle::{
    CallEvent, EmulatorOracle, Exit, MembershipOracle, Observation, StepRecord, Symbol,
};
pub use report::{CertifiedQuotient, QuotientStatus};
