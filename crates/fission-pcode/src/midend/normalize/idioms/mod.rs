//! Small standalone pattern passes (branch hoists, prologue cleanup, subflow).

mod branch_hoist;
mod prologue;
mod split_flow;
mod subflow;

pub(crate) use branch_hoist::apply_branch_prefix_hoist_pass;
pub(crate) use prologue::{
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
pub(crate) use split_flow::apply_split_flow_pass;
pub(crate) use subflow::apply_subflow_pruning;
