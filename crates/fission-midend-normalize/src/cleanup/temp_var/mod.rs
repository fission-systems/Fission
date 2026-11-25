//! Temporary and local binding cleanup passes.

mod assignment_cleanup;
mod binding_liveness;
mod binding_recovery;
mod inline_safety;
mod inline_temps;
mod param_aliases;
mod pointer_bitops;
mod popcount;
mod predicate_copies;
mod return_folding;
mod self_square;
mod stack_slot_names;

pub use assignment_cleanup::{
    eliminate_dead_local_clobber_assigns, eliminate_dead_temp_assigns,
    eliminate_redundant_var_assigns,
};
pub use binding_liveness::{prune_unused_dead_local_bindings, prune_unused_temp_bindings};
pub use binding_recovery::rescue_undeclared_bindings;
pub use inline_temps::inline_single_use_temps;
pub use param_aliases::hoist_param_alias_copies_before_first_use;
pub use pointer_bitops::coerce_ptr_typed_bitop_vars;
pub use popcount::elide_unused_popcount_assigns;
pub use predicate_copies::collapse_adjacent_pure_copy_into_if;
pub use return_folding::collapse_trivial_assign_returns;
pub use self_square::collapse_temp_self_square_assigns;
pub use stack_slot_names::canonicalize_orphaned_stack_slot_names;
