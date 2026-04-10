use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::structuring) fn try_lower_short_circuit_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if let Some(lowered) = self.try_lower_short_circuit_or(idx)? {
            return Ok(Some(lowered));
        }
        if let Some(lowered) = self.try_lower_short_circuit_and_else(idx)? {
            return Ok(Some(lowered));
        }
        if let Some(lowered) = self.try_lower_short_circuit_and(idx)? {
            return Ok(Some(lowered));
        }
        Ok(None)
    }

    pub(in crate::nir::structuring) fn try_lower_short_circuit_and(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let mut conds = Vec::new();
        let mut current_idx = idx;
        let mut join_idx: Option<usize> = None;
        let mut first_prefix: Vec<HirStmt> = Vec::new();

        loop {
            let cond_block = self.pcode_block(current_idx).clone();
            let cond_prefix = self.lower_block_stmts(&cond_block)?;
            if current_idx == idx {
                if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
                    self.condition_fold_rejected_side_effect += 1;
                    return Ok(None);
                }
                first_prefix = cond_prefix;
            } else if !cond_prefix.is_empty() {
                self.condition_fold_rejected_side_effect += 1;
                return Ok(None);
            }

            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(current_idx)?
            else {
                return Ok(None);
            };
            if false_target != Some(self.block_target_key(next_idx)) {
                return Ok(None);
            }
            let Some(current_join_idx) = self
                .find_block_index_by_address(true_target)
                .filter(|join_idx| *join_idx > current_idx)
            else {
                return Ok(None);
            };
            if let Some(join_idx) = join_idx {
                if join_idx != current_join_idx {
                    return Ok(None);
                }
            } else {
                join_idx = Some(current_join_idx);
            }
            conds.push(negate_expr(cond));

            let next_is_conditional = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { .. }
            );
            if next_is_conditional {
                current_idx = next_idx;
                continue;
            }

            let Some(join_idx) = join_idx else {
                return Ok(None);
            };
            self.log_short_circuit_cache(diag, "and", next_idx, LinearExit::Join(join_idx));
            let Some((then_body, skip_to)) =
                self.lower_linear_body(next_idx, LinearExit::Join(join_idx))?
            else {
                return Ok(None);
            };
            if conds.len() < 2 {
                return Ok(None);
            }

            self.condition_fold_and_count += conds.len() - 1;

            let stmt = HirStmt::If {
                cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalAnd)),
                then_body,
                else_body: Vec::new(),
            };

            if first_prefix.is_empty() {
                return Ok(Some((stmt, skip_to)));
            } else {
                let mut wrapped = first_prefix;
                wrapped.push(stmt);
                return Ok(Some((HirStmt::Block(wrapped), skip_to)));
            }
        }
    }

    pub(in crate::nir::structuring) fn try_lower_short_circuit_and_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let mut conds = Vec::new();
        let mut current_idx = idx;
        let mut else_idx: Option<usize> = None;
        let mut first_prefix: Vec<HirStmt> = Vec::new();

        loop {
            let cond_block = self.pcode_block(current_idx).clone();
            let cond_prefix = self.lower_block_stmts(&cond_block)?;
            if current_idx == idx {
                if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
                    self.condition_fold_rejected_side_effect += 1;
                    return Ok(None);
                }
                first_prefix = cond_prefix;
            } else if !cond_prefix.is_empty() {
                self.condition_fold_rejected_side_effect += 1;
                return Ok(None);
            }

            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(current_idx)?
            else {
                return Ok(None);
            };
            if false_target != Some(self.block_target_key(next_idx)) {
                return Ok(None);
            }
            let Some(current_else_idx) = self.find_block_index_by_address(true_target) else {
                return Ok(None);
            };
            if current_else_idx <= current_idx {
                return Ok(None);
            }
            if let Some(else_idx) = else_idx {
                if else_idx != current_else_idx {
                    return Ok(None);
                }
            } else {
                else_idx = Some(current_else_idx);
            }
            conds.push(negate_expr(cond));

            let next_is_conditional = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { .. }
            );
            if next_is_conditional {
                current_idx = next_idx;
                continue;
            }

            let Some(else_idx) = else_idx else {
                return Ok(None);
            };
            let then_idx = next_idx;
            let Some(exit) = self.shared_forward_linear_exit(idx, then_idx, else_idx)? else {
                return Ok(None);
            };
            self.log_short_circuit_cache(diag, "and_else", then_idx, exit);
            let Some((then_body, then_skip)) = self.lower_linear_body(then_idx, exit)? else {
                return Ok(None);
            };
            self.log_short_circuit_cache(diag, "and_else", else_idx, exit);
            let Some((else_body, else_skip)) = self.lower_linear_body(else_idx, exit)? else {
                return Ok(None);
            };
            if conds.len() < 2 {
                return Ok(None);
            }
            let skip_to = match exit {
                LinearExit::Join(join_idx) => join_idx,
                LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
            };
            self.condition_fold_and_count += conds.len() - 1;

            let stmt = HirStmt::If {
                cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalAnd)),
                then_body,
                else_body,
            };

            if first_prefix.is_empty() {
                return Ok(Some((stmt, skip_to)));
            } else {
                let mut wrapped = first_prefix;
                wrapped.push(stmt);
                return Ok(Some((HirStmt::Block(wrapped), skip_to)));
            }
        }
    }

    pub(in crate::nir::structuring) fn try_lower_short_circuit_or(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();

        let first_block = self.pcode_block(idx).clone();
        let first_prefix = self.lower_block_stmts(&first_block)?;
        if !first_prefix.iter().all(Self::is_trivial_structuring_stmt) {
            self.condition_fold_rejected_side_effect += 1;
            return Ok(None);
        }

        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let Some(mut next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        if false_target != Some(self.block_target_key(next_idx)) {
            return Ok(None);
        }
        let Some(body_idx) = self
            .find_block_index_by_address(true_target)
            .filter(|body_idx| *body_idx > idx)
        else {
            return Ok(None);
        };

        let mut conds = vec![cond];
        loop {
            let is_conditional_chain = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { true_target, .. }
                    if self.find_block_index_by_address(true_target) == Some(body_idx)
            );
            if !is_conditional_chain {
                let false_entry_idx = next_idx;
                if conds.len() == 1
                    && let Some(LinearExit::Join(join_idx)) = self.linear_exit(body_idx)?
                    && join_idx > idx
                    && (false_entry_idx == join_idx
                        || self.is_trivial_forwarding_block(false_entry_idx, join_idx))
                {
                    self.log_short_circuit_cache(
                        diag,
                        "or_single_guarded_body",
                        body_idx,
                        LinearExit::Join(join_idx),
                    );
                    let Some((then_body, skip_to)) =
                        self.lower_linear_body(body_idx, LinearExit::Join(join_idx))?
                    else {
                        return Ok(None);
                    };

                    let stmt = HirStmt::If {
                        cond: conds[0].clone(),
                        then_body,
                        else_body: Vec::new(),
                    };

                    if first_prefix.is_empty() {
                        return Ok(Some((stmt, skip_to)));
                    } else {
                        let mut wrapped = first_prefix;
                        wrapped.push(stmt);
                        return Ok(Some((HirStmt::Block(wrapped), skip_to)));
                    }
                }
                let Some(exit) = self.shared_forward_linear_exit(idx, body_idx, false_entry_idx)?
                else {
                    return Ok(None);
                };
                self.log_short_circuit_cache(diag, "or", false_entry_idx, exit);
                let Some((false_body, false_skip)) =
                    self.lower_linear_body(false_entry_idx, exit)?
                else {
                    return Ok(None);
                };
                if !false_body.is_empty() {
                    return Ok(None);
                }
                self.log_short_circuit_cache(diag, "or", body_idx, exit);
                let Some((then_body, then_skip)) = self.lower_linear_body(body_idx, exit)? else {
                    return Ok(None);
                };
                let skip_to = match exit {
                    LinearExit::Join(join_idx) => join_idx,
                    LinearExit::Return | LinearExit::End => then_skip.max(false_skip),
                };

                self.condition_fold_or_count += conds.len() - 1;
                let stmt = HirStmt::If {
                    cond: simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalOr)),
                    then_body,
                    else_body: Vec::new(),
                };

                if first_prefix.is_empty() {
                    return Ok(Some((stmt, skip_to)));
                } else {
                    let mut wrapped = first_prefix;
                    wrapped.push(stmt);
                    return Ok(Some((HirStmt::Block(wrapped), skip_to)));
                }
            }

            let next_block = self.pcode_block(next_idx).clone();
            let next_prefix = self.lower_block_stmts(&next_block)?;
            if !next_prefix.is_empty() {
                self.condition_fold_rejected_side_effect += 1;
                return Ok(None);
            }

            let LoweredTerminator::Cond {
                cond, false_target, ..
            } = self.lower_block_terminator(next_idx)?
            else {
                return Ok(None);
            };
            conds.push(cond);
            let Some(chain_next_idx) = self.fallthrough_index(next_idx) else {
                return Ok(None);
            };
            if false_target != Some(self.block_target_key(chain_next_idx)) {
                return Ok(None);
            }
            next_idx = chain_next_idx;
        }
    }
}
