//! Experimental CSIC (RFC #1101, Track B): counterfactual interventions on
//! machine state, replayed in Fission's emulator.
//!
//! Given a program, a pinned input stream and a sink predicate Φ, the
//! runner records the incident, checks the replay is reproducible, slices
//! backward from Φ for candidate time-indexed locations, and for each one
//! sets an alternative value at its step and replays. A changed Φ that a
//! fresh replay confirms is a [`contract::Verdict::CounterfactualWitness`]:
//! evidence that this location's value mattered *in this experiment*, not a
//! Halpern-Pearl actual-cause claim and not a minimality claim.
//!
//! Isolation: no production crate depends on this one, and nothing here
//! touches decompiler output. Fixtures and the evaluation harness live in
//! `testdata/` and `examples/`; see `docs/proposals/` for the P1 report.

pub mod baseline;
pub mod cfg;
pub mod contract;
pub mod eval;
pub mod machine;
pub mod runner;
pub mod slice;
pub mod static_slice;
pub mod symbolic;
pub mod trace;

pub use contract::{
    CONTRACT_VERSION, Intervention, Location, MemPredicate, SinkPoint, SinkSpec, Verdict,
};
pub use machine::Program;
pub use runner::{Case, CaseReport, Config, run_case};
