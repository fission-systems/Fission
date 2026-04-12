use super::*;

impl<'a> PreviewBuilder<'a> {
    fn classify_plain_if_candidate(
        &mut self,
        idx: usize,
        budget: &mut IfLoweringBudget,
        diag: bool,
    ) -> Result<Option<PlainIfCandidate>, MlilPreviewError> {
        let block_addr = self.block_start_address(idx);
        if budget.checkpoint("cond_prefix_pre") {
            return Ok(None);
        }
        let cond_block = self.pcode_block(idx).clone();
        let cond_prefix = self.lower_block_stmts(&cond_block)?;
        if budget.checkpoint("cond_prefix_post") {
            return Ok(None);
        }
        if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
            self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_nontrivial_prefix");
            return Ok(None);
        }

        let Some(next_idx) = self.fallthrough_index(idx) else {
            self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_no_unique_follow");
            return Ok(None);
        };

        if budget.checkpoint("terminator_pre") {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_not_conditional");
            return Ok(None);
        };
        if budget.checkpoint("terminator_post") {
            return Ok(None);
        }

        let next_addr = self.block_target_key(next_idx);
        let (cond, body_idx, exit) = if true_target == next_addr {
            let exit = if let Some(join_addr) = false_target {
                let Some(join_idx) = self.forward_join_idx_from_address(idx, join_addr) else {
                    self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_nonforward_join");
                    return Ok(None);
                };
                let expected = LinearExit::Join(join_idx);
                if self.linear_exit_with_budget(next_idx, Some(budget))? != Some(expected) {
                    self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_open_body_tail");
                    return Ok(None);
                }
                expected
            } else {
                let Some(exit) = self.linear_exit_with_budget(next_idx, Some(budget))? else {
                    self.log_try_lower_if_reject(
                        diag,
                        idx,
                        block_addr,
                        "rejected_no_unique_follow",
                    );
                    return Ok(None);
                };
                if !self.is_forward_exit_from(idx, exit) {
                    self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_nonforward_join");
                    return Ok(None);
                }
                exit
            };
            (cond, next_idx, exit)
        } else if false_target == Some(next_addr) {
            let Some(join_idx) = self.forward_join_idx_from_address(idx, true_target) else {
                self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_nonforward_join");
                return Ok(None);
            };
            let exit = LinearExit::Join(join_idx);
            if self.linear_exit_with_budget(next_idx, Some(budget))? != Some(exit) {
                self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_open_body_tail");
                return Ok(None);
            }
            (negate_expr(cond), next_idx, exit)
        } else {
            self.log_try_lower_if_reject(diag, idx, block_addr, "rejected_no_unique_follow");
            return Ok(None);
        };

        Ok(Some(PlainIfCandidate {
            cond_prefix,
            cond,
            body_idx,
            exit,
            block_addr,
        }))
    }

    pub(in crate::nir::structuring) fn try_lower_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let mut budget = IfLoweringBudget::new(
            self.options,
            idx,
            self.block_start_address(idx),
            "try_lower_if",
        );
        if diag {
            eprintln!(
                "[DIAG] try_lower_if start: idx={} block=0x{:x} x86_guard={}",
                idx,
                self.block_start_address(idx),
                budget.enabled
            );
        }

        let result = (|| {
            let Some(candidate) = self.classify_plain_if_candidate(idx, &mut budget, diag)? else {
                return Ok(None);
            };

            if diag {
                eprintln!(
                    "[DIAG] try_lower_if chosen_exit: idx={} block=0x{:x} body_idx={} exit={:?}",
                    idx, candidate.block_addr, candidate.body_idx, candidate.exit
                );
                eprintln!(
                    "[DIAG] try_lower_if lower_linear_body {}: idx={} block=0x{:x} body_idx={} exit={:?}",
                    if self.has_linear_body_cache(candidate.body_idx, candidate.exit) {
                        "cache_hit"
                    } else {
                        "cache_miss"
                    },
                    idx,
                    candidate.block_addr,
                    candidate.body_idx,
                    candidate.exit
                );
            }

            let Some((body, skip_to)) = self.lower_linear_body_with_budget(
                candidate.body_idx,
                candidate.exit,
                Some(&mut budget),
            )?
            else {
                return Ok(None);
            };

            let stmt = HirStmt::If {
                cond: candidate.cond,
                then_body: body,
                else_body: Vec::new(),
            };
            if candidate.cond_prefix.is_empty() {
                Ok(Some((stmt, skip_to)))
            } else {
                let mut wrapped = candidate.cond_prefix;
                wrapped.push(stmt);
                Ok(Some((HirStmt::Block(wrapped), skip_to)))
            }
        })();

        if diag {
            eprintln!(
                "[DIAG] try_lower_if done: idx={} block=0x{:x} elapsed={:.3}s success={} budget_tripped={}",
                idx,
                self.block_start_address(idx),
                budget.start.elapsed().as_secs_f64(),
                matches!(result, Ok(Some(_))),
                budget.tripped
            );
        }
        result
    }
}
