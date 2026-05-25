//! Function-wide dataflow optimizations (SCCP, LICM, CSE, memory SSA helpers, etc.).

mod bit_consume;
mod conditional_const;
mod cse;
mod dead_store;
mod gvn_join;
mod licm;
mod likely_trash;
mod mem_ssa;
mod nz_mask;
mod post_assign;
mod redundant_load;
mod sccp;

pub(crate) use bit_consume::apply_bit_consume_dead_code_pass;
pub(crate) use conditional_const::apply_conditional_const_pass;
pub(crate) use cse::apply_cse_pass;
pub(crate) use dead_store::apply_dead_store_elimination;
pub(crate) use gvn_join::apply_gvn_join_hoist_pass;
pub(crate) use licm::apply_licm_pass;
pub(crate) use likely_trash::apply_likely_trash_pass;
pub(crate) use nz_mask::{apply_nz_mask_simplification_pass, compute_nz_masks};
pub(crate) use post_assign::apply_post_assign_value_representative_pass;
pub(crate) use redundant_load::apply_redundant_load_elimination;
pub(crate) use sccp::apply_sccp_pass;
pub(crate) use mem_ssa::{AliasKey, MemDef, MemUse, MemPhi, build_mem_ssa, nir_byte_size};
