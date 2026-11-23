use super::state::{BuilderCacheMap, BuilderCacheSet};
use super::*;
use fission_loader::loader::LoadedBinary;

/// The register spellings a `Call` should count as defining.
///
/// `return_registers` names the integer and floating ABI return slots in
/// `REGISTER_SPACE_ID` and `UNIQUE_SPACE_ID`, but this pipeline's own p-code
/// for x86 emits them in `RUST_SLEIGH_REGISTER_SPACE_ID` -- the same split the
/// register model's own doc comment records for the AArch64 side. Rather than
/// guess which spelling this function's p-code uses, keep only the spelling
/// that actually appears in it.
/// Whether the `Call` at `op_idx` is really this function's *return*.
///
/// ARM's `bx lr` lifts to `Call(target)` immediately followed by
/// `Return(target)`: SLEIGH describes the branch, and only the pair together
/// says it leaves the function. Treating that `Call` as a call would let it
/// claim the result register and shadow the value the function actually
/// returns -- `arm32_bx_lr_returns_primary_r0_not_link_target` and its two
/// siblings caught exactly that.
pub(super) fn op_is_lifted_return(block: &crate::PcodeBasicBlock, op_idx: usize) -> bool {
    block
        .ops
        .get(op_idx + 1)
        .is_some_and(|next| next.opcode == PcodeOpcode::Return)
}

/// The registers a call's result is defined in: the primary integer return
/// register (one spelling), and separately every seen width of the
/// floating-point return register. Floating-point views are a call's result
/// only where that call's result is read through them -- see
/// [`call_result_read_as_float`].
pub(super) fn call_result_definition_varnodes(
    pcode: &PcodeFunction,
    options: &crate::midend::MlilPreviewOptions,
) -> (Vec<Varnode>, Vec<Varnode>) {
    let namer = crate::midend::cspec::RegisterNamer::from_options(options);
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
    let mut out = Vec::new();
    let mut added: HashSet<(u64, u64, u32)> = HashSet::default();

    // Primary integer return slot: keep at most ONE space spelling so ARM32
    // does not claim `REGISTER_SPACE_ID` LR when `RUST_SLEIGH_REGISTER_SPACE_ID` r0 is used.
    'primary: for vn in namer.primary_return_registers() {
        for candidate in [
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                ..vn.clone()
            },
            vn,
        ] {
            let key = (candidate.space_id, candidate.offset, candidate.size);
            if seen.contains(&key) {
                if added.insert(key) {
                    out.push(candidate);
                }
                break 'primary;
            }
        }
    }

    let primary = std::mem::take(&mut out);
    // Float return slot: independently record at most ONE register/space
    // spelling, collecting all seen sub-widths (e.g. f32/f64 views in XMM0) in
    // deterministic size order.
    'float: for vn in namer.float_return_registers() {
        for candidate in [
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                ..vn.clone()
            },
            vn,
        ] {
            let mut matched_sizes: Vec<u32> = seen
                .iter()
                .filter_map(|&(space_id, offset, size)| {
                    (space_id == candidate.space_id
                        && offset == candidate.offset
                        && size > 0
                        && size <= candidate.size)
                        .then_some(size)
                })
                .collect();
            if !matched_sizes.is_empty() {
                matched_sizes.sort_unstable();
                matched_sizes.dedup();
                for size in matched_sizes {
                    let key = (candidate.space_id, candidate.offset, size);
                    if added.insert(key) {
                        out.push(Varnode {
                            size,
                            ..candidate.clone()
                        });
                    }
                }
                break 'float;
            }
        }
    }

    (primary, out)
}

