use super::*;

mod aggregate_fields;
mod arith;
mod bitstream;
mod branch_hoist;
mod cleanup;
mod core;
mod cse;
mod expr_key;
pub(super) mod defuse;
mod dead_store;
mod flag_recovery;
mod for_loops;
mod iv_recovery;
mod licm;
mod mem_ssa;
mod redundant_load;
mod phi_recovery;
mod prologue;
mod ptr_arith;
mod callsite_type_prop;
mod slots;
mod type_infer;
mod use_type_infer;

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
