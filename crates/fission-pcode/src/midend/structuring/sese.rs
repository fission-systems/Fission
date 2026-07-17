//! SESE types re-export (ADR 0012).
//!
//! Entry: call `fission_midend_structuring::structure_cfg_via_sese` directly
//! (see `pass/structuring.rs`). No local thin wrap function.

pub(crate) use fission_midend_structuring::{
    SeseRegion, SeseRegionTree, build_sese_tree, compute_rpo_map, find_sese_regions,
    structure_cfg_via_sese,
};
