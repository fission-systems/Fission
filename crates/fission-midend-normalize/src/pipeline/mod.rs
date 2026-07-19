//! Pass pipeline orchestration (`normalize_hir_function` and helpers).

mod groups;
mod heritage_contracts;
mod run;
mod stages;

pub use groups::{build_normalize_pipeline, run_normalize_pipeline};
pub use run::{GLOBAL_SYMBOL_CONTEXT, GlobalSymbolContext};
pub use run::{
    is_large_hir_function, normalize_expr, normalize_function_body, normalize_hir_function,
    normalize_stmt,
};
pub use stages::{
    run_stage_block_structure_1, run_stage_cleanup, run_stage_heritage_value_recovery,
    run_stage_memory_recovery, run_stage_merge, run_stage_proto_recovery_head,
    run_stage_type_early,
};
