//! Statement and function-level cleanup passes (labels, temps, casts).

mod utils;
mod temp_var;
mod casts;
mod loops_conds;
mod control_flow;

// Re-export all public passes from submodules so the cleanup module's
// public API surface remains unchanged.
pub(crate) use temp_var::{
    collapse_trivial_assign_returns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, eliminate_redundant_var_assigns, elide_unused_popcount_assigns,
    inline_single_use_temps, prune_unused_dead_local_bindings, prune_unused_temp_bindings,
};
pub(crate) use casts::{
    cast_elision_pass, collapse_trivial_pointer_alias_bindings, strip_redundant_assign_casts,
};
pub(crate) use loops_conds::{
    canonicalize_minmax_conditional_returns, collapse_loop_exit_alias_returns,
    collapse_redundant_conditional_returns, inline_loop_condition_trailing_temps,
};
pub(crate) use control_flow::{
    cleanup_redundant_boundary_labels, collapse_common_exit_guard_chain,
    fuse_single_predecessor_boundaries, promote_guarded_jump_target_tail,
    prune_unreachable_after_terminal, remove_unreferenced_leading_labels,
    simplify_empty_and_constant_ifs, simplify_empty_and_constant_ifs_recursive,
    simplify_fallthrough_edges, single_pred_label_inline,
};

// Re-export utility functions used by other modules outside cleanup.
pub(crate) use utils::expr_has_side_effects;

#[cfg(test)]
#[path = "passes_tests.rs"]
mod tests;
