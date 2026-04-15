use super::state::{BuilderCacheMap, BuilderCacheSet};
use super::*;
use fission_loader::loader::LoadedBinary;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn new(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        Self::new_with_binary(pcode, options, None, type_context)
    }

    pub(crate) fn new_with_binary(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        binary: Option<&'a LoadedBinary>,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        let mut defs = HashMap::new();
        let mut def_sites: HashMap<VarnodeKey, Vec<DefSite<'a>>> = HashMap::new();
        let mut block_defs = Vec::with_capacity(pcode.blocks.len());
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            let mut block_def_map: HashMap<VarnodeKey, Vec<usize>> = HashMap::new();
            for (op_idx, op) in block.ops.iter().enumerate() {
                if let Some(output) = &op.output {
                    let key = VarnodeKey::from(output);
                    let site = DefSite {
                        block_idx,
                        op_idx,
                        _marker: std::marker::PhantomData,
                    };
                    block_def_map.entry(key.clone()).or_default().push(op_idx);
                    def_sites.entry(key.clone()).or_default().push(site);
                    defs.insert(key, site);
                }
            }
            block_defs.push(block_def_map);
        }
        let address_to_index = build_address_to_index_map(pcode);
        let block_target_keys = build_block_target_keys(pcode);
        let target_key_to_index = block_target_keys
            .iter()
            .enumerate()
            .map(|(idx, key)| (*key, idx))
            .collect();
        let layout_fallthrough = build_layout_fallthrough_map(pcode);
        let mut successors =
            build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        let mut predecessors = build_predecessor_index_map(&successors);

        let mut dom_tree = crate::nir::structuring::DomTree::analyze(&successors, &predecessors);
        let cfg_analysis =
            crate::nir::structuring::CfgAnalysis::analyze(&successors, &predecessors);
        let irreducible_edges = cfg_analysis.irreducible_edges(&dom_tree);

        let loop_bodies = crate::nir::structuring::loop_analysis::LoopBody::identify_loops(
            &successors,
            &predecessors,
            &cfg_analysis,
            &irreducible_edges,
        );

        // Remove irreducible edges from downstream structuring passes.
        for &(src, dst) in &irreducible_edges {
            if let Some(succs) = successors.get_mut(src) {
                succs.retain(|&s| s != dst);
            }
            if let Some(preds) = predecessors.get_mut(dst) {
                preds.retain(|&p| p != src);
            }
        }
        // Downstream structuring uses the pruned CFG. Keep cached CFG facts
        // aligned with this final successor/predecessor topology.
        let cfg_facts = crate::nir::structuring::CfgFactCache::analyze(&successors, &predecessors);
        dom_tree = cfg_facts.dominators().clone();

        let register_param_aliases =
            entry_analysis::collect_entry_register_param_aliases(pcode, options.calling_convention);
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
            binary,
            type_context,
            defs,
            def_sites,
            block_defs,
            lookup_site_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            peel_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            terminator_cache: BuilderCacheMap::default(),
            x86_branch_recovery_attempts: 0,
            address_to_index,
            block_target_keys,
            target_key_to_index,
            layout_fallthrough,
            successors,
            predecessors,
            cfg_facts,
            dom_tree,
            irreducible_edges,
            virtual_block_map: Vec::new(),
            loop_bodies,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            temps: BTreeMap::new(),
            temp_next_id: 0,
            materialized_vns: HashMap::new(),
            call_result_bindings: HashMap::new(),
            selector_representatives: BuilderCacheMap::default(),
            current_lowering_site: None,
            register_param_aliases,
            stack_frame_size,
            linear_exit_cache: BuilderCacheMap::default(),
            linear_body_cache: BuilderCacheMap::default(),
            active_linear_body_keys: BuilderCacheSet::default(),
            active_conditional_tail_keys: BuilderCacheSet::default(),
            jump_targets_cache: None,
            active_trace_id: None,
            last_trace_id: None,
            next_trace_id: 1,
            lowering_site_depth: 0,
            build_duration_ms: 0,
            normalize_duration_ms: 0,
            forced_linear_structuring_count: 0,
            region_linearize_structuring_count: 0,
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
            rule_block_if_no_exit_count: 0,
            rule_block_if_no_exit_accepted_count: 0,
            structuring_irreducible_header_count: 0,
            loop_control_explicit_reducer_count: 0,
            loop_control_rewrite_break_count: 0,
            loop_control_rewrite_continue_count: 0,
            loop_control_rewrite_skipped_nested_scope_count: 0,
            loop_while_subgraph_lowered_count: 0,
            loop_multi_exit_break_count: 0,
            loop_for_lowered_count: 0,
            region_proof_candidate_count: 0,
            region_proof_completed_count: 0,
            region_emit_ready_failed_count: 0,
            conditional_region_candidate_count: 0,
            conditional_region_promoted_count: 0,
            guarded_tail_candidate_count: 0,
            guarded_tail_promoted_count: 0,
            promotion_candidate_count: 0,
            promoted_region_count: 0,
            promotion_rejected_by_shape_count: 0,
            promotion_rejected_by_shape_missing_terminal_join_target_count: 0,
            promotion_rejected_by_shape_empty_nonterminal_tail_count: 0,
            promotion_rejected_by_gate_count: 0,
            discovery_seen_guarded_tail_like_shape_count: 0,
            guarded_tail_rejected_missing_terminal_join_count: 0,
            guarded_tail_rejected_side_entry_conflict_count: 0,
            guarded_tail_rejected_alias_interleave_conflict_count: 0,
            guarded_tail_rejected_ambiguous_follow_count: 0,
            guarded_tail_replacement_plan_candidate_count: 0,
            guarded_tail_replacement_plan_completed_count: 0,
            guarded_tail_replacement_plan_merge_created_count: 0,
            guarded_tail_replacement_plan_rejected_missing_merge_count: 0,
            guarded_tail_replacement_plan_rejected_unstable_read_count: 0,
            guarded_tail_exported_binding_count: 0,
            guarded_tail_replacement_read_count: 0,
            guarded_tail_replacement_read_rewritten_count: 0,
            guarded_tail_replacement_read_rejected_nondominated_count: 0,
            guarded_tail_replacement_read_rejected_nonremovable_op_count: 0,
            discovery_rejected_noncanonical_layout_count: 0,
            canonicalized_guarded_tail_shape_count: 0,
            canonicalization_failed_multiple_payload_entries: 0,
            canonicalization_failed_interleaved_join_uses: 0,
            canonicalization_failed_interleaved_join_uses_no_next_label_count: 0,
            canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: 0,
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
            condition_fold_and_count: 0,
            condition_fold_or_count: 0,
            condition_fold_rejected_side_effect: 0,
            unsupported_indirect_control_count: 0,
            unsupported_indirect_call_count: 0,
            unsupported_external_target_count: 0,
            indirect_surface_preserved_count: 0,
            indirect_target_set_refined_count: 0,
            dispatcher_shape_recovered_count: 0,
            materialization_stabilized_count: 0,
            replacement_plan_candidate_count: 0,
            replacement_plan_completed_count: 0,
            replacement_plan_merge_binding_count: 0,
            replacement_plan_rejected_alias_unsafe_count: 0,
            replacement_plan_rejected_missing_merge_count: 0,
            materialization_inline_suppressed_count: 0,
            representative_downgrade_count: 0,
            representative_downgrade_no_aliassafe_source_count: 0,
            representative_downgrade_join_conflict_count: 0,
            dispatcher_proof_unit_count: 0,
            dispatcher_proof_completed_count: 0,
            dispatcher_proof_failed_count: 0,
            switch_emit_ready_failed_count: 0,
            pe_admission_profile_mismatch_count: 0,
            proof_payload_direct_emit_count: 0,
        }
    }
}
