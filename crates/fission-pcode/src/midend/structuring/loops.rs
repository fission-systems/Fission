use super::*;

// ---------------------------------------------------------------------------
// Loop-context-aware break/continue rewriting
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LoopControlRewriteStats {
    break_rewrites: usize,
    continue_rewrites: usize,
    skipped_nested_scope_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScopeFrame {
    Loop {
        continue_labels: std::collections::HashSet<String>,
        break_labels: std::collections::HashSet<String>,
    },
    Switch {
        break_labels: std::collections::HashSet<String>,
    },
}

fn rewrite_loop_control_gotos_with_stack(
    stmts: &mut [HirStmt],
    stack: &mut Vec<ScopeFrame>,
    stats: &mut LoopControlRewriteStats,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(label) => {
                let target_label = label.clone();
                // 1. try matching continue: scan top-to-bottom for the innermost Loop frame
                let mut continue_matched = false;
                for frame in stack.iter().rev() {
                    if let ScopeFrame::Loop {
                        continue_labels, ..
                    } = frame
                    {
                        if continue_labels.contains(&target_label) {
                            *stmt = HirStmt::Continue;
                            stats.continue_rewrites += 1;
                            continue_matched = true;
                        }
                        break;
                    }
                }
                if continue_matched {
                    continue;
                }

                // 2. try matching break: only check the innermost frame (top of stack)
                if let Some(innermost) = stack.last() {
                    let break_matched = match innermost {
                        ScopeFrame::Loop { break_labels, .. } => {
                            break_labels.contains(&target_label)
                        }
                        ScopeFrame::Switch { break_labels } => break_labels.contains(&target_label),
                    };
                    if break_matched {
                        *stmt = HirStmt::Break;
                        stats.break_rewrites += 1;
                        continue;
                    }
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                rewrite_loop_control_gotos_with_stack(then_body, stack, stats);
                rewrite_loop_control_gotos_with_stack(else_body, stack, stats);
            }
            HirStmt::Block(body) => {
                rewrite_loop_control_gotos_with_stack(body, stack, stats);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Loop {
                    continue_labels: std::collections::HashSet::new(),
                    break_labels: std::collections::HashSet::new(),
                });
                rewrite_loop_control_gotos_with_stack(body, stack, stats);
                stack.pop();
            }
            HirStmt::For { body, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Loop {
                    continue_labels: std::collections::HashSet::new(),
                    break_labels: std::collections::HashSet::new(),
                });
                rewrite_loop_control_gotos_with_stack(body, stack, stats);
                stack.pop();
            }
            HirStmt::Switch { cases, default, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Switch {
                    break_labels: std::collections::HashSet::new(),
                });
                for case in cases {
                    rewrite_loop_control_gotos_with_stack(&mut case.body, stack, stats);
                }
                rewrite_loop_control_gotos_with_stack(default, stack, stats);
                stack.pop();
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn rewrite_loop_control_gotos_in_stmts(
    stmts: &mut [HirStmt],
    continue_label: Option<&str>,
    break_label: Option<&str>,
    stats: &mut LoopControlRewriteStats,
) {
    let mut continue_labels = std::collections::HashSet::new();
    if let Some(cl) = continue_label {
        continue_labels.insert(cl.to_string());
    }
    let mut break_labels = std::collections::HashSet::new();
    if let Some(bl) = break_label {
        break_labels.insert(bl.to_string());
    }

    let mut stack = vec![ScopeFrame::Loop {
        continue_labels,
        break_labels,
    }];
    rewrite_loop_control_gotos_with_stack(stmts, &mut stack, stats);
}

fn rewrite_loop_control_gotos_multi(
    stmts: &mut [HirStmt],
    continue_labels: &std::collections::HashSet<String>,
    break_labels: &std::collections::HashSet<String>,
    stats: &mut LoopControlRewriteStats,
) {
    let mut stack = vec![ScopeFrame::Loop {
        continue_labels: continue_labels.clone(),
        break_labels: break_labels.clone(),
    }];
    rewrite_loop_control_gotos_with_stack(stmts, &mut stack, stats);
}

fn collect_defined_labels(stmts: &[HirStmt], labels: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Label(label) => {
                labels.insert(label.clone());
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defined_labels(then_body, labels);
                collect_defined_labels(else_body, labels);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                collect_defined_labels(body, labels);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defined_labels(&case.body, labels);
                }
                collect_defined_labels(default, labels);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn has_goto_to_undefined_label(stmts: &[HirStmt]) -> bool {
    let mut labels = HashSet::new();
    collect_defined_labels(stmts, &mut labels);
    stmts_have_goto_to_undefined_label(stmts, &labels)
}

fn stmts_have_goto_to_undefined_label(stmts: &[HirStmt], labels: &HashSet<String>) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_has_goto_to_undefined_label(stmt, labels))
}

