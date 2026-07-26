use super::*;
use fission_midend_structuring::guarded_tail::ConditionAssumption;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn expr_contains_var(expr: &PreHirExpr, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::expr_contains_var(expr, name)
    }

    pub(super) fn lvalue_contains_var(lhs: &PreHirLValue, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::lvalue_contains_var(lhs, name)
    }

    pub(super) fn replace_var_in_expr(expr: &mut PreHirExpr, name: &str, replacement: &PreHirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_expr(expr, name, replacement)
    }

    fn replace_var_in_lvalue(lhs: &mut PreHirLValue, name: &str, replacement: &PreHirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_lvalue(lhs, name, replacement)
    }

    pub(super) fn replace_var_in_stmt(stmt: &mut PreHirStmt, name: &str, replacement: &PreHirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_stmt(stmt, name, replacement)
    }

    pub(super) fn count_var_defs_stmt(stmt: &PreHirStmt, target: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_defs_stmt(stmt, target)
    }

    fn count_var_reads_expr(expr: &PreHirExpr, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_expr(expr, name)
    }

    fn count_var_reads_lvalue(lhs: &PreHirLValue, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_lvalue(lhs, name)
    }

    pub(super) fn count_var_reads_stmt(stmt: &PreHirStmt, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_stmt(stmt, name)
    }

    pub(super) fn find_guarded_tail_preexisting_source(
        body: &[PreHirStmt],
        if_idx: usize,
        binding_name: &str,
    ) -> Option<PreHirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::find_guarded_tail_preexisting_source(
            body,
            if_idx,
            binding_name,
        )
    }

    pub(super) fn resolve_guarded_tail_else_source(
        body: &[PreHirStmt],
        if_idx: usize,
        binding_name: &str,
        cache: &mut GuardedTailReplacementCache,
    ) -> Option<PreHirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::resolve_guarded_tail_else_source(
            body,
            if_idx,
            binding_name,
            cache,
        )
    }

    pub(super) fn classify_stmt_read_kind(
        stmt: &PreHirStmt,
        name: &str,
    ) -> Option<GuardedTailReadKind> {
        fission_midend_structuring::guarded_tail::pure_hir::classify_stmt_read_kind(stmt, name)
    }

    fn condition_matches_assumption(
        expr: &PreHirExpr,
        assumption: &ConditionAssumption,
    ) -> Option<bool> {
        fission_midend_structuring::guarded_tail::pure_hir::condition_matches_assumption(
            expr, assumption,
        )
    }

    pub(super) fn evaluate_condition_assumptions(
        expr: &PreHirExpr,
        assumptions: &[ConditionAssumption],
    ) -> Option<bool> {
        fission_midend_structuring::guarded_tail::pure_hir::evaluate_condition_assumptions(
            expr,
            assumptions,
        )
    }

    pub(super) fn local_forward_branch_target(
        then_body: &[PreHirStmt],
        else_body: &[PreHirStmt],
    ) -> Option<(String, bool)> {
        fission_midend_structuring::guarded_tail::pure_hir::local_forward_branch_target(
            then_body, else_body,
        )
    }
}
