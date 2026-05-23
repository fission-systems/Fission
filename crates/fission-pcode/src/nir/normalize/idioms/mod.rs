//! Small standalone pattern passes (bitstream state machines, branch hoists, prologue cleanup).

mod bitstream;
mod branch_hoist;
mod call_artifact;
mod prologue;
mod recurrence;
mod security_cookie;
mod subflow;

pub(crate) use bitstream::apply_bitstream_idioms;
pub(crate) use branch_hoist::apply_branch_prefix_hoist_pass;
pub(crate) use call_artifact::apply_call_artifact_cleanup_pass;
pub(crate) use prologue::{
    remove_callee_save_prologue_epilogue, remove_entry_stack_scaffold_stores,
};
pub(crate) use recurrence::apply_recurrence_to_self_recursive_call_pass;
pub(crate) use security_cookie::apply_security_cookie_pass;
pub(crate) use subflow::apply_subflow_pruning;
