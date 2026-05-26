use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PassAggregate {
    pub total_time_ms: f64,
    pub total_invocations: usize,
    pub changed_count: usize,
    pub stmts_reduced: isize,
    pub locals_reduced: isize,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NirBuildStats {
    #[serde(default)]
    pub build_duration_ms: usize,
    #[serde(default)]
    pub normalize_duration_ms: usize,
    #[serde(default)]
    pub structuring_duration_ms: usize,
    #[serde(default)]
    pub render_duration_ms: usize,
    #[serde(default)]
    pub rendered_code_len: usize,
    #[serde(default)]
    pub invalid_pcode_shape_count: usize,
    #[serde(default)]
    pub validated_pcode_op_count: usize,
    #[serde(default)]
    pub raw_pcode_compat_import_count: usize,
    #[serde(default)]
    pub max_structuring_scc_component_size: usize,
    /// Total clean-room Ghidra-concept stages observed in this function build.
    #[serde(default)]
    pub ghidra_action_stage_count: usize,
    /// Fission builder stage corresponding to Ghidra `Funcdata` construction.
    #[serde(default)]
    pub ghidra_action_funcdata_build_count: usize,
    /// Fission value/representative stage corresponding to Ghidra `Heritage`.
    #[serde(default)]
    pub ghidra_action_heritage_value_recovery_count: usize,
    /// Fission normalize stage corresponding to Ghidra's core action pipeline.
    #[serde(default)]
    pub ghidra_action_normalize_count: usize,
    /// Fission type/prototype stage corresponding to Ghidra `FuncProto` work.
    #[serde(default)]
    pub ghidra_action_prototype_types_count: usize,
    /// Fission structuring stage corresponding to Ghidra `FlowBlock`/`BlockGraph`.
    #[serde(default)]
    pub ghidra_action_blockgraph_structuring_count: usize,
    /// Fission final renderer stage corresponding to Ghidra `PrintC`.
    #[serde(default)]
    pub ghidra_action_printc_count: usize,
    /// Completed Rust-native pipeline count after all clean-room stage records.
    #[serde(default)]
    pub ghidra_clean_room_pipeline_complete_count: usize,
    /// Shadow MIR projection count for the Ghidra-style semantic working state.
    #[serde(default)]
    pub mir_enabled_count: usize,
    /// Functions observed by the shadow MIR projection.
    #[serde(default)]
    pub mir_function_count: usize,
    /// Basic block-shaped units observed by the shadow MIR projection.
    #[serde(default)]
    pub mir_block_count: usize,
    /// Value nodes observed by the shadow MIR projection.
    #[serde(default)]
    pub mir_value_count: usize,
    /// Memory regions observed by the shadow MIR projection.
    #[serde(default)]
    pub mir_memory_region_count: usize,
    /// Join proof records emitted by the shadow MIR projection.
    #[serde(default)]
    pub mir_join_proof_count: usize,
    /// Region proof records emitted by the shadow MIR projection.
    #[serde(default)]
    pub mir_region_proof_count: usize,
    /// Time spent projecting current HIR into shadow MIR.
    #[serde(default)]
    pub mir_projection_duration_ms: usize,
    /// Env-gated MIR BlockGraph admission policy was evaluated for this function.
    #[serde(default)]
    pub mir_blockgraph_admission_enabled_count: usize,
    /// MIR BlockGraph admitted an irreducible-budget function into graph collapse.
    #[serde(default)]
    pub mir_blockgraph_irreducible_budget_bypass_count: usize,
    /// MIR BlockGraph intentionally left an extreme-budget function fail-closed.
    #[serde(default)]
    pub mir_blockgraph_extreme_budget_blocked_count: usize,
    #[serde(default)]
    pub procedure_summary_contracted_count: usize,
    #[serde(default)]
    pub procedure_summary_tail_wrapper_count: usize,
    #[serde(default)]
    pub procedure_summary_import_thunk_count: usize,
    #[serde(default)]
    pub forced_linear_structuring_count: usize,
    /// Back-edges virtualized as explicit gotos by the FAS fallback when node-splitting
    /// exceeds budget (irreducible SCCs too large to split).
    #[serde(default)]
    pub fas_virtual_goto_count: usize,
    /// Switch cases where a goto-to-next-case was rewritten as `/* fallthrough */`.
    #[serde(default)]
    pub switch_fallthrough_detected_count: usize,
    #[serde(default)]
    pub structuring_force_linear_explicit_count: usize,
    #[serde(default)]
    pub structuring_force_linear_irreducible_budget_count: usize,
    #[serde(default)]
    pub structuring_force_linear_extreme_budget_count: usize,
    #[serde(default)]
    pub region_linearize_structuring_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_structuring_failure_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_no_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_failed_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_advancing_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_irreducible_cfg_count: usize,
    #[serde(default)]
    pub structuring_scc_component_count: usize,
    #[serde(default)]
    pub structuring_irreducible_scc_count: usize,
    #[serde(default)]
    pub rule_block_if_no_exit_count: usize,
    #[serde(default)]
    pub rule_block_if_no_exit_accepted_count: usize,
    #[serde(default)]
    pub structuring_irreducible_header_count: usize,
    #[serde(default)]
    pub loop_control_explicit_reducer_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_break_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_continue_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_skipped_nested_scope_count: usize,
    /// How many while loops used the subgraph body lowering path (complex body with branches).
    #[serde(default)]
    pub loop_while_subgraph_lowered_count: usize,
    /// How many Break statements were emitted via multi-exit CFG path (mid-body break).
    #[serde(default)]
    pub loop_multi_exit_break_count: usize,
    /// How many for-loop patterns were recognised and emitted as HirStmt::For.
    #[serde(default)]
    pub loop_for_lowered_count: usize,
    /// Region proofs produced by the CFG/region structuring owner.
    #[serde(default)]
    pub region_proof_candidate_count: usize,
    /// Region proofs that completed with legality fully closed.
    #[serde(default)]
    pub region_proof_completed_count: usize,
    /// Region proofs that existed but failed emit-ready gating.
    #[serde(default)]
    pub region_emit_ready_failed_count: usize,
    /// Ghidra BlockGraph-style region proof candidates considered by structuring.
    #[serde(default)]
    pub blockgraph_region_candidate_count: usize,
    /// BlockGraph-style region proofs with complete legality and emit readiness.
    #[serde(default)]
    pub blockgraph_region_complete_count: usize,
    /// BlockGraph-style region proofs rejected due to missing follow/postdom witness.
    #[serde(default)]
    pub blockgraph_region_rejected_missing_follow_count: usize,
    /// BlockGraph-style region proofs rejected because the target label must remain.
    #[serde(default)]
    pub blockgraph_region_rejected_must_emit_label_count: usize,
    /// BlockGraph-style regions rejected because a surviving middle reference keeps the join label alive.
    #[serde(default)]
    pub blockgraph_region_rejected_middle_ref_count: usize,
    /// BlockGraph-style regions rejected because a surviving external reference keeps the join label alive.
    #[serde(default)]
    pub blockgraph_region_rejected_external_ref_count: usize,
    /// BlockGraph-style regions rejected because join-label ownership stays ambiguous.
    #[serde(default)]
    pub blockgraph_region_rejected_join_owner_conflict_count: usize,
    /// BlockGraph-style regions rejected because the join target stays nonterminal.
    #[serde(default)]
    pub blockgraph_region_rejected_nonterminal_join_count: usize,
    /// BlockGraph-style regions rejected because follow ownership stays ambiguous.
    #[serde(default)]
    pub blockgraph_region_rejected_follow_owner_conflict_count: usize,
    /// BlockGraph-style region proofs rejected by emit-ready or alias/side-entry legality.
    #[serde(default)]
    pub blockgraph_region_rejected_emit_ready_count: usize,
    /// BlockGraph-style region proofs rejected due to irreducible SCC evidence.
    #[serde(default)]
    pub blockgraph_region_rejected_irreducible_count: usize,
    /// Conditional region candidates considered by the structuring owner.
    #[serde(default)]
    pub conditional_region_candidate_count: usize,
    /// Conditional regions selected into the structured overlay.
    #[serde(default)]
    pub conditional_region_promoted_count: usize,
    #[serde(default)]
    pub guarded_tail_candidate_count: usize,
    #[serde(default)]
    pub guarded_tail_promoted_count: usize,
    #[serde(default)]
    pub promotion_candidate_count: usize,
    #[serde(default)]
    pub promoted_region_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_gate_count: usize,
    #[serde(default)]
    pub discovery_seen_guarded_tail_like_shape_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_missing_terminal_join_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_side_entry_conflict_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_alias_interleave_conflict_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_ambiguous_follow_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_side_effectful_callee_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_candidate_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_completed_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_merge_created_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_rejected_missing_merge_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_rejected_unstable_read_count: usize,
    #[serde(default)]
    pub guarded_tail_exported_binding_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rewritten_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rejected_nondominated_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rejected_nonremovable_op_count: usize,
    #[serde(default)]
    pub discovery_rejected_noncanonical_layout_count: usize,
    #[serde(default)]
    pub canonicalized_guarded_tail_shape_count: usize,
    #[serde(default)]
    pub canonicalization_failed_multiple_payload_entries: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_no_next_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: usize,
    #[serde(default)]
    pub canonicalization_failed_nonterminal_join_label: usize,
    #[serde(default)]
    pub canonicalization_failed_nested_tail_escape: usize,
    #[serde(default)]
    pub canonicalized_interleaved_join_use_count: usize,
    #[serde(default)]
    pub canonicalized_local_nonfallthrough_alias_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_body_not_trivial_count: usize,
    #[serde(default)]
    pub canonicalization_failed_join_has_external_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_payload_crosses_join_count: usize,
    #[serde(default)]
    pub rejected_must_emit_label: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_middle_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_external_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_owner_conflict: usize,
    #[serde(default)]
    pub rejected_not_single_pred_succ: usize,
    #[serde(default)]
    pub rejected_external_entry: usize,
    #[serde(default)]
    pub rejected_loop_or_switch_target: usize,
    #[serde(default)]
    pub condition_fold_and_count: usize,
    #[serde(default)]
    pub condition_fold_or_count: usize,
    #[serde(default)]
    pub condition_fold_rejected_side_effect: usize,
    /// Spill temps renamed to `param_k` in the entry linear prefix (HIR normalize).
    #[serde(default)]
    pub entry_param_promotion_spill_rename_count: usize,
    /// Conservative folds / annotations for variadic tail stack-arg regions (ABI lattice).
    #[serde(default)]
    pub variadic_stack_region_fold_count: usize,
    /// ABI-classified stack / carrier slots recovered from entry/call usage.
    #[serde(default)]
    pub abi_slot_recovered_count: usize,
    /// Home / shadow slots promoted out of generic `local_*` naming.
    #[serde(default)]
    pub home_slot_promoted_count: usize,
    /// Semantic `va_start` statements recovered from ABI carrier setup.
    #[serde(default)]
    pub va_start_recovered_count: usize,
    /// Call targets/signatures tightened beyond anonymous `sub_/FUN_` defaults.
    #[serde(default)]
    pub call_signature_refined_count: usize,
    /// Extra call arguments removed only because an exact API prototype arity was available.
    #[serde(default)]
    pub call_prototype_exact_api_arity_pruned_count: usize,
    /// Calls whose arguments were kept because the target has no known identity.
    #[serde(default)]
    pub call_prototype_unknown_target_kept_count: usize,
    /// Calls resolved through a wrapper summary before prototype lookup / target rewrite.
    #[serde(default)]
    pub call_prototype_wrapper_resolved_count: usize,
    /// Calls with a target identity but no exact API/provider prototype.
    #[serde(default)]
    pub call_prototype_signature_missing_count: usize,
    /// Direct call targets resolved to loader-proven import/API identities.
    #[serde(default)]
    pub call_target_import_resolved_count: usize,
    /// Direct call targets resolved to non-import loader/fact symbols.
    #[serde(default)]
    pub call_target_direct_symbol_resolved_count: usize,
    /// Direct call targets that fell back to synthetic `sub_...` naming.
    #[serde(default)]
    pub call_target_unresolved_sub_fallback_count: usize,
    /// Direct call targets encountered without a type context.
    #[serde(default)]
    pub call_target_context_missing_count: usize,
    /// Call targets resolved by the exact loader/fact call-target index.
    #[serde(default)]
    pub call_target_exact_index_hit_count: usize,
    /// Call targets excluded because exact identities tied at the same provenance rank.
    #[serde(default)]
    pub call_target_exact_index_ambiguous_count: usize,
    /// Export thunk targets resolved through exact loader thunk metadata.
    #[serde(default)]
    pub call_target_export_thunk_target_resolved_count: usize,
    /// Indirect calls resolved only after a COPY-only constant chain proof.
    #[serde(default)]
    pub call_target_indirect_const_resolved_count: usize,
    /// IAT/import slots resolved to exact loader-proven import identities.
    #[serde(default)]
    pub call_target_iat_slot_resolved_count: usize,
    /// Indirect calls resolved through a pointer-sized load from an exact IAT slot.
    #[serde(default)]
    pub call_target_indirect_load_resolved_count: usize,
    /// Indirect call load pointers folded to an exact constant through def-use proof.
    #[serde(default)]
    pub call_target_indirect_ptr_const_folded_count: usize,
    /// Indirect load targets rejected because the load address is not an exact IAT slot.
    #[serde(default)]
    pub call_target_indirect_rejected_non_iat_load_count: usize,
    /// Indirect load targets rejected because the pointer expression is not constant.
    #[serde(default)]
    pub call_target_indirect_rejected_non_const_ptr_count: usize,
    /// Indirect load pointers rejected because a producer opcode is not allowed in exact proof.
    #[serde(default)]
    pub call_target_indirect_rejected_unsupported_ptr_opcode_count: usize,
    /// Indirect load pointers rejected because multiple reaching definitions were available.
    #[serde(default)]
    pub call_target_indirect_rejected_ambiguous_def_count: usize,
    /// Indirect load pointers rejected because no definition dominated the call site.
    #[serde(default)]
    pub call_target_indirect_rejected_non_dominating_def_count: usize,
    /// Indirect load pointers rejected because no definition was available.
    #[serde(default)]
    pub call_target_indirect_rejected_no_def_count: usize,
    /// Indirect load targets rejected because the loaded width is not pointer-sized.
    #[serde(default)]
    pub call_target_indirect_rejected_width_mismatch_count: usize,
    /// Call targets left unresolved because no exact identity was available.
    #[serde(default)]
    pub call_target_unresolved_no_exact_identity_count: usize,
    /// Security-cookie setup/check pairs folded into semantic form.
    #[serde(default)]
    pub security_cookie_fold_count: usize,
    /// Non-semantic call/return scaffolding stores removed after proof.
    #[serde(default)]
    pub call_artifact_removed_count: usize,
    /// Typed object shapes recovered from partitioned memory regions.
    #[serde(default)]
    pub object_shape_recovered_count: usize,
    /// Root object regions recovered before field/array surfacing.
    #[serde(default)]
    pub object_root_recovered_count: usize,
    /// Typed fact evidences collected before canonical object/surface promotion.
    #[serde(default)]
    pub typed_fact_evidence_count: usize,
    /// Conflicting typed facts withheld from canonical promotion.
    #[serde(default)]
    pub typed_fact_conflict_count: usize,
    /// Root-object facts promoted into canonical object inventory.
    #[serde(default)]
    pub object_root_fact_promotion_count: usize,
    /// Existing object shapes refined with field/array/opaque range evidence.
    #[serde(default)]
    pub typed_object_shape_refined_count: usize,
    /// New surface bindings/slot aliases promoted from raw partition evidence.
    #[serde(default)]
    pub surface_binding_promoted_count: usize,
    /// New canonical surface facts promoted before naming/polish.
    #[serde(default)]
    pub surface_fact_promotion_count: usize,
    /// Call prototype summaries refined beyond raw call-site arity bounds.
    #[serde(default)]
    pub prototype_summary_refined_count: usize,
    /// Bounded propagation rounds for canonical prototype summaries.
    #[serde(default)]
    pub prototype_summary_round_count: usize,
    /// Call/effect summaries refined beyond plain arity/name recovery.
    #[serde(default)]
    pub call_effect_summary_refined_count: usize,
    /// Forwarding/wrapper summaries folded into canonical call-target metadata.
    #[serde(default)]
    pub wrapper_summary_fold_count: usize,
    /// Cleanup families skipped due to explicit budget/readiness guards.
    #[serde(default)]
    pub cleanup_budget_skip_count: usize,
    /// Binding-initializer cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_binding_init_count: usize,
    /// Statement canonical cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_stmt_canonical_count: usize,
    /// Statement-fold canonical cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_stmt_fold_count: usize,
    /// Boundary-label cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_boundary_label_count: usize,
    /// Loopish rewrite cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_loopish_rewrite_count: usize,
    /// Dead-binding cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_dead_binding_count: usize,
    /// Rounds of interprocedural signature constraint propagation (call-site arity meet/join).
    #[serde(default)]
    pub interproc_signature_constraint_rounds: usize,
    /// Unsupported indirect-control terminators observed before explicit surfacing.
    #[serde(default)]
    pub unsupported_indirect_control_count: usize,
    /// Unsupported indirect-call targets observed before explicit surfacing.
    #[serde(default)]
    pub unsupported_indirect_call_count: usize,
    /// Unsupported external branch/call targets that resolved outside the canonical CFG slice.
    #[serde(default)]
    pub unsupported_external_target_count: usize,
    /// Unsupported indirect/control sites preserved as explicit pseudo-surface instead of marker calls.
    #[serde(default)]
    pub indirect_surface_preserved_count: usize,
    /// Indirect target sets refined by bounded target proof.
    #[serde(default)]
    pub indirect_target_set_refined_count: usize,
    /// Dispatcher-like control shapes recovered from structural proof.
    #[serde(default)]
    pub dispatcher_shape_recovered_count: usize,
    /// Nontrivial repeated pure expressions stabilized into explicit temporaries.
    #[serde(default)]
    pub materialization_stabilized_count: usize,
    /// Materialization/rewrite candidates considered by the replacement planner.
    #[serde(default)]
    pub replacement_plan_candidate_count: usize,
    /// Candidates whose replacement plan completed without needing a preserved representative.
    #[serde(default)]
    pub replacement_plan_completed_count: usize,
    /// Synthetic merge bindings introduced by the replacement planner.
    #[serde(default)]
    pub replacement_plan_merge_binding_count: usize,
    /// Replacement plans rejected because no alias-safe rewrite existed.
    #[serde(default)]
    pub replacement_plan_rejected_alias_unsafe_count: usize,
    /// Replacement plans rejected because a merge/read bridge was required but unavailable.
    #[serde(default)]
    pub replacement_plan_rejected_missing_merge_count: usize,
    /// Replacement plans rejected because the nonlocal owner was root/entry representative attribution.
    #[serde(default)]
    pub replacement_plan_rejected_representative_root_attribution_count: usize,
    /// Replacement plans rejected because the nonlocal owner was temp-only representative lifecycle.
    #[serde(default)]
    pub replacement_plan_rejected_temp_only_representative_lifecycle_count: usize,
    /// Replacement plans rejected because the nonlocal owner was a dead temp representative.
    #[serde(default)]
    pub replacement_plan_rejected_dead_temp_representative_count: usize,
    /// Legacy inline candidates intentionally kept materialized by the replacement planner.
    #[serde(default)]
    pub materialization_inline_suppressed_count: usize,
    /// Representative selections that fell back to a weaker form instead of a preserved alias/temp.
    #[serde(default)]
    pub representative_downgrade_count: usize,
    /// Representative downgrades caused by the absence of an alias-safe source expression.
    #[serde(default)]
    pub representative_downgrade_no_aliassafe_source_count: usize,
    /// Representative downgrades caused by join-shape conflicts.
    #[serde(default)]
    pub representative_downgrade_join_conflict_count: usize,
    /// Preserved representatives that cleanup/prune intentionally refused to erase.
    #[serde(default)]
    pub preserved_temp_prune_blocked_count: usize,
    /// Preserved representatives intentionally excluded from copy propagation.
    #[serde(default)]
    pub preserved_temp_copyprop_skip_count: usize,
    /// Join-hoisted shared representatives emitted as preserved temps.
    #[serde(default)]
    pub gvn_join_preserved_count: usize,
    /// Switch/dispatcher surfaces emitted directly from proof payloads.
    #[serde(default)]
    pub proof_payload_direct_emit_count: usize,
    /// Heavy reruns skipped because the preserving pass made no structural change.
    #[serde(default)]
    pub pass_rerun_skipped_by_preservation_count: usize,
    /// Dispatcher proof units discovered before emission gating.
    #[serde(default)]
    pub dispatcher_proof_unit_count: usize,
    /// Dispatcher proof units that completed with a legal target map.
    #[serde(default)]
    pub dispatcher_proof_completed_count: usize,
    /// Dispatcher proof units rejected after proof analysis.
    #[serde(default)]
    pub dispatcher_proof_failed_count: usize,
    /// Switch-like regions that had proof/candidate evidence but failed emit-ready gating.
    #[serde(default)]
    pub switch_emit_ready_failed_count: usize,
    /// Compare-chain dispatcher proofs discovered from conditional ladders.
    #[serde(default)]
    pub compare_chain_dispatcher_count: usize,
    /// Candidate-scoped jump resolver admissions that reached the solver.
    #[serde(default)]
    pub candidate_scoped_jump_resolver_count: usize,
    /// SCCP passes skipped because admission analysis found no useful control-flow seeds.
    #[serde(default)]
    pub sccp_skipped_by_admission_count: usize,
    /// Wide dead-assignment reruns admitted after a successful first pass.
    #[serde(default)]
    pub wide_dead_assignment_rerun_admitted_count: usize,
    /// Wide dead-assignment reruns skipped by explicit admission guards.
    #[serde(default)]
    pub wide_dead_assignment_rerun_skipped_by_admission_count: usize,
    /// Preview/render gating disagreed with the canonical target profile.
    #[serde(default)]
    pub pe_admission_profile_mismatch_count: usize,
    /// Memory normalization passes skipped because typed-fact prefilter found no object roots.
    #[serde(default)]
    pub memory_fact_prefilter_skip_count: usize,
    /// Aggregate-field recovery skipped by explicit admission guards.
    #[serde(default)]
    pub aggregate_fields_skipped_by_admission_count: usize,
    /// Memory slot surfacing exited through the cheap path because no alias-safe roots were present.
    #[serde(default)]
    pub memory_slot_cheap_exit_count: usize,
    /// Canonical family totals derived from structuring failures/recovery in pcode.
    #[serde(default)]
    pub structuring_reason_region_legality_count: usize,
    #[serde(default)]
    pub structuring_reason_follow_failure_count: usize,
    #[serde(default)]
    pub structuring_reason_irreducible_count: usize,
    #[serde(default)]
    pub structuring_reason_loop_exit_count: usize,
    #[serde(default)]
    pub structuring_reason_switch_shape_count: usize,
    #[serde(default)]
    pub structuring_reason_budget_count: usize,
    /// SESE structuring succeeded but produced orphan goto labels (Goto without matching Label),
    /// indicating a back-edge label was omitted by the emitter. Fell back to linear structuring.
    #[serde(default)]
    pub structuring_sese_orphan_goto_fallback_count: usize,
    #[serde(default)]
    pub pass_metrics: std::collections::BTreeMap<String, PassAggregate>,
}
