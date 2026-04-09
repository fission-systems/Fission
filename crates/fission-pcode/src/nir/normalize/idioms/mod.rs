//! Small standalone pattern passes (bitstream state machines, branch hoists, prologue cleanup).

mod bitstream;
mod branch_hoist;
mod prologue;

pub(crate) use bitstream::apply_bitstream_idioms;
pub(crate) use branch_hoist::apply_branch_prefix_hoist_pass;
pub(crate) use prologue::remove_callee_save_prologue_epilogue;
