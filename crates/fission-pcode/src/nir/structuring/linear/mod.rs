use super::*;

mod recovery;
mod types;
pub use types::*;

impl<'a> PreviewBuilder<'a> {
    fn record_conditional_tail_mismatch_sample(
        &self,
        origin_idx: usize,
        true_idx: Option<usize>,
        false_idx: Option<usize>,
        exit: LinearExit,
        subtype: ConditionalTailMismatchSubtype,
        stage: &str,
    ) {
        if std::env::var_os("FISSION_RECOVERY_MISMATCH_TRACE").is_none() {
            return;
        }
        let function_addr = self
            .pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or_default();
        let message = format!(
            "{{\"function\":\"0x{function_addr:x}\",\"origin_idx\":{origin_idx},\"true_idx\":{},\"false_idx\":{},\"exit\":\"{:?}\",\"subtype\":\"{:?}\",\"stage\":\"{}\"}}\n",
            true_idx.map_or("null".to_string(), |idx| idx.to_string()),
            false_idx.map_or("null".to_string(), |idx| idx.to_string()),
            exit,
            subtype,
            stage,
        );
        let path = format!("/tmp/fission_preview_{function_addr:x}_conditional_mismatch.jsonl");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, message.as_bytes()));
    }

    fn record_conditional_tail_mismatch_subtype(
        &mut self,
        subtype: ConditionalTailMismatchSubtype,
    ) {
        match subtype {
            ConditionalTailMismatchSubtype::NoCommonFollowInWindow => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count += 1;
            }
            ConditionalTailMismatchSubtype::FollowBeyondWindow => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count += 1;
            }
            ConditionalTailMismatchSubtype::SideEntryOrExit => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count += 1;
            }
            ConditionalTailMismatchSubtype::ComplexArmShape => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count += 1;
            }
            ConditionalTailMismatchSubtype::DepthOrBudgetExceeded => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count += 1;
            }
            ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count += 1;
            }
            ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count += 1;
            }
            ConditionalTailMismatchSubtype::FollowTailLoweringFailed => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += 1;
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count += 1;
            }
        }
    }

    pub(crate) fn has_linear_body_cache(&self, start_idx: usize, exit: LinearExit) -> bool {
        self.linear_body_cache.contains_key(&LinearBodyCacheKey {
            start_idx,
            exit,
            region_recovery: false,
        })
    }

    fn build_linear_multiblock_body_inner(
        &mut self,
        try_switch_recovery: bool,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            let block_key = self.block_target_key(idx);
            let is_orphan_unreachable =
                idx != 0 && self.predecessors[idx].is_empty() && !targeted.contains(&block_key);
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }
            if try_switch_recovery
                && let Some((switch_stmt, skip_to)) = self.try_lower_switch(idx)?
            {
                if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                    body.push(HirStmt::Label(block_label(block_key)));
                }
                body.push(switch_stmt);
                idx = skip_to;
                continue;
            }
            let block = self.pcode_block(idx).clone();
            let block_key = self.block_target_key(idx);
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                body.push(HirStmt::Label(block_label(block_key)));
            }
            body.extend(self.lower_block_stmts(&block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                    {
                        body.push(HirStmt::Return(Some(expr)));
                    } else if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Fallthrough(Some(target)) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                    {
                        body.push(HirStmt::Return(Some(expr)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let then_body = if let Some(true_idx) =
                        self.find_block_index_by_address(true_target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, true_idx)?
                    {
                        vec![HirStmt::Return(Some(expr))]
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = if let Some(false_target) = false_target {
                        if let Some(false_idx) = self.find_block_index_by_address(false_target)
                            && let Some(expr) =
                                self.lower_return_join_expr_for_predecessor(idx, false_idx)?
                        {
                            vec![HirStmt::Return(Some(expr))]
                        } else {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                    } else {
                        Vec::new()
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Unsupported {
                    evidence,
                    target_expr,
                } => {
                    self.record_unsupported_inventory_event(
                        "build_linear_multiblock_unsupported_terminator",
                        None,
                        None,
                        None,
                        Some(block.start_address),
                        None,
                        false,
                        "hir_unsupported_emit",
                    );
                    body.push(self.emit_unsupported_control_surface(evidence, target_expr));
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    if let Some((switch_stmt, skip_to)) = self.lower_structured_switch_terminator(
                        &expr,
                        &targets,
                        default_target,
                        min_val,
                        proof.as_ref(),
                    )? {
                        body.push(switch_stmt);
                        idx = skip_to;
                        continue;
                    }
                    let cases = if let Some(proof) = proof.as_ref()
                        && EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready
                    {
                        self.telemetry.dispatcher.proof_payload_direct_emit_count += 1;
                        proof
                            .recovered_cases
                            .iter()
                            .filter(|(_, target)| Some(*target) != default_target)
                            .map(|(value, target)| crate::nir::types::HirSwitchCase {
                                values: vec![*value],
                                body: vec![HirStmt::Goto(block_label(*target))],
                            })
                            .collect()
                    } else {
                        targets
                            .into_iter()
                            .filter(|target| Some(*target) != default_target)
                            .enumerate()
                            .map(|(i, t)| crate::nir::types::HirSwitchCase {
                                values: vec![min_val + i as i64],
                                body: vec![HirStmt::Goto(block_label(t))],
                            })
                            .collect()
                    };
                    body.push(HirStmt::Switch {
                        expr,
                        cases,
                        default: default_target
                            .map(block_label)
                            .map(HirStmt::Goto)
                            .into_iter()
                            .collect(),
                    });
                }
            }
            idx += 1;
        }
        let mut body = cleanup_redundant_labels(body, None);
        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        Ok(finalize_structured_body(body))
    }

    pub(super) fn build_linear_multiblock_body(
        &mut self,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        self.build_linear_multiblock_body_inner(false)
    }

    pub(super) fn build_proof_first_linear_multiblock_body(
        &mut self,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        self.build_linear_multiblock_body_inner(true)
    }

    fn lower_structured_switch_terminator(
        &mut self,
        expr: &HirExpr,
        targets: &[u64],
        default_target: Option<u64>,
        min_val: i64,
        proof: Option<&DispatcherProofUnit>,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let emit_ready = EmitReadyDecision::from_dispatcher_proof(proof);
        let Some(proof) = proof else {
            return Ok(None);
        };
        if !emit_ready.emit_ready {
            return Ok(None);
        }

        let mut exits = Vec::new();
        let mut indexed_cases = Vec::new();
        let (recovered_cases, used_proof_payload) =
            recovered_switch_case_values(targets, default_target, min_val, Some(proof));
        if used_proof_payload {
            self.telemetry.dispatcher.proof_payload_direct_emit_count += 1;
        }
        for (value, target) in recovered_cases {
            if Some(target) == default_target {
                continue;
            }
            let Some(case_idx) = self.find_block_index_by_address(target) else {
                return Ok(None);
            };
            let case_idx = self.canonicalize_switch_target(case_idx);
            exits.push(case_idx);
            indexed_cases.push((value, case_idx));
        }
        if indexed_cases.len() < 2 {
            return Ok(None);
        }

        let default_idx = if let Some(default_target) = default_target {
            let Some(default_idx) = self.find_block_index_by_address(default_target) else {
                return Ok(None);
            };
            let default_idx = self.canonicalize_switch_target(default_idx);
            exits.push(default_idx);
            Some(default_idx)
        } else {
            None
        };

        let Some(exit) = self.shared_exit_for_indices(&exits)? else {
            return Ok(None);
        };

        let mut cases = Vec::new();
        let mut max_skip = 0usize;
        for (value, case_idx) in indexed_cases {
            let Some((case_body, skip_to)) = self.lower_linear_body(case_idx, exit)? else {
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to);
            cases.push(HirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        super::switch::merge_equivalent_switch_cases(&mut cases);

        let default = if let Some(default_idx) = default_idx {
            let Some((default_body, default_skip)) = self.lower_linear_body(default_idx, exit)?
            else {
                return Ok(None);
            };
            max_skip = max_skip.max(default_skip);
            default_body
        } else {
            Vec::new()
        };

        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => max_skip,
        };
        Ok(Some((
            HirStmt::Switch {
                expr: expr.clone(),
                cases,
                default,
            },
            skip_to,
        )))
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
            return Ok(match cached {
                LinearBodyCachedOutcome::Lowered(lowered) => Some(lowered.clone()),
                LinearBodyCachedOutcome::Rejected(_) => None,
            });
        }
        if !self.active_linear_body_keys.insert(key) {
            return Ok(None);
        }
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = self.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
                self.options,
                start_idx,
                start_addr,
                "lower_linear_body_auto",
                self.structuring_start,
            ));
            auto_budget.as_mut().unwrap()
        };
        if budget_ref.checkpoint("lower_linear_body_start") {
            self.active_linear_body_keys.remove(&key);
            return Ok(None);
        }
        let detailed = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            0,
            Some(budget_ref),
            false,
        )?;
        let result = match &detailed {
            LinearBodyLoweringOutcome::Lowered(lowered) => Some(lowered.clone()),
            LinearBodyLoweringOutcome::Rejected(_) => None,
        };
        self.active_linear_body_keys.remove(&key);
        let should_cache = !budget_ref.tripped;
        if should_cache {
            let cached = match &detailed {
                LinearBodyLoweringOutcome::Lowered(lowered) => {
                    LinearBodyCachedOutcome::Lowered(lowered.clone())
                }
                LinearBodyLoweringOutcome::Rejected(reason) => {
                    LinearBodyCachedOutcome::Rejected(*reason)
                }
            };
            self.linear_body_cache.insert(key, cached);
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
                LinearBodyCachedOutcome::Lowered(lowered) => {
                    LinearBodyLoweringOutcome::Lowered(lowered.clone())
                }
                LinearBodyCachedOutcome::Rejected(reason) => {
                    if region_recovery {
                        LinearBodyLoweringOutcome::Rejected(*reason)
                    } else {
                        LinearBodyLoweringOutcome::Rejected(
                            LinearBodyRejectReason::UnsupportedTerminator,
                        )
                    }
                }
            });
        }
        if !self.active_linear_body_keys.insert(key) {
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::RevisitCycle,
            ));
        }
        let mut auto_budget = None;
        let budget_ref = if let Some(b) = budget {
            b
        } else {
            let start_addr = self.block_start_address(start_idx);
            auto_budget = Some(IfLoweringBudget::new(
                self.options,
                start_idx,
                start_addr,
                "lower_linear_body_detailed_auto",
                self.structuring_start,
            ));
            auto_budget.as_mut().unwrap()
        };
        if budget_ref.checkpoint("lower_linear_body_start") {
            self.active_linear_body_keys.remove(&key);
            return Ok(LinearBodyLoweringOutcome::Rejected(
                LinearBodyRejectReason::BudgetTripped,
            ));
        }
        let result = self.lower_linear_body_with_depth_detailed(
            start_idx,
            exit,
            0,
            Some(budget_ref),
            region_recovery,
        )?;
        self.active_linear_body_keys.remove(&key);
        let should_cache = !budget_ref.tripped;
        if should_cache {
            let cached = match &result {
                LinearBodyLoweringOutcome::Lowered(lowered) => {
                    LinearBodyCachedOutcome::Lowered(lowered.clone())
                }
                LinearBodyLoweringOutcome::Rejected(reason) => {
                    if region_recovery {
                        LinearBodyCachedOutcome::Rejected(*reason)
                    } else {
                        LinearBodyCachedOutcome::Rejected(
                            LinearBodyRejectReason::UnsupportedTerminator,
                        )
                    }
                }
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

        if let LinearExit::Join(join_idx) = exit {
            if start_idx == join_idx {
                return Ok(LinearBodyLoweringOutcome::Lowered((Vec::new(), start_idx)));
            }
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

            let block = self.pcode_block(idx).clone();
            body.extend(self.lower_block_stmts(&block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
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
                        if let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, next_idx)?
                        {
                            body.push(HirStmt::Return(Some(expr)));
                        }
                        return Ok(LinearBodyLoweringOutcome::Lowered((body, next_idx)));
                    }
                    if self.active_switch_targets.contains(&next_idx) {
                        body.push(HirStmt::Goto(block_label(target)));
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
                        self.can_inline_linear_successor_for_region(idx, next_idx, &visited, exit)
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
                        self.block_count(),
                    )));
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let tail_lowering = self.lower_conditional_tail(
                        idx,
                        cond,
                        true_target,
                        false_target,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    let (tail_stmt, skip_to) = match tail_lowering {
                        ConditionalTailLoweringResult::Lowered(lowered) => lowered,
                        ConditionalTailLoweringResult::Mismatch(subtype) => {
                            if region_recovery {
                                self.record_conditional_tail_mismatch_subtype(subtype);
                                self.record_conditional_tail_mismatch_sample(
                                    idx,
                                    self.find_block_index_by_address(true_target),
                                    false_target.and_then(|target| {
                                        self.find_block_index_by_address(target)
                                    }),
                                    exit,
                                    subtype,
                                    "lower_linear_body_with_depth_detailed",
                                );
                            }
                            return Ok(LinearBodyLoweringOutcome::Rejected(
                                LinearBodyRejectReason::ConditionalTailExitMismatch,
                            ));
                        }
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

    fn merge_terminal_exits(&mut self, lhs: LinearExit, rhs: LinearExit) -> Option<LinearExit> {
        match (lhs, rhs) {
            (LinearExit::Return, LinearExit::Return) | (LinearExit::End, LinearExit::End) => {
                self.telemetry.structuring.rule_block_if_no_exit_count += 1;
                self.telemetry
                    .structuring
                    .rule_block_if_no_exit_accepted_count += 1;
                Some(lhs)
            }
            (LinearExit::Join(idx), LinearExit::Return) | (LinearExit::Return, LinearExit::Join(idx)) => {
                Some(LinearExit::Join(idx))
            }
            (LinearExit::End, LinearExit::Return) | (LinearExit::Return, LinearExit::End) => {
                Some(LinearExit::End)
            }
            _ => None,
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
        } else if let (Some(l_exit), Some(r_exit)) = (lhs, rhs) {
            Ok(self.merge_terminal_exits(l_exit, r_exit))
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
        let mut shared = self.linear_exit(first)?;
        for idx in iter {
            if shared == Some(LinearExit::Join(idx)) {
                continue;
            }
            let exit = self.linear_exit(idx)?;
            if exit == Some(LinearExit::Join(first)) {
                shared = Some(LinearExit::Join(first));
                continue;
            }
            if shared.is_some() && shared == exit {
                continue;
            }
            if let (Some(s_exit), Some(c_exit)) = (shared, exit) {
                if let Some(merged) = self.merge_terminal_exits(s_exit, c_exit) {
                    shared = Some(merged);
                    continue;
                }
            }
            return Ok(None);
        }

        let mut exits_set = std::collections::HashSet::new();
        for idx in indices {
            exits_set.insert(*idx);
        }

        while let Some(LinearExit::Join(target)) = shared {
            if exits_set.contains(&target) {
                let next_exit = self.linear_exit(target)?;
                if next_exit == shared {
                    break;
                }
                shared = next_exit;
            } else {
                break;
            }
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
                } else if let (Some(t_exit), Some(f_exit)) = (true_exit, false_exit) {
                    Ok(self.merge_terminal_exits(t_exit, f_exit))
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
        // Dom invariant fast-path: if `idx` dominates `next_idx` in the global dominator
        // tree AND every structural predecessor of `next_idx` is either `idx`, in the current
        // visited set, or itself dominated by `idx`, then the inline is provably safe: every
        // path from the CFG entry to `next_idx` goes through `idx`.
        if self.dom_tree.dominates(idx, next_idx)
            && self.predecessors[next_idx].iter().all(|&pred| {
                pred == idx || visited.contains(&pred) || self.dom_tree.dominates(idx, pred)
            })
        {
            return true;
        }
        if self.predecessors[next_idx]
            .iter()
            .all(|pred| *pred == idx || visited.contains(pred))
        {
            return true;
        }
        if self.successors[next_idx].len() == 1 {
            let forwarded = self.successors[next_idx][0];
            if self.predecessors[next_idx].iter().all(|pred| {
                *pred == idx
                    || visited.contains(pred)
                    || self.is_trivial_forwarding_block(*pred, next_idx)
            }) && self.is_trivial_forwarding_block(next_idx, forwarded)
            {
                return true;
            }
        }
        self.predecessors[next_idx].len() == 1
            && self.predecessors[next_idx][0] == idx
            && self.is_trivial_linear_tail(next_idx)
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
        let block = self.pcode_block(idx).clone();
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

    pub(super) fn forwarding_block_defines_return_tail_live_in(
        &self,
        idx: usize,
        join_idx: usize,
    ) -> bool {
        if self.successors.get(idx).map(Vec::as_slice) != Some(&[join_idx][..]) {
            return false;
        }
        let block = self.pcode_block(idx);
        let join_block = self.pcode_block(join_idx);
        let Some(join_term_idx) = join_block.ops.iter().position(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        }) else {
            return false;
        };
        if join_block.ops[join_term_idx].opcode != PcodeOpcode::Return {
            return false;
        }
        let Some(block_term_idx) = block.ops.iter().position(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        }) else {
            return false;
        };
        let defs = block
            .ops
            .iter()
            .take(block_term_idx)
            .filter_map(|op| op.output.as_ref())
            .collect::<Vec<_>>();
        if defs.is_empty() {
            return false;
        }
        join_block
            .ops
            .iter()
            .take(join_term_idx)
            .flat_map(|op| op.inputs.iter())
            .any(|input| defs.iter().any(|def| Self::varnodes_overlap(def, input)))
    }

    fn varnodes_overlap(lhs: &Varnode, rhs: &Varnode) -> bool {
        if lhs.is_constant || rhs.is_constant || lhs.space_id != rhs.space_id {
            return false;
        }
        if lhs.offset == rhs.offset && lhs.size == rhs.size {
            return true;
        }
        if !is_register_space_id(lhs.space_id) {
            return false;
        }
        let lhs_end = lhs.offset.saturating_add(u64::from(lhs.size));
        let rhs_end = rhs.offset.saturating_add(u64::from(rhs.size));
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }

    fn is_trivial_linear_tail(&self, idx: usize) -> bool {
        let block = self.pcode_block(idx).clone();
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
        origin_idx: usize,
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
        exit: LinearExit,
        depth: usize,
        mut budget: Option<&mut IfLoweringBudget>,
        region_recovery: bool,
    ) -> Result<ConditionalTailLoweringResult, MlilPreviewError> {
        if depth > MAX_LINEAR_STRUCTURING_DEPTH {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::DepthOrBudgetExceeded,
            ));
        }
        if let Some(budget) = budget.as_deref_mut()
            && budget.checkpoint("lower_conditional_tail")
        {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::DepthOrBudgetExceeded,
            ));
        }
        let Some(false_target) = false_target else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };
        let Some(true_idx) = self.find_block_index_by_address(true_target) else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };
        let Some(false_idx) = self.find_block_index_by_address(false_target) else {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        };

        let true_arm = if region_recovery {
            self.normalize_conditional_tail_arm_for_region(origin_idx, true_idx, exit)
        } else {
            NormalizedConditionalTailArm {
                canonical_idx: true_idx,
                effective_start_idx: true_idx,
                reaches_join_trivially: false,
            }
        };
        let false_arm = if region_recovery {
            self.normalize_conditional_tail_arm_for_region(origin_idx, false_idx, exit)
        } else {
            NormalizedConditionalTailArm {
                canonical_idx: false_idx,
                effective_start_idx: false_idx,
                reaches_join_trivially: false,
            }
        };

        let key = ConditionalTailKey {
            true_idx: true_arm.effective_start_idx,
            false_idx: false_arm.effective_start_idx,
            exit,
            region_recovery,
        };
        if !self.active_conditional_tail_keys.insert(key) {
            return Ok(ConditionalTailLoweringResult::Mismatch(
                ConditionalTailMismatchSubtype::ComplexArmShape,
            ));
        }

        let result = (|| {
            if true_arm.reaches_join_trivially
                && let LinearBodyLoweringOutcome::Lowered((false_body, skip_to)) = self
                    .lower_linear_body_with_depth_detailed(
                        false_arm.effective_start_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond: negate_expr(cond.clone()),
                        then_body: false_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            if false_arm.reaches_join_trivially
                && let LinearBodyLoweringOutcome::Lowered((true_body, skip_to)) = self
                    .lower_linear_body_with_depth_detailed(
                        true_arm.effective_start_idx,
                        exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?
            {
                return Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond: cond.clone(),
                        then_body: true_body,
                        else_body: Vec::new(),
                    },
                    skip_to,
                )));
            }

            let mut fallback_mismatch_subtype =
                ConditionalTailMismatchSubtype::NoCommonFollowInWindow;
            if region_recovery && let LinearExit::Join(join_idx) = exit {
                let shared_tail_entries = match self.find_shared_tail_entries_for_region(
                    origin_idx,
                    true_arm.canonical_idx,
                    false_arm.canonical_idx,
                    join_idx,
                ) {
                    Ok(candidates) => candidates,
                    Err(subtype) => {
                        fallback_mismatch_subtype = subtype;
                        Vec::new()
                    }
                };
                for shared_tail_entry_idx in shared_tail_entries {
                    if shared_tail_entry_idx == join_idx {
                        continue;
                    }
                    let shared_exit = LinearExit::Join(shared_tail_entry_idx);
                    let true_branch = self.lower_linear_body_with_depth_detailed(
                        true_arm.canonical_idx,
                        shared_exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    let false_branch = self.lower_linear_body_with_depth_detailed(
                        false_arm.canonical_idx,
                        shared_exit,
                        depth + 1,
                        budget.as_deref_mut(),
                        region_recovery,
                    )?;
                    match (true_branch, false_branch) {
                        (
                            LinearBodyLoweringOutcome::Lowered((then_body, then_skip)),
                            LinearBodyLoweringOutcome::Lowered((else_body, else_skip)),
                        ) => {
                            match self.lower_linear_body_with_depth_detailed(
                                shared_tail_entry_idx,
                                exit,
                                depth + 1,
                                budget.as_deref_mut(),
                                region_recovery,
                            )? {
                                LinearBodyLoweringOutcome::Lowered((
                                    shared_tail_body,
                                    shared_skip,
                                )) => {
                                    let mut block_stmts = vec![HirStmt::If {
                                        cond: cond.clone(),
                                        then_body,
                                        else_body,
                                    }];
                                    block_stmts.extend(shared_tail_body);
                                    return Ok(ConditionalTailLoweringResult::Lowered((
                                        HirStmt::Block(block_stmts),
                                        shared_skip.max(then_skip.max(else_skip)),
                                    )));
                                }
                                LinearBodyLoweringOutcome::Rejected(_) => {
                                    fallback_mismatch_subtype =
                                        ConditionalTailMismatchSubtype::FollowTailLoweringFailed;
                                }
                            }
                        }
                        (
                            LinearBodyLoweringOutcome::Rejected(_),
                            LinearBodyLoweringOutcome::Rejected(_),
                        ) => {
                            fallback_mismatch_subtype =
                                ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed;
                        }
                        _ => {
                            fallback_mismatch_subtype =
                                ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed;
                        }
                    }
                }
            }

            let true_branch = self.lower_linear_body_with_depth_detailed(
                true_arm.effective_start_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            let false_branch = self.lower_linear_body_with_depth_detailed(
                false_arm.effective_start_idx,
                exit,
                depth + 1,
                budget.as_deref_mut(),
                region_recovery,
            )?;
            match (true_branch, false_branch) {
                (
                    LinearBodyLoweringOutcome::Lowered((then_body, then_skip)),
                    LinearBodyLoweringOutcome::Lowered((else_body, else_skip)),
                ) => Ok(ConditionalTailLoweringResult::Lowered((
                    HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    },
                    then_skip.max(else_skip),
                ))),
                (
                    LinearBodyLoweringOutcome::Rejected(_),
                    LinearBodyLoweringOutcome::Rejected(_),
                ) => Ok(ConditionalTailLoweringResult::Mismatch(
                    if fallback_mismatch_subtype
                        == ConditionalTailMismatchSubtype::NoCommonFollowInWindow
                    {
                        ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed
                    } else {
                        fallback_mismatch_subtype
                    },
                )),
                (LinearBodyLoweringOutcome::Rejected(_), LinearBodyLoweringOutcome::Lowered(_))
                | (LinearBodyLoweringOutcome::Lowered(_), LinearBodyLoweringOutcome::Rejected(_)) => {
                    Ok(ConditionalTailLoweringResult::Mismatch(
                        if fallback_mismatch_subtype
                            == ConditionalTailMismatchSubtype::NoCommonFollowInWindow
                        {
                            ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed
                        } else {
                            fallback_mismatch_subtype
                        },
                    ))
                }
            }
        })();
        self.active_conditional_tail_keys.remove(&key);
        result
    }

    fn normalize_conditional_tail_arm_for_region(
        &self,
        origin_idx: usize,
        start_idx: usize,
        exit: LinearExit,
    ) -> NormalizedConditionalTailArm {
        let canonical_idx = self
            .canonicalize_region_target_for_exit(origin_idx, start_idx, exit)
            .unwrap_or(start_idx);
        if let LinearExit::Join(join_idx) = exit {
            let reaches_join_trivially =
                self.trivial_region_chain_reaches_join(origin_idx, start_idx, join_idx);
            let effective_start_idx = if reaches_join_trivially {
                start_idx
            } else {
                canonical_idx
            };
            return NormalizedConditionalTailArm {
                canonical_idx,
                effective_start_idx,
                reaches_join_trivially,
            };
        }
        NormalizedConditionalTailArm {
            canonical_idx,
            effective_start_idx: canonical_idx,
            reaches_join_trivially: false,
        }
    }

    fn find_shared_tail_entries_for_region(
        &self,
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> Result<Vec<usize>, ConditionalTailMismatchSubtype> {
        let (window, reached_beyond_window) = self.collect_local_recovery_window_nodes(
            origin_idx,
            true_start_idx,
            false_start_idx,
            join_idx,
        )?;
        if !window.contains(&true_start_idx) || !window.contains(&false_start_idx) {
            return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
        }
        let postdom = self.compute_local_postdom_sets(&window, join_idx)?;
        let true_postdom = postdom
            .get(&true_start_idx)
            .ok_or(ConditionalTailMismatchSubtype::ComplexArmShape)?;
        let false_postdom = postdom
            .get(&false_start_idx)
            .ok_or(ConditionalTailMismatchSubtype::ComplexArmShape)?;

        let mut common_candidates = true_postdom
            .intersection(false_postdom)
            .copied()
            .filter(|idx| *idx != join_idx)
            .collect::<Vec<_>>();
        common_candidates.sort_unstable();
        common_candidates.dedup();

        if common_candidates.is_empty() {
            if reached_beyond_window {
                return Err(ConditionalTailMismatchSubtype::FollowBeyondWindow);
            }
            return Err(ConditionalTailMismatchSubtype::NoCommonFollowInWindow);
        }
        let mut viable = common_candidates
            .into_iter()
            .filter(|candidate| {
                !self.shared_follow_candidate_has_side_edge(origin_idx, &window, *candidate)
            })
            .collect::<Vec<_>>();
        if viable.is_empty() {
            return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
        }
        viable.sort_unstable_by(|a, b| b.cmp(a));
        viable.dedup();
        Ok(viable)
    }

    fn collect_local_recovery_window_nodes(
        &self,
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> Result<(HashSet<usize>, bool), ConditionalTailMismatchSubtype> {
        let mut nodes = HashSet::new();
        let mut reached_beyond = false;
        for start_idx in [true_start_idx, false_start_idx] {
            if start_idx <= origin_idx {
                return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
            }
            if start_idx > join_idx {
                return Err(ConditionalTailMismatchSubtype::FollowBeyondWindow);
            }
            let mut stack = vec![(start_idx, 0usize)];
            while let Some((idx, depth)) = stack.pop() {
                if depth > MAX_REGION_FOLLOW_DISCOVERY_STEPS {
                    reached_beyond = true;
                    continue;
                }
                if idx <= origin_idx {
                    return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
                }
                if idx > join_idx {
                    reached_beyond = true;
                    continue;
                }
                if !nodes.insert(idx) {
                    continue;
                }
                if idx == join_idx {
                    continue;
                }
                for succ in &self.successors[idx] {
                    if *succ <= origin_idx {
                        return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
                    }
                    if *succ > join_idx {
                        reached_beyond = true;
                        continue;
                    }
                    stack.push((*succ, depth + 1));
                }
            }
        }
        nodes.insert(join_idx);
        if self.window_contains_cycle(&nodes) {
            return Err(ConditionalTailMismatchSubtype::ComplexArmShape);
        }
        Ok((nodes, reached_beyond))
    }

    fn window_contains_cycle(&self, window: &HashSet<usize>) -> bool {
        fn dfs(
            builder: &PreviewBuilder<'_>,
            node: usize,
            window: &HashSet<usize>,
            visiting: &mut HashSet<usize>,
            visited: &mut HashSet<usize>,
        ) -> bool {
            if visiting.contains(&node) {
                return true;
            }
            if visited.contains(&node) {
                return false;
            }
            visiting.insert(node);
            for succ in &builder.successors[node] {
                if !window.contains(succ) {
                    continue;
                }
                if dfs(builder, *succ, window, visiting, visited) {
                    return true;
                }
            }
            visiting.remove(&node);
            visited.insert(node);
            false
        }

        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        for node in window {
            if !visited.contains(node) && dfs(self, *node, window, &mut visiting, &mut visited) {
                return true;
            }
        }
        false
    }

    fn compute_local_postdom_sets(
        &self,
        window: &HashSet<usize>,
        join_idx: usize,
    ) -> Result<HashMap<usize, HashSet<usize>>, ConditionalTailMismatchSubtype> {
        let Some(postdom_tree) =
            PostDomTree::analyze_window_with_exit(&self.successors, window, join_idx)
        else {
            return Err(ConditionalTailMismatchSubtype::ComplexArmShape);
        };

        for idx in window {
            if *idx == join_idx {
                continue;
            }
            let has_window_succ = self.successors[*idx]
                .iter()
                .copied()
                .any(|succ| window.contains(&succ));
            if !has_window_succ {
                return Err(ConditionalTailMismatchSubtype::SideEntryOrExit);
            }
        }

        Ok(postdom_tree
            .postdominators()
            .iter()
            .map(|(node, set)| (*node, set.clone()))
            .collect())
    }

    fn shared_follow_candidate_has_side_edge(
        &self,
        origin_idx: usize,
        window: &HashSet<usize>,
        candidate_idx: usize,
    ) -> bool {
        for pred in &self.predecessors[candidate_idx] {
            if *pred <= origin_idx || !window.contains(pred) {
                return true;
            }
        }
        for succ in &self.successors[candidate_idx] {
            if !window.contains(succ) {
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn find_shared_tail_entries_for_region_for_test(
        &self,
        origin_idx: usize,
        true_start_idx: usize,
        false_start_idx: usize,
        join_idx: usize,
    ) -> (Vec<usize>, Option<&'static str>) {
        match self.find_shared_tail_entries_for_region(
            origin_idx,
            true_start_idx,
            false_start_idx,
            join_idx,
        ) {
            Ok(value) => (value, None),
            Err(ConditionalTailMismatchSubtype::NoCommonFollowInWindow) => {
                (Vec::new(), Some("NoCommonFollowInWindow"))
            }
            Err(ConditionalTailMismatchSubtype::FollowBeyondWindow) => {
                (Vec::new(), Some("FollowBeyondWindow"))
            }
            Err(ConditionalTailMismatchSubtype::SideEntryOrExit) => {
                (Vec::new(), Some("SideEntryOrExit"))
            }
            Err(ConditionalTailMismatchSubtype::ComplexArmShape) => {
                (Vec::new(), Some("ComplexArmShape"))
            }
            Err(ConditionalTailMismatchSubtype::DepthOrBudgetExceeded) => {
                (Vec::new(), Some("DepthOrBudgetExceeded"))
            }
            Err(ConditionalTailMismatchSubtype::OneArmBodyLoweringFailed) => {
                (Vec::new(), Some("OneArmBodyLoweringFailed"))
            }
            Err(ConditionalTailMismatchSubtype::BothArmsBodyLoweringFailed) => {
                (Vec::new(), Some("BothArmsBodyLoweringFailed"))
            }
            Err(ConditionalTailMismatchSubtype::FollowTailLoweringFailed) => {
                (Vec::new(), Some("FollowTailLoweringFailed"))
            }
        }
    }

    fn collect_region_trivial_forward_chain(
        &self,
        origin_idx: usize,
        start_idx: usize,
        join_idx: usize,
    ) -> Vec<usize> {
        if start_idx <= origin_idx || start_idx > join_idx {
            return Vec::new();
        }
        let mut chain = vec![start_idx];
        let mut current = start_idx;
        let mut steps = 0usize;
        let mut seen = HashSet::from([start_idx]);
        while current != join_idx && steps < MAX_REGION_SHARED_TAIL_STEPS {
            if self.successors[current].len() != 1 {
                break;
            }
            let next_idx = self.successors[current][0];
            if next_idx > join_idx
                || !seen.insert(next_idx)
                || !self.is_trivial_forwarding_block(current, next_idx)
            {
                break;
            }
            chain.push(next_idx);
            current = next_idx;
            steps += 1;
        }
        chain
    }

    fn trivial_region_chain_reaches_join(
        &self,
        origin_idx: usize,
        start_idx: usize,
        join_idx: usize,
    ) -> bool {
        if start_idx == join_idx {
            return true;
        }
        self.collect_region_trivial_forward_chain(origin_idx, start_idx, join_idx)
            .last()
            .copied()
            == Some(join_idx)
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
        let mut visited = HashSet::from([target_idx]);
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
            if !visited.insert(next_idx) || !self.is_trivial_forwarding_block(current, next_idx) {
                break;
            }
            current = next_idx;
            steps += 1;
        }
        Some(current)
    }

    #[cfg(test)]
    pub(crate) fn canonicalize_region_target_for_exit_for_test(
        &self,
        origin_idx: usize,
        target_idx: usize,
        exit: LinearExit,
    ) -> Option<usize> {
        self.canonicalize_region_target_for_exit(origin_idx, target_idx, exit)
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
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_has_call(cond)
                    || Self::expr_has_call(then_expr)
                    || Self::expr_has_call(else_expr)
            }
            HirExpr::Var(_, ..) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, ..) => false,
        }
    }

    pub(super) fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        let layout_idx = self.pcode_block_idx(idx);
        self.layout_fallthrough[layout_idx]
            .filter(|succ| self.successors[layout_idx].contains(succ))
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
            // Do not force-lower uncached terminators here: this helper should
            // stay side-effect free for inventory/stat counters.
            if let Some(term) = self.terminator_cache.get(&idx) {
                match term {
                    LoweredTerminator::Goto(target)
                    | LoweredTerminator::Fallthrough(Some(target)) => {
                        targets.insert(*target);
                    }
                    LoweredTerminator::Cond {
                        true_target,
                        false_target,
                        ..
                    } => {
                        targets.insert(*true_target);
                        if let Some(false_target) = false_target {
                            targets.insert(*false_target);
                        }
                    }
                    LoweredTerminator::Switch {
                        targets: switch_targets,
                        default_target,
                        proof,
                        ..
                    } => {
                        targets.extend(switch_targets.iter().copied());
                        if let Some(default_target) = default_target {
                            targets.insert(*default_target);
                        }
                        if let Some(proof) = proof.as_ref() {
                            targets.extend(proof.candidate_targets.iter().copied());
                            targets.extend(proof.recovered_cases.iter().map(|(_, target)| *target));
                            if let Some(default_target) = proof.default_target {
                                targets.insert(default_target);
                            }
                            if let Some(follow_block) = proof.follow_block {
                                targets.insert(follow_block);
                            }
                            if let Some(legality) = proof.legality_witness.as_ref()
                                && let Some(follow_block) = legality.follow_block
                            {
                                targets.insert(follow_block);
                            }
                        }
                    }
                    LoweredTerminator::Unsupported { evidence, .. } => {
                        targets.extend(evidence.successor_targets.iter().copied());
                    }
                    LoweredTerminator::Return(_) | LoweredTerminator::Fallthrough(None) => {}
                }
            }
        }
        self.jump_targets_cache = Some(targets.clone());
        Ok(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PcodeBasicBlock;

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            ..Default::default()
        }
    }

    #[test]
    fn collect_jump_targets_includes_proof_recovered_switch_targets() {
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1100,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1200,
                    successors: vec![],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1300,
                    successors: vec![],
                    ops: vec![],
                },
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&func, &options, None);
        builder.terminator_cache.insert(
            0,
            LoweredTerminator::Switch {
                expr: HirExpr::Var("selector".to_string()),
                targets: vec![0x1100],
                default_target: Some(0x1300),
                min_val: 0,
                proof: Some(DispatcherProofUnit {
                    selector_expr: "selector".to_string(),
                    rendered_selector_expr: Some("selector".to_string()),
                    candidate_targets: vec![0x1100],
                    recovered_cases: vec![(0, 0x1100), (1, 0x1200)],
                    selector_cardinality: 2,
                    target_cardinality: 2,
                    case_map_source: DispatcherCaseMapSource::Merged,
                    default_target: Some(0x1300),
                    guard_set: vec!["ordinal_domain_complete".to_string()],
                    follow_block: Some(0x1300),
                    normalization: None,
                    legality_witness: Some(DispatcherLegality {
                        follow_block: Some(0x1300),
                        postdom_ok: true,
                        side_effect_free_selector: true,
                        ordinal_domain_complete: true,
                        shared_tail_conflict: false,
                        valid: true,
                    }),
                    proof_scope: DispatcherProofScope::OuterDispatch,
                    proof_complete: true,
                    failure_family: None,
                }),
            },
        );

        let targets = builder.collect_jump_targets().expect("targets");
        assert!(targets.contains(&0x1100), "{targets:?}");
        assert!(targets.contains(&0x1200), "{targets:?}");
        assert!(targets.contains(&0x1300), "{targets:?}");
    }

    #[test]
    fn forwarding_block_live_in_guard_detects_return_tail_register_use() {
        let w0 = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let w20 = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let x0 = Varnode {
            size: 8,
            ..w0.clone()
        };
        let sum = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: 0x2000,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let ret_addr = Varnode::constant(0, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![2],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1000,
                        output: None,
                        inputs: vec![Varnode::constant(0x1020, 8)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![2],
                    ops: vec![
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Copy,
                            address: 0x1010,
                            output: Some(w20.clone()),
                            inputs: vec![Varnode::constant(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x1014,
                            output: None,
                            inputs: vec![Varnode::constant(0x1020, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![
                        PcodeOp {
                            seq_num: 3,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x1020,
                            output: Some(sum.clone()),
                            inputs: vec![w0, w20],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 4,
                            opcode: PcodeOpcode::IntZExt,
                            address: 0x1020,
                            output: Some(x0),
                            inputs: vec![sum],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 5,
                            opcode: PcodeOpcode::Return,
                            address: 0x1020,
                            output: None,
                            inputs: vec![ret_addr],
                            asm_mnemonic: None,
                        },
                    ],
                },
            ],
        };
        let options = test_options();
        let builder = PreviewBuilder::new(&func, &options, None);

        assert!(builder.forwarding_block_defines_return_tail_live_in(1, 2));
        assert!(!builder.forwarding_block_defines_return_tail_live_in(0, 2));
    }
}
