//! DIR/HIR differential verification.
//!
//! **DIR** (dynamically-verified IR) is not a new IR type: it is the exact
//! `Vec<HirStmt>` that
//! `fission_pcode`'s structuring stage receives as input (flattened,
//! goto/label-based), captured via [`fission_pcode::take_last_dir_snapshot`]
//! immediately before structuring's CFG-to-AST rewrite runs. **HIR** is the
//! same function's final structured output (if/while/for), which the normal
//! decompile call already returns. Because both are the same type over the
//! same variable namespace, [`interp::interpret`] can evaluate either one,
//! and [`diff::diff_dir_hir`] runs both against the same concrete arguments
//! and reports any divergence as a real, reproducible structuring bug --
//! exactly the class of bug (structuring silently changing a function's
//! behavior while restructuring its control flow) this session found by
//! hand once already (an AArch64 if/else collapsed into a linear block).
//!
//! # Roadmap
//!
//! Phase 1 (this crate today): DIR-vs-HIR differential interpretation by
//! concrete random/boundary sampling ([`diff::default_samples`]). Cannot
//! catch builder/normalize lifting bugs (both DIR and HIR are downstream of
//! that stage) -- only structuring bugs.
//!
//! Phase 2 (not yet implemented, `fission-emulator`/`fission-solver`
//! dependencies are wired in for this): a symbolic mirror of `interp`, built
//! on `fission_solver::SymExpr`/`Solver` (already proven in
//! `fission-emulator`'s symbolic execution, see `fission-emulator`'s `sym`
//! module), to prove DIR/HIR equivalence for *all* inputs or return a
//! guaranteed counterexample instead of relying on sample coverage; and
//! separately, validating DIR itself against the *real compiled binary's*
//! execution via `fission-emulator`, to catch builder/normalize lifting
//! bugs rather than structuring bugs.

pub mod diff;
pub mod interp;
