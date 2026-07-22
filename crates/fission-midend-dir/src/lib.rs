//! Midend **DIR substrate** -- the pre-structuring counterpart to
//! `fission-midend-core`'s HIR substrate.
//!
//! Owns the `Dir*` structured-IR types (see [`ir`]'s module doc on
//! [`ir::DirStmt`] for why these are independently defined from `Hir*`,
//! not the same type under two names), the DIR-typed action-pipeline
//! framework, DIR-typed VSA, and DIR-typed pure helpers. Depended on by
//! `fission-pcode`'s `builder`/`structuring` and by
//! `fission-midend-normalize`/`fission-midend-structuring` -- never by
//! `render`/printer or anything downstream of a finished decompile, which
//! only ever needs `fission-midend-core`'s `HirFunction`.
//!
//! The real `DirFunction -> HirFunction` conversion
//! ([`ir::dir_stmts_to_hir_stmts`]) lives here rather than in
//! `fission-midend-core`, since this crate depends on that one (for
//! `NirType`/`NirBinding`/etc, genuinely shared metadata with no embedded
//! AST) and can therefore see both `Dir*` and `Hir*` types -- the reverse
//! dependency direction would be a cycle.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod action_pipeline;
pub mod ir;
pub mod util;
pub mod vsa;

/// Shared structured-IR types and telemetry contract.
pub use ir::*;

/// Pure DIR helpers.
pub use util::{
    cleanup_redundant_labels, collect_referenced_label_counts, collect_referenced_labels,
    expr_has_side_effecting_call, expr_type, fold_logical_chain, format_expr_key,
    format_lvalue_key, is_pure_intrinsic_call, negate_expr, next_temp_name,
    rename_vars_in_stmts, simplify_logical_expr, strip_casts,
};

/// VSA jump-resolver surface used by the normalize pipeline.
pub use vsa::{apply_jump_resolver_pass, jump_resolver_candidate_count};
