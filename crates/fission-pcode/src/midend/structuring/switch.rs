//! Switch structuring re-exports (ADR 0012).
//!
//! Free-function owners: `fission-midend-structuring::switch`.
//! No PreviewBuilder inherent thin wraps — call free-fns with host.

pub use fission_midend_structuring::{
    SWITCH_CHAIN_PARSE_BUDGET_MAX, canonicalize_switch_target, try_lower_switch,
};
pub use fission_midend_structuring::helpers::{
    detect_and_patch_case_fallthrough, merge_equivalent_switch_cases, recovered_switch_case_values,
};
pub use fission_midend_structuring::switch::{parse_switch_chain, ParsedSwitch};

// Fallthrough sentinel re-export for midend consumers.
pub(crate) use crate::midend::SWITCH_FALLTHROUGH_SENTINEL;
