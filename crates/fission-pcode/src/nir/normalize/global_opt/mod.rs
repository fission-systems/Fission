//! Function-wide dataflow optimizations (SCCP, LICM, CSE, memory SSA helpers, etc.).

mod cse;
mod dead_store;
mod gvn_join;
mod licm;
mod mem_ssa;
mod post_assign;
mod redundant_load;
mod sccp;

pub(crate) use cse::apply_cse_pass;
pub(crate) use dead_store::apply_dead_store_elimination;
pub(crate) use gvn_join::apply_gvn_join_hoist_pass;
pub(crate) use licm::apply_licm_pass;
pub(crate) use post_assign::apply_post_assign_value_representative_pass;
pub(crate) use redundant_load::apply_redundant_load_elimination;
pub(crate) use sccp::apply_sccp_pass;
