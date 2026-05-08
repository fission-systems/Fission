use super::cleanup::finalize_structured_body;
use super::graph::StructureNodeKind;
use super::irreducible::compute_node_splits;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuringAdmissionReason {
    GraphCollapse,
    ExplicitForceLinear,
    IrreducibleBudget,
    ExtremeBudget,
}

#[derive(Debug, Clone, Copy)]
struct StructuringAdmissionInput {
    block_count: usize,
    total_ops: usize,
    edge_count: usize,
    multi_pred_blocks: usize,
    max_predecessors: usize,
    scc_irreducible_count: usize,
    max_scc_component_size: usize,
    explicit_force_linear: bool,
}

fn decide_structuring_admission(input: StructuringAdmissionInput) -> StructuringAdmissionReason {
    if input.explicit_force_linear {
        return StructuringAdmissionReason::ExplicitForceLinear;
    }

    // Keep an escape hatch for truly pathological CFGs, but stop forcing
    // linear lowering for merely "large" reducible functions like `fibonacci`,
    // which are complex but still structurally recoverable.
    let extreme_budget = input.block_count > 192
        || input.total_ops > 3_000
        || (input.edge_count > input.block_count.saturating_mul(4)
            && input.max_predecessors >= 6
            && input.max_scc_component_size > 64);
    if extreme_budget {
        return StructuringAdmissionReason::ExtremeBudget;
    }

    let irreducible_budget = input.scc_irreducible_count > 0
        && (input.block_count > 64
            || input.total_ops > 900
            || input.edge_count > input.block_count.saturating_mul(3)
            || input.multi_pred_blocks > 16
            || input.max_predecessors >= 5
            || input.max_scc_component_size > 24);
    if irreducible_budget {
        return StructuringAdmissionReason::IrreducibleBudget;
    }

    StructuringAdmissionReason::GraphCollapse
}

fn mir_blockgraph_admission_enabled() -> bool {
    std::env::var_os("FISSION_ENABLE_MIR_BLOCKGRAPH").is_some()
}

fn apply_mir_blockgraph_admission_gate(
    admission: StructuringAdmissionReason,
    enabled: bool,
) -> StructuringAdmissionReason {
    if enabled && matches!(admission, StructuringAdmissionReason::IrreducibleBudget) {
        StructuringAdmissionReason::GraphCollapse
    } else {
        admission
    }
}

impl<'a> PreviewBuilder<'a> {
    #[cfg(test)]
    fn is_switch_scaffold_stmt(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Goto(_) => true,
            HirStmt::Block(body) => body.iter().all(Self::is_switch_scaffold_stmt),
            HirStmt::Label(_)
            | HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::If { .. }
            | HirStmt::Switch { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => false,
        }
    }

    #[cfg(test)]
    fn switch_stmt_has_scaffold_only_arms(stmt: &HirStmt) -> bool {
        let HirStmt::Switch { cases, default, .. } = stmt else {
            return false;
        };
        !cases.is_empty()
            && cases
                .iter()
                .all(|case| case.body.iter().all(Self::is_switch_scaffold_stmt))
            && default.iter().all(Self::is_switch_scaffold_stmt)
    }

    fn region_kind_for_stmt(stmt: &HirStmt) -> Option<RegionKind> {
        match stmt {
            HirStmt::Switch { .. } => Some(RegionKind::Switch),
            HirStmt::If { .. } => Some(RegionKind::Conditional),
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {
                Some(RegionKind::Loop)
            }
            HirStmt::Block(_) => Some(RegionKind::Sequence),
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => None,
        }
    }

    fn region_selector_or_condition(stmt: &HirStmt) -> Option<String> {
        match stmt {
            HirStmt::Switch { expr, .. } => Some(print_expr(expr)),
            HirStmt::If { cond, .. }
            | HirStmt::While { cond, .. }
            | HirStmt::DoWhile { cond, .. } => Some(print_expr(cond)),
            HirStmt::For { cond, .. } => cond.as_ref().map(print_expr),
            HirStmt::Block(_)
            | HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => None,
        }
    }

