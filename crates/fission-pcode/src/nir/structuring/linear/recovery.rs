use super::cleanup::cleanup_redundant_labels;
use super::linear::{LinearBodyLoweringOutcome, LinearBodyRejectReason};
use super::*;

impl<'a> PreviewBuilder<'a> {
    fn record_region_body_lowering_reject_reason(&mut self, reason: LinearBodyRejectReason) {
        match reason {
            LinearBodyRejectReason::ConditionalTailExitMismatch => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count +=
                    1;
            }
            LinearBodyRejectReason::SuccessorInlineRejected => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_successor_inline_rejected_count += 1;
            }
            LinearBodyRejectReason::RevisitCycle => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_revisit_cycle_count += 1;
            }
            LinearBodyRejectReason::UnsupportedTerminator => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_unsupported_terminator_count += 1;
            }
            LinearBodyRejectReason::TargetIndexMissing
            | LinearBodyRejectReason::ExitMismatch
            | LinearBodyRejectReason::BudgetTripped => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_unsupported_terminator_count += 1;
            }
        }
    }

    fn region_linearized_exit_candidates_algorithmic(
        &self,
        start_idx: usize,
        targeted: &HashSet<u64>,
    ) -> Vec<LinearExit> {
        let mut candidates = Vec::new();
        let search_limit = self.block_count();

        for idx in (start_idx + 1)..search_limit {
            if self.dom_tree.dominates(start_idx, idx) {
                continue;
            }

            let mut reachable_from_region = false;
            for &p in &self.predecessors[idx] {
                if self.dom_tree.dominates(start_idx, p) {
                    reachable_from_region = true;
                    break;
                }
            }

            if reachable_from_region {
                candidates.push(LinearExit::Join(idx));
            } else {
                let block_key = self.block_target_key(idx);
                if targeted.contains(&block_key) {
                    candidates.push(LinearExit::Join(idx));
                }
            }
        }
        candidates
    }

    fn push_unique_region_exit(candidates: &mut Vec<LinearExit>, candidate: LinearExit) {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    pub(crate) fn try_recover_region_linearized_body(
        &mut self,
        start_idx: usize,
        err: &MlilPreviewError,
        targeted: &HashSet<u64>,
        emitted_labels: &mut HashSet<u64>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        if !self.options.region_linearize_structuring {
            return Ok(None);
        }
        if self.options.conservative_irreducible_fallback {
            let scc = self.analyze_cfg_scc();
            if scc.is_irreducible_node(start_idx) {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_irreducible_cfg_count += 1;
                return Ok(None);
            }
        }
        if err.structuring_failure_kind().is_none() {
            self.telemetry
                .structuring
                .region_linearize_rejected_non_structuring_failure_count += 1;
            return Ok(None);
        }

        let mut exits = Vec::new();
        if let Some(exit) = self.linear_exit(start_idx)? {
            Self::push_unique_region_exit(&mut exits, exit);
        }
        for exit in self.region_linearized_exit_candidates_algorithmic(start_idx, targeted) {
            Self::push_unique_region_exit(&mut exits, exit);
        }
        if exits.is_empty() {
            self.telemetry
                .structuring
                .region_linearize_rejected_no_exit_count += 1;
            return Ok(None);
        }

        let mut lowered = None;
        for exit in exits {
            match self.lower_linear_body_for_region_recovery_detailed(start_idx, exit, None)? {
                LinearBodyLoweringOutcome::Lowered(result) => {
                    lowered = Some(result);
                    break;
                }
                LinearBodyLoweringOutcome::Rejected(reason) => {
                    self.record_region_body_lowering_reject_reason(reason);
                }
            }
        }
        let Some((mut body, skip_to)) = lowered else {
            self.telemetry
                .structuring
                .region_linearize_rejected_body_lowering_failed_count += 1;
            return Ok(None);
        };
        if skip_to <= start_idx {
            self.telemetry
                .structuring
                .region_linearize_rejected_non_advancing_count += 1;
            return Ok(None);
        }

        let block_key = self.block_target_key(start_idx);
        if (start_idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
            body.insert(0, HirStmt::Label(block_label(block_key)));
        }

        self.telemetry
            .structuring
            .region_linearize_structuring_count += 1;
        Ok(Some((cleanup_redundant_labels(body), skip_to)))
    }
}
