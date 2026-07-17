//! Midend **shared substrate**.
//!
//! Owns structured IR types (`Hir*`, options, `NirBuildStats`), the
//! action-pipeline framework, quality-wave counters, pure HIR helpers,
//! VSA, and shared label sentinels (ADR 0012 Phase D).

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod abi_param;
pub mod fast_hash;
pub mod action_pipeline;
pub mod ir;
pub mod labels;
pub mod util;
pub mod vsa;
pub mod wave_stats;

/// Shared structured-IR types and telemetry contract.
pub use ir::*;

/// Switch fallthrough sentinel used by structuring and print.
pub use labels::SWITCH_FALLTHROUGH_SENTINEL;

/// Pure HIR helpers.
pub use util::{
    cleanup_redundant_labels, collect_referenced_label_counts, collect_referenced_labels, expr_has_side_effecting_call, expr_type,
    fold_logical_chain, is_pure_intrinsic_call, negate_expr, next_temp_name, print_expr,
    rename_vars_in_stmts, simplify_logical_expr, strip_casts,
};
pub use abi_param::AbiState;
pub use fission_core::CallingConvention;

/// VSA jump-resolver surface used by the normalize pipeline.
pub use vsa::{apply_jump_resolver_pass, jump_resolver_candidate_count};
