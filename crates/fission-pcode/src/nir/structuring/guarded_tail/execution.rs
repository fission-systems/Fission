use super::replacement::ConditionAssumption;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct GuardedTailRewriteResult {
    stmts: Vec<HirStmt>,
    exits_to_join: bool,
    unresolved_join_refs: usize,
}

impl<'a> PreviewBuilder<'a> {
    fn rewrite_guarded_tail_sequence(
        stmts: &[HirStmt],
        join_label: &str,
        assumptions: &[ConditionAssumption],
    ) -> GuardedTailRewriteResult {
        let mut out = Vec::with_capacity(stmts.len());
        let mut idx = 0usize;
        while idx < stmts.len() {
            match &stmts[idx] {
                HirStmt::Goto(target) if target == join_label => {
                    return GuardedTailRewriteResult {
                        stmts: out,
                        exits_to_join: true,
                        unresolved_join_refs: 0,
                    };
                }
                HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    if let Some((branch_label, branch_when_true)) =
                        Self::local_forward_branch_target(then_body, else_body)
                        && branch_label != join_label
                        && let Some(label_pos) = (idx + 1..stmts.len()).find(|pos| {
                            matches!(
                                stmts.get(*pos),
                                Some(HirStmt::Label(candidate)) if candidate == &branch_label
                            )
                        })
                    {
                        let mut target_assumptions = assumptions.to_vec();
                        target_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: branch_when_true,
                        });
                        let target_rewritten = Self::rewrite_guarded_tail_sequence(
                            &stmts[label_pos + 1..],
                            join_label,
                            &target_assumptions,
                        );

                        let mut fallthrough_assumptions = assumptions.to_vec();
                        fallthrough_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: !branch_when_true,
                        });
                        let fallthrough_rewritten = Self::rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..label_pos],
                            join_label,
                            &fallthrough_assumptions,
                        );

                        let target_body = target_rewritten.stmts.clone();
                        let target_exits = target_rewritten.exits_to_join;

                        let (then_result, then_exits, else_result, else_exits) = if branch_when_true
                        {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                target_body,
                                target_exits,
                                fallthrough_body,
                                fallthrough_exits,
                            )
                        } else {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                fallthrough_body,
                                fallthrough_exits,
                                target_body,
                                target_exits,
                            )
                        };

                        out.push(HirStmt::If {
                            cond: cond.clone(),
                            then_body: then_result,
                            else_body: else_result,
                        });
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: then_exits && else_exits,
                            unresolved_join_refs: target_rewritten.unresolved_join_refs
                                + fallthrough_rewritten.unresolved_join_refs,
                        };
                    }

                    if let Some(value) = Self::evaluate_condition_assumptions(cond, assumptions) {
                        let mut next_assumptions = assumptions.to_vec();
                        next_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value,
                        });
                        let chosen = if value { then_body } else { else_body };
                        let rewritten = Self::rewrite_guarded_tail_sequence(
                            chosen,
                            join_label,
                            &next_assumptions,
                        );
                        out.extend(rewritten.stmts);
                        if rewritten.exits_to_join {
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: rewritten.unresolved_join_refs,
                            };
                        }
                        idx += 1;
                        continue;
                    }

                    let mut then_assumptions = assumptions.to_vec();
                    then_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: true,
                    });
                    let then_rewritten =
                        Self::rewrite_guarded_tail_sequence(then_body, join_label, &then_assumptions);
                    let mut else_assumptions = assumptions.to_vec();
                    else_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: false,
                    });
                    let else_rewritten =
                        Self::rewrite_guarded_tail_sequence(else_body, join_label, &else_assumptions);

                    if then_rewritten.exits_to_join || else_rewritten.exits_to_join {
                        let rest = Self::rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..],
                            join_label,
                            assumptions,
                        );
                        if then_rewritten.exits_to_join && else_rewritten.exits_to_join {
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: else_rewritten.stmts,
                            });
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: then_rewritten.unresolved_join_refs
                                    + else_rewritten.unresolved_join_refs
                                    + rest.unresolved_join_refs,
                            };
                        }

                        if then_rewritten.exits_to_join {
                            let mut continue_body = else_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: continue_body,
                            });
                        } else {
                            let mut continue_body = then_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: continue_body,
                                else_body: else_rewritten.stmts,
                            });
                        }
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: false,
                            unresolved_join_refs: then_rewritten.unresolved_join_refs
                                + else_rewritten.unresolved_join_refs
                                + rest.unresolved_join_refs,
                        };
                    }

                    out.push(HirStmt::If {
                        cond: cond.clone(),
                        then_body: then_rewritten.stmts,
                        else_body: else_rewritten.stmts,
                    });
                }
                HirStmt::Goto(target) => {
                    out.push(HirStmt::Goto(target.clone()));
                }
                HirStmt::Block(inner) => {
                    let rewritten =
                        Self::rewrite_guarded_tail_sequence(inner, join_label, assumptions);
                    out.push(HirStmt::Block(rewritten.stmts));
                    if rewritten.exits_to_join {
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: true,
                            unresolved_join_refs: rewritten.unresolved_join_refs,
                        };
                    }
                }
                stmt => out.push(stmt.clone()),
            }
            idx += 1;
        }

        let unresolved_join_refs = out
            .iter()
            .map(|stmt| Self::stmt_contains_goto_label(stmt, join_label))
            .sum();
        GuardedTailRewriteResult {
            stmts: out,
            exits_to_join: false,
            unresolved_join_refs,
        }
    }

    fn collect_guarded_tail_exported_bindings(
        &mut self,
        middle: &[HirStmt],
        follow_tail: &[HirStmt],
    ) -> Result<Vec<GuardedTailExportedBinding>, GuardedTailExecutionRejection> {
        let mut bindings = Vec::new();
        for (def_stmt_idx, stmt) in middle.iter().enumerate() {
            let HirStmt::Assign {
                lhs: HirLValue::Var(binding_name),
                rhs,
            } = stmt
            else {
                continue;
            };
            let mut read_sites = Vec::new();
            let mut follow_redefined = false;
            let mut nondominated_reads = 0usize;
            for (stmt_idx, stmt) in follow_tail.iter().enumerate() {
                let reads_here = Self::classify_stmt_read_kind(stmt, binding_name);
                let defs_here = Self::count_var_defs_stmt(stmt, binding_name);
                if follow_redefined {
                    if reads_here.is_some() {
                        nondominated_reads += 1;
                    }
                    continue;
                }
                if let Some(kind) = reads_here {
                    read_sites.push(GuardedTailReplacementRead { stmt_idx, kind });
                }
                if defs_here > 0 {
                    follow_redefined = true;
                }
            }
            if read_sites.is_empty() {
                continue;
            }
            self.guarded_tail_exported_binding_count += 1;
            self.guarded_tail_replacement_read_count += read_sites.len();

            if !Self::expr_is_pure_value(rhs) {
                self.guarded_tail_replacement_read_rejected_nonremovable_op_count +=
                    read_sites.len();
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if middle
                .iter()
                .map(|stmt| Self::count_var_defs_stmt(stmt, binding_name))
                .sum::<usize>()
                != 1
            {
                self.guarded_tail_replacement_read_rejected_nondominated_count += read_sites.len();
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if nondominated_reads > 0 {
                self.guarded_tail_replacement_read_rejected_nondominated_count +=
                    read_sites.len() + nondominated_reads;
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }

            bindings.push(GuardedTailExportedBinding {
                def_stmt_idx,
                binding_name: binding_name.clone(),
                replacement_source: rhs.clone(),
                read_sites,
            });
        }
        Ok(bindings)
    }

    fn try_build_guarded_tail_witness(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<RegionShapeWitness, GuardedTailWitnessRejection>> {
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[idx]
        else {
            return None;
        };

        let (initial_target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
            let Some(label) = single_goto_target(then_body) else {
                return None;
            };
            (label.to_string(), false)
        } else if then_body.is_empty() {
            let Some(label) = single_goto_target(else_body) else {
                return None;
            };
            (label.to_string(), true)
        } else {
            return None;
        };

        let Some(original_label_idx) =
            Self::find_top_level_label_after(body, idx, &initial_target_label)
        else {
            return None;
        };
        if !has_non_ignorable_payload(&body[idx + 1..original_label_idx]) {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::NonCanonicalLayout
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_label_idx],
                    20,
                );
            }
            self.mark_noncanonical_layout_rejection();
            return Some(Err(GuardedTailWitnessRejection::NonCanonicalLayout));
        }
        let original_tail_end = (original_label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if original_tail_end < body.len()
            && body[original_label_idx + 1..original_tail_end]
                .iter()
                .all(is_ignorable_discovery_stmt)
        {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_tail_end],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        let Some((resolved_target_label, resolved_label_idx)) =
            self.resolve_terminal_join_target(body, idx, &initial_target_label, referenced)
        else {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    GuardedTailWitnessRejection::MissingTerminalJoin
                );
                let upper = body.len().min(idx + 1 + 20);
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..upper],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::MissingTerminalJoin));
        };

        let (owned_join_label, label_idx) = Self::find_earliest_owned_join_label(
            body,
            idx,
            resolved_label_idx,
            referenced,
            self.guarded_tail_trace_enabled_for_current_fn(),
        )
        .unwrap_or_else(|| (resolved_target_label.clone(), resolved_label_idx));
        let target_label = resolved_target_label.clone();

        if self.guarded_tail_trace_enabled_for_current_fn() && label_idx != resolved_label_idx {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} owned_join_narrowed from={}({}) to={}({})",
                self.guarded_tail_function_address(),
                idx,
                resolved_target_label,
                resolved_label_idx,
                owned_join_label,
                label_idx
            );
        }

        if self.guarded_tail_trace_enabled_for_current_fn() {
            let raw_middle = &body[idx + 1..label_idx];
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} raw_middle_len={}",
                self.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                raw_middle.len()
            );
        }

        let (middle, external_redirects) = match self.canonicalize_guarded_tail_segment(
            &body[idx + 1..label_idx],
            body,
            idx + 1,
            referenced,
        ) {
            Ok(middle) => middle,
            Err(reason) => {
                if self.guarded_tail_trace_enabled_for_current_fn() {
                    eprintln!(
                        "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                        self.guarded_tail_function_address(),
                        idx,
                        target_label,
                        label_idx,
                        reason
                    );
                    Self::guarded_tail_trace_emit_snapshot(
                        "[GT-TRACE] reject_snapshot",
                        &body[idx + 1..label_idx],
                        20,
                    );
                }
                self.mark_guarded_tail_canonicalization_failure(reason);
                return Some(Err(Self::map_guarded_tail_canonicalization_rejection(
                    reason,
                )));
            }
        };
        if middle.is_empty() {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailCanonicalizationFailure::InterleavedJoinUses
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..label_idx],
                    20,
                );
            }
            self.mark_guarded_tail_canonicalization_failure(
                GuardedTailCanonicalizationFailure::InterleavedJoinUses,
            );
            return Some(Err(GuardedTailWitnessRejection::AliasInterleaveConflict));
        }

        if self.guarded_tail_trace_enabled_for_current_fn() {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} canonical_middle_len={} external_redirects={:?}",
                self.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                middle.len(),
                external_redirects
            );
        }

        let tail_end = (label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if body[label_idx + 1..tail_end].is_empty() && label_idx + 1 != body.len() {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..tail_end],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        Some(Ok(RegionShapeWitness {
            target_label,
            label_idx,
            keep_middle_when_cond_true,
            middle,
            external_redirects,
            terminal_join_present: true,
            follow_witness: true,
            side_entry_free: true,
            alias_interleave_legal: true,
        }))
    }

    fn collect_guarded_tail_candidate_reads(
        body: &[HirStmt],
        middle: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> Vec<GuardedTailReplacementRead> {
        let mut reads = Vec::new();
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if stmt_idx >= if_idx && stmt_idx <= label_idx {
                continue;
            }
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::ExternalForwardGoto,
                });
            }
        }
        for (stmt_idx, stmt) in middle.iter().enumerate() {
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::MiddleGoto,
                });
            }
        }
        reads
    }

    pub(super) fn try_build_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<GuardedTailTrial, GuardedTailWitnessRejection>> {
        let witness = self.try_build_guarded_tail_witness(body, idx, referenced)?;
        if Self::guarded_tail_diag_enabled() {
            match &witness {
                Ok(witness) => eprintln!(
                    "[DIAG] guarded-tail trial idx={} label={} middle_stmts={} redirects={}",
                    idx,
                    witness.target_label,
                    witness.middle.len(),
                    witness.external_redirects.len(),
                ),
                Err(reason) => eprintln!("[DIAG] guarded-tail trial idx={} rejected={:?}", idx, reason),
            }
        }
        Some(witness.map(|witness| GuardedTailTrial {
            follow_block: Some(witness.target_label.clone()),
            candidate_reads: Self::collect_guarded_tail_candidate_reads(
                body,
                &witness.middle,
                idx,
                witness.label_idx,
                &witness.target_label,
            ),
            witness,
        }))
    }

    fn guarded_tail_stmt_is_execution_safe(stmt: &HirStmt, label: &str) -> bool {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } => Self::expr_is_pure_value(rhs),
            HirStmt::VaStart { .. } => true,
            HirStmt::Expr(expr) => Self::expr_is_pure_value(expr),
            HirStmt::Goto(target) => target == label,
            HirStmt::Block(body) => Self::guarded_tail_middle_is_execution_safe(body, label),
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                Self::expr_is_pure_value(cond)
                    && Self::guarded_tail_middle_is_execution_safe(then_body, label)
                    && Self::guarded_tail_middle_is_execution_safe(else_body, label)
            }
            HirStmt::Label(_)
            | HirStmt::Switch { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => false,
            HirStmt::Assign { .. } => false,
        }
    }

    fn guarded_tail_middle_is_execution_safe(middle: &[HirStmt], label: &str) -> bool {
        middle
            .iter()
            .all(|stmt| Self::guarded_tail_stmt_is_execution_safe(stmt, label))
    }

    pub(super) fn verify_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
    ) -> GuardedTailVerification {
        let witness = &trial.witness;
        let legality = witness.region_legality();
        self.guarded_tail_replacement_plan_candidate_count += 1;
        let follow_tail = if witness.label_idx + 1 < body.len() {
            &body[witness.label_idx + 1..]
        } else {
            &[]
        };
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} legality={:?}",
                idx, witness.target_label, legality
            );
        }

        if !legality.is_complete_for(RegionKind::GuardedTail) {
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} incomplete_legality",
                    idx, witness.target_label
                );
            }
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    GuardedTailExecutionRejection::Witness(
                        GuardedTailWitnessRejection::NonCanonicalLayout
                    )
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &witness.middle,
                    20,
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: witness.middle.clone(),
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::Witness(
                    GuardedTailWitnessRejection::NonCanonicalLayout,
                )),
            };
        }

        let rewritten = Self::rewrite_guarded_tail_sequence(&witness.middle, &witness.target_label, &[]);
        let (outside_refs, middle_refs) = Self::surviving_label_refs_after_guarded_tail_promotion(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
        );
        let effective_middle_refs = Self::effective_middle_refs_for_promotion(
            &rewritten.stmts,
            &witness.target_label,
            middle_refs,
        );
        let execution_safe =
            Self::guarded_tail_middle_is_execution_safe(&rewritten.stmts, &witness.target_label);
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} outside_refs={} middle_refs={} effective_middle_refs={} unresolved_join_refs={} execution_safe={}",
                idx,
                witness.target_label,
                outside_refs,
                middle_refs,
                effective_middle_refs,
                rewritten.unresolved_join_refs,
                execution_safe,
            );
        }
        if let Some(rejection) = Self::classify_must_emit_label_rejection(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
            outside_refs,
            middle_refs,
        ) {
            self.mark_promotion_gate_rejection(rejection);
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=MustEmitLabelConflict({:?})",
                    idx, witness.target_label, rejection
                );
            }
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject=MustEmitLabelConflict({:?})",
                    self.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    rejection
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &rewritten.stmts,
                    20,
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: rewritten.stmts,
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::MustEmitLabelConflict),
            };
        }
        let removable_ops_legal =
            execution_safe && !has_top_level_label(&rewritten.stmts) && rewritten.unresolved_join_refs == 0;
        let exported_bindings =
            match self.collect_guarded_tail_exported_bindings(&rewritten.stmts, follow_tail) {
                Ok(bindings) => bindings,
                Err(reason) => {
                    if Self::guarded_tail_diag_enabled() {
                        eprintln!(
                            "[DIAG] guarded-tail verify idx={} label={} exported_bindings_rejected={:?}",
                            idx, witness.target_label, reason
                        );
                    }
                    if self.guarded_tail_trace_enabled_for_current_fn() {
                        eprintln!(
                            "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                            self.guarded_tail_function_address(),
                            idx,
                            witness.target_label,
                            witness.label_idx,
                            reason
                        );
                        Self::guarded_tail_trace_emit_snapshot(
                            "[GT-TRACE] reject_snapshot",
                            &rewritten.stmts,
                            20,
                        );
                    }
                    return GuardedTailVerification {
                        region_legality: legality,
                        replacement_complete: false,
                        removable_ops_legal,
                        rewritten_middle: rewritten.stmts,
                        exported_bindings: Vec::new(),
                        rejection_reason: Some(reason),
                    };
                }
            };
        let replacement_complete = removable_ops_legal && effective_middle_refs == 0;

        if replacement_complete
            && exported_bindings.iter().any(|binding| {
                !binding.read_sites.is_empty()
                    && Self::find_guarded_tail_preexisting_source(body, idx, &binding.binding_name)
                        .is_none()
            })
        {
            self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
            if Self::guarded_tail_diag_enabled() {
                let missing = exported_bindings
                    .iter()
                    .filter(|binding| {
                        !binding.read_sites.is_empty()
                            && Self::find_guarded_tail_preexisting_source(
                                body,
                                idx,
                                &binding.binding_name,
                            )
                            .is_none()
                    })
                    .map(|binding| binding.binding_name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=ReplacementIncomplete(missing_else_source=[{}])",
                    idx, witness.target_label, missing
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: Some(GuardedTailExecutionRejection::ReplacementIncomplete),
            };
        }

        if replacement_complete {
            self.guarded_tail_replacement_plan_completed_count += 1;
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} replacement_complete exported_bindings={}",
                    idx,
                    witness.target_label,
                    exported_bindings.len()
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: true,
                removable_ops_legal: true,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: None,
            };
        }

        if !removable_ops_legal || effective_middle_refs > 0 {
            self.guarded_tail_replacement_plan_rejected_unstable_read_count += 1;
        }
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} rejected={:?} removable_ops_legal={} effective_middle_refs={}",
                idx,
                witness.target_label,
                if !removable_ops_legal {
                    GuardedTailExecutionRejection::MustEmitLabelConflict
                } else {
                    GuardedTailExecutionRejection::ReplacementIncomplete
                },
                removable_ops_legal,
                effective_middle_refs
            );
        }

        if self.guarded_tail_trace_enabled_for_current_fn() {
            let reason = if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                GuardedTailExecutionRejection::ReplacementIncomplete
            };
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                self.guarded_tail_function_address(),
                idx,
                witness.target_label,
                witness.label_idx,
                reason
            );
            Self::guarded_tail_trace_emit_snapshot(
                "[GT-TRACE] reject_snapshot",
                &rewritten.stmts,
                20,
            );
        }

        GuardedTailVerification {
            region_legality: legality,
            replacement_complete,
            removable_ops_legal,
            rewritten_middle: rewritten.stmts,
            exported_bindings,
            rejection_reason: Some(if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
                GuardedTailExecutionRejection::ReplacementIncomplete
            }),
        }
    }

    pub(super) fn build_guarded_tail_execution_plan(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
        verification: &GuardedTailVerification,
    ) -> Result<GuardedTailExecutionPlan, GuardedTailExecutionRejection> {
        let mut rewritten_middle = verification.rewritten_middle.clone();
        let mut synthetic_merges = Vec::new();
        let mut replacement_cache = GuardedTailReplacementCache::default();
        let mut exported_bindings = verification.exported_bindings.clone();
        exported_bindings.sort_by_key(|binding| binding.def_stmt_idx);
        let mut obsolete_defs = Vec::new();

        for binding_idx in 0..exported_bindings.len() {
            let binding_name = exported_bindings[binding_idx].binding_name.clone();
            let replacement_source = exported_bindings[binding_idx].replacement_source.clone();
            let def_stmt_idx = exported_bindings[binding_idx].def_stmt_idx;
            for stmt in rewritten_middle.iter_mut().skip(def_stmt_idx.saturating_add(1)) {
                Self::replace_var_in_stmt(stmt, &binding_name, &replacement_source);
            }
            for later_binding in exported_bindings.iter_mut().skip(binding_idx + 1) {
                Self::replace_var_in_expr(
                    &mut later_binding.replacement_source,
                    &binding_name,
                    &replacement_source,
                );
            }
            if rewritten_middle
                .iter()
                .skip(def_stmt_idx.saturating_add(1))
                .all(|stmt| Self::count_var_reads_stmt(stmt, &binding_name) == 0)
            {
                obsolete_defs.push(def_stmt_idx);
            }

            let else_value = if exported_bindings[binding_idx].read_sites.is_empty() {
                continue;
            } else if let Some(expr) = Self::resolve_guarded_tail_else_source(
                body,
                idx,
                &binding_name,
                &mut replacement_cache,
            ) {
                expr
            } else {
                self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            };

            let ty = expr_type(&replacement_source);
            let replacement_target = next_temp_name(&ty, &mut self.temp_next_id);
            self.temps.insert(
                replacement_target.clone(),
                NirBinding {
                    name: replacement_target.clone(),
                    ty,
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::TempPreserved),
                    initializer: None,
                },
            );
            self.guarded_tail_replacement_plan_merge_created_count += 1;
            synthetic_merges.push(GuardedTailSyntheticMerge {
                binding_name,
                replacement_target,
                then_value: replacement_source,
                else_value,
                read_sites: exported_bindings[binding_idx].read_sites.clone(),
            });
        }
        obsolete_defs.sort_unstable();
        obsolete_defs.dedup();
        for def_idx in obsolete_defs.into_iter().rev() {
            if def_idx < rewritten_middle.len() {
                rewritten_middle.remove(def_idx);
            }
        }
        Ok(GuardedTailExecutionPlan {
            synthetic_merges,
            redirects: trial.witness.external_redirects.clone(),
            rewritten_middle,
        })
    }

    fn apply_guarded_tail_replacement_read(stmt: &mut HirStmt, merge: &GuardedTailSyntheticMerge) {
        let replacement_expr = HirExpr::Var(merge.replacement_target.clone());
        Self::replace_var_in_stmt(stmt, &merge.binding_name, &replacement_expr);
    }

    pub(super) fn execute_guarded_tail_plan(
        &mut self,
        body: &mut Vec<HirStmt>,
        idx: usize,
        trial: GuardedTailTrial,
        plan: GuardedTailExecutionPlan,
        cond: HirExpr,
    ) {
        let mut then_body = plan.rewritten_middle;
        let mut else_body = Vec::new();
        for merge in &plan.synthetic_merges {
            then_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.then_value.clone(),
            });
            else_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.else_value.clone(),
            });
        }
        let replacement = HirStmt::If {
            cond: if trial.witness.keep_middle_when_cond_true {
                cond
            } else {
                negate_expr(cond)
            },
            then_body,
            else_body,
        };

        for (from, to) in &plan.redirects {
            Self::rewrite_goto_label_in_stmts(body, from, to);
        }

        body[idx] = replacement;
        body.drain(idx + 1..=trial.witness.label_idx);
        let tail_start = idx + 1;
        for merge in &plan.synthetic_merges {
            for read in &merge.read_sites {
                let stmt_idx = tail_start + read.stmt_idx;
                if let Some(stmt) = body.get_mut(stmt_idx) {
                    Self::apply_guarded_tail_replacement_read(stmt, merge);
                    self.guarded_tail_replacement_read_rewritten_count += 1;
                }
            }
        }
        self.guarded_tail_promoted_count += 1;
        self.promoted_region_count += 1;
    }

    pub(super) fn discover_guarded_tail_candidates_in_body(&mut self, body: &[HirStmt]) {
        for stmt in body {
            match stmt {
                HirStmt::Block(inner)
                | HirStmt::While { body: inner, .. }
                | HirStmt::DoWhile { body: inner, .. }
                | HirStmt::For { body: inner, .. } => {
                    self.discover_guarded_tail_candidates_in_body(inner);
                }
                HirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.discover_guarded_tail_candidates_in_body(then_body);
                    self.discover_guarded_tail_candidates_in_body(else_body);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        self.discover_guarded_tail_candidates_in_body(&case.body);
                    }
                    self.discover_guarded_tail_candidates_in_body(default);
                }
                HirStmt::Assign { .. }
                | HirStmt::VaStart { .. }
                | HirStmt::Expr(_)
                | HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue => {}
            }
        }

        let referenced = collect_referenced_label_counts(body);
        for idx in 0..body.len() {
            let HirStmt::If { .. } = &body[idx] else {
                continue;
            };
            let Some(trial) = self.try_build_guarded_tail_trial(body, idx, &referenced) else {
                continue;
            };
            self.discovery_seen_guarded_tail_like_shape_count += 1;
            let trial = match trial {
                Ok(trial) => trial,
                Err(reason) => {
                    self.mark_guarded_tail_execution_rejection(
                        GuardedTailExecutionRejection::Witness(reason),
                    );
                    match reason {
                        GuardedTailWitnessRejection::MissingTerminalJoin => {
                            self.mark_guarded_tail_canonicalization_failure(
                                GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                            );
                        }
                        GuardedTailWitnessRejection::AliasInterleaveConflict => {}
                        GuardedTailWitnessRejection::NonCanonicalLayout => {}
                        GuardedTailWitnessRejection::AmbiguousFollow
                        | GuardedTailWitnessRejection::SideEntryConflict => {}
                    }
                    continue;
                }
            };
            let verification = self.verify_guarded_tail_trial(body, idx, &trial);
            if let Some(reason) = verification.rejection_reason {
                self.mark_guarded_tail_execution_rejection(reason);
                continue;
            }

            self.guarded_tail_candidate_count += 1;
            self.promotion_candidate_count += 1;
        }
    }
}
