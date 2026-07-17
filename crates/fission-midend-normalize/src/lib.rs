//! Midend **normalize** owner (ADR 0012).
//!
//! HIR normalization passes: arith, idioms, memory, global opts, recovery, typing.
//! Shared substrate comes from [`fission_midend_core`].

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

/// Shared prelude for historical `use super::super::*` midend imports.
pub mod prelude {
    pub use fission_midend_core::action_pipeline::{
        self, ActionGroup, ActionPool, Gate, GhidraActionConcept, Pass, PassBudget, PassCtx,
        PassOutcome, Pipeline, Repeat, STRUCTURING_TIME_CEILING_SECS, count_hir_blocks,
        count_hir_stmts, fn_pass, group, hir_shape, is_large_hir_function, run_pass_logged,
    };
    pub use fission_midend_core::ir::*;
    pub use fission_midend_core::util::{
        cleanup_redundant_labels, collect_referenced_labels, expr_has_side_effecting_call,
        expr_type, fold_logical_chain, is_pure_intrinsic_call, negate_expr, next_temp_name,
        format_expr_key, rename_vars_in_stmts, simplify_logical_expr, strip_casts,
    };
    pub use fission_midend_core::vsa::{
        apply_jump_resolver_pass, jump_resolver_candidate_count,
    };
    pub use fission_midend_core::wave_stats;
    pub use fission_midend_core::{
        AbiState, CallingConvention, HirExpr, HirFunction, HirStmt, NirBuildStats, NirType,
        SWITCH_FALLTHROUGH_SENTINEL,
    };
    pub use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
}

mod analysis;
pub mod arith;
mod cleanup;
pub mod global_opt;
pub mod idioms;
pub mod memory;
pub mod pipeline;
pub mod recovery;
mod rule_normalizer;
mod subvar_flow;
mod types;

pub use rule_normalizer::apply_rule_normalization;

pub use types::is_known_api_signature;

#[allow(dead_code)]
pub fn normalize_function_body(body: &mut Vec<prelude::HirStmt>) {
    pipeline::normalize_function_body(body);
}

/// Run the full normalize pipeline on a structured function.
pub fn normalize_hir_function(func: &mut prelude::HirFunction) {
    pipeline::normalize_hir_function(func);
}

/// Take and reset normalize-wave telemetry counters for the current thread.
pub fn take_normalize_wave_stats() -> fission_midend_core::NirBuildStats {
    fission_midend_core::wave_stats::take_normalize_wave_stats()
}

#[allow(dead_code)]
pub fn normalize_stmt(stmt: &mut prelude::HirStmt) {
    pipeline::normalize_stmt(stmt);
}

// Re-export shared types for facade callers.
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats};