    fn build_region_proof(
        &self,
        start_idx: usize,
        skip_to: usize,
        stmt: &HirStmt,
    ) -> Option<RegionProof> {
        let kind = Self::region_kind_for_stmt(stmt)?;
        Some(RegionProof::structured(
            kind,
            start_idx,
            skip_to,
            Self::region_selector_or_condition(stmt),
        ))
    }

    fn record_region_candidate(&mut self, proof: &RegionProof) {
        self.region_proof_candidate_count += 1;
        if proof.proof_complete {
            self.region_proof_completed_count += 1;
        }
        if matches!(proof.kind, RegionKind::Conditional) {
            self.conditional_region_candidate_count += 1;
        }
    }

    fn record_selected_region(&mut self, node: &StructureNode) {
        if matches!(
            node.kind,
            StructureNodeKind::Region(RegionKind::Conditional)
        ) {
            self.conditional_region_promoted_count += 1;
        }
    }

    fn consider_structured_candidate(
        &mut self,
        rule: CollapseRule,
        start_idx: usize,
        targeted: &HashSet<u64>,
        last_structuring_failure: &mut Option<MlilPreviewError>,
        candidates: &mut Vec<CollapseCandidate>,
        result: Result<Option<(HirStmt, usize)>, MlilPreviewError>,
    ) -> Result<(), MlilPreviewError> {
        if let Some((stmt, skip_to)) =
            Self::capture_structuring_failure(result, last_structuring_failure)?
            && self.accept_structured_region(start_idx, skip_to, targeted)
        {
            let Some(proof) = self.build_region_proof(start_idx, skip_to, &stmt) else {
                return Ok(());
            };
            self.record_region_candidate(&proof);
            candidates.push(CollapseCandidate {
                rule,
                node: StructureNode::region(usize::MAX, stmt, skip_to, proof),
            });
        }
        Ok(())
    }

    fn select_structured_candidate(
        &self,
        candidates: Vec<CollapseCandidate>,
    ) -> Option<CollapseCandidate> {
        candidates.into_iter().next()
    }

    pub(crate) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        if let Some(body) = self.try_lower_intra_instruction_conditional_return()? {
            return Ok(body);
        }
        if let Some(body) = self.try_lower_conditional_tailcall_after_return()? {
            return Ok(body);
        }

        let diag = structuring_diag_enabled();
        let total_start = Instant::now();
        let (
            scc_component_count,
            scc_irreducible_count,
            scc_irreducible_header_count,
            max_scc_component_size,
        ) = {
            let scc = self.cfg_fact_cache().scc();
            (
                scc.component_count(),
                scc.irreducible_count(),
                scc.irreducible_header_total_count(),
                scc.max_component_size(),
            )
        };
        self.structuring_scc_component_count += scc_component_count;
        self.max_structuring_scc_component_size = self
            .max_structuring_scc_component_size
            .max(max_scc_component_size);
        self.structuring_irreducible_scc_count += scc_irreducible_count;
        self.structuring_irreducible_header_count += scc_irreducible_header_count;
        let original_admission =
            self.structuring_admission_reason(scc_irreducible_count, max_scc_component_size);
        let mir_blockgraph_enabled = mir_blockgraph_admission_enabled();
        if mir_blockgraph_enabled {
            self.mir_blockgraph_admission_enabled_count += 1;
            match original_admission {
                StructuringAdmissionReason::IrreducibleBudget => {
                    self.mir_blockgraph_irreducible_budget_bypass_count += 1;
                }
                StructuringAdmissionReason::ExtremeBudget => {
                    self.mir_blockgraph_extreme_budget_blocked_count += 1;
                }
                StructuringAdmissionReason::GraphCollapse
                | StructuringAdmissionReason::ExplicitForceLinear => {}
            }
        }
        let admission =
            apply_mir_blockgraph_admission_gate(original_admission, mir_blockgraph_enabled);
        let force_linear = !matches!(admission, StructuringAdmissionReason::GraphCollapse);
        let mir_blockgraph_irreducible_trial = mir_blockgraph_enabled
            && matches!(
                original_admission,
                StructuringAdmissionReason::IrreducibleBudget
            )
            && matches!(admission, StructuringAdmissionReason::GraphCollapse);
        let pre_trial_successors =
            mir_blockgraph_irreducible_trial.then(|| self.successors.clone());
        let pre_trial_predecessors =
            mir_blockgraph_irreducible_trial.then(|| self.predecessors.clone());
        let pre_trial_virtual_block_map =
            mir_blockgraph_irreducible_trial.then(|| self.virtual_block_map.clone());
        let pre_trial_blockgraph_complete_count = self.blockgraph_region_complete_count;

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
            match admission {
                StructuringAdmissionReason::ExplicitForceLinear => {
                    self.structuring_force_linear_explicit_count += 1;
                }
                StructuringAdmissionReason::IrreducibleBudget => {
                    self.structuring_force_linear_irreducible_budget_count += 1;
                }
                StructuringAdmissionReason::ExtremeBudget => {
                    self.structuring_force_linear_extreme_budget_count += 1;
                }
                StructuringAdmissionReason::GraphCollapse => {}
            }
            let result = self.build_proof_first_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success={} proof_first={} admission={:?}",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok(),
                    true,
                    admission,
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

