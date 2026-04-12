use super::cleanup::{cleanup_redundant_labels, eliminate_redundant_gotos};
use super::irreducible::compute_node_splits;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeCutCost {
    loop_header_violation: usize,
    postdom_damage: usize,
    switch_fanout_damage: usize,
    guard_chain_cut: usize,
    goto_introduction_count: usize,
    label_churn: usize,
    span_penalty: usize,
}

#[derive(Debug, Clone)]
struct StructuredRegionCandidate {
    stmt: HirStmt,
    skip_to: usize,
    cost: EdgeCutCost,
}

impl<'a> PreviewBuilder<'a> {
    fn expr_has_short_circuit_form(expr: &HirExpr) -> bool {
        matches!(
            expr,
            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr,
                ..
            }
        )
    }

    fn stmt_has_nested_if(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body
                    .iter()
                    .any(|stmt| matches!(stmt, HirStmt::If { .. }))
                    || else_body
                        .iter()
                        .any(|stmt| matches!(stmt, HirStmt::If { .. }))
            }
            _ => false,
        }
    }

    fn count_stmt_gotos(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Goto(_) => 1,
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => body.iter().map(Self::count_stmt_gotos).sum(),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.iter().map(Self::count_stmt_gotos).sum::<usize>()
                    + else_body.iter().map(Self::count_stmt_gotos).sum::<usize>()
            }
            HirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| case.body.iter().map(Self::count_stmt_gotos).sum::<usize>())
                    .sum::<usize>()
                    + default.iter().map(Self::count_stmt_gotos).sum::<usize>()
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    fn count_stmt_labels(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Label(_) => 1,
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => body.iter().map(Self::count_stmt_labels).sum(),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.iter().map(Self::count_stmt_labels).sum::<usize>()
                    + else_body.iter().map(Self::count_stmt_labels).sum::<usize>()
            }
            HirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| case.body.iter().map(Self::count_stmt_labels).sum::<usize>())
                    .sum::<usize>()
                    + default.iter().map(Self::count_stmt_labels).sum::<usize>()
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    fn score_structured_candidate(
        &self,
        start_idx: usize,
        skip_to: usize,
        stmt: &HirStmt,
        targeted: &HashSet<u64>,
    ) -> EdgeCutCost {
        let internal_target_count = ((start_idx + 1)..skip_to)
            .filter(|idx| targeted.contains(&self.block_target_key(*idx)))
            .count();
        let span = skip_to.saturating_sub(start_idx);
        let switch_damage = match stmt {
            HirStmt::Switch { cases, .. } => usize::from(cases.is_empty()),
            _ => 1,
        };
        let guard_chain_cut = match stmt {
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                usize::from(!Self::expr_has_short_circuit_form(cond))
                    + usize::from(then_body.is_empty() || else_body.is_empty())
                    + usize::from(Self::stmt_has_nested_if(stmt))
            }
            _ => 0,
        };
        let loop_header_violation = usize::from(matches!(stmt, HirStmt::Goto(_)));
        EdgeCutCost {
            loop_header_violation,
            postdom_damage: internal_target_count,
            switch_fanout_damage: switch_damage,
            guard_chain_cut,
            goto_introduction_count: Self::count_stmt_gotos(stmt),
            label_churn: Self::count_stmt_labels(stmt),
            span_penalty: 256usize.saturating_sub(span.min(256)),
        }
    }

    fn consider_structured_candidate(
        &mut self,
        start_idx: usize,
        targeted: &HashSet<u64>,
        last_structuring_failure: &mut Option<MlilPreviewError>,
        candidates: &mut Vec<StructuredRegionCandidate>,
        result: Result<Option<(HirStmt, usize)>, MlilPreviewError>,
    ) -> Result<(), MlilPreviewError> {
        if let Some((stmt, skip_to)) =
            Self::capture_structuring_failure(result, last_structuring_failure)?
            && self.accept_structured_region(start_idx, skip_to, targeted)
        {
            let cost = self.score_structured_candidate(start_idx, skip_to, &stmt, targeted);
            candidates.push(StructuredRegionCandidate {
                stmt,
                skip_to,
                cost,
            });
        }
        Ok(())
    }

    pub(crate) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let total_start = Instant::now();
        let force_linear = self.should_force_linear_structuring();
        let (scc_component_count, scc_irreducible_count, scc_irreducible_header_count) = {
            let scc = self.cfg_fact_cache().scc();
            (
                scc.component_count(),
                scc.irreducible_count(),
                scc.irreducible_header_total_count(),
            )
        };
        self.structuring_scc_component_count += scc_component_count;
        self.structuring_irreducible_scc_count += scc_irreducible_count;
        self.structuring_irreducible_header_count += scc_irreducible_header_count;

        // Node-splitting for irreducible CFGs: when the SCC analysis shows
        // irreducible SCCs, attempt to make the CFG reducible by splitting
        // the extra header nodes into virtual clones.  This allows the
        // structured-code reducer to succeed where it would otherwise fall
        // back to goto-based linearisation.
        if scc_irreducible_count > 0 && !force_linear {
            let block_stmt_counts: Vec<usize> =
                self.pcode.blocks.iter().map(|b| b.ops.len()).collect();
            if let Some(split) =
                compute_node_splits(&self.successors, &self.predecessors, &block_stmt_counts)
            {
                if diag {
                    eprintln!(
                        "[DIAG] node-splitting: applied {} splits, virtual_blocks={}",
                        split.splits_applied,
                        split.virtual_to_original.len()
                    );
                }
                self.successors = split.new_successors;
                self.predecessors = split.new_predecessors;
                self.virtual_block_map = split.virtual_to_original;
                self.refresh_cfg_fact_cache();
            }
        }
        if diag {
            eprintln!(
                "[DIAG] structuring start: blocks={} edges={} force_linear={}",
                self.pcode.blocks.len(),
                self.successors.iter().map(Vec::len).sum::<usize>(),
                force_linear
            );
        }
        if force_linear {
            self.forced_linear_structuring_count += 1;
            let result = self.build_proof_first_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success={} proof_first={}",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok(),
                    true
                );
            }
            return result;
        }

        // Dom and postdom are computed unconditionally and used by the primary reducer to
        // determine follow blocks.  The diag path additionally logs edge-class statistics.
        // NOTE: These are computed AFTER node-splitting so they reflect the augmented CFG.
        let cfg_facts = self.cfg_fact_cache();
        let dom = cfg_facts.dominators();
        let postdom = cfg_facts.postdominators();
        let dom_frontier = cfg_facts.dominance_frontier();

        if diag {
            let cfg = cfg_facts.edges();
            let total_b = self.pcode.blocks.len() + self.virtual_block_map.len();
            let sample_ncd = if total_b >= 2 {
                dom.nearest_common_dominator(&[0, total_b - 1])
            } else {
                Some(0)
            };
            eprintln!(
                "[DIAG] structuring cfg-analysis: roots={} tree={} back={} forward={} cross={} dom_roots={} postdom_exits={} scc_components={} irreducible_scc={} sample_ncd={:?}",
                cfg.roots().len(),
                cfg.count_class(EdgeClass::Tree),
                cfg.count_class(EdgeClass::Back),
                cfg.count_class(EdgeClass::Forward),
                cfg.count_class(EdgeClass::Cross),
                dom.roots().len(),
                postdom.exits().len(),
                scc_component_count,
                scc_irreducible_count,
                sample_ncd,
            );
        }

        // Pre-compute the immediate-postdominator tree using Cooper's algorithm (O(n log n)).
        // This is more efficient than the set-based PostDomTree for large functions and gives
        // O(depth) LCA queries.
        let imm_postdom = cfg_facts.immediate_postdominators();

        // Pre-compute nearest common postdominator ("follow block") for each block,
        // including any virtual blocks created by node-splitting.
        let total_blocks_for_follow = self.pcode.blocks.len() + self.virtual_block_map.len();
        let follow_blocks: Vec<Option<usize>> = (0..total_blocks_for_follow)
            .map(|i| {
                let succs = self.successors.get(i)?;
                if succs.len() < 2 {
                    return None;
                }
                // Use efficient LCA on the idom tree instead of set intersection.
                let follow = imm_postdom.nearest_common_postdominator(succs)?;
                // Only use as follow if it's strictly after the branch block (forward edge).
                if follow <= i {
                    return None;
                }
                // Guard postdom-based follow discovery with a dominance-frontier witness
                // from at least one outgoing arm. This avoids selecting distant common
                // postdominators that are not a real local join for this branch.
                let has_frontier_witness = succs
                    .iter()
                    .copied()
                    .any(|succ| succ == follow || dom_frontier.contains(succ, follow));
                has_frontier_witness.then_some(follow)
            })
            .collect();

        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut last_structuring_failure = None;
        let mut idx = 0usize;
        // Total blocks = original pcode blocks + any virtual split nodes.
        let total_blocks = self.pcode.blocks.len() + self.virtual_block_map.len();
        while idx < total_blocks {
            if diag && idx > 0 && idx % 32 == 0 {
                eprintln!(
                    "[DIAG] structuring progress: idx={} elapsed={:.3}s",
                    idx,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let block_key = self.block_target_key(idx);
            let block_start = self.block_start_address(idx);
            let has_same_start_peer = self
                .pcode
                .blocks
                .iter()
                .enumerate()
                .any(|(peer_idx, block)| peer_idx != self.pcode_block_idx(idx) && block.start_address == block_start);
            let is_orphan_unreachable =
                idx != 0
                    && self.predecessors[idx].is_empty()
                    && !targeted.contains(&block_key)
                    && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }
            let pcode_idx = self.pcode_block_idx(idx);
            let mut structured_candidates = Vec::new();
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=switch elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let switch_candidate = self.try_lower_switch(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                switch_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=for elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let for_candidate = self.try_lower_for(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                for_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=dowhile elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let dowhile_candidate = self.try_lower_dowhile(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                dowhile_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=while elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let while_candidate = self.try_lower_while(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                while_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=loop_control elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let infloop_break_candidate = self.try_lower_infloop_with_break(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                infloop_break_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=infloop elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let infloop_candidate = self.try_lower_infloop(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                infloop_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=multiblock_infloop elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let multiblock_infloop_candidate = self.try_lower_multiblock_infloop(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                multiblock_infloop_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=short_if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let short_circuit_candidate = self.try_lower_short_circuit_if(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                short_circuit_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if_else_follow elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            // Postdominance-guided if-then-else: try before the heuristic variant.
            let if_else_follow_candidate =
                self.try_reduce_if_else_with_follow(idx, follow_blocks.get(idx).copied().flatten());
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                if_else_follow_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if_else elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let if_else_candidate = self.try_lower_if_else(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                if_else_candidate,
            )?;
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[pcode_idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            let if_candidate = self.try_lower_if(idx);
            self.consider_structured_candidate(
                idx,
                &targeted,
                &mut last_structuring_failure,
                &mut structured_candidates,
                if_candidate,
            )?;
            structured_candidates.sort_by(|lhs, rhs| {
                lhs.cost
                    .cmp(&rhs.cost)
                    .then_with(|| rhs.skip_to.cmp(&lhs.skip_to))
            });
            if let Some(best) = structured_candidates.into_iter().next() {
                body.push(best.stmt);
                idx = best.skip_to;
                last_structuring_failure = None;
                continue;
            }
            if let Some(err) = last_structuring_failure.take() {
                if let Some((recovered_body, skip_to)) = self.try_recover_region_linearized_body(
                    idx,
                    &err,
                    &targeted,
                    &mut emitted_labels,
                )? {
                    body.extend(recovered_body);
                    idx = skip_to;
                    continue;
                }
                return Err(err);
            }

            let pcode_idx_fallback = self.pcode_block_idx(idx);
            let block = &self.pcode.blocks[pcode_idx_fallback];
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                body.push(HirStmt::Label(block_label(block_key)));
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_stmts elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            body.extend(self.lower_block_stmts(block)?);
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_terminator elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            match self.lower_block_terminator(pcode_idx_fallback)? {
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
                LoweredTerminator::Unsupported {
                    evidence,
                    target_expr,
                } => {
                    self.record_unsupported_inventory_event(
                        "build_hir_multiblock_unsupported_terminator",
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
                    let cases: Vec<HirSwitchCase> = if let Some(proof) = proof.as_ref()
                        && proof.failure_family.is_none()
                        && !proof.recovered_cases.is_empty()
                    {
                        self.proof_payload_direct_emit_count += 1;
                        proof
                            .recovered_cases
                            .iter()
                            .filter(|(_, target)| Some(*target) != default_target)
                            .map(|(value, target)| HirSwitchCase {
                                values: vec![*value],
                                body: vec![HirStmt::Goto(block_label(*target))],
                            })
                            .collect()
                    } else if let Some(parsed) = self.parse_switch_chain(idx).ok().flatten() {
                        parsed
                            .cases
                            .into_iter()
                            .filter(|(_, block_idx)| {
                                let target = self.block_target_key(*block_idx);
                                Some(target) != default_target
                            })
                            .map(|(value, block_idx)| HirSwitchCase {
                                values: vec![value],
                                body: vec![HirStmt::Goto(block_label(
                                    self.block_target_key(block_idx),
                                ))],
                            })
                            .collect()
                    } else {
                        targets
                            .into_iter()
                            .filter(|target| Some(*target) != default_target)
                            .enumerate()
                            .map(|(i, t)| HirSwitchCase {
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
        // Note: We originally planned to emit explicit Gotos for irreducible edges here.
        // However, Fission's structural invariants (e.g. `can_inline_linear_successor` rejecting back-edges
        // and non-dominated cross-edges) naturally prevent irreducible edges from being swallowed into structured scopes.
        // Any block with an irreducible exit fails linear structuring and falls back to `lower_block_terminator`,
        // which correctly emits the explicit `If { Goto }` or unconditional `Goto` since it reads from the unmasked p-code.
        // Thus, Phase 4 (Irreducible Goto Pass) is inherently satisfied by the existing fallback mechanisms!

        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        if diag {
            eprintln!(
                "[DIAG] structuring done: elapsed={:.3}s stmts={}",
                total_start.elapsed().as_secs_f64(),
                body.len()
            );
            eprintln!(
                "[DIAG] structuring promotions: candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        } else if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage=structuring promotions candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        }
        metrics::histogram!("fission.structuring.total_ms")
            .record(total_start.elapsed().as_secs_f64() * 1000.0);
        metrics::counter!("fission.structuring.invocations_total").increment(1);
        let body = eliminate_redundant_gotos(body);
        Ok(cleanup_redundant_labels(body))
    }

    fn should_force_linear_structuring(&self) -> bool {
        if self.options.force_linear_structuring {
            return true;
        }
        let total_ops: usize = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        if self.pcode.blocks.len() > 80 {
            return true;
        }

        if self.options.is_64bit && self.pcode.blocks.len() >= 28 && total_ops >= 350 {
            return true;
        }

        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);

        self.pcode.blocks.len() > 32
            && (edge_count > self.pcode.blocks.len().saturating_mul(2)
                || multi_pred_blocks > 8
                || max_predecessors >= 4)
    }
}

pub(crate) fn structuring_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}

#[cfg(test)]
pub(crate) fn promote_single_entry_guarded_tail_regions_for_test(
    body: &mut Vec<HirStmt>,
) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        global_names: Default::default(),
        calling_convention: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    while builder.promote_single_entry_guarded_tail_regions(body) {}
    builder.preview_build_stats()
}

#[cfg(test)]
pub(crate) fn discover_guarded_tail_candidates_for_test(body: &[HirStmt]) -> PreviewBuildStats {
    discover_guarded_tail_candidates_for_stats(body)
}

pub(crate) fn discover_guarded_tail_candidates_for_stats(body: &[HirStmt]) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        global_names: Default::default(),
        calling_convention: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}
