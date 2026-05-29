//! Small standalone pattern passes (bitstream state machines, branch hoists, prologue cleanup).

mod bitstream;
mod branch_hoist;
mod call_artifact;
mod prologue;
mod recurrence;
mod security_cookie;
mod split_flow;
mod string_copy;
mod subflow;
mod xor_swap;

pub(crate) use bitstream::apply_bitstream_idioms;
pub(crate) use branch_hoist::apply_branch_prefix_hoist_pass;
pub(crate) use call_artifact::apply_call_artifact_cleanup_pass;
pub(crate) use prologue::{
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
pub(crate) use recurrence::apply_recurrence_to_self_recursive_call_pass;
pub(crate) use security_cookie::apply_security_cookie_pass;
pub(crate) use split_flow::apply_split_flow_pass;
pub(crate) use string_copy::apply_string_copy_pass;
pub(crate) use subflow::apply_subflow_pruning;
pub(crate) use xor_swap::apply_xor_swap_pass;
