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
            current_function_name: None,
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
            reachability_cache: std::cell::RefCell::new(BuilderCacheMap::default()),
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
            explicit_merge_bindings: HashMap::new(),
            call_result_bindings: HashMap::new(),
            selector_representatives: BuilderCacheMap::default(),
            current_lowering_site: None,
            register_param_aliases,
            suppress_entry_register_params: false,
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
            materialize_owner_repartition: std::cell::RefCell::new(
                super::materialize::MaterializeOwnerRepartition::default(),
            ),
            telemetry: super::telemetry::BuilderTelemetry::default(),
        }
    }
}