        let mut graph = StructureGraph::default();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut last_structuring_failure = None;
        let mut previous_node_id = None;
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
            let has_same_start_peer =
                self.pcode
                    .blocks
                    .iter()
                    .enumerate()
                    .any(|(peer_idx, block)| {
                        peer_idx != self.pcode_block_idx(idx) && block.start_address == block_start
                    });
            let is_orphan_unreachable = idx != 0
                && self.predecessors[idx].is_empty()
                && !targeted.contains(&block_key)
                && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }
            let pcode_idx = self.pcode_block_idx(idx);
            let mut structured_candidates = Vec::new();
            let follow = follow_blocks.get(idx).copied().flatten();
            for rule in ACTIVE_COLLAPSE_RULES {
                if diag {
                    eprintln!(
                        "[DIAG] structuring idx={} block=0x{:x} attempt={} elapsed={:.3}s",
                        idx,
                        self.pcode.blocks[pcode_idx].start_address,
                        rule.name(),
                        total_start.elapsed().as_secs_f64()
                    );
                }
                match rule {
                    CollapseRule::Switch => {
                        let switch_candidate = self.try_lower_switch(idx);
                        self.consider_structured_candidate(
                            rule,
                            idx,
                            &targeted,
                            &mut last_structuring_failure,
                            &mut structured_candidates,
                            switch_candidate,
                        )?;
                    }
                    CollapseRule::ForLoop => {
                        let for_candidate = self.try_lower_for(idx);
                        self.consider_structured_candidate(
                            rule,
                            idx,
                            &targeted,
                            &mut last_structuring_failure,
                            &mut structured_candidates,
                            for_candidate,
                        )?;
                    }
                    CollapseRule::DoWhile => {
                        let dowhile_candidate = self.try_lower_dowhile(idx);
                        self.consider_structured_candidate(
                            rule,
                            idx,
                            &targeted,
                            &mut last_structuring_failure,
                            &mut structured_candidates,
                            dowhile_candidate,
                        )?;
                    }
                    CollapseRule::WhileDo => {
                        let while_candidate = self.try_lower_while(idx);
                        self.consider_structured_candidate(
                            rule,
                            idx,
                            &targeted,
                            &mut last_structuring_failure,
                            &mut structured_candidates,
                            while_candidate,
                        )?;
                    }
                    CollapseRule::InfLoopBreak => {
                        let infloop_break_candidate = self.try_lower_infloop_with_break(idx);
                        self.consider_structured_candidate(
                            rule,
                            idx,
                            &targeted,
                            &mut last_structuring_failure,
                            &mut structured_candidates,
                            infloop_break_candidate,
                        )?;
                    }
                    CollapseRule::InfLoop => {
                        for result in [
                            self.try_lower_infloop(idx),
                            self.try_lower_multiblock_infloop(idx),
                        ] {
                            self.consider_structured_candidate(
                                rule,
                                idx,
                                &targeted,
                                &mut last_structuring_failure,
                                &mut structured_candidates,
                                result,
                            )?;
                        }
                    }
                    CollapseRule::Conditional => {
                        for result in [
                            self.try_lower_short_circuit_if(idx),
                            self.try_reduce_if_else_with_follow(idx, follow),
                            self.try_lower_if_else(idx),
                            self.try_lower_if(idx),
                        ] {
                            self.consider_structured_candidate(
                                rule,
                                idx,
                                &targeted,
                                &mut last_structuring_failure,
                                &mut structured_candidates,
                                result,
                            )?;
                        }
                    }
                    CollapseRule::Sequence | CollapseRule::Unstructured => {}
                }
            }
            if let Some(mut best) = self.select_structured_candidate(structured_candidates) {
                if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                    best.node
                        .statements
                        .insert(0, HirStmt::Label(block_label(block_key)));
                }
                if diag {
                    eprintln!(
                        "[DIAG] structuring idx={} selected_rule={} skip_to={}",
                        idx,
                        best.rule.name(),
                        best.node.skip_to
                    );
                }
                self.record_selected_region(&best.node);
                idx = best.node.skip_to;
                best.node.id = graph.next_node_id();
                let node_id = graph.push(best.node);
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
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
                    let node_id = graph.next_node_id();
                    let node_id = graph.push(StructureNode::unstructured(
                        node_id,
                        recovered_body,
                        skip_to,
                    ));
                    if let Some(prev) = previous_node_id {
                        graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                    }
                    previous_node_id = Some(node_id);
                    idx = skip_to;
                    continue;
                }
                return Err(err);
            }

            let pcode_idx_fallback = self.pcode_block_idx(idx);
            let block = &self.pcode.blocks[pcode_idx_fallback];
            let mut node_body = Vec::new();
            let mut explicit_edge_surface = false;
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                node_body.push(HirStmt::Label(block_label(block_key)));
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_stmts elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            node_body.extend(self.lower_block_stmts(block)?);
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_terminator elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => node_body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                    {
                        node_body.push(HirStmt::Return(Some(expr)));
                        explicit_edge_surface = true;
                    } else if self.next_block_address(idx) != Some(target) {
                        node_body.push(HirStmt::Goto(block_label(target)));
                        explicit_edge_surface = true;
                    }
                }
                LoweredTerminator::Fallthrough(Some(target)) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                    {
                        node_body.push(HirStmt::Return(Some(expr)));
                        explicit_edge_surface = true;
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if let Some(true_idx) =
                        self.find_block_index_by_address(true_target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, true_idx)?
                    {
                        vec![HirStmt::Return(Some(expr))]
                    } else if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target)
                            if let Some(false_idx) =
                                self.find_block_index_by_address(false_target)
                                && let Some(expr) = self
                                    .lower_return_join_expr_for_predecessor(idx, false_idx)? =>
                        {
                            vec![HirStmt::Return(Some(expr))]
                        }
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    node_body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Fallthrough(None) => {}
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
                    node_body.push(self.emit_unsupported_control_surface(evidence, target_expr));
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    let cases: Vec<HirSwitchCase> = if let Some(proof) = proof.as_ref() {
                        if EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready {
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
                        } else {
                            recovered_switch_case_values(
                                &targets,
                                default_target,
                                min_val,
                                Some(proof),
                            )
                            .0
                            .into_iter()
                            .map(|(value, target)| HirSwitchCase {
                                values: vec![value],
                                body: vec![HirStmt::Goto(block_label(target))],
                            })
                            .collect()
                        }
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
                    node_body.push(HirStmt::Switch {
                        expr,
                        cases,
                        default: default_target
                            .map(block_label)
                            .map(HirStmt::Goto)
                            .into_iter()
                            .collect(),
                    });
                    explicit_edge_surface = true;
                }
            }
            if explicit_edge_surface {
                let node_id = graph.next_node_id();
                let node_id = graph.push(StructureNode::unstructured(node_id, node_body, idx + 1));
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            } else {
                let node_id = graph.next_node_id();
                let node_id = graph.push(StructureNode::basic(node_id, node_body, idx + 1));
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            }
            idx += 1;
        }
        // Note: We originally planned to emit explicit Gotos for irreducible edges here.
        // However, Fission's structural invariants (e.g. `can_inline_linear_successor` rejecting back-edges
        // and non-dominated cross-edges) naturally prevent irreducible edges from being swallowed into structured scopes.
        // Any block with an irreducible exit fails linear structuring and falls back to `lower_block_terminator`,
        // which correctly emits the explicit `If { Goto }` or unconditional `Goto` since it reads from the unmasked p-code.
        // Thus, Phase 4 (Irreducible Goto Pass) is inherently satisfied by the existing fallback mechanisms!
        let mut body = surface_structure_graph(graph);
        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        if mir_blockgraph_irreducible_trial
            && self.blockgraph_region_complete_count == pre_trial_blockgraph_complete_count
        {
            if let (Some(successors), Some(predecessors), Some(virtual_block_map)) = (
                pre_trial_successors,
                pre_trial_predecessors,
                pre_trial_virtual_block_map,
            ) {
                self.successors = successors;
                self.predecessors = predecessors;
                self.virtual_block_map = virtual_block_map;
                self.refresh_cfg_fact_cache();
            }
            self.forced_linear_structuring_count += 1;
            self.structuring_force_linear_irreducible_budget_count += 1;
            let result = self.build_proof_first_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring mir-blockgraph fail-closed: elapsed={:.3}s success={} complete_delta=0",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok(),
                );
            }
            return result;
        }
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
        Ok(finalize_structured_body(body))
    }

    fn structuring_admission_reason(
        &self,
        scc_irreducible_count: usize,
        max_scc_component_size: usize,
    ) -> StructuringAdmissionReason {
        let total_ops: usize = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        let block_count = self.pcode.blocks.len();
        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);
        decide_structuring_admission(StructuringAdmissionInput {
            block_count,
            total_ops,
            edge_count,
            multi_pred_blocks,
            max_predecessors,
            scc_irreducible_count,
            max_scc_component_size,
            explicit_force_linear: self.options.force_linear_structuring,
        })
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
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
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
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        calling_convention: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}