/// Whether the call at `op_idx` returns through the floating-point return
/// register: the first read of a return register after the call, on any path
/// through the call's block and its successors, reads a floating-point view
/// before anything writes it.
///
/// One function returns in both families from different calls. Declaring
/// every call a definition of both made an integer call's result the
/// definition of `s0`/`d0` too, and on AArch64 the function's own `return s0`
/// read the integer call instead (`return x0`).
fn call_result_read_as_float(
    pcode: &PcodeFunction,
    block_idx: usize,
    op_idx: usize,
    float_views: &[Varnode],
    primary: &[Varnode],
) -> bool {
    if float_views.is_empty() {
        return false;
    }
    let overlaps = |vn: &Varnode, set: &[Varnode]| {
        set.iter()
            .any(|reg| reg.space_id == vn.space_id && reg.offset == vn.offset)
    };
    // (block, first op) pairs to scan, bounded: a result is read close to
    // the call that produced it.
    let mut pending = vec![(block_idx, op_idx + 1)];
    let mut visited = HashSet::default();
    while let Some((block, start)) = pending.pop() {
        if !visited.insert((block, start)) || visited.len() > 16 {
            continue;
        }
        let Some(current) = pcode.blocks.get(block) else {
            continue;
        };
        let mut decided = false;
        for op in current.ops.iter().skip(start) {
            if op.inputs.iter().any(|input| overlaps(input, float_views)) {
                return true;
            }
            if op.opcode.is_call()
                || op.inputs.iter().any(|input| overlaps(input, primary))
                || op
                    .output
                    .as_ref()
                    .is_some_and(|out| overlaps(out, float_views))
            {
                decided = true;
                break;
            }
        }
        if !decided {
            pending.extend(current.successors.iter().map(|&succ| (succ as usize, 0)));
        }
    }
    false
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
        let mut use_counts: HashMap<VarnodeKey, usize> = HashMap::default();
        for block in &pcode.blocks {
            for op in &block.ops {
                for input in &op.inputs {
                    if input.is_constant {
                        continue;
                    }
                    *use_counts.entry(VarnodeKey::from(input)).or_insert(0) += 1;
                }
            }
        }
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
        let (primary_result_registers, float_result_registers) =
            call_result_definition_varnodes(pcode, options);
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            let mut block_def_map: HashMap<VarnodeKey, Vec<usize>> = HashMap::default();
            for (op_idx, op) in block.ops.iter().enumerate() {
                let record =
                    |key: VarnodeKey,
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
                    let float_result = call_result_read_as_float(
                        pcode,
                        block_idx,
                        op_idx,
                        &float_result_registers,
                        &primary_result_registers,
                    );
                    let registers = primary_result_registers.iter().chain(
                        float_result
                            .then_some(float_result_registers.iter())
                            .into_iter()
                            .flatten(),
                    );
                    for vn in registers {
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
        let (scalar_ssa, heritage_idom) = super::scalar_ssa::build_scalar_ssa_with_dominators(
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
        let declared_variadic_fixed_arity = type_context
            .and_then(|context| context.function_hints.as_ref())
            .and_then(|hints| hints.variadic_fixed_arity);
        let mut register_param_aliases =
            entry_analysis::collect_entry_register_param_aliases(pcode, &register_namer);
        register_param_aliases.retain(|_, idx| *idx < entry_arity);
        let (
            stack_frame_size,
            entry_frame_pointer_established,
            rbp_frame_bias,
            rsp_prologue_delta_table,
        ) = entry_analysis::infer_entry_stack_layout(pcode, options, type_context);
        let rsp_frame_coordinate_table = entry_analysis::infer_frame_relative_rsp_offsets(
            pcode,
            options,
            &heritage_successors,
            stack_frame_size,
            entry_frame_pointer_established,
            rbp_frame_bias,
        );
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
            use_counts,
            materialized_output_names: HashMap::default(),
            block_defs,
            lookup_site_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            peel_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            candidate_def_keys_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
            terminator_cache: BuilderCacheMap::default(),
            x86_branch_recovery_attempts: 0,
            address_to_index,
            block_target_keys,
            target_key_to_index,
            layout_fallthrough,
            heritage_successors,
            heritage_predecessors,
            heritage_idom,
            scalar_ssa,
            ssa_emission: super::ssa_emission::SsaEmissionPlan::default(),
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
            value_used_varnodes: None,
            lowering_memory_pointer: false,
            locals: BTreeMap::new(),
            locals_next_id: 0,
            stack_slot_memory_owners: BTreeMap::new(),
            temps: BTreeMap::new(),
            used_param_local_names: HashSet::default(),
            temp_next_id: 0,
            materialized_vns: HashMap::default(),
            active_materialized_rhs_keys: BuilderCacheSet::default(),
            runtime_space_numbering: std::cell::Cell::new(None),
            opaque_global_index: std::sync::OnceLock::new(),
            sized_global_index: std::sync::OnceLock::new(),
            load_address_bindings: HashSet::default(),
            load_value_bindings: HashSet::default(),
            explicit_merge_bindings: HashMap::default(),
            call_result_bindings: HashMap::default(),
            call_result_types: HashMap::default(),
            selector_representatives: BuilderCacheMap::default(),
            current_lowering_site: None,
            instruction_loops: HashMap::default(),
            instruction_loops_present: None,
            instruction_loop_exit: None,
            register_param_aliases,
            entry_arity,
            declared_variadic_fixed_arity,
            suppress_entry_register_params: false,
            stack_frame_size,
            stack_frame_backing: None,
            entry_frame_pointer_established,
            rbp_frame_bias,
            rsp_prologue_delta_table,
            rsp_frame_coordinate_table,
            linear_exit_cache: BuilderCacheMap::default(),
            linear_body_cache: BuilderCacheMap::default(),
            active_linear_body_keys: BuilderCacheSet::default(),
            active_conditional_tail_keys: BuilderCacheSet::default(),
            jump_targets_cache: None,
            branchind_jump_table_exprs: HashMap::default(),
            string_literal_addresses: HashMap::default(),
            branchind_bound_probes: HashSet::default(),
            branchind_jump_table_targets: HashMap::default(),
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
            active_loop_heads: Vec::new(),
            varnode_redirect_depth: 0,
            varnode_lowering_work: 0,
            phi_operand_values: None,
            explicit_def_sites: None,
            diamond_select_depth: 0,
        };
        let mut b = b;
        // Before any isolated builder is forked from this one: each fork
        // allocates temps from the shared counter, so a name first handed
        // out inside a fork can be handed out again by its sibling.
        b.prepare_instruction_loops();
        b
    }
}
