//! Pass pipeline orchestration (`normalize_hir_function` and helpers).

mod groups;
mod heritage_contracts;
mod run;
mod stages;

pub(crate) use groups::{build_normalize_pipeline, run_normalize_pipeline};
pub(crate) use run::{
    is_large_hir_function, normalize_expr, normalize_function_body, normalize_hir_function,
    normalize_stmt, run_canonical_normalize_passes, GLOBAL_SYMBOL_CONTEXT, GlobalSymbolContext,
};
pub(crate) use stages::{
    run_stage_block_structure_1, run_stage_cleanup, run_stage_deadcode_dynamic,
    run_stage_heritage_value_recovery, run_stage_memory_recovery, run_stage_merge,
    run_stage_proto_recovery, run_stage_stackstall, run_stage_type_early,
};
