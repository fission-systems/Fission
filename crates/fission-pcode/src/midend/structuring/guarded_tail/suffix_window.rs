use super::*;

// Suffix-window types owned by fission-midend-structuring::guarded_tail::types.
use fission_midend_structuring::guarded_tail::{
    ExternalEntryRefKind, NestedBoundaryPairTrace, NestedBoundaryRefTrace,
    NestedEntryBoundaryContext, NestedSuffixShapeKind, SuffixCallEffectShapeKind,
    SuffixExternalEntryBudget, SuffixSideEffectShapeKind, SuffixTailRejection,
};

impl<'a> PreviewBuilder<'a> {
    fn suffix_call_expr(stmt: &HirStmt) -> Option<(&str, &[HirExpr], bool)> {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_call_expr(stmt)
    }

    fn top_level_label_definition_count_for_owned_tail(body: &[HirStmt], label: &str) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_label_definition_count_for_owned_tail(body, label)
    }

    fn stmt_is_sink_safe_return_goto_for_owned_tail(stmt: &HirStmt, body: &[HirStmt]) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_sink_safe_return_goto_for_owned_tail(stmt, body)
    }

    fn suffix_stmt_has_nested_or_nonlocal_ref(stmt: &HirStmt) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_stmt_has_nested_or_nonlocal_ref(stmt)
    }

    fn classify_nested_suffix_shape(
        stmt: &HirStmt,
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> NestedSuffixShapeKind {
        fission_midend_structuring::guarded_tail::pure_hir::classify_nested_suffix_shape(stmt, body, current_label_idx, terminal_label_idx, next_label)
    }

    fn expr_contains_load(expr: &HirExpr) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::expr_contains_load(expr)
    }

    pub(super) fn suffix_expr_contains_call(expr: &HirExpr) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_expr_contains_call(expr)
    }

    fn classify_suffix_side_effect_shape(stmt: &HirStmt) -> SuffixSideEffectShapeKind {
        fission_midend_structuring::guarded_tail::pure_hir::classify_suffix_side_effect_shape(stmt)
    }

    fn call_target_is_known_pure_helper(target: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::call_target_is_known_pure_helper(target)
    }

    fn call_target_is_memory_mutating(target: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::call_target_is_memory_mutating(target)
    }

    fn call_target_is_control_effect(target: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::call_target_is_control_effect(target)
    }

    fn classify_suffix_call_effect_shape(stmt: &HirStmt) -> SuffixCallEffectShapeKind {
        fission_midend_structuring::guarded_tail::pure_hir::classify_suffix_call_effect_shape(stmt)
    }

    fn stmt_reads_binding_only_in_owned_safe_context(stmt: &HirStmt, name: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_reads_binding_only_in_owned_safe_context(stmt, name)
    }

    fn suffix_memory_read_only_assign_is_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_memory_read_only_assign_is_owned_safe(body, stmt_idx, terminal_label_idx)
    }

    fn suffix_known_pure_helper_call_is_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_known_pure_helper_call_is_owned_safe(body, stmt_idx, terminal_label_idx)
    }

    pub(crate) fn trace_suffix_unknown_call_provenance_impl(&self, stmt_idx: usize, stmt: &HirStmt) {
        let Some((target, _args, return_used)) = Self::suffix_call_expr(stmt) else {
            return;
        };

        let target_addr = crate::midend::ir::parse_call_target_address(target);
        let import = crate::midend::is_known_api_signature(target);
        let internal = !import && target_addr.is_some();
        let summary_available = import
            || Self::call_target_is_known_pure_helper(target)
            || Self::call_target_is_memory_mutating(target)
            || Self::call_target_is_control_effect(target);
        let binary_function_present = target_addr.is_some_and(|addr| {
            self.binary
                .is_some_and(|binary| binary.functions.iter().any(|func| func.address == addr))
        });
        let target_ref = target_addr.and_then(|addr| {
            self.type_context
                .and_then(|ctx| ctx.call_target_refs.get(&addr))
        });
        let effect_summary = self
            .type_context
            .and_then(|ctx| ctx.call_effect_summaries.get(target));
        let summary_available = effect_summary.is_some() || summary_available;
        let writes_memory = effect_summary
            .and_then(|summary| summary.writes_memory)
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("unknown");
        let writes_global = "unknown";
        let may_call_unknown = effect_summary
            .and_then(|summary| summary.may_call_unknown)
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("unknown");
        let may_exit = effect_summary
            .and_then(|summary| summary.may_exit)
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("unknown");
        let effect_summary_source = effect_summary
            .and_then(|summary| summary.source)
            .map(|source| format!("{source:?}"))
            .unwrap_or_else(|| "None".to_string());

        eprintln!(
            "[GT-TRACE] suffix-unknown-call-provenance stmt_idx={} target={} target_addr={:?} internal={} import={} summary_available={}",
            stmt_idx, target, target_addr, internal, import, summary_available
        );
        eprintln!(
            "[GT-TRACE] suffix-unknown-call-summary target={} binary_function_present={} target_ref_present={} target_ref_provenance={} effect_summary_source={}",
            target,
            binary_function_present,
            target_ref.is_some(),
            target_ref
                .map(|target_ref| format!("{:?}", target_ref.provenance))
                .unwrap_or_else(|| "None".to_string()),
            effect_summary_source,
        );
        eprintln!(
            "[GT-TRACE] suffix-unknown-call-effect target={} writes_memory={} writes_global={} may_call_unknown={} may_exit={} return_used={}",
            target, writes_memory, writes_global, may_call_unknown, may_exit, return_used
        );
    }

    pub(crate) fn suffix_call_uses_preview_unsafe_callee_impl(&self, stmt: &HirStmt) -> Option<String> {
        let (target, _args, _return_used) = Self::suffix_call_expr(stmt)?;
        let summary = self
            .type_context
            .and_then(|ctx| ctx.call_effect_summaries.get(target))?;
        if summary.source != Some(CallEffectSummarySource::PreviewCalleeAnalysis) {
            return None;
        }
        let unsafe_effect = summary.writes_memory == Some(true)
            || summary.may_call_unknown == Some(true)
            || summary.may_exit == Some(true);
        unsafe_effect.then(|| target.to_string())
    }

    fn resolve_suffix_redirect_to_terminal(
        body: &[HirStmt],
        target_label: &str,
        next_label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::resolve_suffix_redirect_to_terminal(body, target_label, next_label)
    }

    fn classify_suffix_stmt(
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::pure_hir::classify_suffix_stmt(stmt, body, stmt_idx, current_label_idx, terminal_label_idx, next_label)
    }

    fn classify_suffix_stmt_with_diag(
        &mut self,
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::classify_suffix_stmt_with_diag(self, stmt, body, stmt_idx, current_label_idx, terminal_label_idx, next_label)
    }

    fn suffix_stmt_is_terminal_join_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        next_label_idx: usize,
        terminal_label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_stmt_is_terminal_join_owned_safe(body, stmt_idx, next_label_idx, terminal_label)
    }

    fn count_candidate_internal_top_level_refs_in_suffix_window(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_candidate_internal_top_level_refs_in_suffix_window(body, label, anchor_idx, terminal_label_idx)
    }

    fn count_suffix_safe_self_terminal_refs_in_suffix_window(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_suffix_safe_self_terminal_refs_in_suffix_window(body, label, anchor_idx, terminal_label_idx)
    }

    fn compute_suffix_external_entry_budget(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        raw_refs: usize,
        rewrites: usize,
    ) -> SuffixExternalEntryBudget {
        fission_midend_structuring::guarded_tail::pure_hir::compute_suffix_external_entry_budget(body, label, anchor_idx, current_label_idx, terminal_label_idx, raw_refs, rewrites)
    }

    fn stmt_is_single_goto_then_if_to_label<'b>(
        stmt: &'b HirStmt,
        label: &str,
    ) -> Option<&'b HirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_single_goto_then_if_to_label(stmt, label)
    }

    pub(super) fn stmt_is_single_branch_if_to_label<'b>(
        stmt: &'b HirStmt,
        label: &str,
    ) -> Option<&'b HirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_single_branch_if_to_label(stmt, label)
    }

    pub(super) fn exprs_share_guard_family(lhs: &HirExpr, rhs: &HirExpr) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::exprs_share_guard_family(lhs, rhs)
    }

    fn guard_family_match_reason(lhs: &HirExpr, rhs: &HirExpr) -> &'static str {
        fission_midend_structuring::guarded_tail::pure_hir::guard_family_match_reason(lhs, rhs)
    }

    fn find_terminal_guard_family_match_excluding(
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &HirExpr,
        excluded_stmt_idx: Option<usize>,
    ) -> Option<HirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::find_terminal_guard_family_match_excluding(body, current_label_idx, terminal_label_idx, entry_cond, excluded_stmt_idx)
    }

    fn suffix_window_has_terminal_guard_family_match(
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &HirExpr,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_window_has_terminal_guard_family_match(body, current_label_idx, terminal_label_idx, entry_cond)
    }

    fn nested_terminal_join_tail_is_guard_family_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::nested_terminal_join_tail_is_guard_family_owned_safe(body, stmt_idx, current_label_idx, terminal_label_idx)
    }

    fn nested_conditional_entry_is_guard_family_internal(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        stmt_idx: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::nested_conditional_entry_is_guard_family_internal(body, label, anchor_idx, current_label_idx, terminal_label_idx, stmt_idx)
    }

    fn nested_entry_boundary_context(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> NestedEntryBoundaryContext {
        fission_midend_structuring::guarded_tail::pure_hir::nested_entry_boundary_context(body, label, anchor_idx, current_label_idx, terminal_label_idx)
    }

    fn collect_nested_boundary_ref_traces(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> Vec<NestedBoundaryRefTrace> {
        fission_midend_structuring::guarded_tail::pure_hir::collect_nested_boundary_ref_traces(body, label, anchor_idx, terminal_label_idx)
    }

    fn build_nested_boundary_pair_trace(
        refs: &[NestedBoundaryRefTrace],
    ) -> NestedBoundaryPairTrace {
        fission_midend_structuring::guarded_tail::pure_hir::build_nested_boundary_pair_trace(refs)
    }

    pub(super) fn count_internalized_paired_nested_boundary_refs(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        raw_refs: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_internalized_paired_nested_boundary_refs(body, label, anchor_idx, current_label_idx, terminal_label_idx, raw_refs)
    }

    pub(super) fn count_internalized_guard_family_nested_conditional_entries(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_internalized_guard_family_nested_conditional_entries(body, label, anchor_idx, current_label_idx, terminal_label_idx)
    }

    fn classify_external_entry_ref_kind_for_stmt(
        stmt: &HirStmt,
        label: &str,
    ) -> ExternalEntryRefKind {
        fission_midend_structuring::guarded_tail::pure_hir::classify_external_entry_ref_kind_for_stmt(stmt, label)
    }

    fn classify_external_entry_ref_kind(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> Option<(ExternalEntryRefKind, usize)> {
        fission_midend_structuring::guarded_tail::pure_hir::classify_external_entry_ref_kind(body, label, anchor_idx, terminal_label_idx)
    }

    fn suffix_is_nonowned_terminal_tail(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label: &str,
        start_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::pure_hir::suffix_is_nonowned_terminal_tail(body, anchor_idx, start_label, start_label_idx, terminal_label_idx, referenced)
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
        fission_midend_structuring::guarded_tail::suffix_is_nonowned_terminal_tail_with_diag(self, body, anchor_idx, start_label, start_label_idx, terminal_label_idx, referenced)
    }

    fn candidate_window_can_shrink_to_label(
        body: &[HirStmt],
        anchor_idx: usize,
        candidate_label: &str,
        candidate_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        fission_midend_structuring::guarded_tail::pure_hir::candidate_window_can_shrink_to_label(body, anchor_idx, candidate_label, candidate_label_idx, terminal_label_idx, referenced)
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
        fission_midend_structuring::guarded_tail::candidate_window_can_shrink_to_label_with_diag(self, body, anchor_idx, candidate_label, candidate_label_idx, terminal_label_idx, referenced)
    }

    pub(super) fn find_earliest_owned_join_label(
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        fission_midend_structuring::guarded_tail::pure_hir::find_earliest_owned_join_label(body, anchor_idx, terminal_label_idx, referenced, trace_enabled)
    }

    pub(crate) fn find_earliest_owned_join_label_with_diag_impl(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        fission_midend_structuring::guarded_tail::find_earliest_owned_join_label_with_diag(self, body, anchor_idx, terminal_label_idx, referenced, trace_enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_if_goto(label: &str) -> HirStmt {
        HirStmt::If {
            cond: HirExpr::Var("cond".to_string()),
            then_body: vec![HirStmt::Goto(label.to_string())],
            else_body: Vec::new(),
        }
    }

    fn assert_suffix_accepts(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label_idx: usize,
        terminal_label_idx: usize,
    ) {
        let HirStmt::Label(start_label) = &body[start_label_idx] else {
            panic!("start label missing at {start_label_idx}");
        };
        let referenced = collect_referenced_label_counts(body);
        let result = PreviewBuilder::suffix_is_nonowned_terminal_tail(
            body,
            anchor_idx,
            start_label,
            start_label_idx,
            terminal_label_idx,
            &referenced,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_ok(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) {
        let result = PreviewBuilder::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_rejection(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: SuffixTailRejection,
    ) {
        let result = PreviewBuilder::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Err(expected));
    }

    fn assert_nested_suffix_shape_kind(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: NestedSuffixShapeKind,
    ) {
        let stmt = &body[stmt_idx];
        let kind = PreviewBuilder::classify_nested_suffix_shape(
            stmt,
            body,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(kind, expected);
    }

    fn assert_suffix_side_effect_shape_kind(stmt: HirStmt, expected: SuffixSideEffectShapeKind) {
        let kind = PreviewBuilder::classify_suffix_side_effect_shape(&stmt);
        assert_eq!(kind, expected);
    }

    fn assert_suffix_call_effect_shape_kind(stmt: HirStmt, expected: SuffixCallEffectShapeKind) {
        let kind = PreviewBuilder::classify_suffix_call_effect_shape(&stmt);
        assert_eq!(kind, expected);
    }

    fn assert_suffix_external_budget(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        rewrites: usize,
        expected: SuffixExternalEntryBudget,
    ) {
        let referenced = collect_referenced_label_counts(body);
        let raw_refs = referenced.get(label).copied().unwrap_or(0);
        let budget = PreviewBuilder::compute_suffix_external_entry_budget(
            body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
            rewrites,
        );
        assert_eq!(budget, expected);
    }

    fn assert_external_entry_ref_kind(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
        expected: Option<(ExternalEntryRefKind, usize)>,
    ) {
        let classified = PreviewBuilder::classify_external_entry_ref_kind(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        );
        assert_eq!(classified, expected);
    }

    #[test]
    fn earliest_owned_join_window_accepts_sink_safe_terminal_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join1".to_string()),
            HirStmt::Label("join1".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 6, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_empty_block_alias_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_alias_redirect_only_suffix() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join1".to_string()),
            HirStmt::Label("join1".to_string()),
            HirStmt::Goto("join2".to_string()),
            HirStmt::Label("join2".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 8, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_rejects_side_effectful_suffix() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("not_safe".to_string())),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_external_entry_in_suffix() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 1, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_nested_nonlocal_suffix_ref() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("sink".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 4, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_when_terminal_join_is_already_owned() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 2, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn suffix_accepts_ignorable_and_empty_block_only() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 5);
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_next_label() {
        let body = vec![
            HirStmt::Goto("skip".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Label("skip".to_string()),
            HirStmt::Expr(HirExpr::Var("redirect_gap".to_string())),
            HirStmt::Goto("alias".to_string()),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 3, "alias");
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_terminal_return() {
        let body = vec![
            HirStmt::Goto("skip".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Label("skip".to_string()),
            HirStmt::Expr(HirExpr::Var("redirect_gap".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("done".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 6, "alias");
    }

    #[test]
    fn suffix_accepts_self_terminal_join_goto_with_pure_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 9);
    }

    #[test]
    fn suffix_budget_counts_candidate_internal_top_level_refs_inside_window() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join0".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 1,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_keeps_nested_candidate_ref_external() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_internalizes_same_guard_family_nested_conditional_entry() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 1,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_different_guard_family_nested_entry() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("other_cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_internalizes_paired_same_guard_nested_boundary() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("body".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 2,
                effective_external_refs: 0,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_paired_nested_boundary_with_guard_mismatch() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("other".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("body".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_paired_boundary_when_ref_kind_is_not_nested() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("body".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_single_goto_then() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("next".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedSingleGotoThen,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_guard_family_mismatch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("other".to_string()),
                then_body: vec![HirStmt::Goto("next".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedGuardFamilyMismatch,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_crosses_terminal_join() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            4,
            "next",
            NestedSuffixShapeKind::NestedCrossesTerminalJoin,
        );
    }

    #[test]
    fn suffix_accepts_nested_terminal_join_tail_same_guard_family_then_branch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_accepts_nested_terminal_join_tail_negated_guard_match_else_branch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: Vec::new(),
                else_body: vec![HirStmt::Goto("terminal".to_string())],
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_different_guard_family() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("other".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_nonterminal_target() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("next".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_nonempty_else_payload() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: vec![HirStmt::Expr(HirExpr::Var("payload".to_string()))],
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_side_effectful_branch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![
                    HirStmt::Expr(HirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Unknown,
                    }),
                    HirStmt::Goto("terminal".to_string()),
                ],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_read_only_assign() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar116".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("xVar43".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixSideEffectShapeKind::MemoryReadOnlyAssign,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_call_expr_side_effect() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixSideEffectShapeKind::CallExprSideEffect,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_void_unknown_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::VoidUnknownCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_ignored_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            SuffixCallEffectShapeKind::ReturnValueIgnoredCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_assigned_local() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Call {
                    target: "helper".to_string(),
                    args: vec![HirExpr::Var("arg".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::ReturnValueAssignedLocal,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_pure_known_helper_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::PureKnownHelperCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_flag_intrinsics_as_pure_helpers() {
        for target in ["__carry", "__scarry", "__sborrow"] {
            assert_suffix_call_effect_shape_kind(
                HirStmt::Assign {
                    lhs: HirLValue::Var("tmp".to_string()),
                    rhs: HirExpr::Call {
                        target: target.to_string(),
                        args: vec![
                            HirExpr::Var("lhs".to_string()),
                            HirExpr::Const(
                                1,
                                NirType::Int {
                                    bits: 32,
                                    signed: false,
                                },
                            ),
                        ],
                        ty: NirType::Bool,
                    },
                },
                SuffixCallEffectShapeKind::PureKnownHelperCall,
            );
        }
    }

    #[test]
    fn guarded_tail_pure_value_accepts_flag_intrinsic_exprs() {
        for target in ["__carry", "__scarry", "__sborrow"] {
            assert!(PreviewBuilder::test_expr_is_pure_value(&HirExpr::Call {
                target: target.to_string(),
                args: vec![
                    HirExpr::Var("lhs".to_string()),
                    HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                ],
                ty: NirType::Bool,
            }));
        }
    }

    #[test]
    fn suffix_call_effect_shape_classifies_memory_mutating_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "memcpy".to_string(),
                args: vec![
                    HirExpr::Var("dst".to_string()),
                    HirExpr::Var("src".to_string()),
                ],
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            }),
            SuffixCallEffectShapeKind::MemoryMutatingCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_control_effect_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "abort".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::ControlEffectCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_nested_call_as_unknown_effect() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }),
                    rhs: Box::new(HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::UnknownCallEffect,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_write() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            SuffixSideEffectShapeKind::MemoryWrite,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_volatile_or_unknown_load() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Call {
                    target: "addr_provider".to_string(),
                    args: vec![],
                    ty: NirType::Ptr(Box::new(NirType::Int {
                        bits: 8,
                        signed: false,
                    })),
                }),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
        );
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_condition_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_pure_ptroffset() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("base".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Expr(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("loaded".to_string())),
                ty: NirType::Bool,
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_unknown_load_type() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Unknown,
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_reused_in_return() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("loaded".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_when_ptr_contains_call() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Call {
                        target: "ptr_source".to_string(),
                        args: vec![],
                        ty: NirType::Ptr(Box::new(NirType::Int {
                            bits: 8,
                            signed: false,
                        })),
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_memory_visible_alias_risk() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("loaded".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_condition_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_pure_expr_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Expr(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("count".to_string())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_unknown_target() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount64".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_call_arg() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Call {
                        target: "value_provider".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_reused_in_return() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("count".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_memory_visible_alias_risk() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("count".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_ignored_result() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "__popcount".to_string(),
                args: vec![HirExpr::Var("value".to_string())],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn external_entry_kind_classifies_top_level_external_goto() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::TopLevelExternalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_nested_conditional_goto() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::NestedConditionalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_loop_switch_derived_goto() {
        let body = vec![
            HirStmt::While {
                cond: HirExpr::Var("cond".to_string()),
                body: vec![HirStmt::Goto("join0".to_string())],
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::LoopSwitchDerived, 0)),
        );
    }

    #[test]
    fn external_entry_kind_skips_candidate_internal_top_level_goto() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(&body, "join0", 0, 4, None);
    }

    #[test]
    fn suffix_rejects_self_terminal_join_goto_with_nested_tail_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_side_effectful_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_nonterminal_goto() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("other".to_string()),
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx: 2,
                target: "other".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_nested_nonlocal_ref() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("other".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_label_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result = PreviewBuilder::candidate_window_can_shrink_to_label(
            &body,
            0,
            "join0",
            1,
            1,
            &referenced,
        );
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: 1,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_external_entry() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result = PreviewBuilder::candidate_window_can_shrink_to_label(
            &body,
            1,
            "join0",
            2,
            4,
            &referenced,
        );
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasExternalEntry {
                stmt_idx: 2,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_loop_or_switch_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::While {
                cond: HirExpr::Var("cond".to_string()),
                body: vec![],
            },
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_unresolved_alias_redirect() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("unknown".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixAliasRedirectUnresolved {
                stmt_idx: 2,
                label: "unknown".to_string(),
            },
        );
    }
}
