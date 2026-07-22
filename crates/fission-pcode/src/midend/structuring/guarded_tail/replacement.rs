use fission_midend_structuring::guarded_tail::ConditionAssumption;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn expr_contains_var(expr: &DirExpr, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::expr_contains_var(expr, name)
    }


    pub(super) fn lvalue_contains_var(lhs: &DirLValue, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::lvalue_contains_var(lhs, name)
    }


    pub(super) fn replace_var_in_expr(expr: &mut DirExpr, name: &str, replacement: &DirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_expr(expr, name, replacement)
    }


    fn replace_var_in_lvalue(lhs: &mut DirLValue, name: &str, replacement: &DirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_lvalue(lhs, name, replacement)
    }


    pub(super) fn replace_var_in_stmt(stmt: &mut DirStmt, name: &str, replacement: &DirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_stmt(stmt, name, replacement)
    }


    pub(super) fn count_var_defs_stmt(stmt: &DirStmt, target: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_defs_stmt(stmt, target)
    }


    fn count_var_reads_expr(expr: &DirExpr, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_expr(expr, name)
    }


    fn count_var_reads_lvalue(lhs: &DirLValue, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_lvalue(lhs, name)
    }


    pub(super) fn count_var_reads_stmt(stmt: &DirStmt, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_stmt(stmt, name)
    }


    pub(super) fn find_guarded_tail_preexisting_source(
        body: &[DirStmt],
        if_idx: usize,
        binding_name: &str,
    ) -> Option<DirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::find_guarded_tail_preexisting_source(body, if_idx, binding_name)
    }

    pub(super) fn resolve_guarded_tail_else_source(
        body: &[DirStmt],
        if_idx: usize,
        binding_name: &str,
        cache: &mut GuardedTailReplacementCache,
    ) -> Option<DirExpr> {
        fission_midend_structuring::guarded_tail::pure_hir::resolve_guarded_tail_else_source(body, if_idx, binding_name, cache)
    }

    pub(super) fn classify_stmt_read_kind(
        stmt: &DirStmt,
        name: &str,
    ) -> Option<GuardedTailReadKind> {
        fission_midend_structuring::guarded_tail::pure_hir::classify_stmt_read_kind(stmt, name)
    }

    fn condition_matches_assumption(
        expr: &DirExpr,
        assumption: &ConditionAssumption,
    ) -> Option<bool> {
        fission_midend_structuring::guarded_tail::pure_hir::condition_matches_assumption(expr, assumption)
    }

    pub(super) fn evaluate_condition_assumptions(
        expr: &DirExpr,
        assumptions: &[ConditionAssumption],
    ) -> Option<bool> {
        fission_midend_structuring::guarded_tail::pure_hir::evaluate_condition_assumptions(expr, assumptions)
    }

    pub(super) fn local_forward_branch_target(
        then_body: &[DirStmt],
        else_body: &[DirStmt],
    ) -> Option<(String, bool)> {
        fission_midend_structuring::guarded_tail::pure_hir::local_forward_branch_target(then_body, else_body)
    }
}
