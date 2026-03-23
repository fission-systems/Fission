use super::*;

const MAX_LINEAR_STRUCTURING_DEPTH: usize = 256;
const MAX_REGION_TARGET_CANONICALIZE_STEPS: usize = 4;
const MAX_REGION_JOIN_TRAMPOLINE_DISTANCE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinearBodyRejectReason {
    ConditionalTailExitMismatch,
    SuccessorInlineRejected,
    RevisitCycle,
    UnsupportedTerminator,
    TargetIndexMissing,
    ExitMismatch,
    BudgetTripped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinearBodyLoweringOutcome {
    Lowered((Vec<HirStmt>, usize)),
    Rejected(LinearBodyRejectReason),
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn has_linear_body_cache(&self, start_idx: usize, exit: LinearExit) -> bool {
        self.linear_body_cache
            .contains_key(&LinearBodyCacheKey {
                start_idx,
                exit,
                region_recovery: false,
            })
    }

    pub(super) fn build_linear_multiblock_body(
        &mut self,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        for idx in 0..self.pcode.blocks.len() {
            let block = &self.pcode.blocks[idx];
            let block_key = self.block_target_key(idx);
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                body.push(HirStmt::Label(block_label(block_key)));
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
                } => body.push(HirStmt::If {
                    cond,
                    then_body: vec![HirStmt::Goto(block_label(true_target))],
                    else_body: false_target
                        .map(block_label)
                        .map(HirStmt::Goto)
                        .into_iter()
                        .collect(),
                }),
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
        }
        let mut body = cleanup_redundant_labels(body);
        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        Ok(cleanup_redundant_labels(body))
    }

    pub(crate) fn lower_linear_body(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        self.lower_linear_body_with_budget(start_idx, exit, None)
    }

    pub(super) fn lower_linear_body_with_budget(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let key = LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery: false,
        };
        if let Some(cached) = self.linear_body_cache.get(&key) {
            return Ok(cached.clone());
        }
        if !self.active_linear_body_keys.insert(key) {
            return Ok(None);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_linear_body_start")
        {
            self.active_linear_body_keys.remove(&key);
            return Ok(None);
        }
        let detailed = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            0,
            budget.as_deref_mut(),
            false,
        )?;
        let result = match detailed {
            LinearBodyLoweringOutcome::Lowered(lowered) => Some(lowered),
            LinearBodyLoweringOutcome::Rejected(_) => None,
        };
        self.active_linear_body_keys.remove(&key);
        let should_cache = budget.as_deref().is_none_or(|budget| !budget.tripped);
        if should_cache {
            self.linear_body_cache.insert(key, result.clone());
        }
        Ok(result)
    }

    pub(crate) fn lower_linear_body_for_region_recovery_detailed(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        self.lower_linear_body_detailed_with_mode(start_idx, exit, budget.as_deref_mut(), true)
    }

    fn lower_linear_body_detailed_with_mode(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        let key = LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery,
        };
        if let Some(cached) = self.linear_body_cache.get(&key) {
            return Ok(match cached {
                Some(lowered) => LinearBodyLoweringOutcome::Lowered(lowered.clone()),
                None => LinearBodyLoweringOutcome::Rejected(
                    LinearBodyRejectReason::UnsupportedTerminator,
                ),
            });
        }
        if !self.active_linear_body_keys.insert(key) {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::RevisitCycle,
            ));
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_linear_body_start")
        {
            self.active_linear_body_keys.remove(&key);
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }
        let result = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            0,
            budget.as_deref_mut(),
            region_recovery,
        )?;
        self.active_linear_body_keys.remove(&key);
        let should_cache = budget.as_deref().is_none_or(|budget| !budget.tripped);
        if should_cache {
            let cached = match &result {
                LinearBodyLoweringOutcome::Lowered(lowered) => Some(lowered.clone()),
                LinearBodyLoweringOutcome::Rejected(_) => None,
            };
            self.linear_body_cache.insert(key, cached);
        }
        Ok(result)
    }

    fn lower_linear_body_with_depth_detailed(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_linear_body_depth")
        {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }

        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if let Some(budget) = budget.as_deref_mut()
                && budget.checkpoint("lower_linear_body_loop")
            {
                return Ok(LinearBodyLoweringOutcome::Rejected(
                    LinearBodyRejectReason::BudgetTripped,
                ));
            }
            if !visited.insert(idx) {
                return Ok(LinearBodyLoweringOutcome::Rejected(
                    LinearBodyRejectReason::RevisitCycle,
                ));
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
                    if exit != LinearExit::Return {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::ExitMismatch,
                        ));
                    }
                    body.push(HirStmt::Return(expr));
                    return Ok(LinearBodyLoweringOutcome::Lowered((body, idx + 1)));
                }
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::TargetIndexMissing,
                        ));
                    };
                    if exit == LinearExit::Join(next_idx) {
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    if body.is_empty()
                        && self.is_trivial_forwarding_block(idx, next_idx)
                        && self.linear_exit_with_budget(next_idx, budget.as_deref_mut())?
                            == Some(exit)
                    {
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    let can_inline = if region_recovery {
                        self.can_inline_linear_successor_for_region(
                            idx,
                            next_idx,
                            &visited,
                            exit,
                        )
                    } else {
                        self.can_inline_linear_successor(idx, next_idx, &visited)
                    };
                    if can_inline {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(LinearBodyLoweringOutcome::Rejected(
                        LinearBodyRejectReason::SuccessorInlineRejected,
                    ));
                }
                LoweredTerminator::Fallthrough(None) => {
                    if exit != LinearExit::End {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::ExitMismatch,
                        ));
                    }
                    return Ok(LinearBodyLoweringOutcome::Lowered((
                        body,
                        self.pcode.blocks.len(),
                    )));
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let Some((tail_stmt, skip_to)) = self.lower_conditional_tail(
                        cond,
                        true_target,
                        false_target,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
                    else {
                        return Ok(LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::ConditionalTailExitMismatch,
                        ));
                    };
                    body.push(tail_stmt);
                    return Ok(LinearBodyLoweringOutcome::Lowered((body, skip_to)));
                }
                _ => {
                    return Ok(LinearBodyLoweringOutcome::Rejected(
                        LinearBodyRejectReason::UnsupportedTerminator,
                    ));
                }
            }
        }
    }

    pub(super) fn shared_linear_exit(
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

    pub(super) fn shared_exit_for_indices(
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

    pub(super) fn linear_exit(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        self.linear_exit_with_budget(start_idx, None)
    }

    pub(super) fn linear_exit_with_budget(
        &mut self,
        start_idx: usize,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        if let Some(cached) = self.linear_exit_cache.get(&start_idx) {
            return Ok(*cached);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("linear_exit_start")
        {
            return Ok(None);
        }
        let result =
            self.linear_exit_from(start_idx, &mut HashSet::new(), 0, budget.as_deref_mut())?;
        let should_cache = budget.as_deref().is_none_or(|budget| !budget.tripped);
        if should_cache {
            self.linear_exit_cache.insert(start_idx, result);
        }
        Ok(result)
    }

    fn linear_exit_from(
        &mut self,
        idx: usize,
        visited: &mut HashSet<usize>,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(None);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("linear_exit_depth")
        {
            return Ok(None);
        }
        if !visited.insert(idx) {
            return Ok(None);
        }
        match self.lower_block_terminator(idx)? {
            LoweredTerminator::Return(_) => Ok(Some(LinearExit::Return)),
            LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                let Some(next_idx) = self.find_block_index_by_address(target) else {
                    return Ok(None);
                };
                if self.can_inline_linear_successor(idx, next_idx, visited) {
                    self.linear_exit_from(next_idx, visited, depth + 1, budget.as_deref_mut())
                } else {
                    Ok(Some(LinearExit::Join(next_idx)))
                }
            }
            LoweredTerminator::Fallthrough(None) => Ok(Some(LinearExit::End)),
            LoweredTerminator::Cond {
                true_target,
                false_target,
                ..
            } => {
                let Some(false_target) = false_target else {
                    return Ok(None);
                };
                let Some(true_idx) = self.find_block_index_by_address(true_target) else {
                    return Ok(None);
                };
                let Some(false_idx) = self.find_block_index_by_address(false_target) else {
                    return Ok(None);
                };
                let mut true_visited = visited.clone();
                let mut false_visited = visited.clone();
                let true_exit = self.linear_exit_from(
                    true_idx,
                    &mut true_visited,
                    depth + 1,
                    budget.as_deref_mut(),
                )?;
                let false_exit = self.linear_exit_from(
                    false_idx,
                    &mut false_visited,
                    depth + 1,
                    budget.as_deref_mut(),
                )?;
                if true_exit.is_some() && true_exit == false_exit {
                    Ok(true_exit)
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    pub(super) fn can_inline_linear_successor(
        &self,
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool {
        if next_idx <= idx {
            return false;
        }
        if self.predecessors[next_idx].iter().all(|pred| {
            *pred == idx
                || visited.contains(pred)
                || self.is_trivial_forwarding_block(*pred, next_idx)
        }) {
            return true;
        }
        self.is_trivial_linear_tail(next_idx)
    }

    fn can_inline_linear_successor_for_region(
        &self,
        idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
        exit: LinearExit,
    ) -> bool {
        if next_idx <= idx {
            return false;
        }
        if self.can_inline_linear_successor(idx, next_idx, visited) {
            return true;
        }
        let LinearExit::Join(join_idx) = exit else {
            return false;
        };
        if next_idx >= join_idx {
            return false;
        }
        self.canonicalize_region_target_for_exit(idx, next_idx, exit)
            .is_some_and(|normalized| normalized == join_idx)
    }

    pub(super) fn is_trivial_forwarding_block(&self, idx: usize, next_idx: usize) -> bool {
        if idx >= next_idx {
            return false;
        }
        let block = &self.pcode.blocks[idx];
        if block.ops.len() > 8 {
            return false;
        }
        if self.successors[idx].len() != 1 || self.successors[idx][0] != next_idx {
            return false;
        }
        let Some((last, prefix)) = block.ops.split_last() else {
            return false;
        };
        if !prefix
            .iter()
            .all(|op| self.is_trivial_forwarding_op(op.opcode))
        {
            return false;
        }
        self.is_linear_tail_terminator(idx, last.opcode)
            || self.is_trivial_forwarding_op(last.opcode)
    }

    fn is_trivial_linear_tail(&self, idx: usize) -> bool {
        let block = &self.pcode.blocks[idx];
        if block.ops.len() > 24 {
            return false;
        }
        let Some((last, prefix)) = block.ops.split_last() else {
            return false;
        };
        prefix.iter().all(|op| self.is_trivial_tail_op(op.opcode))
            && (self.is_linear_tail_terminator(idx, last.opcode)
                || self.is_trivial_tail_op(last.opcode))
    }

    fn is_linear_tail_terminator(&self, idx: usize, opcode: PcodeOpcode) -> bool {
        match opcode {
            PcodeOpcode::Return => self.successors[idx].is_empty(),
            PcodeOpcode::Branch => self.successors[idx].len() == 1,
            _ => false,
        }
    }

    fn is_trivial_forwarding_op(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::MultiEqual
                | PcodeOpcode::Indirect
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
        )
    }

    fn is_trivial_tail_op(&self, opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Load
                | PcodeOpcode::Cast
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntCarry
                | PcodeOpcode::IntSCarry
                | PcodeOpcode::IntSBorrow
                | PcodeOpcode::Int2Comp
                | PcodeOpcode::IntNegate
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::MultiEqual
                | PcodeOpcode::Indirect
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::Call
        )
    }

    pub(super) fn lower_conditional_tail(
        &mut self,
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(None);
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_conditional_tail")
        {
            return Ok(None);
        }
        let Some(false_target) = false_target else {
            return Ok(None);
        };
        let Some(true_idx) = self.find_block_index_by_address(true_target) else {
            return Ok(None);
        };
        let Some(false_idx) = self.find_block_index_by_address(false_target) else {
            return Ok(None);
        };

        let canonical_true_idx = if region_recovery {
            self.canonicalize_region_target_for_exit(0, true_idx, exit)
                .unwrap_or(true_idx)
        } else {
            true_idx
        };
        let canonical_false_idx = if region_recovery {
            self.canonicalize_region_target_for_exit(0, false_idx, exit)
                .unwrap_or(false_idx)
        } else {
            false_idx
        };
        let (effective_true_idx, effective_false_idx) = if let LinearExit::Join(join_idx) = exit {
            (
                if canonical_true_idx == join_idx {
                    true_idx
                } else {
                    canonical_true_idx
                },
                if canonical_false_idx == join_idx {
                    false_idx
                } else {
                    canonical_false_idx
                },
            )
        } else {
            (canonical_true_idx, canonical_false_idx)
        };

        let key = ConditionalTailKey {
            true_idx: effective_true_idx,
            false_idx: effective_false_idx,
            exit,
            region_recovery,
        };
        if !self.active_conditional_tail_keys.insert(key) {
            return Ok(None);
        }

        let result = (|| {
            if exit == LinearExit::Join(canonical_true_idx)
                && let LinearBodyLoweringOutcome::Lowered((false_body, skip_to)) =
                    self.lower_linear_body_with_depth_detailed(
                        effective_false_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(Some((
                    HirStmt::If {
                        cond: negate_expr(cond.clone()),
                        then_body: false_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            if exit == LinearExit::Join(canonical_false_idx)
                && let LinearBodyLoweringOutcome::Lowered((true_body, skip_to)) =
                    self.lower_linear_body_with_depth_detailed(
                        effective_true_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(Some((
                    HirStmt::If {
                        cond: cond.clone(),
                        then_body: true_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            let true_branch = self.lower_linear_body_with_depth_detailed(
                effective_true_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            let false_branch = self.lower_linear_body_with_depth_detailed(
                effective_false_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            match (true_branch, false_branch) {
                (
                    LinearBodyLoweringOutcome::Lowered((then_body, then_skip)),
                    LinearBodyLoweringOutcome::Lowered((else_body, else_skip)),
                ) => Ok(Some((
                    HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    },
                    then_skip.max(else_skip),
                ))),
                _ => Ok(None),
            }
        })();
        self.active_conditional_tail_keys.remove(&key);
        result
    }

    fn canonicalize_region_target_for_exit(
        &self,
        origin_idx: usize,
        target_idx: usize,
        exit: LinearExit,
    ) -> Option<usize> {
        if target_idx <= origin_idx {
            return None;
        }
        let mut current = target_idx;
        let mut steps = 0usize;
        loop {
            if let LinearExit::Join(join_idx) = exit {
                if current == join_idx {
                    return Some(current);
                }
                if current < join_idx
                    && join_idx - current <= MAX_REGION_JOIN_TRAMPOLINE_DISTANCE
                    && self.is_trivial_forwarding_block(current, join_idx)
                {
                    return Some(join_idx);
                }
            }
            if steps >= MAX_REGION_TARGET_CANONICALIZE_STEPS {
                break;
            }
            let next_idx = if self.successors[current].len() == 1 {
                self.successors[current][0]
            } else {
                break;
            };
            if next_idx <= current || !self.is_trivial_forwarding_block(current, next_idx) {
                break;
            }
            current = next_idx;
            steps += 1;
        }
        Some(current)
    }

    pub(super) fn is_trivial_structuring_stmt(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } => !Self::expr_has_call(rhs),
            HirStmt::Expr(expr) => !Self::expr_has_call(expr),
            _ => false,
        }
    }

    fn expr_has_call(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => Self::expr_has_call(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_has_call(lhs) || Self::expr_has_call(rhs)
            }
            HirExpr::Load { ptr, .. } => Self::expr_has_call(ptr),
            HirExpr::PtrOffset { base, .. } => Self::expr_has_call(base),
            HirExpr::Index { base, index, .. } => {
                Self::expr_has_call(base) || Self::expr_has_call(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::expr_has_call(src),
            HirExpr::Var(_, ..) | HirExpr::Const(_, ..) => false,
        }
    }

    pub(super) fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        self.layout_fallthrough[idx].filter(|succ| self.successors[idx].contains(succ))
    }

    pub(super) fn find_block_index_by_address(&self, address: u64) -> Option<usize> {
        self.target_key_to_index.get(&address).copied().or_else(|| {
            canonical_block_index_for_address(self.pcode, &self.address_to_index, address)
        })
    }

    pub(super) fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError> {
        if let Some(cached) = &self.jump_targets_cache {
            return Ok(cached.clone());
        }
        let mut targets = HashSet::new();
        for idx in 0..self.pcode.blocks.len() {
            for succ in &self.successors[idx] {
                targets.insert(self.block_target_key(*succ));
            }
        }
        self.jump_targets_cache = Some(targets.clone());
        Ok(targets)
    }
}