fn stmt_has_goto_to_undefined_label(stmt: &HirStmt, labels: &HashSet<String>) -> bool {
    match stmt {
        HirStmt::Goto(label) => !labels.contains(label),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmts_have_goto_to_undefined_label(then_body, labels)
                || stmts_have_goto_to_undefined_label(else_body, labels)
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => stmts_have_goto_to_undefined_label(body, labels),
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmts_have_goto_to_undefined_label(&case.body, labels))
                || stmts_have_goto_to_undefined_label(default, labels)
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn get_loop_body(
        &self,
        head_idx: usize,
    ) -> Option<&crate::midend::structuring::loop_analysis::LoopBody> {
        self.loop_bodies.iter().find(|lb| lb.head == head_idx)
    }

    fn track_loop_control_rewrite_stats(&mut self, stats: LoopControlRewriteStats) {
        self.telemetry.structuring.loop_control_rewrite_break_count += stats.break_rewrites;
        self.telemetry
            .structuring
            .loop_control_rewrite_continue_count += stats.continue_rewrites;
        self.telemetry
            .structuring
            .loop_control_rewrite_skipped_nested_scope_count += stats.skipped_nested_scope_count;
    }

    pub(super) fn try_lower_infloop_with_break(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let block = self.pcode_block(idx).clone();
        let block_addr = block.start_address;
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let candidate = if true_target == block_addr {
            false_target.map(|addr| (negate_expr(cond), addr))
        } else if false_target == Some(block_addr) {
            Some((cond, true_target))
        } else {
            None
        };
        let Some((break_cond, break_addr)) = candidate else {
            return Ok(None);
        };

        let Some(exit_idx) = self.find_block_index_by_address(break_addr) else {
            return Ok(None);
        };
        if exit_idx == idx {
            return Ok(None);
        }

        let mut body = self.lower_block_stmts(&block)?;
        body.push(HirStmt::If {
            cond: break_cond,
            then_body: vec![HirStmt::Break],
            else_body: Vec::new(),
        });
        self.telemetry
            .structuring
            .loop_control_explicit_reducer_count += 1;

        Ok(Some((
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body,
            },
            exit_idx,
        )))
    }

    pub(super) fn try_lower_infloop(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if self.successors[idx].len() != 1 {
            return Ok(None);
        }
        let block = self.pcode_block(idx).clone();
        let block_addr = block.start_address;
        let terminator = self.lower_block_terminator(idx)?;
        let loops_to_self = matches!(
            terminator,
            LoweredTerminator::Goto(target) if target == block_addr
        ) || matches!(
            terminator,
            LoweredTerminator::Fallthrough(Some(target)) if target == block_addr
        );
        if !loops_to_self {
            return Ok(None);
        }

        let body = self.lower_block_stmts(&block)?;
        let mut body = body;
        let continue_label = block_label(block_addr);
        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(&mut body, Some(&continue_label), None, &mut stats);
        self.track_loop_control_rewrite_stats(stats);
        Ok(Some((
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body,
            },
            idx + 1,
        )))
    }

    pub(super) fn try_lower_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some((mut body, cond, cond_idx, skip_to)) = self.lower_do_while_region(idx)? else {
            return Ok(None);
        };
        let continue_label = block_label(self.block_target_key(cond_idx));
        let break_label = block_label(self.block_target_key(skip_to));
        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some(&continue_label),
            Some(&break_label),
            &mut stats,
        );
        self.track_loop_control_rewrite_stats(stats);
        Ok(Some((HirStmt::DoWhile { body, cond }, skip_to)))
    }

    pub(super) fn try_lower_while(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if let Some(start) = self.structuring_start {
            if start.elapsed().as_secs_f64() * 1000.0 > 5000.0 {
                return Ok(None);
            }
        }

        let diag = structuring_diag_enabled();
        let block_addr = self.block_start_address(idx);
        let mut budget = IfLoweringBudget::new(
            self.options,
            idx,
            block_addr,
            "try_lower_while",
            self.structuring_start,
        );
        if diag {
            eprintln!(
                "[DIAG] try_lower_while start: idx={} block=0x{:x} x86_guard={}",
                idx, block_addr, budget.enabled
            );
        }

        let result = (|| {
            if budget.checkpoint("terminator_pre") {
                return Ok(None);
            }
            let cond_block = self.pcode_block(idx).clone();
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(idx)?
            else {
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=non_conditional_head",
                        idx, block_addr
                    );
                }
                return Ok(None);
            };
            if budget.checkpoint("terminator_post") {
                return Ok(None);
            }

            if budget.checkpoint("cond_prefix_pre") {
                return Ok(None);
            }
            let cond_prefix = self.lower_block_stmts(&cond_block)?;
            if budget.checkpoint("cond_prefix_post") {
                return Ok(None);
            }
            if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=nontrivial_condition_prefix stmt_count={}",
                        idx,
                        block_addr,
                        cond_prefix.len()
                    );
                }
                return Ok(None);
            }

            let loop_body = self.get_loop_body(idx);

            // While loops should always have an exit target
            let Some(exit_idx) = loop_body.and_then(|lb| lb.exit_idx) else {
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=missing_loop_exit loop_body={:?}",
                        idx, block_addr, loop_body
                    );
                }
                return Ok(None);
            };

            let exit_addr = self.block_target_key(exit_idx);

            let (cond, body_addr) = if true_target == exit_addr {
                let Some(body_addr) = false_target else {
                    return Ok(None);
                };
                (negate_expr(cond), body_addr)
            } else if false_target == Some(exit_addr) {
                let body_addr = true_target;
                (cond, body_addr)
            } else {
                // If neither branch goes to the computed exit edge, this is not a strictly formed while loop tail
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=exit_target_mismatch true=0x{:x} false={:?} exit=0x{:x}",
                        idx, block_addr, true_target, false_target, exit_addr
                    );
                }
                return Ok(None);
            };

            let body_idx = self
                .find_block_index_by_address(body_addr)
                .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;

            if budget.checkpoint("body_pre") {
                return Ok(None);
            }
            let Some((body, loop_join_idx)) = self.lower_linear_body_with_budget(
                body_idx,
                LinearExit::Join(idx),
                Some(&mut budget),
            )?
            else {
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=linear_body_rejected body_idx={}",
                        idx, block_addr, body_idx
                    );
                }
                return Ok(None);
            };
            if budget.checkpoint("body_post") {
                return Ok(None);
            }
            if loop_join_idx != idx {
                if diag {
                    eprintln!(
                        "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=linear_body_wrong_join actual={} expected={}",
                        idx, block_addr, loop_join_idx, idx
                    );
                }
                return Ok(None);
            }
            let continue_label = block_label(self.block_target_key(idx));
            let break_label = block_label(self.block_target_key(exit_idx));
            let mut body = body;
            let mut stats = LoopControlRewriteStats::default();
            rewrite_loop_control_gotos_in_stmts(
                &mut body,
                Some(&continue_label),
                Some(&break_label),
                &mut stats,
            );
            self.track_loop_control_rewrite_stats(stats);
            if cond_prefix.is_empty() {
                return Ok(Some((HirStmt::While { cond, body }, exit_idx)));
            }

            let mut guarded_body = cond_prefix;
            guarded_body.push(HirStmt::If {
                cond: negate_expr(cond),
                then_body: vec![HirStmt::Break],
                else_body: Vec::new(),
            });
            guarded_body.extend(body);
            Ok(Some((
                HirStmt::While {
                    cond: HirExpr::Const(1, NirType::Bool),
                    body: guarded_body,
                },
                exit_idx,
            )))
        })();

        // Fast path succeeded: return it directly.
        if result.is_ok() && result.as_ref().unwrap().is_some() {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while done (fast path): idx={} block=0x{:x} elapsed={:.3}s",
                    idx,
                    block_addr,
                    budget.start.elapsed().as_secs_f64(),
                );
            }
            return result;
        }

        // ------------------------------------------------------------------
        // Subgraph fallback: use the full body-set lowering when the linear
        // chain traversal failed (body has internal branching / multi-exit).
        // ------------------------------------------------------------------
        let subgraph_result = (|| -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
            // Re-derive the loop shape from LoopBody (must be valid while-loop).
            let Some(loop_body) = self.get_loop_body(idx) else {
                return Ok(None);
            };
            let Some(exit_idx) = loop_body.exit_idx else {
                return Ok(None);
            };

            let exit_addr = self.block_target_key(exit_idx);

            // Head must still have a CBranch with one arm pointing to exit.
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(idx)?
            else {
                return Ok(None);
            };
            let cond_block = self.pcode_block(idx).clone();
            let cond_prefix = self.lower_block_stmts(&cond_block)?;
            if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
                return Ok(None);
            }

            let (cond, body_addr) = if true_target == exit_addr {
                let Some(body_addr) = false_target else {
                    return Ok(None);
                };
                (negate_expr(cond), body_addr)
            } else if false_target == Some(exit_addr) {
                (cond, true_target)
            } else {
                return Ok(None);
            };

            let Some(body_start_idx) = self.find_block_index_by_address(body_addr) else {
                return Ok(None);
            };

            // Build body_set: all loop body blocks except the head.
            let body_set: HashSet<usize> = {
                let Some(lb) = self.get_loop_body(idx) else {
                    return Ok(None);
                };
                lb.body.iter().copied().filter(|&b| b != idx).collect()
            };

            if body_set.is_empty() {
                return Ok(None);
            }

            let Some(lowered_body) =
                self.lower_loop_body_subgraph(&body_set, body_start_idx, Some(exit_idx), idx)?
            else {
                return Ok(None);
            };

            self.telemetry.structuring.loop_while_subgraph_lowered_count += 1;

            let body = if cond_prefix.is_empty() {
                lowered_body
            } else {
                let mut guarded = cond_prefix;
                guarded.push(HirStmt::If {
                    cond: negate_expr(cond.clone()),
                    then_body: vec![HirStmt::Break],
                    else_body: Vec::new(),
                });
                guarded.extend(lowered_body);
                return Ok(Some((
                    HirStmt::While {
                        cond: HirExpr::Const(1, NirType::Bool),
                        body: guarded,
                    },
                    exit_idx,
                )));
            };

            Ok(Some((HirStmt::While { cond, body }, exit_idx)))
        })();

        if diag {
            eprintln!(
                "[DIAG] try_lower_while done: idx={} block=0x{:x} elapsed={:.3}s success={} budget_tripped={} subgraph={}",
                idx,
                block_addr,
                budget.start.elapsed().as_secs_f64(),
                matches!(subgraph_result, Ok(Some(_))),
                budget.tripped,
                matches!(subgraph_result, Ok(Some(_))),
            );
        }
        subgraph_result
    }

    pub(super) fn lower_do_while_region(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<(Vec<HirStmt>, HirExpr, usize, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let (cond_idx, exit_idx) = loop {
            if self.sese_region_proof_budget_exceeded() {
                return Ok(None);
            }
            if !visited.insert(idx) {
                return Ok(None);
            }
            path.push(idx);

            let successors = self.successors.get(idx).cloned().unwrap_or_default();
            if successors.len() == 2 && successors.contains(&start_idx) {
                if self.region_has_external_entry(&visited, start_idx) {
                    return Ok(None);
                }
                let Some(exit_idx) = successors
                    .into_iter()
                    .find(|successor| *successor != start_idx)
                else {
                    return Ok(None);
                };
                break (idx, exit_idx);
            }
            let [next_idx] = successors.as_slice() else {
                return Ok(None);
            };
            if !self.can_inline_linear_successor(idx, *next_idx, &visited) {
                return Ok(None);
            }
            idx = *next_idx;
        };

        if diag {
            eprintln!(
                "[DIAG] lower_do_while_region: cfg proof start={} latch={} exit={} blocks={}",
                start_idx,
                cond_idx,
                exit_idx,
                path.len()
            );
        }

        let mut body = Vec::new();
        for (path_pos, block_idx) in path.iter().copied().enumerate() {
            if self.sese_region_proof_budget_exceeded() {
                return Ok(None);
            }
            let block = self.pcode_block(block_idx).clone();
            body.extend(self.lower_block_stmts(&block)?);
            let terminator = self.lower_block_terminator(block_idx)?;
            if block_idx != cond_idx {
                let Some(expected_next) = path.get(path_pos + 1).copied() else {
                    return Ok(None);
                };
                let target = match terminator {
                    LoweredTerminator::Fallthrough(Some(target))
                    | LoweredTerminator::Goto(target) => target,
                    _ => return Ok(None),
                };
                if self.find_block_index_by_address(target) != Some(expected_next) {
                    return Ok(None);
                }
                continue;
            }

            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = terminator
            else {
                return Ok(None);
            };
            let start_addr = self.block_target_key(start_idx);
            let exit_addr = self.block_target_key(exit_idx);
            if true_target == start_addr && false_target == Some(exit_addr) {
                return Ok(Some((body, cond, cond_idx, exit_idx)));
            }
            if false_target == Some(start_addr) && true_target == exit_addr {
                return Ok(Some((body, negate_expr(cond), cond_idx, exit_idx)));
            }
            return Ok(None);
        }

        Ok(None)
    }

    pub(super) fn try_lower_multiblock_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if let Some(start) = self.structuring_start {
            if start.elapsed().as_secs_f64() * 1000.0 > 5000.0 {
                return Ok(None);
            }
        }
        let diag = structuring_diag_enabled();

        // After `order_tails_by_exit()` (Ghidra LoopBody::orderTails equivalent),
        // tails[0] is the preferred latch — the tail with a direct edge to the exit
        // block.  We accept multi-tail loops here: additional tails are handled as
        // mid-body break/continue edges by the subgraph lowerer.
        let (exit_idx, latch_idx, body_set, multi_tail) = {
            let Some(loop_body) = self.get_loop_body(idx) else {
                return Ok(None);
            };
            let Some(exit_idx) = loop_body.exit_idx else {
                return Ok(None);
            };
            if loop_body.tails.is_empty() {
                return Ok(None);
            }
            let multi_tail = loop_body.tails.len() > 1;
            // tails[0] is always the preferred latch after order_tails_by_exit().
            let latch_idx = loop_body.tails[0];
            let body_set: HashSet<usize> = loop_body.body.iter().copied().collect();
            (exit_idx, latch_idx, body_set, multi_tail)
        };

        let start_addr = self.block_target_key(idx);
        let exit_addr = self.block_target_key(exit_idx);

        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(latch_idx)?
        else {
            return Ok(None);
        };

        let _while_cond = if true_target == start_addr && false_target == Some(exit_addr) {
            cond
        } else if false_target == Some(start_addr) && true_target == exit_addr {
            negate_expr(cond)
        } else {
            return Ok(None);
        };

        if diag {
            eprintln!(
                "[DIAG] try_lower_multiblock_dowhile: attempting subgraph for idx={} multi_tail={}",
                idx, multi_tail
            );
        }

        let Some(lowered) = self.lower_loop_body_subgraph(&body_set, idx, Some(exit_idx), idx)?
        else {
            return Ok(None);
        };

        self.telemetry.structuring.loop_while_subgraph_lowered_count += 1;
        if multi_tail {
            self.telemetry
                .structuring
                .loop_multi_tail_dowhile_lowered_count += 1;
        }

        Ok(Some((
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: lowered,
            },
            exit_idx,
        )))
    }

    // -----------------------------------------------------------------------
    // For-loop pattern detection
    // -----------------------------------------------------------------------

    /// Attempt to recognise and lower a for-loop pattern starting at `idx`.
    ///
    /// CFG invariants that must ALL hold:
    ///
    /// 1. `idx` is a valid while-loop head: CBranch with one arm pointing to `exit_idx`.
    /// 2. **Latch invariant**: the LoopBody has exactly one tail, and the tail is dominated
    ///    by the head (`dom_tree.dominates(head_idx, tail_idx)`).
    /// 3. **Init invariant**: the head has exactly one predecessor that is OUTSIDE the loop
    ///    body (the init block), and that init block lowers to a single `Assign` statement.
    /// 4. **Update invariant**: the latch block (excluding its back-edge) lowers to a single
    ///    `Assign` statement (the loop counter update).
    /// 5. **Variable invariant**: init's LHS and update's LHS name the same variable.
    ///
    /// On success emits `HirStmt::For { init, cond, update, body }` and returns
    /// `(stmt, exit_idx)`. The init block is skipped by returning the adjusted `skip_to`.
    pub(super) fn try_lower_for(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        // ── Invariant 1: valid while-loop head (CBranch + LoopBody with exit) ──
        // Extract all needed data from LoopBody before taking &mut self borrows.
        let (exit_idx, latch_idx, body_set) = {
            let Some(lb) = self.get_loop_body(idx) else {
                return Ok(None);
            };
            let Some(exit_idx) = lb.exit_idx else {
                return Ok(None);
            };
            if lb.tails.len() != 1 {
                return Ok(None);
            }
            let latch_idx = lb.tails[0];
            let body_set: HashSet<usize> = lb.body.iter().copied().collect();
            (exit_idx, latch_idx, body_set)
        };

        // ── Invariant 2: latch dominated by head ──
        if !self.dom_tree.dominates(idx, latch_idx) {
            return Ok(None);
        }

        // ── Confirm head has CBranch with one arm → exit ──
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let exit_addr = self.block_target_key(exit_idx);
        let (while_cond, body_addr) = if true_target == exit_addr {
            let Some(body_addr) = false_target else {
                return Ok(None);
            };
            (negate_expr(cond), body_addr)
        } else if false_target == Some(exit_addr) {
            (cond, true_target)
        } else {
            return Ok(None);
        };

        // A for-loop head contributes only its condition to the resulting HIR.
        // Prove from raw p-code that lowering the discarded prefix cannot emit
        // a side effect or nested control-flow statement. Materializing the
        // entire head merely to perform this rejection is prohibitively costly
        // for unrolled arithmetic loops and cannot improve the candidate.
        let head_block = self.pcode_block(idx).clone();
        if !Self::for_condition_head_has_only_discardable_pure_ops(&head_block) {
            return Ok(None);
        }

        // ── Invariant 3: exactly one outside-loop predecessor of head (init block) ──
        let outside_preds: Vec<usize> = self.predecessors[idx]
            .iter()
            .copied()
            .filter(|&p| !body_set.contains(&p))
            .collect();
        if outside_preds.len() != 1 {
            return Ok(None);
        }
        let init_idx = outside_preds[0];

        // Init block must lower to exactly one Assign statement
        let init_block = self.pcode_block(init_idx).clone();
        let init_stmts = self.lower_block_stmts(&init_block)?;
        if init_stmts.len() != 1 {
            return Ok(None);
        }
        let HirStmt::Assign {
            lhs: ref init_lhs, ..
        } = init_stmts[0]
        else {
            return Ok(None);
        };
        let init_var_name = match init_lhs {
            HirLValue::Var(name) => name.clone(),
            _ => return Ok(None),
        };

        // ── Invariant 4: latch lowers to exactly one Assign (the update) ──
        // We lower latch stmts only (not the back-edge terminator).
        let latch_block = self.pcode_block(latch_idx).clone();
        let latch_stmts = self.lower_block_stmts(&latch_block)?;
        if latch_stmts.len() != 1 {
            return Ok(None);
        }
        let HirStmt::Assign {
            lhs: ref update_lhs,
            ..
        } = latch_stmts[0]
        else {
            return Ok(None);
        };

        // ── Invariant 5: init and update assign to the same variable ──
        let update_var_name = match update_lhs {
            HirLValue::Var(name) => name.clone(),
            _ => return Ok(None),
        };
        if init_var_name != update_var_name {
            return Ok(None);
        }

        // ── Lower the loop body: body_blocks = body_set \ {head, latch} ──
        let body_blocks: HashSet<usize> = body_set
            .iter()
            .copied()
            .filter(|&b| b != idx && b != latch_idx)
            .collect();

        let Some(body_start_idx) = self.find_block_index_by_address(body_addr) else {
            return Ok(None);
        };

        let for_body = if body_blocks.is_empty() {
            // Empty body (tight counter loop)
            Vec::new()
        } else {
            let Some(lowered) =
                self.lower_loop_body_subgraph(&body_blocks, body_start_idx, Some(exit_idx), idx)?
            else {
                return Ok(None);
            };
            lowered
        };

        let init_box = Box::new(init_stmts.into_iter().next().unwrap());
        let update_box = Box::new(latch_stmts.into_iter().next().unwrap());

        self.telemetry.structuring.loop_for_lowered_count += 1;

        Ok(Some((
            HirStmt::For {
                init: Some(init_box),
                cond: Some(while_cond),
                update: Some(update_box),
                body: for_body,
            },
            exit_idx,
        )))
    }

    fn for_condition_head_has_only_discardable_pure_ops(
        block: &crate::pcode::PcodeBasicBlock,
    ) -> bool {
        let terminator_idx = block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        });
        block.ops.iter().enumerate().all(|(op_idx, op)| {
            if Some(op_idx) == terminator_idx {
                return op.opcode == PcodeOpcode::CBranch;
            }
            !matches!(
                op.opcode,
                PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    // -----------------------------------------------------------------------
    // Subgraph body lowering: lower a loop body as a CFG subgraph with
    // explicit break/continue context, enabling multi-exit loops.
    // -----------------------------------------------------------------------

    /// Lower all blocks in `body_set` (the loop body excluding the head) into a HIR statement
    /// sequence, treating jumps to `break_idx` as `Break` and jumps to `head_idx` as `Continue`.
    ///
    /// Algorithm (based on natural loop structure):
    /// 1. Process body blocks in sorted index order (forward dominance order for reducible CFGs).
    /// 2. For each block, attempt the same structured reducers as `build_multiblock_body`.
    /// 3. At the fallback terminator level, intercept exits to break/continue targets directly.
    ///
    /// Returns `None` if the subgraph cannot be lowered (e.g. irreducible subgraph, budget
    /// exceeded). Callers should fall through to the goto-based fallback in that case.
    pub(super) fn lower_loop_body_subgraph(
        &mut self,
        body_set: &HashSet<usize>,
        start_idx: usize,
        break_idx: Option<usize>,
        head_idx: usize,
    ) -> Result<Option<Vec<HirStmt>>, MlilPreviewError> {
        if let Some(start) = self.structuring_start {
            if start.elapsed().as_secs_f64() * 1000.0 > 5000.0 {
                return Ok(None);
            }
        }

        if body_set.is_empty() {
            return Ok(Some(Vec::new()));
        }

        let break_addr: Option<u64> = break_idx.map(|bi| self.block_target_key(bi));
        let break_addrs: HashSet<u64> = self
            .get_loop_body(head_idx)
            .map(|lb| {
                lb.all_exits
                    .iter()
                    .filter_map(|&exit| self.pcode.blocks.get(exit).map(|b| b.start_address))
                    .collect()
            })
            .filter(|exits: &HashSet<u64>| !exits.is_empty())
            .unwrap_or_else(|| break_addr.into_iter().collect());

        let break_indices: HashSet<usize> = self
            .get_loop_body(head_idx)
            .map(|lb| lb.all_exits.iter().copied().collect())
            .filter(|exits: &HashSet<usize>| !exits.is_empty())
            .unwrap_or_else(|| break_idx.into_iter().collect());

        let head_addr: u64 = self.block_target_key(head_idx);

        let targeted = self.collect_jump_targets()?;

        // Process blocks in sorted index order; this is preorder-compatible for reducible bodies.
        let mut sorted_body: Vec<usize> = body_set.iter().copied().collect();
        sorted_body.sort_unstable();

        let Some(start_pos) = sorted_body.iter().position(|&i| i == start_idx) else {
            return Ok(None);
        };

        let mut result_stmts: Vec<HirStmt> = Vec::new();
        let mut emitted_labels: HashSet<u64> = HashSet::new();
        // Addresses within body_set that must have a label emitted regardless of `targeted`.
        // Pre-populated by scanning:
        //   (a) terminator_cache for already-lowered terminators, and
        //   (b) raw CFG successors (always available) for body-internal edges.
        // This handles both forward and backward jump references before blocks are lowered.
        let mut force_labels: HashSet<u64> = HashSet::new();
        {
            let body_addrs: HashSet<u64> = body_set
                .iter()
                .filter_map(|&bi| self.pcode.blocks.get(bi).map(|b| b.start_address))
                .collect();
            for &bi in body_set.iter() {
                // (a) Check terminator cache (already-lowered terminators).
                if let Some(term) = self.terminator_cache.get(&bi) {
                    let add_if_body = |addr: u64, fl: &mut HashSet<u64>| {
                        if body_addrs.contains(&addr) {
                            fl.insert(addr);
                        }
                    };
                    match term {
                        LoweredTerminator::Goto(t) | LoweredTerminator::Fallthrough(Some(t)) => {
                            add_if_body(*t, &mut force_labels);
                        }
                        LoweredTerminator::Cond {
                            true_target,
                            false_target,
                            ..
                        } => {
                            add_if_body(*true_target, &mut force_labels);
                            if let Some(ft) = false_target {
                                add_if_body(*ft, &mut force_labels);
                            }
                        }
                        LoweredTerminator::Switch {
                            targets,
                            default_target,
                            ..
                        } => {
                            for &t in targets.iter() {
                                add_if_body(t, &mut force_labels);
                            }
                            if let Some(dt) = default_target {
                                add_if_body(*dt, &mut force_labels);
                            }
                        }
                        _ => {}
                    }
                }
                // (b) Check raw CFG successors (always available, pre-dominates terminator cache).
                // If a body block has a successor that is also in body_set, mark it for a label,
                // because a goto may be emitted during fallback lowering.
                for &succ_idx in &self.successors[bi] {
                    if body_set.contains(&succ_idx) {
                        if let Some(succ_block) = self.pcode.blocks.get(succ_idx) {
                            force_labels.insert(succ_block.start_address);
                        }
                    }
                }
            }
        }
        let mut last_structuring_failure = None;
        let mut pos = start_pos;

        // Helper closure: is the skip_to index within the body set or equal to break_idx?
        let is_valid_skip = |skip_to: usize| -> bool {
            body_set.contains(&skip_to) || break_indices.contains(&skip_to)
        };

        while pos < sorted_body.len() {
            let idx = sorted_body[pos];

            // --- Attempt structured reducers, but only accept if skip_to stays within bounds ---
            macro_rules! try_reducer {
                ($call:expr) => {{
                    if let Some((stmt, skip_to)) =
                        capture_structuring_failure($call, &mut last_structuring_failure)?
                    {
                        if is_valid_skip(skip_to)
                            && self.accept_structured_region(idx, skip_to, &targeted)
                        {
                            result_stmts.push(stmt);
                            // Advance pos to the block at skip_to (or end if skip_to is an exit)
                            if break_indices.contains(&skip_to) {
                                // The structured region consumed everything up to the break exit.
                                return Ok(Some(result_stmts));
                            }
                            pos = sorted_body
                                .iter()
                                .position(|&i| i == skip_to)
                                .unwrap_or(sorted_body.len());
                            continue;
                        }
                    }
                }};
            }

            try_reducer!(self.try_lower_switch(idx));
            try_reducer!(self.try_lower_dowhile(idx));
            try_reducer!(self.try_lower_while(idx));
            try_reducer!(self.try_lower_infloop_with_break(idx));
            try_reducer!(self.try_lower_infloop(idx));
            try_reducer!(self.try_lower_short_circuit_if(idx));
            try_reducer!(self.try_lower_if_else(idx));
            try_reducer!(self.try_lower_if(idx));

            // --- Fallback: emit block with loop-context-aware terminator ---
            let block = self.pcode_block(idx).clone();
            let block_key = self.block_target_key(idx);
            if (idx == start_idx
                || targeted.contains(&block_key)
                || force_labels.contains(&block_key))
                && emitted_labels.insert(block_key)
            {
                result_stmts.push(HirStmt::Label(block_label(block_key)));
            }
            result_stmts.extend(self.lower_block_stmts(&block)?);

            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
                    result_stmts.push(HirStmt::Return(expr));
                }
                LoweredTerminator::Goto(target) | LoweredTerminator::Fallthrough(Some(target)) => {
                    if break_addrs.contains(&target) {
                        result_stmts.push(HirStmt::Break);
                        self.telemetry.structuring.loop_multi_exit_break_count += 1;
                    } else if target == head_addr {
                        result_stmts.push(HirStmt::Continue);
                    } else if self.next_block_address(idx) != Some(target) {
                        // Track this target as requiring a label if it is in the body.
                        if let Some(target_idx) = self.find_block_index_by_address(target) {
                            if body_set.contains(&target_idx) {
                                force_labels.insert(target);
                            }
                        }
                        result_stmts.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    // Check if either arm is the break or continue target
                    let true_is_break = break_addrs.contains(&true_target);
                    let false_is_break =
                        false_target.is_some_and(|target| break_addrs.contains(&target));
                    let true_is_continue = true_target == head_addr;
                    let false_is_continue = false_target == Some(head_addr);

                    if true_is_break && false_is_continue {
                        result_stmts.push(HirStmt::If {
                            cond,
                            then_body: vec![HirStmt::Break],
                            else_body: vec![HirStmt::Continue],
                        });
                        self.telemetry.structuring.loop_multi_exit_break_count += 1;
                    } else if false_is_break && true_is_continue {
                        result_stmts.push(HirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![HirStmt::Break],
                            else_body: vec![HirStmt::Continue],
                        });
                        self.telemetry.structuring.loop_multi_exit_break_count += 1;
                    } else if true_is_break && !false_is_break {
                        // `if (cond) break;` then continue with false arm
                        result_stmts.push(HirStmt::If {
                            cond,
                            then_body: vec![HirStmt::Break],
                            else_body: Vec::new(),
                        });
                        self.telemetry.structuring.loop_multi_exit_break_count += 1;
                    } else if false_is_break && !true_is_break {
                        // `if (!cond) break;` then continue with true arm
                        result_stmts.push(HirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![HirStmt::Break],
                            else_body: Vec::new(),
                        });
                        self.telemetry.structuring.loop_multi_exit_break_count += 1;
                    } else if true_is_continue && !false_is_continue {
                        result_stmts.push(HirStmt::If {
                            cond,
                            then_body: vec![HirStmt::Continue],
                            else_body: Vec::new(),
                        });
                    } else if false_is_continue && !true_is_continue {
                        result_stmts.push(HirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![HirStmt::Continue],
                            else_body: Vec::new(),
                        });
                    } else {
                        // General conditional: emit as if/goto like build_multiblock_body fallback
                        let then_body = if next_addr == Some(true_target) {
                            Vec::new()
                        } else {
                            // Track this as requiring a label if it is in the body.
                            if let Some(target_idx) = self.find_block_index_by_address(true_target)
                            {
                                if body_set.contains(&target_idx) {
                                    force_labels.insert(true_target);
                                }
                            }
                            vec![HirStmt::Goto(block_label(true_target))]
                        };
                        let else_body = match false_target {
                            Some(ft) if Some(ft) != next_addr => {
                                // Track this as requiring a label if it is in the body.
                                if let Some(target_idx) = self.find_block_index_by_address(ft) {
                                    if body_set.contains(&target_idx) {
                                        force_labels.insert(ft);
                                    }
                                }
                                vec![HirStmt::Goto(block_label(ft))]
                            }
                            _ => Vec::new(),
                        };
                        result_stmts.push(HirStmt::If {
                            cond,
                            then_body,
                            else_body,
                        });
                    }
                }
                LoweredTerminator::Unsupported { .. } => {
                    // Propagate as an unsupported marker; caller will fall back.
                    return Ok(None);
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    // Switch inside loop body: emit as switch with gotos, rewrite pass will clean
                    let (case_values, _used_proof_payload) = recovered_switch_case_values(
                        &targets,
                        default_target,
                        min_val,
                        proof.as_ref(),
                    );
                    let cases = case_values
                        .into_iter()
                        .map(|(value, target)| HirSwitchCase {
                            values: vec![value],
                            body: vec![HirStmt::Goto(block_label(target))],
                        })
                        .collect();
                    result_stmts.push(HirStmt::Switch {
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

            pos += 1;
        }

        // Apply break/continue rewriting to catch any Goto labels that escaped the fallback
        // (e.g. produced by nested if/else structuring that still emits gotos).
        //
        // CFG-based: build break_labels from ALL exits of this loop body, not just the
        // canonical one.  This converts multi-exit gotos to `break` when they all exit
        // the loop, keeping the generated code clean without changing semantics.
        let continue_label_str = block_label(head_addr);
        let continue_set: std::collections::HashSet<String> =
            std::iter::once(continue_label_str.clone()).collect();
        let break_labels: std::collections::HashSet<String> = {
            if let Some(lb) = self.get_loop_body(head_idx) {
                let all_exits_labels: std::collections::HashSet<String> = lb
                    .all_exits
                    .iter()
                    .filter_map(|&exit| {
                        self.pcode
                            .blocks
                            .get(exit)
                            .map(|b| block_label(b.start_address))
                    })
                    .collect();
                if !all_exits_labels.is_empty() {
                    all_exits_labels
                } else if let Some(ref bstr) = break_addr.map(block_label) {
                    std::iter::once(bstr.clone()).collect()
                } else {
                    std::collections::HashSet::new()
                }
            } else if let Some(ref bstr) = break_addr.map(block_label) {
                std::iter::once(bstr.clone()).collect()
            } else {
                std::collections::HashSet::new()
            }
        };
        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_multi(
            &mut result_stmts,
            &continue_set,
            &break_labels,
            &mut stats,
        );
        self.track_loop_control_rewrite_stats(stats);

        // Strip trailing `Continue` at the end of the body: the latch block naturally jumps back
        // to the head, so a Continue there is redundant. Only strip at the very end; a Continue
        // inside an if-branch earlier in the body must be preserved.
        while result_stmts.last() == Some(&HirStmt::Continue) {
            result_stmts.pop();
        }

        Ok(Some(result_stmts))
    }

    /// Structures a **multi-block infinite loop** — a loop whose `all_exits` is empty,
    /// meaning no edge inside the body ever leaves the loop.
    ///
    /// These are not caught by `try_lower_infloop` (single-block self-loop) or
    /// `try_lower_while` (requires a conditional exit at the head).  This reducer
    /// detects them via `LoopBody::is_infinite_loop_candidate` and emits
    /// `while(true) { body }`.
    pub(super) fn try_lower_multiblock_infloop(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let body_blocks: Vec<usize> = {
            let Some(loop_body) = self.get_loop_body(idx) else {
                return Ok(None);
            };
            if !loop_body.is_infinite_loop_candidate() {
                return Ok(None);
            }
            if loop_body.body.len() < 2 {
                // Single-block infinite loops are handled by try_lower_infloop.
                return Ok(None);
            }
            loop_body.body.clone()
        };

        // Include ALL body blocks (including the head) in the subgraph so that the head
        // block's statements are naturally emitted first.  The head block is the start.
        let body_set: HashSet<usize> = body_blocks.iter().copied().collect();

        let Some(lowered) = self.lower_loop_body_subgraph(
            &body_set, idx,  // start at the loop head
            None, // no break exit — truly infinite
            idx,  // head for continue detection
        )?
        else {
            return Ok(None);
        };

        let max_body_idx = body_blocks.iter().copied().max().unwrap_or(idx);
        Ok(Some((
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: lowered,
            },
            max_body_idx + 1,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_loop_control_gotos_converts_break_and_continue_targets() {
        let mut body = vec![
            HirStmt::Goto("block_header".to_string()),
            HirStmt::Goto("block_exit".to_string()),
            HirStmt::If {
                cond: HirExpr::Const(1, NirType::Bool),
                then_body: vec![HirStmt::Goto("block_header".to_string())],
                else_body: vec![HirStmt::Goto("block_exit".to_string())],
            },
        ];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("block_header"),
            Some("block_exit"),
            &mut stats,
        );

        assert!(matches!(body[0], HirStmt::Continue));
        assert!(matches!(body[1], HirStmt::Break));
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[2]
        else {
            panic!("expected if statement in rewritten loop body");
        };
        assert!(matches!(then_body.as_slice(), [HirStmt::Continue]));
        assert!(matches!(else_body.as_slice(), [HirStmt::Break]));
        assert_eq!(stats.break_rewrites, 2);
        assert_eq!(stats.continue_rewrites, 2);
        assert_eq!(stats.skipped_nested_scope_count, 0);
    }

    #[test]
    fn rewrite_loop_control_gotos_does_not_rewrite_inside_nested_loop_or_switch() {
        let mut body = vec![
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: vec![HirStmt::Goto("block_header".to_string())],
            },
            HirStmt::Switch {
                expr: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
                cases: vec![HirSwitchCase {
                    values: vec![1],
                    body: vec![HirStmt::Goto("block_exit".to_string())],
                }],
                default: vec![HirStmt::Goto("block_header".to_string())],
            },
        ];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("block_header"),
            Some("block_exit"),
            &mut stats,
        );

        let HirStmt::While {
            body: nested_while_body,
            ..
        } = &body[0]
        else {
            panic!("expected nested while");
        };
        assert!(
            matches!(nested_while_body.as_slice(), [HirStmt::Goto(label)] if label == "block_header")
        );

        let HirStmt::Switch { cases, default, .. } = &body[1] else {
            panic!("expected switch statement");
        };
        // Inside switch, outer loop break target is shielded (Goto)
        assert!(
            matches!(cases[0].body.as_slice(), [HirStmt::Goto(label)] if label == "block_exit")
        );
        // Inside switch, outer loop continue target is propagated (Continue)
        assert!(matches!(default.as_slice(), [HirStmt::Continue]));
        assert_eq!(stats.break_rewrites, 0);
        assert_eq!(stats.continue_rewrites, 1); // 1 continue propagated through switch
        assert_eq!(stats.skipped_nested_scope_count, 2);
    }

    #[test]
    fn rewrite_loop_control_gotos_with_nested_switch_converts_continue_but_preserves_break() {
        let mut body = vec![HirStmt::Switch {
            expr: HirExpr::Const(1, NirType::Bool),
            cases: vec![HirSwitchCase {
                values: vec![1],
                body: vec![
                    HirStmt::Goto("outer_continue".to_string()),
                    HirStmt::Goto("outer_break".to_string()),
                ],
            }],
            default: Vec::new(),
        }];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("outer_continue"),
            Some("outer_break"),
            &mut stats,
        );

        let HirStmt::Switch { cases, .. } = &body[0] else {
            panic!("expected switch");
        };
        let case_body = &cases[0].body;
        assert!(matches!(case_body[0], HirStmt::Continue)); // Outer continue is permitted in switch
        assert!(matches!(case_body[1], HirStmt::Goto(ref l) if l == "outer_break")); // Outer break is shielded by switch
    }

    #[test]
    fn rewrite_loop_control_gotos_with_nested_loop_preserves_both() {
        let mut body = vec![HirStmt::While {
            cond: HirExpr::Const(1, NirType::Bool),
            body: vec![
                HirStmt::Goto("outer_continue".to_string()),
                HirStmt::Goto("outer_break".to_string()),
            ],
        }];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("outer_continue"),
            Some("outer_break"),
            &mut stats,
        );

        let HirStmt::While {
            body: inner_body, ..
        } = &body[0]
        else {
            panic!("expected while");
        };
        // Both are shielded by the inner loop frame
        assert!(matches!(inner_body[0], HirStmt::Goto(ref l) if l == "outer_continue"));
        assert!(matches!(inner_body[1], HirStmt::Goto(ref l) if l == "outer_break"));
    }

    #[test]
    fn undefined_goto_guard_rejects_missing_label_in_structured_loop_body() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Const(1, NirType::Bool),
            then_body: vec![HirStmt::Goto("block_missing".to_string())],
            else_body: Vec::new(),
        }];

        assert!(has_goto_to_undefined_label(&body));
    }

    #[test]
    fn undefined_goto_guard_allows_labels_defined_in_loop_body() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Const(1, NirType::Bool),
                then_body: vec![HirStmt::Goto("block_join".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("block_join".to_string()),
            HirStmt::Break,
        ];

        assert!(!has_goto_to_undefined_label(&body));
    }
}
