use super::*;

mod arith;
mod bitstream;
mod cleanup;
mod core;
pub(super) mod defuse;
mod for_loops;
mod phi_recovery;
mod slots;
mod type_infer;

#[allow(dead_code)]
pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    core::normalize_function_body(body);
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    core::normalize_hir_function(func);
}

#[allow(dead_code)]
pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    core::normalize_stmt(stmt);
}
