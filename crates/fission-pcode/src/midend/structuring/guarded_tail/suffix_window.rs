use super::*;

// Suffix-window types owned by fission-midend-structuring::guarded_tail::types.
use fission_midend_structuring::guarded_tail::{
    NestedSuffixShapeKind, SuffixCallEffectShapeKind, SuffixExternalEntryBudget,
    SuffixSideEffectShapeKind, SuffixTailRejection,
};

impl<'a> PreviewBuilder<'a> {
    // Residual host data gatherers are on StructuringHost (host_impl).
    // With-diag free-fn thin wraps:

    fn classify_suffix_stmt_with_diag(
        &mut self,
        stmt: &PreHirStmt,
        body: &[PreHirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::classify_suffix_stmt_with_diag(
            self,
            stmt,
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
            // The trait adapter has no candidate loop above it, so each call
            // gets its own memo; the sharing that matters happens inside
            // `find_earliest_owned_join_label_with_diag`.
            &mut fission_midend_structuring::guarded_tail::OwnedSafeMemo::default(),
        )
    }

    fn suffix_is_nonowned_terminal_tail_with_diag(
        &mut self,
        body: &[PreHirStmt],
        anchor_idx: usize,
        start_label: &str,
        start_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::suffix_is_nonowned_terminal_tail_with_diag(
            self,
            body,
            anchor_idx,
            start_label,
            start_label_idx,
            terminal_label_idx,
            referenced,
            &mut fission_midend_structuring::guarded_tail::OwnedSafeMemo::default(),
        )
    }

    fn candidate_window_can_shrink_to_label_with_diag(
        &mut self,
        body: &[PreHirStmt],
        anchor_idx: usize,
        candidate_label: &str,
        candidate_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::candidate_window_can_shrink_to_label_with_diag(
            self,
            body,
            anchor_idx,
            candidate_label,
            candidate_label_idx,
            terminal_label_idx,
            referenced,
            &mut fission_midend_structuring::guarded_tail::OwnedSafeMemo::default(),
        )
    }

    pub(crate) fn find_earliest_owned_join_label_with_diag_impl(
        &mut self,
        body: &[PreHirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        fission_midend_structuring::guarded_tail::find_earliest_owned_join_label_with_diag(
            self,
            body,
            anchor_idx,
            terminal_label_idx,
            referenced,
            trace_enabled,
        )
    }
}
