use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn mark_alias_nonlocal_external_before_impl(&mut self) {
        self.telemetry
            .structuring
            .canonicalization_failed_alias_has_nonlocal_ref_external_before_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_nested_before_impl(&mut self) {
        self.telemetry
            .structuring
            .canonicalization_failed_alias_has_nonlocal_ref_nested_before_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_post_segment_ref_impl(&mut self) {
        self.telemetry
            .structuring
            .canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_from_external_sites_impl(
        &mut self,
        external_top_level_before: usize,
        external_nested_before: usize,
        external_refs_after: usize,
    ) {
        if external_nested_before > 0 {
            self.mark_alias_nonlocal_nested_before_impl();
        } else if external_refs_after > 0 {
            self.mark_alias_nonlocal_post_segment_ref_impl();
        } else if external_top_level_before > 0 {
            self.mark_alias_nonlocal_external_before_impl();
        }
    }

    pub(super) fn expr_is_pure_value(expr: &DirExpr) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::expr_is_pure_value(expr)
    }

    pub(super) fn stmt_is_pure_value_expr(stmt: &DirStmt) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_pure_value_expr(stmt)
    }

    pub(super) fn stmt_is_pure_value_assign(stmt: &DirStmt) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_pure_value_assign(stmt)
    }

    #[cfg(test)]
    pub(super) fn test_expr_is_pure_value(expr: &DirExpr) -> bool {
        Self::expr_is_pure_value(expr)
    }

    fn stmt_is_alias_forward_safe(stmt: &DirStmt, label: &str, next_label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_alias_forward_safe(stmt, label, next_label)
    }

    pub(super) fn classify_external_alias_ref_sites(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        fission_midend_structuring::guarded_tail::pure_hir::classify_external_alias_ref_sites(full_body, segment_start, segment_end, label)
    }

    pub(super) fn classify_external_alias_ref_sites_detailed(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize, usize) {
        fission_midend_structuring::guarded_tail::pure_hir::classify_external_alias_ref_sites_detailed(full_body, segment_start, segment_end, label)
    }

    pub(super) fn stmt_contains_goto_label(stmt: &DirStmt, label: &str) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_contains_goto_label(stmt, label)
    }

    pub(super) fn are_all_external_refs_top_level_goto(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::are_all_external_refs_top_level_goto(full_body, segment_start, segment_end, label)
    }

    pub(super) fn classify_alias_ref_sites(
        body: &[DirStmt],
        label_idx: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        fission_midend_structuring::guarded_tail::pure_hir::classify_alias_ref_sites(body, label_idx, label)
    }

    fn stmt_is_pure_nested_single_branch_goto_to_label(stmt: &DirStmt, label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_pure_nested_single_branch_goto_to_label(stmt, label)
    }

    fn classify_nested_before_nonlocal_payload(stmt: &DirStmt, label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::classify_nested_before_nonlocal_payload(stmt, label)
    }

    fn classify_nested_before_alias_witnesses(
        full_body: &[DirStmt],
        segment_start: usize,
        label: &str,
    ) -> Vec<NestedBeforeAliasWitness> {
        fission_midend_structuring::guarded_tail::pure_hir::classify_nested_before_alias_witnesses(full_body, segment_start, label)
    }

    pub(super) fn build_nested_before_alias_ownership_proof(
        full_body: &[DirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
        raw_nested_before: usize,
    ) -> AliasOwnershipProof {
        fission_midend_structuring::guarded_tail::pure_hir::build_nested_before_alias_ownership_proof(full_body, segment_start, segment_end, label, raw_nested_before)
    }

    pub(super) fn local_goto_positions_by_label(body: &[DirStmt]) -> HashMap<String, Vec<usize>> {
        fission_midend_structuring::guarded_tail::pure_hir::local_goto_positions_by_label(body)
    }

    pub(super) fn is_local_alias_forward_segment(segment: &[DirStmt], next_label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::is_local_alias_forward_segment(segment, next_label)
    }

    pub(super) fn is_local_alias_forward_segment_with_after_label_refs(
        segment: &[DirStmt],
        label: &str,
        next_label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::is_local_alias_forward_segment_with_after_label_refs(segment, label, next_label)
    }

    pub(super) fn inferred_alias_forward_target_with_after_label_refs(
        segment: &[DirStmt],
        label: &str,
    ) -> Option<String> {
        fission_midend_structuring::guarded_tail::pure_hir::inferred_alias_forward_target_with_after_label_refs(segment, label)
    }

    pub(super) fn is_trivial_join_forward_segment(segment: &[DirStmt], next_label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::is_trivial_join_forward_segment(segment, next_label)
    }

    pub(super) fn is_trivial_join_forward_or_pure_segment(
        segment: &[DirStmt],
        next_label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::is_trivial_join_forward_or_pure_segment(segment, next_label)
    }

    pub(super) fn is_pure_multi_goto_gap_to_label(
        body: &[DirStmt],
        goto_positions: &[usize],
        label_idx: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::is_pure_multi_goto_gap_to_label(body, goto_positions, label_idx, label)
    }

    pub(super) fn count_top_level_goto_refs_in_range(
        body: &[DirStmt],
        label: &str,
        start_exclusive: usize,
        end_exclusive: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_top_level_goto_refs_in_range(body, label, start_exclusive, end_exclusive)
    }

    pub(crate) fn resolve_terminal_join_target_impl(
        &mut self,
        body: &[DirStmt],
        anchor_idx: usize,
        target_label: &str,
        referenced: &HashMap<String, usize>,
    ) -> Option<(String, usize)> {
        let mut current = target_label.to_string();
        let mut seen = HashSet::default();
        let mut rewrites = 0usize;

        loop {
            if !seen.insert(current.clone()) {
                return None;
            }

            let label_idx = (anchor_idx + 1..body.len()).find(
                |pos| matches!(body.get(*pos), Some(DirStmt::Label(label)) if label == &current),
            )?;
            let next_label_idx =
                (label_idx + 1..body.len()).find(|pos| matches!(body[*pos], DirStmt::Label(_)));
            let Some(next_label_idx) = next_label_idx else {
                if rewrites > 0 {
                    self.telemetry
                        .structuring
                        .canonicalized_guarded_tail_shape_count += rewrites;
                }
                return Some((current, label_idx));
            };
            let DirStmt::Label(next_label) = &body[next_label_idx] else {
                unreachable!();
            };
            let segment = &body[label_idx + 1..next_label_idx];
            let top_level_window_refs =
                Self::count_top_level_goto_refs_in_range(body, &current, anchor_idx, label_idx);
            let hop_ref_budget = if rewrites == 0 {
                top_level_window_refs + 1
            } else {
                top_level_window_refs
            };
            let no_nonlocal_refs = referenced.get(&current).copied().unwrap_or(0) <= hop_ref_budget;
            if no_nonlocal_refs
                && (Self::is_trivial_join_forward_segment(segment, next_label)
                    || Self::is_trivial_join_forward_or_pure_segment(segment, next_label))
            {
                current = next_label.clone();
                rewrites += 1;
                continue;
            }

            if rewrites > 0 {
                self.telemetry
                    .structuring
                    .canonicalized_guarded_tail_shape_count += rewrites;
            }
            return Some((current, label_idx));
        }
    }

    pub(super) fn resolve_alias_redirect(
        label: &str,
        redirects: &HashMap<String, Option<String>>,
    ) -> Option<String> {
        fission_midend_structuring::guarded_tail::pure_hir::resolve_alias_redirect(label, redirects)
    }

    pub(super) fn count_goto_refs_in_stmt(stmt: &DirStmt, out: &mut HashMap<String, usize>) {
        fission_midend_structuring::guarded_tail::pure_hir::count_goto_refs_in_stmt(stmt, out)
    }

    pub(super) fn goto_ref_counts(body: &[DirStmt]) -> HashMap<String, usize> {
        fission_midend_structuring::guarded_tail::pure_hir::goto_ref_counts(body)
    }

    pub(super) fn rewrite_goto_label_in_stmt(stmt: &mut DirStmt, from: &str, to: &str) {
        fission_midend_structuring::guarded_tail::pure_hir::rewrite_goto_label_in_stmt(stmt, from, to)
    }

    pub(super) fn rewrite_goto_label_in_stmts(stmts: &mut [DirStmt], from: &str, to: &str) {
        fission_midend_structuring::guarded_tail::pure_hir::rewrite_goto_label_in_stmts(stmts, from, to)
    }

    pub(super) fn terminalizable_join_alias_target(
        body: &[DirStmt],
        label_idx: usize,
    ) -> Option<(String, usize)> {
        fission_midend_structuring::guarded_tail::pure_hir::terminalizable_join_alias_target(body, label_idx)
    }

    pub(super) fn resolve_terminal_tail_exit_stmt(
        body: &[DirStmt],
        target_label: &str,
    ) -> Option<DirStmt> {
        fission_midend_structuring::guarded_tail::pure_hir::resolve_terminal_tail_exit_stmt(body, target_label)
    }

    pub(super) fn flatten_guarded_tail_segment(segment: &[DirStmt], out: &mut Vec<DirStmt>) {
        fission_midend_structuring::guarded_tail::pure_hir::flatten_guarded_tail_segment(segment, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_before_alias_ownership_internalizes_same_guard_family_ref() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("reg".to_string()),
                then_body: vec![DirStmt::Goto("block_tail".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Expr(DirExpr::Var("middle".to_string())),
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("block_mid".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Goto("block_mid".to_string()),
            DirStmt::Label("block_mid".to_string()),
            DirStmt::If {
                cond: DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Goto("block_tail".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Goto("block_tail".to_string()),
            DirStmt::Label("block_tail".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let proof =
            PreviewBuilder::build_nested_before_alias_ownership_proof(&body, 1, 8, "block_mid", 1);

        assert_eq!(
            proof.class,
            NestedBeforeOwnershipClass::GuardFamilyInternalizable
        );
        assert_eq!(
            proof.legality_reason,
            AliasOwnershipLegalityReason::Complete
        );
        assert_eq!(proof.internalized_nested_before, 1);
        assert!(proof.is_complete());
    }

    #[test]
    fn nested_before_alias_ownership_internalizes_paired_boundary_refs() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Expr(DirExpr::Var("payload".to_string())),
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("join0".to_string()),
            DirStmt::Expr(DirExpr::Var("body".to_string())),
            DirStmt::Goto("terminal".to_string()),
            DirStmt::Label("terminal".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let proof =
            PreviewBuilder::build_nested_before_alias_ownership_proof(&body, 1, 6, "join0", 2);

        assert_eq!(
            proof.class,
            NestedBeforeOwnershipClass::PairedBoundaryInternalizable
        );
        assert_eq!(
            proof.legality_reason,
            AliasOwnershipLegalityReason::Complete
        );
        assert_eq!(proof.internalized_nested_before, 2);
        assert!(proof.is_complete());
    }
}
