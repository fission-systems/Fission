//! Midend **shared substrate** -- the HIR half, plus genuinely shared
//! (AST-free) metadata.
//!
//! Owns the structured `Hir*` IR types, `NirType`/`NirBinding`/etc
//! (shared declaration/type metadata, consumed by both DIR and HIR),
//! `NirBuildStats`, pure HIR helpers, and shared label sentinels
//! (ADR 0012 Phase D).
//!
//! The DIR-side counterpart -- `Dir*` IR types, the action-pipeline
//! framework, VSA, and their own pure-helper twins -- lives in
//! `fission-midend-dir` (depends on this crate, not the other way around):
//! `action_pipeline`/`vsa` turned out to have zero HIR-side callers when
//! this crate was split, and `render`/printer (the only HIR-side consumer
//! of `util`'s functions) never needs `DirStmt`, so keeping this crate to
//! just HIR-and-shared keeps it the smaller dependency for anything
//! downstream of a finished decompile (`fission-decompiler`, `fission-cli`,
//! ...), which never needs to compile in `DirStmt`/`action_pipeline`/`vsa`
//! at all.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod abi_param;
pub mod fast_hash;
pub mod ir;
pub mod labels;
pub mod util;
pub mod wave_stats;

/// Shared structured-IR types and telemetry contract.
pub use ir::*;

/// Switch fallthrough sentinel used by structuring and print.
pub use labels::SWITCH_FALLTHROUGH_SENTINEL;

/// Pure HIR helpers.
pub use util::{
    cleanup_redundant_labels, collect_referenced_label_counts, collect_referenced_labels, expr_has_side_effecting_call, expr_type,
    fold_logical_chain, format_expr_key, format_lvalue_key, is_pure_intrinsic_call, negate_expr,
    next_temp_name,
    rename_vars_in_stmts, simplify_logical_expr, strip_casts,
};
pub use abi_param::AbiState;
pub use fission_core::CallingConvention;
