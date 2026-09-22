use super::*;
use fission_midend_core::ir::{SsaUseSite, SsaValueDefinition};

/// Total `lower_varnode_inner` entries one top-level lowering may spend.
///
/// The two depth caps in this file bound how *deep* a chain may go; neither
/// bounds how *wide* it gets. `visiting` is path-scoped -- a key is removed on
/// the way back out -- so a value reachable by many paths is lowered once per
/// path, and the work is exponential in the number of joins rather than linear
/// in the ops. Measured on `coreutils/fmt`'s `put_line`, four blocks and 162
/// p-code ops in 201 bytes: `lower_varnode_inner` was entered more than 14
/// million times and the function never finished.
///
/// Ordinary functions stay far below this -- it does not fire anywhere in
/// `gzip`, `bzip2` or `coreutils/ls`. A 200,000-entry ceiling still allowed a
/// join-heavy value to expand into a 1.8-million-character expression before
/// normalization, so keep the fallback well below that measured output cliff.
const VARNODE_LOWERING_WORK_BUDGET: u64 = 20_000;

/// Node ceiling for one lowered varnode expression.
///
/// The work budget above bounds how many times `lower_varnode_inner` is
/// *entered*; it does not bound how large the expression that comes back is.
/// Those are different quantities once `visiting` is path-scoped: a
/// loop-carried accumulator resolves through its own phi on every path, and
/// 20,000 units of work still produced a **31,522-node** right-hand side for
/// `lhblFlashWaitComplete` (4 source CFG nodes; every other decompiler scores
/// it perfectly) and a 201,839-character line for `coreutils/sort`'s `merge`.
/// Those two shapes are about a third of Fission's excess structure distance
/// on the full DecBench sweep.
///
/// So bound the output too, and fall back to the same named binding the work
/// budget already falls back to -- a variable reference is what the source had
/// there anyway.
const VARNODE_LOWERING_EXPR_NODE_CAP: usize = 256;

