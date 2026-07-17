//! PHI / flags / induction-variable and loop-structure recovery passes.

mod flag_recovery;
mod for_loops;
mod iv_recovery;
mod phi_recovery;
mod variable_merge;

pub(crate) use flag_recovery::{apply_dead_flag_cleanup_pass, apply_flag_recovery_pass};
pub(crate) use for_loops::apply_for_loop_folding;
pub(crate) use iv_recovery::{apply_break_continue_pass, apply_iv_recovery_pass};
pub(crate) use phi_recovery::{copy_propagation_pass, join_coalescing_pass};
pub(crate) use variable_merge::apply_variable_merge_pass;
