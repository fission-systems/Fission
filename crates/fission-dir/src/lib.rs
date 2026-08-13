//! P-code-rooted DIR product contracts and verification backends.
//!
//! [`product`] owns the public, independent DIR artifact: a readable candidate
//! plus an explicit assurance report tied to an immutable validated P-code
//! foundation.  HIR and PreHIR are candidate sources, never the oracle.
//!
//! [`native`] implements the first real product slice: a typed reconstruction
//! of pure single-block integer P-code, concrete observations through
//! `fission-emulator`, and universal proof or counterexample search through
//! `fission-solver`.
//!
//! The legacy PreHIR/HIR comparison modules below remain transitional backend
//! infrastructure.  Their three tiers are increasingly expensive and strong:
//!
//! - **concrete** ([`eval`]/[`diff`]): a shared tree-walking evaluator over
//!   `PreHirStmt`/`HirStmt`, diffed on a fixed cross-product of boundary-value
//!   samples. Zero solver/emulator dependency -- infrastructure the other
//!   two tiers build on, not itself a strong check (blind to a bug shared by
//!   both PreHIR and HIR).
//! - **ground-truth** ([`emu_driver`]/[`ground_truth`]): drives the real
//!   `fission-emulator` to actually call the compiled function with concrete
//!   arguments and read back its real return value, then compares that
//!   ground truth against both concrete PreHIR and HIR evaluations. Catches
//!   bugs the concrete tier can't (both decompiler paths agreeing with each
//!   other but not with the real machine).
//! - **symbolic** ([`lower_sym`]): lowers `PreHirExpr`/`HirExpr` to
//!   `fission_solver::ast::SymExpr` and asks the real SAT/SMT-style solver
//!   to either prove PreHIR and HIR equivalent (`Unsat`) or produce a genuine
//!   counterexample (`Sat`), for loop-free functions.
//!
//! This is a heavy, opt-in "deep verify" tier -- `scripts/quality/
//! prehir_hir_check.py` stays the fast, cheap, no-solver/no-emulator structural
//! heuristic that runs routinely across the full corpus. See
//! `crates/fission-dir`'s design note in `PROJECT.md` for the full
//! rationale and phasing.

pub mod decompile;
pub mod diff;
pub mod emu_driver;
pub mod eval;
pub mod ground_truth;
pub mod lower_sym;
pub mod native;
pub mod product;
pub mod report;
pub mod symbolic;

pub use decompile::{DecompileError, PreHirHirPair, decompile_one};
pub use diff::{default_samples, diff_prehir_hir};
pub use emu_driver::{CallOutcome, EmulatorHarness};
pub use eval::{
    interpret_hir, interpret_hir_with_memory, interpret_prehir, interpret_prehir_with_memory,
};
pub use ground_truth::check_ground_truth;
pub use native::{PcodeNativeReconstructError, PcodeNativeReconstructor, PcodeObservationVerifier};
pub use product::*;
pub use report::{Divergence, UnsupportedReason, VerifyOutcome};
pub use symbolic::check_symbolic_equivalence;
