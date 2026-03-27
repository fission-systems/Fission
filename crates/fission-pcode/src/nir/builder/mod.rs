use super::*;

mod aggregate_recovery;
mod call_recovery;
mod entry_analysis;
mod lower_expr;
mod materialize;
mod stack_slots;
mod terminator;
mod type_hints;

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    type_hints::apply_preview_type_hints(func, context)
}

#[cfg(test)]
pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    local_hints: &mut HashMap<String, String>,
) {
    type_hints::collect_local_surface_hints(body, pointer_hints, func, local_hints);
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn new(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        let mut defs = HashMap::new();
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                if let Some(output) = &op.output {
                    defs.insert(
                        VarnodeKey::from(output),
                        DefSite {
                            block_idx,
                            op_idx,
                            op,
                        },
                    );
                }
            }
        }
        let address_to_index = build_address_to_index_map(pcode);
        let block_target_keys = build_block_target_keys(pcode);
        let target_key_to_index = block_target_keys
            .iter()
            .enumerate()
            .map(|(idx, key)| (*key, idx))
            .collect();
        let layout_fallthrough = build_layout_fallthrough_map(pcode);
        let successors = build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        let predecessors = build_predecessor_index_map(&successors);
        let register_param_aliases = entry_analysis::collect_entry_register_param_aliases(pcode);
        let stack_frame_size = entry_analysis::infer_entry_stack_frame_size(pcode, options);
        if preview_builder_diag_enabled() {
            let duplicate_starts = duplicate_block_start_count(pcode);
            if duplicate_starts > 0 {
                eprintln!(
                    "[DIAG] build_hir duplicate_block_starts={} unique_block_starts={}",
                    duplicate_starts,
                    address_to_index.len()
                );
            }
        }
        Self {
            pcode,
            options,
            type_context,
            defs,
            address_to_index,
            block_target_keys,
            target_key_to_index,
            layout_fallthrough,
            successors,
            predecessors,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            temps: BTreeMap::new(),
            temp_next_id: 0,
            materialized_vns: HashMap::new(),
            current_lowering_site: None,
            register_param_aliases,
            stack_frame_size,
            linear_exit_cache: HashMap::new(),
            linear_body_cache: HashMap::new(),
            active_linear_body_keys: HashSet::new(),
            active_conditional_tail_keys: HashSet::new(),
            jump_targets_cache: None,
            active_trace_id: None,
            last_trace_id: None,
            next_trace_id: 1,
            lowering_site_depth: 0,
            forced_linear_structuring_count: 0,
            region_linearize_structuring_count: 0,
            region_linearize_heuristic_exit_count: 0,
            region_linearize_rejected_non_structuring_failure_count: 0,
            region_linearize_rejected_no_exit_count: 0,
            region_linearize_rejected_body_lowering_failed_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
                0,
            region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
                0,
            region_linearize_rejected_body_lowering_successor_inline_rejected_count: 0,
            region_linearize_rejected_body_lowering_revisit_cycle_count: 0,
            region_linearize_rejected_body_lowering_unsupported_terminator_count: 0,
            region_linearize_rejected_non_advancing_count: 0,
            region_linearize_rejected_irreducible_cfg_count: 0,
            structuring_scc_component_count: 0,
            structuring_irreducible_scc_count: 0,
            structuring_irreducible_header_count: 0,
            loop_control_explicit_reducer_count: 0,
            loop_control_rewrite_break_count: 0,
            loop_control_rewrite_continue_count: 0,
            loop_control_rewrite_skipped_nested_scope_count: 0,
            promotion_candidate_count: 0,
            promoted_region_count: 0,
            promotion_rejected_by_shape_count: 0,
            promotion_rejected_by_shape_missing_terminal_join_target_count: 0,
            promotion_rejected_by_shape_empty_nonterminal_tail_count: 0,
            promotion_rejected_by_gate_count: 0,
            discovery_seen_guarded_tail_like_shape_count: 0,
            discovery_rejected_noncanonical_layout_count: 0,
            canonicalized_guarded_tail_shape_count: 0,
            canonicalization_failed_multiple_payload_entries: 0,
            canonicalization_failed_interleaved_join_uses: 0,
            canonicalization_failed_nonterminal_join_label: 0,
            canonicalization_failed_nested_tail_escape: 0,
            canonicalized_interleaved_join_use_count: 0,
            canonicalized_local_nonfallthrough_alias_count: 0,
            canonicalization_failed_alias_not_fallthrough_count: 0,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: 0,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count: 0,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count: 0,
            canonicalization_failed_alias_has_nonlocal_ref_count: 0,
            canonicalization_failed_alias_has_nonlocal_ref_external_before_count: 0,
            canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: 0,
            canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: 0,
            canonicalization_failed_alias_body_not_trivial_count: 0,
            canonicalization_failed_join_has_external_ref_count: 0,
            canonicalization_failed_payload_crosses_join_count: 0,
            rejected_must_emit_label: 0,
            rejected_must_emit_label_surviving_middle_ref: 0,
            rejected_must_emit_label_surviving_external_ref: 0,
            rejected_must_emit_label_owner_conflict: 0,
            rejected_not_single_pred_succ: 0,
            rejected_external_entry: 0,
            rejected_loop_or_switch_target: 0,
        }
    }

    pub(super) fn build_hir(
        &mut self,
        name: &str,
        _address: u64,
    ) -> Result<HirFunction, MlilPreviewError> {
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

        let mut body = Vec::new();
        if self.pcode.blocks.len() == 1 {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir single_block_start: block=0x{:x} ops={}",
                    self.pcode.blocks[0].start_address,
                    self.pcode.blocks[0].ops.len()
                );
            }
            let block = &self.pcode.blocks[0];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(0)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    body.push(HirStmt::Goto(block_label(target)))
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
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir single_block_done: stmts={}", body.len());
            }
        } else {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir multiblock_start: blocks={} ops={}",
                    self.pcode.blocks.len(),
                    self.pcode
                        .blocks
                        .iter()
                        .map(|block| block.ops.len())
                        .sum::<usize>()
                );
            }
            body = self.build_multiblock_body()?;
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir multiblock_done: stmts={}", body.len());
            }
        }

        let return_type = body
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr_type(expr)),
                HirStmt::Return(None) => Some(NirType::Unknown),
                _ => None,
            })
            .unwrap_or(NirType::Unknown);

        Ok(HirFunction {
            name: name.to_string(),
            params: self.params.values().cloned().collect(),
            locals: self
                .locals
                .iter()
                .map(|(offset, slot)| NirBinding {
                    name: slot.name.clone(),
                    ty: slot.ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::StackOffset(*offset)),
                    initializer: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            surface_return_type_name: None,
            body,
        })
    }

    pub(super) fn preview_build_stats(&self) -> PreviewBuildStats {
        PreviewBuildStats {
            forced_linear_structuring_count: self.forced_linear_structuring_count,
            region_linearize_structuring_count: self.region_linearize_structuring_count,
            region_linearize_heuristic_exit_count: self.region_linearize_heuristic_exit_count,
            region_linearize_rejected_non_structuring_failure_count: self
                .region_linearize_rejected_non_structuring_failure_count,
            region_linearize_rejected_no_exit_count: self.region_linearize_rejected_no_exit_count,
            region_linearize_rejected_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count,
            region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
            region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
            region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count,
            region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
            region_linearize_rejected_body_lowering_successor_inline_rejected_count: self
                .region_linearize_rejected_body_lowering_successor_inline_rejected_count,
            region_linearize_rejected_body_lowering_revisit_cycle_count: self
                .region_linearize_rejected_body_lowering_revisit_cycle_count,
            region_linearize_rejected_body_lowering_unsupported_terminator_count: self
                .region_linearize_rejected_body_lowering_unsupported_terminator_count,
            region_linearize_rejected_non_advancing_count: self
                .region_linearize_rejected_non_advancing_count,
            region_linearize_rejected_irreducible_cfg_count: self
                .region_linearize_rejected_irreducible_cfg_count,
            structuring_scc_component_count: self.structuring_scc_component_count,
            structuring_irreducible_scc_count: self.structuring_irreducible_scc_count,
            structuring_irreducible_header_count: self.structuring_irreducible_header_count,
            loop_control_explicit_reducer_count: self.loop_control_explicit_reducer_count,
            loop_control_rewrite_break_count: self.loop_control_rewrite_break_count,
            loop_control_rewrite_continue_count: self.loop_control_rewrite_continue_count,
            loop_control_rewrite_skipped_nested_scope_count: self
                .loop_control_rewrite_skipped_nested_scope_count,
            promotion_candidate_count: self.promotion_candidate_count,
            promoted_region_count: self.promoted_region_count,
            promotion_rejected_by_shape_count: self.promotion_rejected_by_shape_count,
            promotion_rejected_by_shape_missing_terminal_join_target_count: self
                .promotion_rejected_by_shape_missing_terminal_join_target_count,
            promotion_rejected_by_shape_empty_nonterminal_tail_count: self
                .promotion_rejected_by_shape_empty_nonterminal_tail_count,
            promotion_rejected_by_gate_count: self.promotion_rejected_by_gate_count,
            discovery_seen_guarded_tail_like_shape_count: self
                .discovery_seen_guarded_tail_like_shape_count,
            discovery_rejected_noncanonical_layout_count: self
                .discovery_rejected_noncanonical_layout_count,
            canonicalized_guarded_tail_shape_count: self.canonicalized_guarded_tail_shape_count,
            canonicalization_failed_multiple_payload_entries: self
                .canonicalization_failed_multiple_payload_entries,
            canonicalization_failed_interleaved_join_uses: self
                .canonicalization_failed_interleaved_join_uses,
            canonicalization_failed_nonterminal_join_label: self
                .canonicalization_failed_nonterminal_join_label,
            canonicalization_failed_nested_tail_escape: self
                .canonicalization_failed_nested_tail_escape,
            canonicalized_interleaved_join_use_count: self.canonicalized_interleaved_join_use_count,
            canonicalized_local_nonfallthrough_alias_count: self
                .canonicalized_local_nonfallthrough_alias_count,
            canonicalization_failed_alias_not_fallthrough_count: self
                .canonicalization_failed_alias_not_fallthrough_count,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: self
                .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count: self
                .canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count: self
                .canonicalization_failed_alias_has_multiple_internal_predecessors_count,
            canonicalization_failed_alias_has_nonlocal_ref_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_count,
            canonicalization_failed_alias_has_nonlocal_ref_external_before_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
            canonicalization_failed_alias_body_not_trivial_count: self
                .canonicalization_failed_alias_body_not_trivial_count,
            canonicalization_failed_join_has_external_ref_count: self
                .canonicalization_failed_join_has_external_ref_count,
            canonicalization_failed_payload_crosses_join_count: self
                .canonicalization_failed_payload_crosses_join_count,
            rejected_must_emit_label: self.rejected_must_emit_label,
            rejected_must_emit_label_surviving_middle_ref: self
                .rejected_must_emit_label_surviving_middle_ref,
            rejected_must_emit_label_surviving_external_ref: self
                .rejected_must_emit_label_surviving_external_ref,
            rejected_must_emit_label_owner_conflict: self
                .rejected_must_emit_label_owner_conflict,
            rejected_not_single_pred_succ: self.rejected_not_single_pred_succ,
            rejected_external_entry: self.rejected_external_entry,
            rejected_loop_or_switch_target: self.rejected_loop_or_switch_target,
        }
    }

    fn with_lowering_site<T>(&mut self, site: LoweringSite, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.current_lowering_site;
        self.lowering_site_depth += 1;
        self.current_lowering_site = Some(site);
        let result = f(self);
        self.current_lowering_site = prev;
        self.lowering_site_depth = self.lowering_site_depth.saturating_sub(1);
        result
    }

    pub(super) fn next_block_address(&self, idx: usize) -> Option<u64> {
        self.layout_fallthrough[idx].map(|next_idx| self.block_target_key(next_idx))
    }

    pub(super) fn block_target_key(&self, idx: usize) -> u64 {
        self.block_target_keys[idx]
    }

    pub(super) fn ensure_temp_binding_for_output(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
    ) -> NirBinding {
        let key = MaterializedVarnodeKey::new(output, op);
        if let Some(name) = self.materialized_vns.get(&key)
            && let Some(binding) = self.temps.get(name)
        {
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        let name = next_temp_name(&ty, &mut self.temp_next_id);
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }

    fn debug_lowering_error(
        &self,
        stage: &str,
        block_addr: u64,
        seq: u64,
        opcode: PcodeOpcode,
        err: &MlilPreviewError,
    ) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            let message = format!(
                "[mlil-preview] stage={} block=0x{:x} seq=0x{:x} opcode={:?} err={}",
                stage, block_addr, seq, opcode, err
            );
            eprintln!("{message}");
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(format!(
                    "/tmp/fission_preview_{:x}.log",
                    self.function_address()
                ))
                .and_then(|mut f| {
                    std::io::Write::write_all(&mut f, format!("{message}\n").as_bytes())
                });
        }

        if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
            self.record_unsupported_inventory_event(
                stage,
                None,
                None,
                Some(opcode),
                Some(block_addr),
                Some(seq),
                true,
                "builder_root",
            );
        }
    }

    fn function_address(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or_default()
    }

    fn preview_log_path(&self) -> String {
        format!("/tmp/fission_preview_{:x}.log", self.function_address())
    }

    fn unsupported_inventory_path(&self) -> String {
        format!(
            "/tmp/fission_preview_{:x}_unsupported.json",
            self.function_address()
        )
    }

    fn next_trace_id(&mut self) -> u64 {
        let trace_id = self.next_trace_id;
        self.next_trace_id += 1;
        trace_id
    }

    fn inventory_trace_id(&self) -> Option<u64> {
        self.active_trace_id.or(self.last_trace_id)
    }

    fn format_varnode(&self, vn: &Varnode) -> String {
        format!(
            "space={} off=0x{:x} size={} const={} val={}",
            vn.space_id, vn.offset, vn.size, vn.is_constant, vn.constant_val
        )
    }

    fn format_op_snippet(&self, op: &PcodeOp) -> String {
        let output = op
            .output
            .as_ref()
            .map(|vn| self.format_varnode(vn))
            .unwrap_or_else(|| "<none>".to_string());
        let inputs = op
            .inputs
            .iter()
            .map(|vn| self.format_varnode(vn))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "addr=0x{:x} seq=0x{:x} opcode={:?} out={} inputs=[{}] asm={}",
            op.address,
            op.seq_num,
            op.opcode,
            output,
            inputs,
            op.asm_mnemonic.as_deref().unwrap_or("<none>")
        )
    }

    pub(super) fn record_unsupported_inventory_event(
        &self,
        stage: &str,
        vn: Option<&Varnode>,
        op: Option<&PcodeOp>,
        opcode: Option<PcodeOpcode>,
        block_addr: Option<u64>,
        seq: Option<u64>,
        fatal: bool,
        context: &str,
    ) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_none() {
            return;
        }
        let trace_id = self.inventory_trace_id().unwrap_or(0);

        let def_op = vn
            .and_then(|vn| self.lookup_def_site(vn))
            .map(|(_, def)| format!("{:?}", def.opcode));
        let snippet = op
            .map(|op| self.format_op_snippet(op))
            .or_else(|| {
                vn.and_then(|vn| self.lookup_def_site(vn))
                    .map(|(_, def)| self.format_op_snippet(def))
            })
            .unwrap_or_else(|| "<none>".to_string());
        let event = serde_json::json!({
            "trace_id": trace_id,
            "stage": stage,
            "opcode": opcode.map(|op| format!("{op:?}")),
            "address": op.map(|op| op.address).or(block_addr),
            "block_start": block_addr
                .or_else(|| self.current_lowering_site.map(|site| self.pcode.blocks[site.block_idx].start_address)),
            "varnode": vn.map(|vn| self.format_varnode(vn)),
            "def_op": def_op,
            "def_chain_depth": self.lowering_site_depth,
            "snippet": snippet,
            "fatal": fatal,
            "context": context,
            "seq": op.map(|op| u64::from(op.seq_num)).or(seq),
        });

        let path = self.unsupported_inventory_path();
        let mut events = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<serde_json::Value>>(&raw).ok())
            .unwrap_or_default();
        events.push(event);
        let _ = std::fs::write(
            path,
            serde_json::to_vec_pretty(&events).unwrap_or_else(|_| b"[]".to_vec()),
        );
    }
}

fn preview_builder_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}
