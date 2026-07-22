//! Solver- and emulator-backed DIR/HIR correctness verification.
//!
//! Three tiers, increasingly expensive and increasingly strong:
//!
//! - **concrete** ([`eval`]/[`diff`]): a shared tree-walking evaluator over
//!   `DirStmt`/`HirStmt`, diffed on a fixed cross-product of boundary-value
//!   samples. Zero solver/emulator dependency -- infrastructure the other
//!   two tiers build on, not itself a strong check (blind to a bug shared by
//!   both DIR and HIR).
//! - **ground-truth** ([`emu_driver`]/[`ground_truth`]): drives the real
//!   `fission-emulator` to actually call the compiled function with concrete
//!   arguments and read back its real return value, then compares that
//!   ground truth against both the concrete DIR and HIR evaluations. Catches
//!   bugs the concrete tier can't (both decompiler paths agreeing with each
//!   other but not with the real machine).
//! - **symbolic** ([`lower_sym`]): lowers `DirExpr`/`HirExpr` to
//!   `fission_solver::ast::SymExpr` and asks the real SAT/SMT-style solver
//!   to either prove DIR and HIR equivalent (`Unsat`) or produce a genuine
//!   counterexample (`Sat`), for loop-free functions.
//!
//! This is a heavy, opt-in "deep verify" tier -- `scripts/quality/
//! dir_hir_check.py` stays the fast, cheap, no-solver/no-emulator structural
//! heuristic that runs routinely across the full corpus. See
//! `crates/fission-verify`'s design note in `PROJECT.md` for the full
//! rationale and phasing.

pub mod decompile;
pub mod diff;
pub mod emu_driver;
pub mod eval;
pub mod ground_truth;
pub mod report;

pub use decompile::{DecompileError, DirHirPair, decompile_one};
pub use diff::{default_samples, diff_dir_hir};
pub use emu_driver::{CallOutcome, EmulatorHarness};
pub use eval::{interpret_dir, interpret_hir};
pub use ground_truth::check_ground_truth;
pub use report::{Divergence, UnsupportedReason, VerifyOutcome};
