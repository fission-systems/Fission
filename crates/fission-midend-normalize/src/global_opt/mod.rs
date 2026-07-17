//! Function-wide dataflow optimizations (SCCP, LICM, CSE, memory SSA helpers, etc.).

mod bit_consume;
mod conditional_const;
mod cse;
mod dead_store;
mod gvn_join;
mod licm;
mod mem_ssa;
mod nz_mask;
mod post_assign;
mod redundant_load;
mod sccp;

pub use bit_consume::apply_bit_consume_dead_code_pass;
pub use conditional_const::apply_conditional_const_pass;
pub use cse::apply_cse_pass;
pub use dead_store::apply_dead_store_elimination;
pub use gvn_join::apply_gvn_join_hoist_pass;
pub use licm::apply_licm_pass;
pub use mem_ssa::{AliasKey, MemDef, MemPhi, MemUse, build_mem_ssa, nir_byte_size};
pub use nz_mask::{apply_nz_mask_simplification_pass, compute_nz_masks};
pub use post_assign::apply_post_assign_value_representative_pass;
pub use redundant_load::apply_redundant_load_elimination;
pub use sccp::apply_sccp_pass;
