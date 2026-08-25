use super::state::{BuilderCacheMap, BuilderCacheSet};
use super::*;
use fission_loader::loader::LoadedBinary;

/// The register spellings a `Call` should count as defining.
///
/// `primary_return_registers` names the x86 return slot in `REGISTER_SPACE_ID`
/// and `UNIQUE_SPACE_ID`, but this pipeline's own p-code for x86 emits it in
/// `RUST_SLEIGH_REGISTER_SPACE_ID` -- the same split the register model's own
/// doc comment records for the AArch64 side. Rather than guess which one this
/// function's p-code uses, keep only the spellings that actually appear in it.
/// Whether the `Call` at `op_idx` is really this function's *return*.
///
/// ARM's `bx lr` lifts to `Call(target)` immediately followed by
/// `Return(target)`: SLEIGH describes the branch, and only the pair together
/// says it leaves the function. Treating that `Call` as a call would let it
/// claim the result register and shadow the value the function actually
/// returns -- `arm32_bx_lr_returns_primary_r0_not_link_target` and its two
/// siblings caught exactly that.
fn op_is_lifted_return(block: &crate::PcodeBasicBlock, op_idx: usize) -> bool {
    block
        .ops
        .get(op_idx + 1)
        .is_some_and(|next| next.opcode == PcodeOpcode::Return)
}

