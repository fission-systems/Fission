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
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::classify_suffix_stmt_with_diag(
            self, stmt, body, stmt_idx, current_label_idx, terminal_label_idx, next_label,
        )
    }

    fn suffix_is_nonowned_terminal_tail_with_diag(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        start_label: &str,
        start_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::suffix_is_nonowned_terminal_tail_with_diag(
            self, body, anchor_idx, start_label, start_label_idx, terminal_label_idx, referenced,
        )
    }

    fn candidate_window_can_shrink_to_label_with_diag(
        &mut self,
        body: &[HirStmt],
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
        )
    }

    pub(crate) fn find_earliest_owned_join_label_with_diag_impl(
        &mut self,
        body: &[HirStmt],
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
