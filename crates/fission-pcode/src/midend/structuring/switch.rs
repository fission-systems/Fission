//! Switch structuring — thin wrappers over free functions in
//! `fission-midend-structuring::switch`.

use super::*;

pub use fission_midend_structuring::{
    SWITCH_CHAIN_PARSE_BUDGET_MAX, canonicalize_switch_target, try_lower_switch,
};
pub use fission_midend_structuring::helpers::{
    detect_and_patch_case_fallthrough, merge_equivalent_switch_cases, recovered_switch_case_values,
};
pub use fission_midend_structuring::switch::{parse_switch_chain, ParsedSwitch};

// Fallthrough sentinel re-export for midend consumers.
pub(crate) use crate::midend::SWITCH_FALLTHROUGH_SENTINEL;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_switch(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_switch(self, idx)
    }

    pub(super) fn canonicalize_switch_target(&self, start_idx: usize) -> usize {
        canonicalize_switch_target(self, start_idx)
    }

    pub(super) fn parse_switch_chain(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
        parse_switch_chain(self, start_idx)
    }
}