fn call_result_definition_varnodes(
    pcode: &PcodeFunction,
    options: &crate::midend::MlilPreviewOptions,
) -> Vec<Varnode> {
    let namer = crate::midend::cspec::RegisterNamer::from_options(options);
    let mut candidates = Vec::new();
    for vn in namer.primary_return_registers() {
        candidates.push(Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            ..vn.clone()
        });
        candidates.push(vn);
    }
    let mut seen: HashSet<(u64, u64, u32)> = HashSet::default();
    for block in &pcode.blocks {
        for op in &block.ops {
            for vn in op.output.iter().chain(op.inputs.iter()) {
                if !vn.is_constant {
                    seen.insert((vn.space_id, vn.offset, vn.size));
                }
            }
        }
    }
    // Exactly one spelling. Registering every candidate that happens to
    // appear was measured to break ARM32 return recovery: at that
    // architecture's return offset `REGISTER_SPACE_ID` names the link
    // register rather than r0, so claiming it as the call's result made a
    // read of the link register resolve to the call.
    candidates
        .into_iter()
        .find(|vn| seen.contains(&(vn.space_id, vn.offset, vn.size)))
        .into_iter()
        .collect()
}

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
        let mut defs = HashMap::default();
        let mut def_sites: HashMap<VarnodeKey, Vec<DefSite<'a>>> = HashMap::default();
        let mut block_defs = Vec::with_capacity(pcode.blocks.len());
        // SLEIGH models a call as a transfer, not a definition: `Call` carries
        // the target and declares no output. Which register holds the result
        // is a fact of the calling convention, and until it is stated here a
        // read of that register after the call resolves to the last definition
        // *before* it.
        //
        // At `-O0` that is wrong in one specific, very common way. Arguments
        // are staged through the return register (`lea RAX, [..]; mov RSI,
        // RAX`), so the definition standing when the call returns is an
        // argument. Measured before this: `stat("/run/initctl", &s) < 0`
        // lowered to `if (__buf < 0)` and `unlink(p) < 0` to `if (__name <
        // 0)`, while an argument-less `getpid() < 0` was right -- there being
        // no earlier definition to find.
        let call_result_registers = call_result_definition_varnodes(pcode, options);
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            let mut block_def_map: HashMap<VarnodeKey, Vec<usize>> = HashMap::default();
            for (op_idx, op) in block.ops.iter().enumerate() {
                let record = |key: VarnodeKey,
                                  block_def_map: &mut HashMap<VarnodeKey, Vec<usize>>,
                                  def_sites: &mut HashMap<VarnodeKey, Vec<DefSite<'a>>>,
                                  defs: &mut HashMap<VarnodeKey, DefSite<'a>>| {
                    let site = DefSite {
                        block_idx,
                        op_idx,
                        _marker: std::marker::PhantomData,
                    };
                    block_def_map.entry(key.clone()).or_default().push(op_idx);
                    def_sites.entry(key.clone()).or_default().push(site);
                    defs.insert(key, site);
                };
                if let Some(output) = &op.output {
                    record(
                        VarnodeKey::from(output),
                        &mut block_def_map,
                        &mut def_sites,
                        &mut defs,
                    );
                } else if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                    && !op_is_lifted_return(block, op_idx)
                {
                    for vn in &call_result_registers {
                        record(
                            VarnodeKey::from(vn),
                            &mut block_def_map,
                            &mut def_sites,
                            &mut defs,
                        );
                    }
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
        prune_proven_noreturn_successors(pcode, &mut successors, options, type_context);
        for (from, to) in lsda_extra_edges(pcode, &address_to_index, binary) {
            if let Some(succs) = successors.get_mut(from) {
                if !succs.contains(&to) {
                    succs.push(to);
                    succs.sort_unstable();
                }
            }
        }
        let mut predecessors = build_predecessor_index_map(&successors);
        let heritage_successors = successors.clone();
        let heritage_predecessors = predecessors.clone();
        let scalar_ssa = super::scalar_ssa::build_scalar_ssa_with_context(
            pcode,
            &heritage_successors,
            &heritage_predecessors,
            options,
            type_context,
        );

        let mut dom_tree = crate::midend::structuring::DomTree::analyze(&successors, &predecessors);
        let cfg_analysis =
            crate::midend::structuring::CfgAnalysis::analyze(&successors, &predecessors);
        let irreducible_edges = cfg_analysis.irreducible_edges(&dom_tree);

        let loop_bodies = crate::midend::structuring::loop_analysis::LoopBody::identify_loops(
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
        let cfg_facts =
            crate::midend::structuring::CfgFactCache::analyze(&successors, &predecessors);
        dom_tree = cfg_facts.dominators().clone();

        let register_namer = RegisterNamer::from_options(options);
        let entry_arity = infer_entry_register_param_arity(pcode, &register_namer).unwrap_or(0);
        let mut register_param_aliases =
            entry_analysis::collect_entry_register_param_aliases(pcode, &register_namer);
        register_param_aliases.retain(|_, idx| *idx < entry_arity);
        let (
            stack_frame_size,
            entry_frame_pointer_established,
            rbp_frame_bias,
            rsp_prologue_delta_table,
        ) = entry_analysis::infer_entry_stack_layout(pcode, options, type_context);
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
        let b = Self {
            pcode,
            options,
            binary,
            type_context,
            current_function_name: None,
            operand_metatypes: HashMap::default(),
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
            heritage_successors,
            heritage_predecessors,
            scalar_ssa,
            successors,
            predecessors,
            reachability_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            cmov_body_spans: std::cell::RefCell::new(BuilderCacheMap::default()),
            gpr_family_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            cfg_facts,
            dom_tree,
            irreducible_edges,
            virtual_block_map: Vec::new(),
            loop_bodies,
            extra_absorbed_members: Vec::new(),
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            stack_slot_memory_owners: BTreeMap::new(),
            temps: BTreeMap::new(),
            used_param_local_names: HashSet::default(),
            temp_next_id: 0,
            materialized_vns: HashMap::default(),
            active_materialized_rhs_keys: BuilderCacheSet::default(),
            load_address_bindings: HashSet::default(),
            load_value_bindings: HashSet::default(),
            explicit_merge_bindings: HashMap::default(),
            call_result_bindings: HashMap::default(),
            selector_representatives: BuilderCacheMap::default(),
            current_lowering_site: None,
            register_param_aliases,
            entry_arity,
            suppress_entry_register_params: false,
            stack_frame_size,
            entry_frame_pointer_established,
            rbp_frame_bias,
            rsp_prologue_delta_table,
            linear_exit_cache: BuilderCacheMap::default(),
            linear_body_cache: BuilderCacheMap::default(),
            active_linear_body_keys: BuilderCacheSet::default(),
            active_conditional_tail_keys: BuilderCacheSet::default(),
            jump_targets_cache: None,
            active_trace_id: None,
            last_trace_id: None,
            next_trace_id: 1,
            lowering_site_depth: 0,
            materialize_owner_repartition: std::cell::RefCell::new(
                super::materialize::MaterializeOwnerRepartition::default(),
            ),
            current_stack_home_ptr: None,
            active_switch_targets: HashSet::default(),
            telemetry: super::telemetry::BuilderTelemetry::default(),
            structuring_start: None,
            fas_virtual_edges: Vec::new(),
            lowered_block_stmts_cache: Default::default(),
            partial_gpr_live_binding_cache: Default::default(),
            follow_blocks: Vec::new(),
            failed_loop_subgraphs: HashSet::default(),
            lower_varnode_cache: Default::default(),
            structured_body: None,
            register_namer_cache: std::cell::OnceCell::new(),
            sese_region_proof_calls: std::cell::Cell::new(0),
            loop_carried_proof_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            structuring_total_work_units: std::rc::Rc::new(std::cell::Cell::new(0)),
            varnode_redirect_depth: 0,
            diamond_select_depth: 0,
        };
        b
    }
}
