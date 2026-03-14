use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_short_circuit_if(
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

    pub(super) fn try_lower_short_circuit_and(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let mut conds = Vec::new();
        let mut current_idx = idx;
        let mut join_idx: Option<usize> = None;

        loop {
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
            if false_target != Some(self.pcode.blocks[next_idx].start_address) {
                return Ok(None);
            }
            let current_join_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
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
            let Some((then_body, skip_to)) =
                self.lower_linear_body(next_idx, LinearExit::Join(join_idx))?
            else {
                return Ok(None);
            };
            if conds.len() < 2 {
                return Ok(None);
            }
            return Ok(Some((
                HirStmt::If {
                    cond: fold_logical_chain(conds, HirBinaryOp::LogicalAnd),
                    then_body,
                    else_body: Vec::new(),
                },
                skip_to,
            )));
        }
    }

    pub(super) fn try_lower_short_circuit_and_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let mut conds = Vec::new();
        let mut current_idx = idx;
        let mut else_idx: Option<usize> = None;

        loop {
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
            if false_target != Some(self.pcode.blocks[next_idx].start_address) {
                return Ok(None);
            }
            let current_else_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
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
            let Some(exit) = self.shared_linear_exit(then_idx, else_idx)? else {
                return Ok(None);
            };
            let Some((then_body, then_skip)) = self.lower_linear_body(then_idx, exit)? else {
                return Ok(None);
            };
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
            return Ok(Some((
                HirStmt::If {
                    cond: fold_logical_chain(conds, HirBinaryOp::LogicalAnd),
                    then_body,
                    else_body,
                },
                skip_to,
            )));
        }
    }

    pub(super) fn try_lower_short_circuit_or(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
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
        if false_target != Some(self.pcode.blocks[next_idx].start_address) {
            return Ok(None);
        }
        let body_idx = self
            .find_block_index_by_address(true_target)
            .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
        if body_idx <= idx {
            return Ok(None);
        }

        let mut conds = vec![cond];
        loop {
            let is_conditional_chain = matches!(
                self.lower_block_terminator(next_idx)?,
                LoweredTerminator::Cond { true_target, .. }
                    if self.find_block_index_by_address(true_target) == Some(body_idx)
            );
            if !is_conditional_chain {
                let false_entry_idx = next_idx;
                let Some(exit) = self.shared_linear_exit(body_idx, false_entry_idx)? else {
                    return Ok(None);
                };
                let Some((false_body, false_skip)) =
                    self.lower_linear_body(false_entry_idx, exit)?
                else {
                    return Ok(None);
                };
                if !false_body.is_empty() {
                    return Ok(None);
                }
                let Some((then_body, then_skip)) =
                    self.lower_linear_body(body_idx, exit)?
                else {
                    return Ok(None);
                };
                if conds.len() < 2 {
                    return Ok(None);
                }
                let skip_to = match exit {
                    LinearExit::Join(join_idx) => join_idx,
                    LinearExit::Return | LinearExit::End => then_skip.max(false_skip),
                };
                return Ok(Some((
                    HirStmt::If {
                        cond: fold_logical_chain(conds, HirBinaryOp::LogicalOr),
                        then_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            let LoweredTerminator::Cond {
                cond,
                false_target,
                ..
            } = self.lower_block_terminator(next_idx)?
            else {
                return Ok(None);
            };
            conds.push(cond);
            let Some(chain_next_idx) = self.fallthrough_index(next_idx) else {
                return Ok(None);
            };
            if false_target != Some(self.pcode.blocks[chain_next_idx].start_address) {
                return Ok(None);
            }
            next_idx = chain_next_idx;
        }
    }

    pub(super) fn try_lower_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_prefix = self.lower_block_stmts(&self.pcode.blocks[idx])?;
        if !cond_prefix
            .iter()
            .all(Self::is_trivial_structuring_stmt)
        {
            return Ok(None);
        }
        let Some(next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let next_addr = self.pcode.blocks[next_idx].start_address;

        let (cond, body_idx, exit) = if true_target == next_addr {
            let exit = if let Some(join_addr) = false_target {
                let join_idx = self
                    .find_block_index_by_address(join_addr)
                    .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
                LinearExit::Join(join_idx)
            } else {
                self.linear_exit(next_idx)?
                    .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?
            };
            (cond, next_idx, exit)
        } else if false_target == Some(next_addr) {
            let join_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
            (negate_expr(cond), next_idx, LinearExit::Join(join_idx))
        } else {
            return Ok(None);
        };

        let Some((body, skip_to)) = self.lower_linear_body(body_idx, exit)? else {
            return Ok(None);
        };
        let stmt = HirStmt::If {
            cond,
            then_body: body,
            else_body: Vec::new(),
        };
        if cond_prefix.is_empty() {
            Ok(Some((stmt, skip_to)))
        } else {
            let mut wrapped = cond_prefix;
            wrapped.push(stmt);
            Ok(Some((HirStmt::Block(wrapped), skip_to)))
        }
    }

    pub(super) fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_prefix = self.lower_block_stmts(&self.pcode.blocks[idx])?;
        if !cond_prefix
            .iter()
            .all(Self::is_trivial_structuring_stmt)
        {
            return Ok(None);
        }
        if idx + 2 >= self.pcode.blocks.len() {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target: Some(false_target),
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let Some(next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let next_addr = self.pcode.blocks[next_idx].start_address;

        let (cond, then_idx, else_idx) = if true_target == next_addr {
            let Some(else_idx) = self.find_block_index_by_address(false_target) else {
                return Ok(None);
            };
            (cond, next_idx, else_idx)
        } else if false_target == next_addr {
            let Some(then_idx) = self.find_block_index_by_address(true_target) else {
                return Ok(None);
            };
            (negate_expr(cond), then_idx, next_idx)
        } else {
            return Ok(None);
        };

        let Some(exit) = self.shared_linear_exit(then_idx, else_idx)? else {
            return Ok(None);
        };
        let Some((then_body, then_skip)) = self.lower_linear_body(then_idx, exit)? else {
            return Ok(None);
        };
        let Some((else_body, else_skip)) = self.lower_linear_body(else_idx, exit)? else {
            return Ok(None);
        };
        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
        };
        let stmt = HirStmt::If {
            cond,
            then_body,
            else_body,
        };
        if cond_prefix.is_empty() {
            Ok(Some((stmt, skip_to)))
        } else {
            let mut wrapped = cond_prefix;
            wrapped.push(stmt);
            Ok(Some((HirStmt::Block(wrapped), skip_to)))
        }
    }
}
