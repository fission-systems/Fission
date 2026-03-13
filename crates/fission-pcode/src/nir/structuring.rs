use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if let Some((stmt, skip_to)) = self.try_lower_switch(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_dowhile(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_while(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_short_circuit_if(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_if_else(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) = self.try_lower_if(idx)? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }

            let block = &self.pcode.blocks[idx];
            if idx == 0 || targeted.contains(&block.start_address) {
                body.push(HirStmt::Label(block_label(block.start_address)));
            }
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedControlFlow);
                }
            }
            idx += 1;
        }
        Ok(body)
    }

    fn try_lower_switch(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some(parsed) = self.parse_switch_chain(idx)? else {
            return Ok(None);
        };
        if parsed.cases.len() < 2 {
            return Ok(None);
        }

        let mut exits = parsed
            .cases
            .iter()
            .map(|(_, block_idx)| *block_idx)
            .collect::<Vec<_>>();
        exits.push(parsed.default_idx);
        let Some(exit) = self.shared_exit_for_indices(&exits)? else {
            return Ok(None);
        };

        let mut cases = Vec::new();
        let mut max_skip = 0usize;
        for (value, case_idx) in parsed.cases {
            let Some((case_body, skip_to)) = self.lower_linear_body(case_idx, exit)? else {
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to);
            cases.push(HirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        merge_adjacent_switch_cases(&mut cases);
        let Some((default_body, default_skip)) = self.lower_linear_body(parsed.default_idx, exit)? else {
            return Ok(None);
        };
        max_skip = max_skip.max(default_skip);

        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => max_skip,
        };
        Ok(Some((
            HirStmt::Switch {
                expr: parsed.selector,
                cases,
                default: default_body,
            },
            skip_to,
        )))
    }

    fn try_lower_short_circuit_if(
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

    fn try_lower_short_circuit_and(
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
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
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

    fn try_lower_short_circuit_and_else(
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
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
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

    fn try_lower_short_circuit_or(
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
            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
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

    fn try_lower_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
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
                    .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                LinearExit::Join(join_idx)
            } else {
                self.linear_exit(next_idx)?
                    .ok_or(MlilPreviewError::UnsupportedControlFlow)?
            };
            (cond, next_idx, exit)
        } else if false_target == Some(next_addr) {
            let join_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (negate_expr(cond), next_idx, LinearExit::Join(join_idx))
        } else {
            return Ok(None);
        };

        let Some((body, skip_to)) = self.lower_linear_body(body_idx, exit)? else {
            return Ok(None);
        };
        Ok(Some((
            HirStmt::If {
                cond,
                then_body: body,
                else_body: Vec::new(),
            },
            skip_to,
        )))
    }

    fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
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
        Ok(Some((
            HirStmt::If {
                cond,
                then_body,
                else_body,
            },
            skip_to,
        )))
    }

    fn try_lower_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some((body, cond, skip_to)) = self.lower_do_while_region(idx)? else {
            return Ok(None);
        };
        Ok(Some((HirStmt::DoWhile { body, cond }, skip_to)))
    }

    fn try_lower_while(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_block = &self.pcode.blocks[idx];
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        if !self.lower_block_stmts(cond_block)?.is_empty() {
            return Ok(None);
        }

        let Some(body_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let body_addr = self.pcode.blocks[body_idx].start_address;

        let (cond, exit_idx) = if false_target == Some(body_addr) {
            let exit_idx = self
                .find_block_index_by_address(true_target)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (negate_expr(cond), exit_idx)
        } else if true_target == body_addr {
            let Some(exit_addr) = false_target else {
                return Ok(None);
            };
            let exit_idx = self
                .find_block_index_by_address(exit_addr)
                .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
            (cond, exit_idx)
        } else {
            return Ok(None);
        };

        let Some((body, loop_join_idx)) = self.lower_linear_body(body_idx, LinearExit::Join(idx))?
        else {
            return Ok(None);
        };
        if loop_join_idx != idx {
            return Ok(None);
        }
        Ok(Some((HirStmt::While { cond, body }, exit_idx)))
    }

    fn lower_do_while_region(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<(Vec<HirStmt>, HirExpr, usize)>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let start_addr = self.pcode.blocks[start_idx].start_address;
                    if true_target == start_addr {
                        let Some(exit_addr) = false_target else {
                            return Ok(None);
                        };
                        let exit_idx = self
                            .find_block_index_by_address(exit_addr)
                            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                        return Ok(Some((body, cond, exit_idx)));
                    }
                    if false_target == Some(start_addr) {
                        let exit_idx = self
                            .find_block_index_by_address(true_target)
                            .ok_or(MlilPreviewError::UnsupportedControlFlow)?;
                        return Ok(Some((body, negate_expr(cond), exit_idx)));
                    }
                    return Ok(None);
                }
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(None);
                }
                _ => return Ok(None),
            }
        }
    }

    fn lower_linear_body(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
                    if exit != LinearExit::Return {
                        return Ok(None);
                    }
                    body.push(HirStmt::Return(expr));
                    return Ok(Some((body, idx + 1)));
                }
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if exit == LinearExit::Join(next_idx) {
                        return Ok(Some((body, next_idx)));
                    }
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(None);
                }
                LoweredTerminator::Fallthrough(None) => {
                    if exit != LinearExit::End {
                        return Ok(None);
                    }
                    return Ok(Some((body, self.pcode.blocks.len())));
                }
                _ => return Ok(None),
            }
        }
    }

    fn shared_linear_exit(
        &mut self,
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let lhs = self.linear_exit(lhs_idx)?;
        let rhs = self.linear_exit(rhs_idx)?;
        if lhs.is_some() && lhs == rhs {
            Ok(lhs)
        } else {
            Ok(None)
        }
    }

    fn shared_exit_for_indices(
        &mut self,
        indices: &[usize],
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let mut iter = indices.iter().copied();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let shared = self.linear_exit(first)?;
        for idx in iter {
            let exit = self.linear_exit(idx)?;
            if shared.is_some() && shared == exit {
                continue;
            }
            return Ok(None);
        }
        Ok(shared)
    }

    fn linear_exit(&mut self, start_idx: usize) -> Result<Option<LinearExit>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(_) => return Ok(Some(LinearExit::Return)),
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(Some(LinearExit::Join(next_idx)));
                }
                LoweredTerminator::Fallthrough(None) => return Ok(Some(LinearExit::End)),
                _ => return Ok(None),
            }
        }
    }

    fn can_inline_linear_successor(
        &self,
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool {
        if next_idx <= idx {
            return false;
        }
        if self.predecessors[next_idx]
            .iter()
            .all(|pred| *pred == idx || visited.contains(pred))
        {
            return true;
        }
        self.is_trivial_linear_tail(next_idx)
    }

    fn is_trivial_linear_tail(&self, idx: usize) -> bool {
        let block = &self.pcode.blocks[idx];
        if block.ops.len() > 4 {
            return false;
        }
        block
            .ops
            .iter()
            .all(|op| self.is_trivial_tail_op(op.opcode))
    }

    fn is_trivial_tail_op(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
        )
    }

    fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        self.layout_fallthrough[idx]
            .filter(|succ| self.successors[idx].contains(succ))
    }

    fn find_block_index_by_address(&self, address: u64) -> Option<usize> {
        self.address_to_index.get(&address).copied()
    }

    fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError> {
        let mut targets = HashSet::new();
        for idx in 0..self.pcode.blocks.len() {
            for succ in &self.successors[idx] {
                targets.insert(self.pcode.blocks[*succ].start_address);
            }
        }
        Ok(targets)
    }

    fn parse_switch_chain(&mut self, start_idx: usize) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
        let mut current_idx = start_idx;
        let mut selector: Option<HirExpr> = None;
        let mut cases = Vec::new();

        loop {
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(current_idx)?
            else {
                return Ok(None);
            };
            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let next_addr = self.pcode.blocks[next_idx].start_address;
            let (case_target, case_on_true) = if false_target == Some(next_addr) {
                (true_target, true)
            } else if true_target == next_addr {
                let Some(false_target) = false_target else {
                    return Ok(None);
                };
                (false_target, false)
            } else {
                return Ok(None);
            };
            let Some(case_idx) = self.find_block_index_by_address(case_target) else {
                return Ok(None);
            };
            let Some((case_selector, value)) = extract_eq_const_for_case(&cond, case_on_true) else {
                return Ok(None);
            };
            if let Some(existing) = &selector {
                if strip_casts(existing) != strip_casts(&case_selector) {
                    return Ok(None);
                }
            } else {
                selector = Some(case_selector);
            }
            cases.push((value, case_idx));

            match self.lower_block_terminator(next_idx)? {
                LoweredTerminator::Cond { .. } => {
                    current_idx = next_idx;
                    continue;
                }
                _ => {
                    let Some(selector) = selector else {
                        return Ok(None);
                    };
                    return Ok(Some(ParsedSwitch {
                        selector,
                        cases,
                        default_idx: next_idx,
                    }));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedSwitch {
    selector: HirExpr,
    cases: Vec<(i64, usize)>,
    default_idx: usize,
}

fn extract_eq_const_for_case(expr: &HirExpr, case_on_true: bool) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if !case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => extract_eq_const_for_case(expr.as_ref(), !case_on_true),
        _ => None,
    }
}

fn extract_eq_const_operands(lhs: &HirExpr, rhs: &HirExpr) -> Option<(HirExpr, i64)> {
    match (strip_casts(lhs), strip_casts(rhs)) {
        (HirExpr::Const(value, _), other) => Some((other, value)),
        (other, HirExpr::Const(value, _)) => Some((other, value)),
        _ => None,
    }
}

fn merge_adjacent_switch_cases(cases: &mut Vec<HirSwitchCase>) {
    let mut merged: Vec<HirSwitchCase> = Vec::with_capacity(cases.len());
    for case in cases.drain(..) {
        if let Some(prev) = merged.last_mut()
            && prev.body == case.body
        {
            prev.values.extend(case.values);
            continue;
        }
        merged.push(case);
    }
    *cases = merged;
}
