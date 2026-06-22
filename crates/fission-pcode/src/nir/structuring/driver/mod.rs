use super::cleanup::child_body_has_entry_label;
use super::irreducible::{compute_fas_virtual_gotos, compute_node_splits};
use super::*;

mod admission;
pub use admission::*;

pub(crate) mod collapse;
mod orphan_repair;

fn apply_blockgraph_collapse_admission_gate(
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
        self.telemetry.structuring.region_proof_candidate_count += 1;
        if proof.proof_complete {
            self.telemetry.structuring.region_proof_completed_count += 1;
        }
        if matches!(proof.kind, RegionKind::Conditional) {
            self.telemetry
                .structuring
                .conditional_region_candidate_count += 1;
        }
    }

    fn record_selected_region(&mut self, node: &StructureNode) {
        if matches!(
            node.kind,
            StructureNodeKind::Region(RegionKind::Conditional)
        ) {
            self.telemetry.structuring.conditional_region_promoted_count += 1;
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
        {
            let accepted = if matches!(rule, CollapseRule::Switch) {
                let region: HashSet<usize> = (start_idx..skip_to).collect();
                !self.region_has_external_entry(&region, start_idx)
            } else {
                self.accept_structured_region(start_idx, skip_to, targeted)
            };
            if accepted {
                let Some(proof) = self.build_region_proof(start_idx, skip_to, &stmt) else {
                    return Ok(());
                };
                self.record_region_candidate(&proof);
                candidates.push(CollapseCandidate {
                    rule,
                    node: StructureNode::region(usize::MAX, stmt, skip_to, proof),
                });
            }
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
        CollapseDriver::run(self)
    }

    pub(crate) fn build_sese_region_body(
        &mut self,
        entry: usize,
        exit: usize,
        child_map: crate::fast_hash::FastMap<usize, (Vec<HirStmt>, usize, RegionProof)>,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        if let Some(total_start) = self.structuring_start {
            if total_start.elapsed().as_secs_f64() > 0.5 {
                if diag {
                    eprintln!("[DIAG] build_sese_region_body: aborting structuring entry due to 500ms time ceiling");
                }
                return Err(MlilPreviewError::UnsupportedCfgRegionShape);
            }
        }
        let mut graph = StructureGraph::default();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut previous_node_id = None;

        let dom_frontier = self.cfg_fact_cache().dominance_frontier();
        let imm_postdom = self.cfg_fact_cache().immediate_postdominators();

        let follow_blocks = self.compute_follow_blocks();

        let mut active_child_map = child_map;
        active_child_map.retain(|&k, &mut (_, exit, _)| exit > k);
        let mut progress = true;
        let mut tier1_failures = std::collections::HashMap::new();
        let mut collapse_iterations = 0;

        // Tier 1 & Tier 2 Collapsing Loop
        while progress {
            if let Some(total_start) = self.structuring_start {
                if total_start.elapsed().as_secs_f64() > 0.5 {
                    if diag {
                        eprintln!("[DIAG] build_sese_region_body: aborting collapse loop due to 500ms time ceiling");
                    }
                    return Err(MlilPreviewError::UnsupportedCfgRegionShape);
                }
            }
            progress = false;
            collapse_iterations += 1;
            if collapse_iterations > 100 {
                if diag {
                    eprintln!(
                        "[DIAG] build_sese_region_body collapsing loop: tripped budget at {} iterations",
                        collapse_iterations
                    );
                }
                break;
            }

            // Tier 1: Try to match only standard IDEAL rules across the entire range
            let mut idx = entry;
            while idx < exit {
                if let Some((_, child_exit, _)) = active_child_map.get(&idx) {
                    idx = *child_exit;
                    continue;
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

                let mut ideal_candidates = Vec::new();
                let follow = follow_blocks.get(idx).copied().flatten();
                let mut last_structuring_failure = None;

                for rule in ACTIVE_COLLAPSE_RULES {
                    if matches!(rule, CollapseRule::Sequence | CollapseRule::Unstructured) {
                        continue;
                    }
                    let res = match rule {
                        CollapseRule::Switch => self.try_lower_switch(idx),
                        CollapseRule::ForLoop => self.try_lower_for(idx),
                        CollapseRule::DoWhile => {
                            let mut dw = self.try_lower_dowhile(idx)?;
                            if dw.is_none() {
                                dw = self.try_lower_multiblock_dowhile(idx)?;
                            }
                            Ok(dw)
                        }
                        CollapseRule::WhileDo => self.try_lower_while(idx),
                        CollapseRule::InfLoopBreak => self.try_lower_infloop_with_break(idx),
                        CollapseRule::InfLoop => {
                            let mut inf = self.try_lower_infloop(idx);
                            if inf.is_err() || matches!(inf, Ok(None)) {
                                inf = self.try_lower_multiblock_infloop(idx);
                            }
                            inf
                        }
                        CollapseRule::Conditional => {
                            let mut cond = self.try_lower_short_circuit_if(idx);
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_reduce_if_else_with_follow(idx, follow);
                            }
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_lower_if_else(idx);
                            }
                            if cond.is_err() || matches!(cond, Ok(None)) {
                                cond = self.try_lower_if(idx);
                            }
                            cond
                        }
                        _ => Ok(None),
                    };

                    self.consider_structured_candidate(
                        rule,
                        idx,
                        &targeted,
                        &mut last_structuring_failure,
                        &mut ideal_candidates,
                        res,
                    )?;
                }
                if let Some(ref err) = last_structuring_failure {
                    tier1_failures.insert(idx, err.clone());
                }

                if let Some(best) = self.select_structured_candidate(ideal_candidates) {
                    let skip_to = best.node.skip_to;
                    if skip_to <= idx {
                        if diag {
                            eprintln!("[DIAG] select_structured_candidate returned non-advancing skip_to: {} <= {}", skip_to, idx);
                        }
                        idx += 1;
                        continue;
                    }
                    let proof = best.node.proof.unwrap();
                    active_child_map.insert(idx, (best.node.statements, skip_to, proof));
                    progress = true;
                    break;
                }

                idx += 1;
            }

            if progress {
                continue;
            }

            // Tier 2: Deferred Linearization Fallback
            let mut idx = entry;
            while idx < exit {
                if let Some((_, child_exit, _)) = active_child_map.get(&idx) {
                    idx = *child_exit;
                    continue;
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

                let last_structuring_failure = tier1_failures.remove(&idx);

                if let Some(err) = last_structuring_failure {
                    let mut temp_emitted_labels = emitted_labels.clone();
                    if let Some((recovered_body, skip_to)) = self.try_recover_region_linearized_body(
                        idx,
                        &err,
                        &targeted,
                        &mut temp_emitted_labels,
                    )? {
                        emitted_labels = temp_emitted_labels;
                        let dummy_proof = RegionProof::structured(RegionKind::Sequence, idx, skip_to, None);
                        active_child_map.insert(idx, (recovered_body, skip_to, dummy_proof));
                        progress = true;
                        break;
                    }
                }

                idx += 1;
            }

            if !progress && super::collapse_loop::collapse_loop_admission_enabled() {
                if self.try_virtualize_one_bad_edge(entry, exit)? {
                    if diag {
                        eprintln!(
                            "[DIAG] build_sese_region_body: virtualized bad edge, continuing collapse loop"
                        );
                    }
                    progress = true;
                }
            }
        }

        // Final reconstruction scan: reconstruct graph step-by-step
        let mut idx = entry;
        while idx < exit {
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

            if let Some((child_body, child_exit, child_proof)) = active_child_map.get(&idx) {
                let mut node_statements = child_body.clone();
                let header_label = block_label(block_key);
                if (idx == 0 || targeted.contains(&block_key))
                    && emitted_labels.insert(block_key)
                    && !child_body_has_entry_label(child_body, &header_label)
                {
                    node_statements.insert(0, HirStmt::Label(header_label));
                }

                let node = StructureNode {
                    id: graph.next_node_id(),
                    kind: StructureNodeKind::Region(child_proof.kind),
                    skip_to: *child_exit,
                    statements: node_statements,
                    proof: Some(child_proof.clone()),
                };

                let node_id = graph.push(node);
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
                let next_idx = *child_exit;
                if next_idx <= idx {
                    if diag {
                        eprintln!("[DIAG] final reconstruction SESE scan: non-advancing child_exit: {} <= {}", next_idx, idx);
                    }
                    idx += 1;
                    continue;
                }
                idx = next_idx;
                continue;
            }

            let pcode_idx_fallback = self.pcode_block_idx(idx);
            let block = &self.pcode.blocks[pcode_idx_fallback];
            let mut node_body = Vec::new();
            let mut explicit_edge_surface = false;
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                node_body.push(HirStmt::Label(block_label(block_key)));
            }
            node_body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => node_body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target) {
                        if let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(HirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        } else if self.next_block_address(idx) != Some(target) {
                            node_body.push(HirStmt::Goto(block_label(target)));
                            explicit_edge_surface = true;
                        }
                    } else if self.next_block_address(idx) != Some(target) {
                        node_body.push(HirStmt::Goto(block_label(target)));
                        explicit_edge_surface = true;
                    }
                }
                LoweredTerminator::Fallthrough(Some(target)) => {
                    if let Some(target_idx) = self.find_block_index_by_address(target) {
                        if let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(HirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        }
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let true_idx = self.find_block_index_by_address(true_target);
                    let false_idx = false_target
                        .and_then(|target| self.find_block_index_by_address(target));
                    let true_virtual = true_idx.is_some_and(|ti| self.is_virtual_goto_edge(idx, ti));
                    let false_virtual =
                        false_idx.is_some_and(|fi| self.is_virtual_goto_edge(idx, fi));
                    let mut then_body = if true_virtual || next_addr != Some(true_target) {
                        vec![HirStmt::Goto(block_label(true_target))]
                    } else {
                        Vec::new()
                    };
                    if let Some(true_idx) = true_idx {
                        if let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(idx, true_idx)?
                        {
                            then_body = vec![HirStmt::Return(Some(expr))];
                        }
                    }
                    let else_body = match false_target {
                        Some(false_target) => {
                            let mut else_body = if false_virtual || Some(false_target) != next_addr
                            {
                                vec![HirStmt::Goto(block_label(false_target))]
                            } else {
                                Vec::new()
                            };
                            if let Some(false_idx) = false_idx {
                                if let Some(expr) =
                                    self.lower_return_join_expr_for_predecessor(idx, false_idx)?
                                {
                                    else_body = vec![HirStmt::Return(Some(expr))];
                                }
                            }
                            else_body
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

        let mut body = surface_structure_graph(graph);
        let mut gt_iterations = 0;
        while self.promote_single_entry_guarded_tail_regions(&mut body) {
            gt_iterations += 1;
            if gt_iterations >= 30 {
                if diag {
                    eprintln!(
                        "[DIAG] promote_single_entry_guarded_tail_regions: budget tripped at {} iterations",
                        gt_iterations
                    );
                }
                break;
            }
        }

        Ok(body)
    }

    pub(crate) fn structuring_admission_reason(
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
        is_big_endian: false,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        is_data_ref_origin: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: Default::default(),
        userops: Default::default(),
        cspec_param_offsets: None,
        cspec_stack_arg_base: None,
        cspec_extrapop: None,
        sla_register_map: None,
        cspec_return_offset: None,
        cspec_return_target: None,
        pspec_programcounter: None,
        pspec_tracked_context: Vec::new(),
        pspec_hidden_registers: Default::default(),
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
        is_big_endian: false,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        is_data_ref_origin: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: Default::default(),
        userops: Default::default(),
        cspec_param_offsets: None,
        cspec_stack_arg_base: None,
        cspec_extrapop: None,
        sla_register_map: None,
        cspec_return_offset: None,
        cspec_return_target: None,
        pspec_programcounter: None,
        pspec_tracked_context: Vec::new(),
        pspec_hidden_registers: Default::default(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}

#[cfg(test)]
mod tests {
    use super::{
        PreviewBuilder, StructuringAdmissionInput, StructuringAdmissionReason,
        apply_blockgraph_collapse_admission_gate, decide_structuring_admission,
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
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            is_data_ref_origin: false,
            structuring_engine: engine,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
        cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_return_target: None,
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: Default::default(),
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
    fn blockgraph_collapse_gate_allows_irreducible_budget_graph_collapse() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::GraphCollapse);
    }

    #[test]
    fn blockgraph_collapse_gate_stays_fail_closed_for_extreme_budget() {
        let decision =
            apply_blockgraph_collapse_admission_gate(StructuringAdmissionReason::ExtremeBudget, true);
        assert_eq!(decision, StructuringAdmissionReason::ExtremeBudget);
    }

    #[test]
    fn blockgraph_collapse_gate_stays_fail_closed_for_explicit_override() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::ExplicitForceLinear,
            true,
        );
        assert_eq!(decision, StructuringAdmissionReason::ExplicitForceLinear);
    }

    #[test]
    fn blockgraph_collapse_gate_is_noop_when_disabled() {
        let decision = apply_blockgraph_collapse_admission_gate(
            StructuringAdmissionReason::IrreducibleBudget,
            false,
        );
        assert_eq!(decision, StructuringAdmissionReason::IrreducibleBudget);
    }
}
