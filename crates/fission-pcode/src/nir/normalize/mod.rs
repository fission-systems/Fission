use super::*;

mod analysis;
mod arith;
mod cleanup;
mod pipeline;
mod global_opt;
mod idioms;
mod memory;
mod recovery;
mod types;
mod wave_stats;

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