#[cfg(test)]
mod tests {
    use super::{
        PreviewBuilder, StructuringAdmissionInput, StructuringAdmissionReason,
        apply_mir_blockgraph_admission_gate, decide_structuring_admission,
    };
    use crate::PcodeFunction;
    use crate::nir::types::{
        HirExpr, HirStmt, HirSwitchCase, MlilPreviewOptions, NirType, StructuringEngineKind,
    };
    use crate::nir::{CollapseCandidate, CollapseRule, RegionKind, RegionProof, StructureNode};

    fn const_expr(value: i64) -> HirExpr {
        HirExpr::Const(
            value,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )
    }

    #[test]
    fn switch_scaffold_detection_accepts_goto_only_arms() {
        let stmt = HirStmt::Switch {
            expr: const_expr(0),
            cases: vec![
                HirSwitchCase {
                    values: vec![0],
                    body: vec![HirStmt::Goto("case_0".to_string())],
                },
                HirSwitchCase {
                    values: vec![1],
                    body: vec![HirStmt::Goto("case_1".to_string())],
                },
            ],
            default: vec![HirStmt::Goto("default".to_string())],
        };
        assert!(PreviewBuilder::switch_stmt_has_scaffold_only_arms(&stmt));
    }

    #[test]
    fn switch_scaffold_detection_rejects_payload_arms() {
        let stmt = HirStmt::Switch {
            expr: const_expr(0),
            cases: vec![HirSwitchCase {
                values: vec![0],
                body: vec![HirStmt::Expr(const_expr(1))],
            }],
            default: vec![],
        };
        assert!(!PreviewBuilder::switch_stmt_has_scaffold_only_arms(&stmt));
    }

