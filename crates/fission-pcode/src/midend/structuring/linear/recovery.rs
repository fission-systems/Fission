//! Region linearization recovery — thin wrappers over free functions in
//! `fission-midend-structuring::linear_recovery`.

use super::*;

pub use fission_midend_structuring::{
    build_linear_sese_child_fallback, region_linearized_exit_candidates_algorithmic,
    try_recover_region_linearized_body,
};

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn try_recover_region_linearized_body(
        &mut self,
        start_idx: usize,
        err: &MlilPreviewError,
        targeted: &HashSet<u64>,
        emitted_labels: &mut HashSet<u64>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        try_recover_region_linearized_body(self, start_idx, err, targeted, emitted_labels)
    }

    /// Linear fallback for a single SESE child region without discarding parent structure.
    pub(crate) fn build_linear_sese_child_fallback(
        &mut self,
        entry: usize,
        exit: usize,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        build_linear_sese_child_fallback(self, entry, exit)
    }
}
