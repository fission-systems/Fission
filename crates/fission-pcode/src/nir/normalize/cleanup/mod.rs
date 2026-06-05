//! Statement and function-level cleanup passes (labels, temps, casts).

pub(crate) mod utils;
mod temp_var;
mod casts;
mod loops_conds;
mod control_flow;
mod switch_norm;
mod condexe;
mod expand_load;
mod deindirect;
mod subvar_trim;

// Re-export all public passes from submodules so the cleanup module's
// public API surface remains unchanged.
pub(crate) use temp_var::{
    collapse_trivial_assign_returns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, eliminate_redundant_var_assigns, elide_unused_popcount_assigns,
    inline_single_use_temps, prune_unused_dead_local_bindings, prune_unused_temp_bindings,
    rescue_undeclared_bindings, coerce_ptr_typed_bitop_vars,
};
pub(crate) use casts::{
    cast_elision_pass, collapse_trivial_pointer_alias_bindings, normalize_pointer_and_struct_casts,
    strip_redundant_assign_casts,
};
pub(crate) use loops_conds::{
    canonicalize_minmax_conditional_returns, collapse_loop_exit_alias_returns,
    collapse_redundant_conditional_returns, conditional_select_pass,
    inline_loop_condition_trailing_temps, normalize_dowhile_decrement_condition,
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
pub(crate) use switch_norm::apply_switch_norm_pass;
pub(crate) use condexe::{apply_condexe_folding_pass, apply_iblock_phi_elimination};
pub(crate) use expand_load::apply_expand_load_pass;
pub(crate) use deindirect::apply_deindirect_pass;
pub(crate) use subvar_trim::apply_subvar_trim_pass;

#[cfg(test)]
#[path = "passes_tests.rs"]
mod tests;