    fn test_builder_with_engine(engine: StructuringEngineKind) -> PreviewBuilder<'static> {
        let dummy = Box::leak(Box::new(PcodeFunction { blocks: Vec::new() }));
        let options = Box::leak(Box::new(MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: engine,
            global_names: Default::default(),
            calling_convention: Default::default(),
        }));
        PreviewBuilder::new(dummy, options, None)
    }

    fn candidate(skip_to: usize, rule: CollapseRule) -> CollapseCandidate {
        CollapseCandidate {
            rule,
            node: StructureNode::region(
                usize::MAX,
                HirStmt::If {
                    cond: const_expr(1),
                    then_body: vec![],
                    else_body: vec![],
                },
                skip_to,
                RegionProof::structured(RegionKind::Conditional, 0, skip_to, Some("cond".into())),
            ),
        }
    }

    #[test]
    fn graph_collapse_v1_preserves_attempt_order() {
        let builder = test_builder_with_engine(StructuringEngineKind::GraphCollapseV1);
        let selected = builder
            .select_structured_candidate(vec![
                candidate(2, CollapseRule::Conditional),
                candidate(8, CollapseRule::Switch),
            ])
            .expect("graph candidate");
        assert_eq!(selected.node.skip_to, 2);
    }

    #[test]
    fn legacy_scored_alias_still_preserves_graph_attempt_order() {
        let builder = test_builder_with_engine(StructuringEngineKind::LegacyScored);
        let selected = builder
            .select_structured_candidate(vec![
                candidate(2, CollapseRule::Conditional),
                candidate(8, CollapseRule::Switch),
            ])
            .expect("legacy candidate");
        assert_eq!(selected.node.skip_to, 2);
    }

    #[test]
    fn structuring_admission_prefers_graph_collapse_for_reducible_medium_cfg() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 31,
            total_ops: 620,
            edge_count: 58,
            multi_pred_blocks: 10,
            max_predecessors: 3,
            scc_irreducible_count: 0,
            max_scc_component_size: 31,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::GraphCollapse);
    }

    #[test]
    fn structuring_admission_forces_linear_for_irreducible_budget() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 72,
            total_ops: 960,
            edge_count: 220,
            multi_pred_blocks: 18,
            max_predecessors: 6,
            scc_irreducible_count: 2,
            max_scc_component_size: 28,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::IrreducibleBudget);
    }

    #[test]
    fn structuring_admission_forces_linear_for_explicit_override() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 12,
            total_ops: 80,
            edge_count: 14,
            multi_pred_blocks: 1,
            max_predecessors: 2,
            scc_irreducible_count: 0,
            max_scc_component_size: 4,
            explicit_force_linear: true,
        });
        assert_eq!(decision, StructuringAdmissionReason::ExplicitForceLinear);
    }

    #[test]
    fn structuring_admission_forces_linear_for_extreme_budget() {
        let decision = decide_structuring_admission(StructuringAdmissionInput {
            block_count: 220,
            total_ops: 3_400,
            edge_count: 980,
            multi_pred_blocks: 40,
            max_predecessors: 8,
            scc_irreducible_count: 0,
            max_scc_component_size: 80,
            explicit_force_linear: false,
        });
        assert_eq!(decision, StructuringAdmissionReason::ExtremeBudget);
    }

    #[test]
    fn mir_blockgraph_gate_allows_irreducible_budget_graph_collapse() {
        let decision = apply_mir_blockgraph_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::GraphCollapse);
    }

    #[test]
    fn mir_blockgraph_gate_stays_fail_closed_for_extreme_budget() {
        let decision =
            apply_mir_blockgraph_admission_gate(StructuringAdmissionReason::ExtremeBudget, true);
        assert_eq!(decision, StructuringAdmissionReason::ExtremeBudget);
    }

    #[test]
    fn mir_blockgraph_gate_stays_fail_closed_for_explicit_override() {
        let decision = apply_mir_blockgraph_admission_gate(
            StructuringAdmissionReason::ExplicitForceLinear,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::ExplicitForceLinear);
    }

    #[test]
    fn mir_blockgraph_gate_is_noop_when_disabled() {
        let decision = apply_mir_blockgraph_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            false,
        );
        assert_eq!(decision, StructuringAdmissionReason::IrreducibleBudget);
    }
}