/// Whether `e` has more than `cap` nodes, counted with an early exit so the
/// check costs O(cap) rather than O(size of e).
fn expr_exceeds_node_cap(e: &PreHirExpr, cap: usize) -> bool {
    fn walk(e: &PreHirExpr, left: &mut usize) -> bool {
        if *left == 0 {
            return true;
        }
        *left -= 1;
        match e {
            PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(..) => false,
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. } => walk(expr, left),
            PreHirExpr::Binary { lhs, rhs, .. } => walk(lhs, left) || walk(rhs, left),
            PreHirExpr::Index { base, index, .. } => walk(base, left) || walk(index, left),
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => walk(cond, left) || walk(then_expr, left) || walk(else_expr, left),
            PreHirExpr::Call { args, .. } => args.iter().any(|a| walk(a, left)),
        }
    }
    let mut left = cap;
    walk(e, &mut left)
}

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn stack_pointer_register_name(
        &self,
        vn: &Varnode,
    ) -> Option<String> {
        match vn.space_id {
            UNIQUE_SPACE_ID => {
                crate::arch::x86::unique_x86_register_name(vn.offset, vn.size).map(str::to_string)
            }
            space_id if is_register_space_id(space_id) => {
                if self.options.calling_convention == CallingConvention::X86_32 && vn.size == 4 {
                    match vn.offset {
                        0x10 => return Some("esp".to_string()),
                        0x14 => return Some("ebp".to_string()),
                        _ => {}
                    }
                }
                let namer = self.register_namer();
                namer
                    .register_name_with_param_owned(vn.offset, vn.size)
                    .and_then(|(name, idx)| {
                        if let Some(idx) = idx {
                            (idx < self.entry_arity).then_some(name)
                        } else {
                            Some(name)
                        }
                    })
                    .or_else(|| self.sla_hw_name(vn.offset, vn.size))
                    .or_else(|| Some("reg".to_string()))
            }
            _ => None,
        }
    }

    pub(in crate::midend::builder) fn live_call_result_binding_for_return_register(
        &self,
        vn: &Varnode,
    ) -> Option<String> {
        if !self.register_namer().is_primary_return_register(vn) {
            return None;
        }
        let site = self.current_lowering_site?;
        if let Some(name) = self.live_call_result_binding_in_block_for_return_register(
            vn,
            site.block_idx,
            site.op_idx,
        ) {
            return Some(name);
        }
        let mut memo = HashMap::default();
        let (call_site, name) = self
            .live_call_result_binding_from_predecessors_for_return_register(
                vn,
                site.block_idx,
                &mut memo,
            )?;
        let def_site = self.lookup_def_site(vn).map(|(site, _)| site);
        self.call_result_site_outranks_def_site(call_site, def_site)
            .then_some(name)
    }

    pub(super) fn live_call_result_binding_in_block_for_return_register(
        &self,
        vn: &Varnode,
        block_idx: usize,
        before_op_idx: usize,
    ) -> Option<String> {
        let block = self.pcode.blocks.get(block_idx)?;
        for (prior_idx, op) in block.ops.iter().enumerate().take(before_op_idx).rev() {
            let prior_site = LoweringSite {
                block_idx,
                op_idx: prior_idx,
            };
            if op.output.is_none()
                && matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
                && let Some(name) = self.call_result_bindings.get(&prior_site)
            {
                return Some(name.clone());
            }
            if let Some(output) = op.output.as_ref()
                && self.varnode_aliases_value(output, vn)
            {
                return None;
            }
        }
        None
    }

    /// Recursive predecessor-graph walk for the "is the return register still
    /// holding a live call result" check. `memo` is scoped to a single call
    /// of `live_call_result_binding_for_return_register` (not cached across
    /// calls -- `self.call_result_bindings` grows as lowering proceeds, so a
    /// builder-lifetime cache could observe a stale answer from before a
    /// predecessor block was lowered).
    ///
    /// Within one walk, `memo` both memoizes finished subtree results (so a
    /// diamond-shaped predecessor structure -- common in real CFGs -- isn't
    /// re-explored once per incoming branch) and detects cycles: a block
    /// still `None`-valued while its own recursive call is on the stack
    /// means we looped back via a back-edge, matching the old `visited`
    /// set's early-return-on-revisit behavior. Replaces the previous
    /// per-branch `visited.clone()`, which gave every sibling predecessor
    /// its own independent copy and so re-walked shared ancestors from
    /// scratch -- worst-case exponential in CFGs with repeated diamonds
    /// (e.g. `_nl_load_domain`, ~40% of this function's own decompile time
    /// before this fix).
    fn live_call_result_binding_from_predecessors_for_return_register(
        &self,
        vn: &Varnode,
        block_idx: usize,
        memo: &mut HashMap<usize, Option<(LoweringSite, String)>>,
    ) -> Option<(LoweringSite, String)> {
        if let Some(cached) = memo.get(&block_idx) {
            return cached.clone();
        }
        memo.insert(block_idx, None);

        let result = (|| {
            let predecessors = self.predecessors.get(block_idx)?;
            if predecessors.is_empty() {
                return None;
            }

            let mut shared_binding: Option<(LoweringSite, String)> = None;
            for pred_idx in predecessors {
                let pred_block = self.pcode.blocks.get(*pred_idx)?;
                let candidate = self
                    .live_call_result_site_in_block_for_return_register(
                        vn,
                        *pred_idx,
                        pred_block.ops.len(),
                    )
                    .or_else(|| {
                        self.live_call_result_binding_from_predecessors_for_return_register(
                            vn, *pred_idx, memo,
                        )
                    })?;
                if shared_binding
                    .as_ref()
                    .is_some_and(|(_, name)| name != &candidate.1)
                {
                    return None;
                }
                shared_binding = Some(candidate);
            }
            shared_binding
        })();

        memo.insert(block_idx, result.clone());
        result
    }

    fn live_call_result_site_in_block_for_return_register(
        &self,
        vn: &Varnode,
        block_idx: usize,
        before_op_idx: usize,
    ) -> Option<(LoweringSite, String)> {
        let block = self.pcode.blocks.get(block_idx)?;
        for (prior_idx, op) in block.ops.iter().enumerate().take(before_op_idx).rev() {
            let prior_site = LoweringSite {
                block_idx,
                op_idx: prior_idx,
            };
            if op.output.is_none()
                && matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
                && let Some(name) = self.call_result_bindings.get(&prior_site)
            {
                return Some((prior_site, name.clone()));
            }
            if let Some(output) = op.output.as_ref()
                && self.varnode_aliases_value(output, vn)
            {
                return None;
            }
        }
        None
    }

    fn call_result_site_outranks_def_site(
        &self,
        call_site: LoweringSite,
        def_site: Option<LoweringSite>,
    ) -> bool {
        let Some(def_site) = def_site else {
            return true;
        };
        if def_site.block_idx == call_site.block_idx {
            return def_site.op_idx < call_site.op_idx;
        }
        self.dom_tree
            .dominates(def_site.block_idx, call_site.block_idx)
    }

    pub(super) fn debug_preview_log(&self, message: &str) {
        if !preview_debug_enabled() {
            return;
        }
        eprint!("{message}");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.preview_log_path())
            .and_then(|mut f| std::io::Write::write_all(&mut f, message.as_bytes()));
    }

    pub(in crate::midend) fn lookup_def_site(
        &self,
        vn: &Varnode,
    ) -> Option<(LoweringSite, &'a PcodeOp)> {
        let scope = self.current_lowering_site;
        let key = VarnodeKey::from(vn);
        let cache_key = (scope, key.clone());
        if let Some(cached_site) = self.lookup_site_cache.borrow().get(&cache_key).copied() {
            return cached_site.map(|site| {
                let op = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
                (site, op)
            });
        }

        let candidate_keys = self.lookup_candidate_def_keys(&key);
        let mut resolved_site: Option<LoweringSite> = None;
        if let Some(site) = scope {
            if let Some(defs_in_block) = self.block_defs.get(site.block_idx) {
                // Find the most recent definition among all candidate keys (exact and aliased) block-locally
                let mut best_def_idx: Option<usize> = None;
                for candidate_key in &candidate_keys {
                    if let Some(def_indices) = defs_in_block.get(candidate_key) {
                        let prior_count = def_indices.partition_point(|idx| *idx < site.op_idx);
                        if prior_count > 0 {
                            let mut idx = prior_count - 1;
                            while idx < prior_count {
                                let def_idx = def_indices[idx];
                                let candidate_op = &self.pcode.blocks[site.block_idx].ops[def_idx];
                                if Self::is_identity_copy_def(candidate_op) {
                                    if idx == 0 {
                                        break;
                                    }
                                    idx -= 1;
                                    continue;
                                }
                                if best_def_idx.is_none_or(|best| def_idx > best) {
                                    best_def_idx = Some(def_idx);
                                }
                                break;
                            }
                        }
                    }
                }
                if let Some(def_idx) = best_def_idx {
                    resolved_site = Some(LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: def_idx,
                    });
                }
            }
        }

        if resolved_site.is_none() {
            if let Some(scope_site) = scope {
                // Find the most dominating/recent definition among all candidate keys across blocks
                resolved_site = candidate_keys
                    .iter()
                    .filter_map(|candidate_key| self.def_sites.get(candidate_key))
                    .flat_map(|sites| sites.iter())
                    .filter_map(|site| {
                        let candidate = LoweringSite {
                            block_idx: site.block_idx,
                            op_idx: site.op_idx,
                        };
                        let candidate_op =
                            &self.pcode.blocks[candidate.block_idx].ops[candidate.op_idx];
                        if Self::is_identity_copy_def(candidate_op) {
                            return None;
                        }
                        if candidate.block_idx == scope_site.block_idx {
                            return (candidate.op_idx < scope_site.op_idx).then_some((
                                usize::MAX,
                                candidate.op_idx,
                                candidate,
                            ));
                        }
                        self.dom_tree
                            .dominates(candidate.block_idx, scope_site.block_idx)
                            .then_some((
                                self.dom_tree.dominance_depth(candidate.block_idx),
                                candidate.op_idx,
                                candidate,
                            ))
                    })
                    .max_by_key(|(dom_depth, op_idx, candidate)| {
                        (*dom_depth, candidate.block_idx, *op_idx)
                    })
                    .map(|(_, _, candidate)| candidate);
            } else {
                // Find the most recent definition when scope is None
                resolved_site = candidate_keys
                    .iter()
                    .filter_map(|candidate_key| self.defs.get(candidate_key))
                    .filter_map(|def| {
                        let site = LoweringSite {
                            block_idx: def.block_idx,
                            op_idx: def.op_idx,
                        };
                        let op = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
                        (!Self::is_identity_copy_def(op)).then_some(site)
                    })
                    .max_by_key(|site| (site.block_idx, site.op_idx));
            }
        }

        self.lookup_site_cache
            .borrow_mut()
            .insert(cache_key, resolved_site);

        resolved_site.map(|site| {
            let op = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
            (site, op)
        })
    }

    fn lookup_candidate_def_keys(&self, key: &VarnodeKey) -> Vec<VarnodeKey> {
        let mut candidates = vec![key.clone()];
        if key.is_constant {
            return candidates;
        }
        if is_register_space_id(key.space_id) {
            candidates.extend(
                self.def_sites
                    .keys()
                    .filter(|candidate| *candidate != key)
                    .filter(|candidate| {
                        Self::register_key_covers(candidate, key)
                            || self.register_key_zero_extends(candidate, key)
                            || self.register_key_cross_space_covers(candidate, key)
                            || self.register_key_cross_space_zero_extends(candidate, key)
                    })
                    .cloned(),
            );
        } else if is_unique_space_id(key.space_id) {
            candidates.extend(
                self.def_sites
                    .keys()
                    .filter(|candidate| *candidate != key)
                    .filter(|candidate| Self::unique_key_covers(candidate, key))
                    .cloned(),
            );
        }
        candidates
    }

    pub(in crate::midend::builder) fn has_prior_local_def_for_varnode(
        &self,
        vn: &Varnode,
        site: LoweringSite,
    ) -> bool {
        let key = VarnodeKey::from(vn);
        let candidate_keys = self.lookup_candidate_def_keys(&key);
        let Some(defs_in_block) = self.block_defs.get(site.block_idx) else {
            return false;
        };
        candidate_keys.iter().any(|candidate_key| {
            defs_in_block.get(candidate_key).is_some_and(|def_indices| {
                def_indices.iter().any(|def_idx| {
                    *def_idx < site.op_idx
                        && !Self::is_identity_copy_def(
                            &self.pcode.blocks[site.block_idx].ops[*def_idx],
                        )
                })
            })
        })
    }

    fn is_identity_copy_def(op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::Copy
            && op.output.as_ref().is_some_and(|output| {
                op.inputs
                    .first()
                    .is_some_and(|input| VarnodeKey::from(output) == VarnodeKey::from(input))
            })
    }

    pub(in crate::midend) fn lower_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let created_trace = if self.active_trace_id.is_none() {
            let trace_id = self.next_trace_id();
            self.active_trace_id = Some(trace_id);
            true
        } else {
            false
        };
        let result = self.lower_varnode_inner(vn, visiting);
        if created_trace {
            self.last_trace_id = self.active_trace_id;
            self.active_trace_id = None;
        }
        result
    }

    fn lower_varnode_inner(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if vn.is_constant {
            let address = vn.constant_val as u64;
            if let Some(name) = self.options.global_names.get(&address) {
                return Ok(PreHirExpr::AddressOfGlobal(name.clone()));
            }
            // Short strings the load-time scan is right to skip -- see
            // `read_c_string_from_binary`.
            if let Some(text) = self.read_c_string_from_binary(address) {
                return Ok(PreHirExpr::AddressOfGlobal(format!(
                    "\"{}\"",
                    text.escape_default()
                )));
            }
            return Ok(PreHirExpr::Const(
                vn.constant_val,
                type_from_size(vn.size, false),
            ));
        }

        if vn.space_id == REGISTER_SPACE_ID
            && vn.size >= 16
            && let Some(site) = self.current_lowering_site
        {
            let block = &self.pcode.blocks[site.block_idx];
            if let Some((source, earliest_idx)) =
                aggregate_recovery::recover_wide_register_source_from_block(block, site.op_idx, vn)
            {
                return self.with_lowering_site(
                    LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: earliest_idx,
                    },
                    |this| this.lower_varnode(&source, visiting),
                );
            }
        }

        let key = VarnodeKey::from(vn);
        // A passthrough register definition can be read from a successor before
        // the defining block has been materialized.  Seed the same binding that
        // the later materializer will reuse; otherwise lowering the Copy RHS
        // reuses the source register's ABI name even after that source has been
        // overwritten on the path to this use.
        if let Some(use_site) = self.current_lowering_site
            && let Some((def_site, def_op)) = self.lookup_def_site(vn)
            && def_site.block_idx != use_site.block_idx
            && let Some(output) = def_op.output.as_ref()
            && VarnodeKey::from(output) == key
            && is_register_varnode(output)
        {
            if !self
                .materialized_vns
                .contains_key(&MaterializedVarnodeKey::new(output, def_op))
                && let Some(name) = self.same_block_cmov_entry_register_binding_name_at(
                    def_site.block_idx,
                    def_site.op_idx,
                    output,
                )
            {
                self.ensure_live_register_binding(&name, output.size);
                return Ok(PreHirExpr::Var(name));
            }
            if !self.register_namer().is_primary_return_register(output)
                && !self
                    .materialized_vns
                    .contains_key(&MaterializedVarnodeKey::new(output, def_op))
                && matches!(
                    def_op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                )
                && def_op.inputs.first().is_some_and(is_register_varnode)
                // A width-changing view of the same register storage is an
                // alias derivation, not an independently materialized value.
                // Seeding it here reserves a name before its narrow source is
                // lowered; the later identity/cast cleanup can then discard
                // the assignment and leave cross-block reads bound to an
                // uninitialized temporary. Let the normal alias recovery path
                // derive this value from the narrow definition instead.
                && !def_op.inputs.first().is_some_and(|input| {
                    input.space_id == output.space_id
                        && input.offset == output.offset
                        && input.size != output.size
                })
            {
                let def_op = def_op.clone();
                let binding = self.ensure_temp_binding_for_output(&def_op, output, true);
                return Ok(PreHirExpr::Var(binding.name));
            }
        }
        if let Some(expr) = self.try_lower_scalar_ssa_piece_reassembly(vn, &key, visiting)? {
            return Ok(expr);
        }
        if let Some(site) = self.current_lowering_site {
            if !self.has_prior_local_def_for_varnode(vn, site) {
                if let Some(name) = self
                    .explicit_merge_bindings
                    .get(&(site.block_idx, key.clone()))
                {
                    return Ok(PreHirExpr::Var(name.clone()));
                }
                if let Some(((_, candidate_key), name)) =
                    self.explicit_merge_bindings
                        .iter()
                        .find(|((block_idx, candidate_key), _)| {
                            *block_idx == site.block_idx
                                && (Self::register_key_covers(candidate_key, &key)
                                    || self.register_key_zero_extends(candidate_key, &key)
                                    || self.register_key_cross_space_covers(candidate_key, &key)
                                    || self
                                        .register_key_cross_space_zero_extends(candidate_key, &key))
                        })
                {
                    let expr = PreHirExpr::Var(name.clone());
                    if candidate_key.size == key.size {
                        return Ok(expr);
                    }
                    return Ok(PreHirExpr::Cast {
                        ty: type_from_size(vn.size, false),
                        expr: Box::new(expr),
                    });
                }
            }
        }
        let def_site = self.lookup_def_site(vn);
        // The definition reaching this use was settled as explicit before any
        // lowering began, so it ships as its own statement and this use is a
        // reference to it. Returning here is the point of the whole exercise:
        // rebuilding the definition is what duplicates a value once per path,
        // which is what Ghidra's explicit marking prevents and what Fission's
        // work budgets exist to survive without it.
        if let Some((site, def_op)) = def_site.map(|(site, def)| (site, def.clone()))
            && let Some(output) = def_op.output.clone()
            && self.def_site_is_explicit(site.block_idx, site.op_idx)
        {
            let name = self.explicit_binding_name(site.block_idx, site.op_idx, &output);
            return Ok(PreHirExpr::Var(name));
        }
        if let Some((site, def)) = def_site.map(|(site, def)| (site, def.clone()))
            && let Some(expr) =
                self.lower_covering_passthrough_register_lane(vn, site, &def, visiting)?
        {
            return Ok(expr);
        }
        if let Some(name) = self.live_call_result_binding_for_return_register(vn) {
            return Ok(PreHirExpr::Var(name));
        }
        // Loop body: LOAD/use of a loop-carried register must share the binding
        // that the loop's self-update (e.g. INT_ADD stride) will use — not a
        // frozen preheader snapshot vs a distinct bare hardware name.
        // A loop-carried name is only a fallback for a bare register read.
        // If this use already has a same-block reaching definition, that local
        // write is the semantic value even when a later update of the same
        // physical register is carried around a nested loop. Without this
        // precedence, a byte load into an ABI parameter register can be
        // replaced by the entry parameter before its consumer is lowered.
        let has_prior_local_def = self
            .current_lowering_site
            .is_some_and(|site| self.has_prior_local_def_for_varnode(vn, site));
        if !has_prior_local_def && let Some(name) = self.loop_body_carried_register_read_name(vn) {
            let name = self.ensure_live_register_binding(&name, vn.size);
            return Ok(PreHirExpr::Var(name));
        }
        if let Some(expr) = self.loop_exit_materialized_register_binding(vn) {
            return Ok(expr);
        }
        if let Some(expr) = self.try_lower_zero_extended_partial_register(vn, visiting)? {
            return Ok(expr);
        }
        // The diamond path reconstructs a merge from the CFG shape, and it
        // runs even when a definite reaching definition was already found
        // above. Probed on `TIM_OC4Init`: every input at the sites it fires
        // on resolves to `Operation(SsaOpSite { .. })` in the same block --
        // SSA has an answer and this rebuilds it anyway, once per use, which
        // is where the ternaries come from.
        if !(def_site.is_some() && !std::env::var("FISSION_DIAMOND_ALWAYS").is_ok())
            && let Some(expr) = self.try_lower_diamond_select_for_varnode(vn, visiting)?
        {
            return Ok(expr);
        }
        if def_site.is_none() {
            if is_register_space_id(vn.space_id)
                && self.current_lowering_site.is_some_and(|site| {
                    self.predecessors
                        .get(site.block_idx)
                        .is_some_and(|preds| preds.len() > 1)
                })
                && let Some(name) = self.prior_materialized_same_register_output_name(vn)
                && self
                    .temps
                    .get(&name)
                    .is_some_and(|binding| binding.initializer.is_some())
            {
                return Ok(PreHirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id)
                && self.current_store_value_read_at_join(vn)
                && let Some(name) = self.live_register_name_for_join_register_read(vn)
            {
                self.ensure_live_register_binding(&name, vn.size);
                return Ok(PreHirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id)
                && self.current_join_register_update_reads_live_register(vn)
                && let Some(name) = self.live_register_name_for_join_register_read(vn)
            {
                self.ensure_live_register_binding(&name, vn.size);
                return Ok(PreHirExpr::Var(name));
            }
            if let Some(param) = self.register_param(vn) {
                return Ok(PreHirExpr::Var(param));
            }
            if is_unique_space_id(vn.space_id)
                && let Some(name) = crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
            {
                return Ok(PreHirExpr::Var(name.to_string()));
            }
            if !self.options.is_64bit
                && is_register_space_id(vn.space_id)
                && matches!(
                    self.options.calling_convention,
                    CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                )
                && let Some(name) = self.sla_hw_name(vn.offset, vn.size)
            {
                let name = self.ensure_live_register_binding(&name, vn.size);
                return Ok(PreHirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id) {
                let namer = self.register_namer();
                let name = if (!self.options.is_64bit
                    && matches!(
                        self.options.calling_convention,
                        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                    ))
                    || self.suppress_entry_register_params
                {
                    self.sla_hw_name(vn.offset, vn.size)
                        .unwrap_or_else(|| "reg".to_string())
                } else {
                    namer
                        .register_name_with_param_owned(vn.offset, vn.size)
                        .and_then(|(name, idx)| {
                            if let Some(idx) = idx {
                                (idx < self.entry_arity).then_some(name)
                            } else {
                                Some(name)
                            }
                        })
                        .unwrap_or_else(|| {
                            self.sla_hw_name(vn.offset, vn.size)
                                .unwrap_or_else(|| "reg".to_string())
                        })
                };
                let name = self.ensure_live_register_binding(&name, vn.size);
                return Ok(PreHirExpr::Var(name));
            }
        }
        let stack_reg_name = self.stack_pointer_register_name(vn);
        if let Some(name) = stack_reg_name
            && matches!(name.as_str(), "rsp" | "esp" | "sp")
        {
            return Ok(PreHirExpr::Var(name));
        }
        if let Some((_, op)) = def_site {
            if op.output.is_none()
                && matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
                && self.register_namer().is_primary_return_register(vn)
                && let Some((site, _)) = def_site
                && let Some(name) = self.call_result_bindings.get(&site)
            {
                return Ok(PreHirExpr::Var(name.clone()));
            }
            let materialized_key = MaterializedVarnodeKey::new(vn, op);
            if let Some(name) = self.materialized_vns.get(&materialized_key) {
                return Ok(PreHirExpr::Var(name.clone()));
            }
            // Look-through passthrough: when the best def op is a widening passthrough
            // (ZExt/SExt/Copy/Cast) that reads exactly `vn` (same space/offset/size), the
            // passthrough was only picked over the real narrow-register def because it has a
            // higher op_idx in the same block (x86-64 implicit zero-extension pattern).
            // Scan backwards to find the actual narrow-register definition and return its
            // materialized name, avoiding projection through the wider temp (e.g. "xVar27")
            // instead of the accumulated register name (e.g. "rax").
            if matches!(
                op.opcode,
                PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Copy | PcodeOpcode::Cast
            ) && op.inputs.first().is_some_and(|input| {
                !input.is_constant
                    && input.space_id == vn.space_id
                    && input.offset == vn.offset
                    && input.size == vn.size
            }) {
                if let Some((site, _)) = def_site {
                    if let Some(block) = self.pcode.blocks.get(site.block_idx) {
                        for prior_idx in (0..site.op_idx).rev() {
                            let prior_op = &block.ops[prior_idx];
                            if let Some(prior_output) = prior_op.output.as_ref() {
                                if prior_output.space_id == vn.space_id
                                    && prior_output.offset == vn.offset
                                    && prior_output.size == vn.size
                                {
                                    let narrow_key =
                                        MaterializedVarnodeKey::new(prior_output, prior_op);
                                    if let Some(name) = self.materialized_vns.get(&narrow_key) {
                                        return Ok(PreHirExpr::Var(name.clone()));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if let Some(output) = op.output.as_ref()
                && self.varnode_aliases_value(output, vn)
            {
                let output_materialized_key = MaterializedVarnodeKey::new(output, op);
                if let Some(name) = self.materialized_vns.get(&output_materialized_key) {
                    return Ok(self.project_alias_def_expr(vn, op, PreHirExpr::Var(name.clone())));
                }
            }
        }
        const VARNODE_REDIRECT_DEPTH_CAP: u32 = 64;
        self.varnode_lowering_work += 1;
        let over_budget = self.varnode_lowering_work > VARNODE_LOWERING_WORK_BUDGET;
        if over_budget || !visiting.insert(key.clone()) {
            if !over_budget
                && let Some((site, op)) = def_site
                && Some(site) != self.current_lowering_site
                && self.varnode_redirect_depth < VARNODE_REDIRECT_DEPTH_CAP
            {
                let mut prior_visiting = visiting.clone();
                self.varnode_redirect_depth += 1;
                let result = self
                    .with_lowering_site(site, |this| this.lower_def_op(op, &mut prior_visiting))
                    .map(|expr| self.project_alias_def_expr(vn, op, expr))
                    .map_err(|err| {
                        let classified = self.classify_varnode_lowering_error(op, err);
                        if matches!(classified, MlilPreviewError::UnsupportedPattern("opcode")) {
                            self.record_unsupported_inventory_event(
                                "lower_varnode_prior_def_reentry",
                                Some(vn),
                                Some(op),
                                Some(op.opcode),
                                Some(self.pcode.blocks[site.block_idx].start_address),
                                Some(u64::from(op.seq_num)),
                                false,
                                "varnode_prior_def_reentry_failed",
                            );
                        }
                        classified
                    });
                self.varnode_redirect_depth -= 1;
                return result;
            }
            let cycle_name = if is_unique_space_id(vn.space_id) {
                crate::arch::x86::unique_x86_register_name(vn.offset, vn.size).map_or_else(
                    || {
                        let name = format!("tmp_{:x}", vn.offset);
                        self.ensure_live_register_binding(&name, vn.size)
                    },
                    ToString::to_string,
                )
            } else {
                let name = format!("tmp_{:x}", vn.offset);
                self.ensure_live_register_binding(&name, vn.size)
            };
            return Ok(PreHirExpr::Var(cycle_name));
        }

        let fallback_name = || -> Option<String> {
            if is_unique_space_id(vn.space_id) {
                crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                    .map(ToString::to_string)
            } else {
                None
            }
        };

        let result = match def_site {
            Some((site, op)) => self
                .with_lowering_site(site, |this| this.lower_def_op(op, visiting))
                .map(|expr| self.project_alias_def_expr(vn, op, expr))
                .map_err(|err| {
                    let classified = self.classify_varnode_lowering_error(op, err);
                    if matches!(classified, MlilPreviewError::UnsupportedPattern("opcode")) {
                        self.record_unsupported_inventory_event(
                            "lower_varnode",
                            Some(vn),
                            Some(op),
                            Some(op.opcode),
                            Some(self.pcode.blocks[site.block_idx].start_address),
                            Some(u64::from(op.seq_num)),
                            false,
                            "varnode_def_lowering_failed",
                        );
                    }
                    classified
                }),
            None if self.options.global_names.contains_key(&vn.offset) => Ok(PreHirExpr::Var(
                self.options
                    .global_names
                    .get(&vn.offset)
                    .expect("global name exists after contains_key")
                    .clone(),
            )),
            None if is_unique_space_id(vn.space_id) => {
                let name = format!("tmp_{:x}", vn.offset);
                let name = self.ensure_live_register_binding(&name, vn.size);
                Ok(PreHirExpr::Var(name))
            }
            None if self.options.is_mapped_global(vn.offset) => {
                Ok(PreHirExpr::Var(format!("DAT_{:x}", vn.offset)))
            }
            None => {
                let name = format!("var_{:x}", vn.offset);
                let name = self.ensure_live_register_binding(&name, vn.size);
                Ok(PreHirExpr::Var(name))
            }
        };
        visiting.remove(&key);
        // Bound the *output*, not just the work spent producing it. Anything
        // past the cap becomes the named binding for this storage, which is
        // what the register was called before it was inlined.
        if let Ok(expr) = &result
            && expr_exceeds_node_cap(expr, VARNODE_LOWERING_EXPR_NODE_CAP)
        {
            let name = fallback_name().unwrap_or_else(|| {
                let name = format!("tmp_{:x}", vn.offset);
                self.ensure_live_register_binding(&name, vn.size)
            });
            return Ok(PreHirExpr::Var(name));
        }
        result
    }

    /// Reconstruct a scalar register read whose reaching value is split across
    /// multiple exact partial-register definitions.
    ///
    /// Scalar Heritage has already partitioned overlapping storage and proven
    /// the reaching definition for every disjoint byte range. Consuming that
    /// cover here avoids the lossy exact-key fallback, which can otherwise pick
    /// only the latest overlapping lane for a wider read. Entry and phi values,
    /// incomplete covers, non-register storage, and values wider than the scalar
    /// expression model deliberately stay on the existing lowering paths.
    fn classify_varnode_lowering_error(
        &self,
        op: &PcodeOp,
        err: MlilPreviewError,
    ) -> MlilPreviewError {
        if !matches!(err, MlilPreviewError::LoweringFailed) {
            return err;
        }
        match op.opcode {
            PcodeOpcode::Load => MlilPreviewError::UnsupportedExprMemoryBackedVarnode,
            PcodeOpcode::Indirect => MlilPreviewError::UnsupportedExprIndirectValueSource,
            PcodeOpcode::Piece | PcodeOpcode::SubPiece => {
                MlilPreviewError::UnsupportedExprPieceShape
            }
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => {
                MlilPreviewError::UnsupportedExprPtrArithmetic
            }
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub => MlilPreviewError::UnsupportedExprAddressMaterialization,
            _ => MlilPreviewError::UnsupportedExprVarnodeLowering,
        }
    }
}

#[cfg(test)]
#[path = "lower_expr_tests.rs"]
mod tests;
