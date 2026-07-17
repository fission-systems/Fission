//! HIR normalization passes (arith, idioms, memory, global opts, recovery, typing).
//!
//! Entry points delegate to [`pipeline`]; wave statistics integrate with build telemetry.
//!
//! Guide: `crates/fission-pcode/src/nir/normalize/AGENTS.md`.

use super::*;

mod analysis;
pub(crate) mod arith;
mod cleanup;
pub(crate) mod global_opt;
pub(crate) mod idioms;
pub(crate) mod memory;
pub(crate) mod pipeline;
pub(crate) mod recovery;
mod rule_normalizer;
mod subvar_flow;
mod types;
pub(crate) mod wave_stats;

pub(crate) use rule_normalizer::apply_rule_normalization;

pub(crate) use types::is_known_api_signature;
pub use types::{summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode};

#[allow(dead_code)]
pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    pipeline::normalize_function_body(body);
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    pipeline::normalize_hir_function(func);
}

pub(super) fn take_normalize_wave_stats() -> crate::midend::ir::NirBuildStats {
    wave_stats::take_normalize_wave_stats()
}

#[allow(dead_code)]
pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    pipeline::normalize_stmt(stmt);
}
