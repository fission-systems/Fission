use super::*;

mod analysis;
mod arith;
mod cleanup;
mod global_opt;
mod idioms;
mod memory;
mod pipeline;
mod recovery;
mod types;
pub(crate) mod wave_stats;

pub use types::{summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode};

#[allow(dead_code)]
pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    pipeline::normalize_function_body(body);
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    pipeline::normalize_hir_function(func);
}

pub(super) fn take_normalize_wave_stats() -> crate::nir::types::NirBuildStats {
    wave_stats::take_normalize_wave_stats()
}

#[allow(dead_code)]
pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    pipeline::normalize_stmt(stmt);
}
