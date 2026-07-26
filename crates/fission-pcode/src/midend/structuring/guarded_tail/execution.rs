use super::*;
use fission_midend_structuring::guarded_tail::{ConditionAssumption, GuardedTailRewriteResult};

impl<'a> PreviewBuilder<'a> {
    fn rewrite_guarded_tail_sequence(
        stmts: &[PreHirStmt],
        join_label: &str,
        assumptions: &[ConditionAssumption],
    ) -> GuardedTailRewriteResult {
        fission_midend_structuring::guarded_tail::pure_hir::rewrite_guarded_tail_sequence(
            stmts,
            join_label,
            assumptions,
        )
    }

    fn collect_guarded_tail_exported_bindings(
        &mut self,
        middle: &[PreHirStmt],
        follow_tail: &[PreHirStmt],
    ) -> Result<Vec<GuardedTailExportedBinding>, GuardedTailExecutionRejection> {
        fission_midend_structuring::guarded_tail::collect_guarded_tail_exported_bindings(
            self,
            middle,
            follow_tail,
        )
    }

    fn try_build_guarded_tail_witness(
        &mut self,
        body: &[PreHirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<RegionShapeWitness, GuardedTailWitnessRejection>> {
        fission_midend_structuring::guarded_tail::try_build_guarded_tail_witness(
            self, body, idx, referenced,
        )
    }

    fn collect_guarded_tail_candidate_reads(
        body: &[PreHirStmt],
        middle: &[PreHirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> Vec<GuardedTailReplacementRead> {
        fission_midend_structuring::guarded_tail::pure_hir::collect_guarded_tail_candidate_reads(
            body, middle, if_idx, label_idx, label,
        )
    }

    pub(crate) fn try_build_guarded_tail_trial(
        &mut self,
        body: &[PreHirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<GuardedTailTrial, GuardedTailWitnessRejection>> {
        fission_midend_structuring::guarded_tail::try_build_guarded_tail_trial(
            self, body, idx, referenced,
        )
    }

    fn guarded_tail_stmt_is_execution_safe(stmt: &PreHirStmt, label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::guarded_tail_stmt_is_execution_safe(
            stmt, label,
        )
    }

    fn guarded_tail_middle_is_execution_safe(middle: &[PreHirStmt], label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::guarded_tail_middle_is_execution_safe(
            middle, label,
        )
    }

    pub(crate) fn verify_guarded_tail_trial(
        &mut self,
        body: &[PreHirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
    ) -> GuardedTailVerification {
        fission_midend_structuring::guarded_tail::verify_guarded_tail_trial(self, body, idx, trial)
    }

    pub(crate) fn build_guarded_tail_execution_plan(
        &mut self,
        body: &[PreHirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
        verification: &GuardedTailVerification,
    ) -> Result<GuardedTailExecutionPlan, GuardedTailExecutionRejection> {
        fission_midend_structuring::guarded_tail::build_guarded_tail_execution_plan(
            self,
            body,
            idx,
            trial,
            verification,
        )
    }

    fn apply_guarded_tail_replacement_read(
        stmt: &mut PreHirStmt,
        merge: &GuardedTailSyntheticMerge,
    ) {
        fission_midend_structuring::guarded_tail::pure_hir::apply_guarded_tail_replacement_read(
            stmt, merge,
        )
    }

    pub(crate) fn execute_guarded_tail_plan(
        &mut self,
        body: &mut Vec<PreHirStmt>,
        idx: usize,
        trial: GuardedTailTrial,
        plan: GuardedTailExecutionPlan,
        cond: PreHirExpr,
    ) {
        fission_midend_structuring::guarded_tail::execute_guarded_tail_plan(
            self, body, idx, trial, plan, cond,
        )
    }

    pub(crate) fn discover_guarded_tail_candidates_in_body(&mut self, body: &[PreHirStmt]) {
        fission_midend_structuring::guarded_tail::discover_guarded_tail_candidates_in_body(
            self, body,
        )
    }
}

fn statement_sequence_always_terminates(stmts: &[PreHirStmt]) -> bool {
    fission_midend_structuring::guarded_tail::pure_hir::statement_sequence_always_terminates(stmts)
}

fn stmt_always_terminates(stmt: &PreHirStmt) -> bool {
    fission_midend_structuring::guarded_tail::pure_hir::stmt_always_terminates(stmt)
}
