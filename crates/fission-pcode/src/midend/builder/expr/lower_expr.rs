use super::*;

const CALL_TARGET_CONST_FOLD_BUDGET: usize = 16;
const CALL_TARGET_DESCRIPTOR_RECOVERY_BUDGET: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallTargetConstReject {
    UnsupportedOpcode,
    AmbiguousDef,
    NonDominatingDef,
    NoDef,
}

fn callother_index(input: &Varnode) -> Option<u64> {
    if !input.is_constant {
        return None;
    }
    if input.offset != 0 {
        Some(input.offset)
    } else {
        u64::try_from(input.constant_val).ok()
    }
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
        let mut visited = HashSet::default();
        let (call_site, name) = self
            .live_call_result_binding_from_predecessors_for_return_register(
                vn,
                site.block_idx,
                &mut visited,
            )?;
        let def_site = self.lookup_def_site(vn).map(|(site, _)| site);
        self.call_result_site_outranks_def_site(call_site, def_site)
            .then_some(name)
    }

    fn live_call_result_binding_in_block_for_return_register(
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

    fn live_call_result_binding_from_predecessors_for_return_register(
        &self,
        vn: &Varnode,
        block_idx: usize,
        visited: &mut HashSet<usize>,
    ) -> Option<(LoweringSite, String)> {
        if !visited.insert(block_idx) {
            return None;
        }
        let predecessors = self.predecessors.get(block_idx)?;
        if predecessors.is_empty() {
            return None;
        }

        let mut shared_binding: Option<(LoweringSite, String)> = None;
        for pred_idx in predecessors {
            let pred_block = self.pcode.blocks.get(*pred_idx)?;
            let mut pred_visited = visited.clone();
            let candidate = self
                .live_call_result_site_in_block_for_return_register(
                    vn,
                    *pred_idx,
                    pred_block.ops.len(),
                )
                .or_else(|| {
                    self.live_call_result_binding_from_predecessors_for_return_register(
                        vn,
                        *pred_idx,
                        &mut pred_visited,
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

    fn debug_preview_log(&self, message: &str) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_none() {
            return;
        }
        eprint!("{message}");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.preview_log_path())
            .and_then(|mut f| std::io::Write::write_all(&mut f, message.as_bytes()));
    }

    fn resolve_call_target_by_address(&mut self, addr: u64) -> Option<String> {
        let Some(ctx) = self.type_context else {
            self.telemetry
                .call_targets
                .call_target_context_missing_count += 1;
            return None;
        };
        if let Some(target_ref) = ctx.call_target_refs.get(&addr) {
            self.telemetry
                .call_targets
                .call_target_exact_index_hit_count += 1;
            match target_ref.provenance {
                CallTargetProvenance::Import => {
                    self.telemetry
                        .call_targets
                        .call_target_import_resolved_count += 1;
                }
                CallTargetProvenance::ExportThunkTarget => {
                    self.telemetry
                        .call_targets
                        .call_target_direct_symbol_resolved_count += 1;
                    self.telemetry
                        .call_targets
                        .call_target_export_thunk_target_resolved_count += 1;
                }
                _ => {
                    self.telemetry
                        .call_targets
                        .call_target_direct_symbol_resolved_count += 1;
                }
            }
            return Some(target_ref.symbol.clone());
        }
        // Fallback: the address may be an IAT slot VA reached via a constant
        // (e.g. Windows x64 `COPY rcx <- const(0x1400082b8)` followed by
        // `CALLIND rcx` where 0x1400082b8 is the IAT slot for CreateFileA).
        if let Some(target_ref) = ctx.iat_target_refs.get(&addr) {
            self.telemetry
                .call_targets
                .call_target_iat_slot_resolved_count += 1;
            self.telemetry
                .call_targets
                .call_target_import_resolved_count += 1;
            return Some(target_ref.symbol.clone());
        }
        if ctx.ambiguous_call_targets.contains(&addr) {
            self.telemetry
                .call_targets
                .call_target_exact_index_ambiguous_count += 1;
        } else {
            self.telemetry
                .call_targets
                .call_target_unresolved_no_exact_identity_count += 1;
        }
        None
    }

    fn resolve_call_target_by_iat_slot(&mut self, addr: u64) -> Option<String> {
        let Some(ctx) = self.type_context else {
            self.telemetry
                .call_targets
                .call_target_context_missing_count += 1;
            return None;
        };
        let Some(target_ref) = ctx.iat_target_refs.get(&addr) else {
            self.telemetry
                .call_targets
                .call_target_indirect_rejected_non_iat_load_count += 1;
            return None;
        };
        self.telemetry
            .call_targets
            .call_target_iat_slot_resolved_count += 1;
        self.telemetry
            .call_targets
            .call_target_indirect_load_resolved_count += 1;
        self.telemetry
            .call_targets
            .call_target_import_resolved_count += 1;
        Some(target_ref.symbol.clone())
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

    fn has_prior_local_def_for_varnode(&self, vn: &Varnode, site: LoweringSite) -> bool {
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

    pub(in crate::midend::builder) fn register_key_covers(
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        if candidate.is_constant
            || requested.is_constant
            || candidate.space_id != requested.space_id
            || !is_register_space_id(candidate.space_id)
            || candidate.size < requested.size
        {
            return false;
        }
        let candidate_start = candidate.offset;
        let requested_start = requested.offset;
        let Some(candidate_end) = candidate_start.checked_add(u64::from(candidate.size)) else {
            return false;
        };
        let Some(requested_end) = requested_start.checked_add(u64::from(requested.size)) else {
            return false;
        };
        candidate_start <= requested_start && candidate_end >= requested_end
    }

    pub(in crate::midend::builder) fn register_key_zero_extends(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id == requested.space_id
            && is_register_space_id(candidate.space_id)
            && candidate.offset == requested.offset
            && candidate.size == 4
            && requested.size == 8
            && self
                .register_namer()
                .hw_name_at(candidate.offset, candidate.size)
                .is_some()
    }

    pub(in crate::midend::builder) fn register_key_cross_space_covers(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id != requested.space_id
            && candidate.size >= requested.size
            && self.gpr_family_index_for_key(candidate) == self.gpr_family_index_for_key(requested)
            && self.gpr_family_index_for_key(candidate).is_some()
    }

    pub(in crate::midend::builder) fn register_key_cross_space_zero_extends(
        &self,
        candidate: &VarnodeKey,
        requested: &VarnodeKey,
    ) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id != requested.space_id
            && candidate.size == 4
            && requested.size == 8
            && self.gpr_family_index_for_key(candidate) == self.gpr_family_index_for_key(requested)
            && self.gpr_family_index_for_key(candidate).is_some()
    }

    pub(in crate::midend::builder) fn gpr_family_index_for_key(
        &self,
        key: &VarnodeKey,
    ) -> Option<usize> {
        if key.is_constant {
            return None;
        }
        if is_register_space_id(key.space_id) {
            return self
                .register_namer()
                .gpr_family_index_at(key.offset, key.size);
        }
        if is_unique_space_id(key.space_id) {
            let name = crate::arch::x86::unique_x86_register_name(key.offset, key.size)?;
            return crate::arch::x86::x86_gpr_family_index(name);
        }
        None
    }

    fn varnode_covers(candidate: &Varnode, requested: &Varnode) -> bool {
        let candidate_key = VarnodeKey::from(candidate);
        let requested_key = VarnodeKey::from(requested);
        Self::register_key_covers(&candidate_key, &requested_key)
            || Self::unique_key_covers(&candidate_key, &requested_key)
    }

    fn unique_key_covers(candidate: &VarnodeKey, requested: &VarnodeKey) -> bool {
        if candidate.is_constant
            || requested.is_constant
            || !is_unique_space_id(candidate.space_id)
            || !is_unique_space_id(requested.space_id)
            || candidate.space_id != requested.space_id
            || candidate.size < requested.size
        {
            return false;
        }
        let candidate_start = candidate.offset;
        let requested_start = requested.offset;
        let Some(candidate_end) = candidate_start.checked_add(u64::from(candidate.size)) else {
            return false;
        };
        let Some(requested_end) = requested_start.checked_add(u64::from(requested.size)) else {
            return false;
        };
        candidate_start <= requested_start && candidate_end >= requested_end
    }

    pub(in crate::midend::builder) fn varnode_aliases_value(
        &self,
        candidate: &Varnode,
        requested: &Varnode,
    ) -> bool {
        let candidate_key = VarnodeKey::from(candidate);
        let requested_key = VarnodeKey::from(requested);
        Self::varnode_covers(candidate, requested)
            || self.register_key_zero_extends(&candidate_key, &requested_key)
            || self.register_key_cross_space_covers(&candidate_key, &requested_key)
            || self.register_key_cross_space_zero_extends(&candidate_key, &requested_key)
    }

    fn project_alias_def_expr(&self, requested: &Varnode, op: &PcodeOp, expr: HirExpr) -> HirExpr {
        let Some(output) = op.output.as_ref() else {
            return expr;
        };
        if VarnodeKey::from(output) == VarnodeKey::from(requested) {
            return expr;
        }
        if self.register_key_zero_extends(&VarnodeKey::from(output), &VarnodeKey::from(requested)) {
            return HirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if self.register_key_cross_space_zero_extends(
            &VarnodeKey::from(output),
            &VarnodeKey::from(requested),
        ) || self.register_key_cross_space_covers(
            &VarnodeKey::from(output),
            &VarnodeKey::from(requested),
        ) {
            return HirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if self.aarch64_gpr_low_view_alias(output, requested) {
            return HirExpr::Cast {
                ty: type_from_size(requested.size, false),
                expr: Box::new(expr),
            };
        }
        if !Self::varnode_covers(output, requested) {
            return expr;
        }
        let byte_offset = self.projection_shift_bytes(output, requested);
        let shifted = if byte_offset == 0 {
            expr
        } else {
            HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: Box::new(expr),
                rhs: Box::new(HirExpr::Const(
                    (byte_offset * 8) as i64,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: type_from_size(output.size, false),
            }
        };
        HirExpr::Cast {
            ty: type_from_size(requested.size, false),
            expr: Box::new(shifted),
        }
    }

    fn projection_shift_bytes(&self, output: &Varnode, requested: &Varnode) -> u64 {
        if self.options.is_big_endian {
            let output_end = output.offset.saturating_add(u64::from(output.size));
            let requested_end = requested.offset.saturating_add(u64::from(requested.size));
            return output_end.saturating_sub(requested_end);
        }
        requested.offset.saturating_sub(output.offset)
    }

    fn aarch64_gpr_low_view_alias(&self, output: &Varnode, requested: &Varnode) -> bool {
        self.options.calling_convention == CallingConvention::AArch64
            && !output.is_constant
            && !requested.is_constant
            && output.space_id == requested.space_id
            && is_register_space_id(output.space_id)
            && output.size == 8
            && requested.size == 4
            && self
                .register_namer()
                .gpr_family_index_at(output.offset, output.size)
                .is_some_and(|output_family| {
                    self.register_namer()
                        .gpr_family_index_at(requested.offset, requested.size)
                        == Some(output_family)
                })
    }

    fn find_loop_carried_variable_for_register(
        &self,
        vn: &Varnode,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
    ) -> Option<String> {
        let output_key = VarnodeKey::from(vn);
        let mut candidates = std::collections::BTreeSet::new();
        for (mkey, name) in &self.materialized_vns {
            if !Self::varnode_key_may_alias_output(&mkey.varnode, &output_key)
                || mkey.varnode.size != output_key.size
                || name.starts_with("param_")
            {
                continue;
            }
            if let Some(sites) = self.def_sites.get(&mkey.varnode) {
                for site in sites {
                    if loop_body.body.contains(&site.block_idx) {
                        let op = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
                        if op.address == mkey.def_addr && op.seq_num == mkey.def_seq {
                            candidates.insert(name.clone());
                        }
                    }
                }
            }
        }
        if candidates.len() == 1 {
            candidates.into_iter().next()
        } else {
            None
        }
    }

    fn loop_exit_materialized_register_binding(&mut self, vn: &Varnode) -> Option<HirExpr> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size < 4 {
            return None;
        }
        let site = self.current_lowering_site?;
        if self.has_prior_local_def_for_varnode(vn, site) {
            return None;
        }
        let predecessor_idxs = self.predecessors.get(site.block_idx)?.clone();

        // Single predecessor path: check if predecessor is inside a loop, and we are exiting it.
        if predecessor_idxs.len() == 1 {
            let pred_idx = predecessor_idxs[0];
            if pred_idx != site.block_idx {
                for loop_body in &self.loop_bodies {
                    if loop_body.body.contains(&pred_idx)
                        && !loop_body.body.contains(&site.block_idx)
                    {
                        if let Some(name) =
                            self.find_loop_carried_variable_for_register(vn, loop_body)
                        {
                            return Some(HirExpr::Var(name));
                        }
                    }
                }
            }
        }

        if predecessor_idxs.len() < 2 || predecessor_idxs.contains(&site.block_idx) {
            return None;
        }

        let mut materialized_name = None;
        let mut materialized_expr = None;
        let mut zero_incoming = false;
        for pred_idx in predecessor_idxs {
            let pred_block = self.pcode.blocks.get(pred_idx)?;
            if self.predecessor_edge_forces_register_zero(pred_idx, site.block_idx, vn) {
                zero_incoming = true;
                continue;
            }
            let term_idx = self
                .block_terminator_index(pred_block)
                .unwrap_or(pred_block.ops.len());
            let Some((_, pred_op)) =
                self.last_register_redefinition_before(pred_block, term_idx, vn)
            else {
                let mut visiting = HashSet::default();
                if self.predecessor_path_has_zero_register_seed(
                    pred_idx,
                    site.block_idx,
                    vn,
                    0,
                    &mut visiting,
                ) {
                    zero_incoming = true;
                    continue;
                }
                return None;
            };
            if self.register_redefinition_is_zero(pred_block, term_idx, pred_op) {
                zero_incoming = true;
                continue;
            }
            if let Some(name) = pred_op.output.as_ref().and_then(|output| {
                self.materialized_vns
                    .get(&MaterializedVarnodeKey::new(output, pred_op))
                    .filter(|_| self.varnode_aliases_value(output, vn))
                    .cloned()
            }) {
                match &materialized_name {
                    Some(existing) if existing != &name => return None,
                    None => {
                        materialized_expr = Some(self.project_alias_def_expr(
                            vn,
                            pred_op,
                            HirExpr::Var(name.clone()),
                        ));
                        materialized_name = Some(name);
                    }
                    _ => {}
                }
                continue;
            }
            return None;
        }

        let name = materialized_name?;
        if !zero_incoming {
            return None;
        }
        if let Some(binding) = self.temps.get_mut(&name)
            && binding.initializer.is_none()
        {
            binding.initializer = Some(HirExpr::Const(0, type_from_size(vn.size, false)));
        }
        materialized_expr
    }

    fn predecessor_path_has_zero_register_seed(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        vn: &Varnode,
        depth: usize,
        visiting: &mut HashSet<usize>,
    ) -> bool {
        if depth > 8 || pred_idx == succ_idx || !visiting.insert(pred_idx) {
            return false;
        }
        let result = self.pcode.blocks.get(pred_idx).is_some_and(|block| {
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            if let Some((def_idx, def_op)) =
                self.last_register_redefinition_before(block, term_idx, vn)
            {
                return self.register_redefinition_is_zero(block, term_idx, def_op)
                    && !Self::block_has_aliasing_side_effect_range(block, def_idx + 1, term_idx);
            }
            if Self::block_has_aliasing_side_effect_range(block, 0, term_idx) {
                return false;
            }
            let incoming = self
                .predecessors
                .get(pred_idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|incoming_idx| *incoming_idx != succ_idx)
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming.into_iter().all(|incoming_idx| {
                    self.predecessor_path_has_zero_register_seed(
                        incoming_idx,
                        pred_idx,
                        vn,
                        depth + 1,
                        visiting,
                    )
                })
        });
        visiting.remove(&pred_idx);
        result
    }

    pub(in crate::midend::builder) fn block_has_aliasing_side_effect_range(
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Load
                    | PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
            )
        })
    }

    pub(in crate::midend::builder) fn predecessor_edge_forces_register_zero(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        vn: &Varnode,
    ) -> bool {
        if vn.is_constant || !is_register_space_id(vn.space_id) {
            return false;
        }
        let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
            return false;
        };
        let Some(term_idx) = self.block_terminator_index(pred_block) else {
            return false;
        };
        let Some(term) = pred_block.ops.get(term_idx) else {
            return false;
        };
        if term.opcode != PcodeOpcode::CBranch || term.inputs.len() < 2 {
            return false;
        }
        let Some(edge_is_taken) = self.cbranch_successor_is_taken_edge(pred_block, succ_idx) else {
            return false;
        };
        let predicate = &term.inputs[1];
        self.predicate_edge_forces_register_zero(pred_block, term_idx, predicate, vn, edge_is_taken)
    }

    fn cbranch_successor_is_taken_edge(
        &self,
        pred_block: &crate::pcode::PcodeBasicBlock,
        succ_idx: usize,
    ) -> Option<bool> {
        let succ_idx = u32::try_from(succ_idx).ok()?;
        let first = pred_block.successors.first().copied()?;
        if first == succ_idx {
            return Some(true);
        }
        if pred_block.successors.get(1).copied() == Some(succ_idx) {
            return Some(false);
        }
        None
    }

    fn predicate_edge_forces_register_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        predicate: &Varnode,
        vn: &Varnode,
        predicate_value: bool,
    ) -> bool {
        if predicate.is_constant {
            return false;
        }
        let key = VarnodeKey::from(predicate);
        let Some(pred_op) = block.ops.iter().take(before_idx).rev().find(|op| {
            op.output
                .as_ref()
                .is_some_and(|output| VarnodeKey::from(output) == key)
        }) else {
            return false;
        };
        match pred_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual if pred_op.inputs.len() == 2 => {
                let forces_equal = match pred_op.opcode {
                    PcodeOpcode::IntEqual => predicate_value,
                    PcodeOpcode::IntNotEqual => !predicate_value,
                    _ => false,
                };
                forces_equal && self.compare_predicate_tests_register_against_zero(pred_op, vn)
            }
            _ => false,
        }
    }

    fn compare_predicate_tests_register_against_zero(&self, op: &PcodeOp, vn: &Varnode) -> bool {
        let [left, right] = op.inputs.as_slice() else {
            return false;
        };
        (self.varnode_aliases_value(left, vn) && self.varnode_is_const_zero(right))
            || (self.varnode_aliases_value(right, vn) && self.varnode_is_const_zero(left))
    }

    fn varnode_is_const_zero(&self, vn: &Varnode) -> bool {
        vn.is_constant && vn.constant_val == 0
    }

    pub(in crate::midend::builder) fn last_register_redefinition_before<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        vn: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        let requested = VarnodeKey::from(vn);
        block
            .ops
            .iter()
            .enumerate()
            .take(before_idx)
            .rev()
            .find(|(_, op)| {
                op.output.as_ref().is_some_and(|output| {
                    let candidate = VarnodeKey::from(output);
                    !candidate.is_constant
                        && candidate.space_id == requested.space_id
                        && is_register_space_id(candidate.space_id)
                        && Self::register_key_ranges_overlap_for_lookup(&candidate, &requested)
                })
            })
    }

    fn register_key_ranges_overlap_for_lookup(lhs: &VarnodeKey, rhs: &VarnodeKey) -> bool {
        let Some(lhs_end) = lhs.offset.checked_add(u64::from(lhs.size)) else {
            return false;
        };
        let Some(rhs_end) = rhs.offset.checked_add(u64::from(rhs.size)) else {
            return false;
        };
        lhs.offset < rhs_end && rhs.offset < lhs_end
    }

    fn register_redefinition_is_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        op: &PcodeOp,
    ) -> bool {
        match op.opcode {
            PcodeOpcode::Copy => op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0),
            PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast => op
                .inputs
                .first()
                .is_some_and(|input| self.varnode_is_same_block_zero(block, before_idx, input)),
            _ => false,
        }
    }

    fn varnode_is_same_block_zero(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        vn: &Varnode,
    ) -> bool {
        if vn.is_constant && vn.constant_val == 0 {
            return true;
        }
        let key = VarnodeKey::from(vn);
        block
            .ops
            .iter()
            .take(before_idx)
            .rev()
            .find(|op| {
                op.output
                    .as_ref()
                    .is_some_and(|output| VarnodeKey::from(output) == key)
            })
            .is_some_and(|op| {
                op.opcode == PcodeOpcode::Copy
                    && op
                        .inputs
                        .first()
                        .is_some_and(|input| input.is_constant && input.constant_val == 0)
            })
    }

    pub(in crate::midend) fn lower_call(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<HirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let created_trace = if self.active_trace_id.is_none() {
            let trace_id = self.next_trace_id();
            self.active_trace_id = Some(trace_id);
            true
        } else {
            false
        };
        let result = self.lower_call_inner(op, recovered_args, visiting);
        if created_trace {
            self.last_trace_id = self.active_trace_id;
            self.active_trace_id = None;
        }
        result
    }

    fn lower_call_inner(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<HirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if matches!(op.opcode, PcodeOpcode::CallOther) {
            return self.lower_callother(op, recovered_args, visiting);
        }
        let target = if let Some(target) = op.inputs.first() {
            if let Some(name) = self.resolve_relocation_call_target_name(op) {
                name
            } else if let Some(name) = self.resolve_constant_call_target_name(op, target) {
                name
            } else {
                match self.lower_varnode(target, visiting) {
                    Ok(HirExpr::Const(val, _)) => {
                        let addr = val as u64;
                        if let Some(name) = self.resolve_call_target_by_address(addr) {
                            if matches!(op.opcode, PcodeOpcode::CallInd) {
                                self.telemetry
                                    .call_targets
                                    .call_target_indirect_const_resolved_count += 1;
                            }
                            name
                        } else {
                            self.telemetry
                                .call_targets
                                .call_target_unresolved_sub_fallback_count += 1;
                            format!("sub_{addr:x}")
                        }
                    }
                    Ok(HirExpr::Var(name)) if matches!(op.opcode, PcodeOpcode::CallInd) => {
                        if let Some(addr) = self.resolve_copy_only_constant_chain(target) {
                            if let Some(name) = self.resolve_call_target_by_address(addr) {
                                self.telemetry
                                    .call_targets
                                    .call_target_indirect_const_resolved_count += 1;
                                name
                            } else {
                                self.telemetry
                                    .call_targets
                                    .call_target_unresolved_sub_fallback_count += 1;
                                format!("sub_{addr:x}")
                            }
                        } else if let Some(name) =
                            self.resolve_indirect_scalar_call_target_name(target)
                        {
                            name
                        } else if let Some(name) = self.resolve_iat_load_call_target(target) {
                            name
                        } else if let Some(name) =
                            self.recover_powerpc64_descriptor_call_target(target)
                        {
                            name
                        } else {
                            // Register/stack function pointer (e.g. `call r8`).
                            // Do not treat the temp/reg name as a C function
                            // symbol — printer special-cases the opaque target
                            // as `(*(fp))(args)`.
                            let _ = name;
                            "__fission_callind_opaque".to_string()
                        }
                    }
                    Ok(HirExpr::Var(name)) => self
                        .resolve_address_like_call_target_name(&name)
                        .unwrap_or(name),
                    Ok(other) if matches!(op.opcode, PcodeOpcode::CallInd) => {
                        if let Some(addr) = self.resolve_copy_only_constant_chain(target) {
                            if let Some(name) = self.resolve_call_target_by_address(addr) {
                                self.telemetry
                                    .call_targets
                                    .call_target_indirect_const_resolved_count += 1;
                                name
                            } else {
                                self.telemetry
                                    .call_targets
                                    .call_target_unresolved_sub_fallback_count += 1;
                                format!("sub_{addr:x}")
                            }
                        } else if let Some(name) =
                            self.resolve_indirect_scalar_call_target_name(target)
                        {
                            name
                        } else if matches!(other, HirExpr::Load { .. }) {
                            if let Some(name) = self.resolve_iat_load_call_target(target) {
                                name
                            } else if let Some(name) =
                                self.recover_powerpc64_descriptor_call_target(target)
                            {
                                name
                            } else {
                                // Memory-indirect function pointer without IAT
                                // resolution: keep opaque so printer uses
                                // `(*(load))(args)`.
                                let _ = other;
                                "__fission_callind_opaque".to_string()
                            }
                        } else {
                            // Non-var expression target (cast/ptr): opaque call.
                            let _ = other;
                            "__fission_callind_opaque".to_string()
                        }
                    }
                    Ok(other) => print_expr(&other),
                    Err(MlilPreviewError::UnsupportedPattern("opcode"))
                        if matches!(op.opcode, PcodeOpcode::CallInd) =>
                    {
                        if let Some(target) = self.recover_opaque_callind_target(target) {
                            target
                        } else {
                            let target_expr = self.lower_varnode(target, visiting).ok();
                            self.record_unsupported_inventory_event(
                                "call_target_unsupported",
                                Some(target),
                                Some(op),
                                Some(op.opcode),
                                self.current_lowering_site
                                    .map(|site| self.pcode.blocks[site.block_idx].start_address),
                                Some(u64::from(op.seq_num)),
                                true,
                                "callind_target_recovery_failed",
                            );
                            self.debug_preview_log(&format!(
                                "[mlil-preview] stage=call_target_unsupported asm={} target_space={} target_off=0x{:x} target_size={}\n",
                                op.asm_mnemonic.as_deref().unwrap_or("<none>"),
                                target.space_id,
                                target.offset,
                                target.size
                            ));
                            let _evidence = self.build_unsupported_control_evidence(
                                op.opcode,
                                self.current_lowering_site
                                    .map(|site| self.pcode.blocks[site.block_idx].start_address),
                                target_expr.as_ref(),
                                Vec::new(),
                                UnsupportedControlFamily::CallRegion,
                                IndirectControlSurface::CallInd,
                                24,
                            );
                            "__fission_callind_opaque".to_string()
                        }
                    }
                    Err(err) => {
                        if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
                            self.record_unsupported_inventory_event(
                                "call_target_lowering_error",
                                Some(target),
                                Some(op),
                                Some(op.opcode),
                                self.current_lowering_site
                                    .map(|site| self.pcode.blocks[site.block_idx].start_address),
                                Some(u64::from(op.seq_num)),
                                false,
                                "call_target_lowering_error",
                            );
                            self.debug_preview_log(&format!(
                                "[mlil-preview] stage=call_target_lowering_error opcode={:?} asm={} target_space={} target_off=0x{:x} target_size={}\n",
                                op.opcode,
                                op.asm_mnemonic.as_deref().unwrap_or("<none>"),
                                target.space_id,
                                target.offset,
                                target.size
                            ));
                        }
                        return Err(err);
                    }
                }
            }
        } else {
            "callee".to_string()
        };
        let mut args = if let Some(recovered_args) = recovered_args {
            recovered_args
        } else {
            op.inputs
                .iter()
                .skip(1)
                .map(|input| self.lower_varnode(input, visiting))
                .collect::<Result<Vec<_>, _>>()?
        };
        if target == "__fission_callind_opaque" {
            if let Some(target_vn) = op.inputs.first() {
                if let Ok(target_expr) = self.lower_varnode(target_vn, visiting) {
                    // Drop recovered ABI args that are the call-target carrier
                    // itself (e.g. win64 `call r8` also wrote r8 as param slot 2).
                    args.retain(|arg| !Self::call_arg_is_callind_target_carrier(arg, &target_expr));
                    args.insert(0, target_expr);
                }
            }
        }
        Ok(HirExpr::Call {
            target,
            args,
            ty: op
                .output
                .as_ref()
                .map(|out| type_from_size(out.size, false))
                .unwrap_or(NirType::Unknown),
        })
    }

    /// True when a recovered call argument expression is the same surface as
    /// the CallInd target (function pointer used as both target and "arg").
    fn call_arg_is_callind_target_carrier(arg: &HirExpr, target: &HirExpr) -> bool {
        match (arg, target) {
            (HirExpr::Var(a), HirExpr::Var(b)) => a == b,
            (HirExpr::Cast { expr: a_inner, .. }, _) => {
                Self::call_arg_is_callind_target_carrier(a_inner, target)
            }
            (_, HirExpr::Cast { expr: t_inner, .. }) => {
                Self::call_arg_is_callind_target_carrier(arg, t_inner)
            }
            _ => false,
        }
    }

    fn lower_callother(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<HirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let target = op
            .inputs
            .first()
            .and_then(callother_index)
            .map(|index| {
                self.options
                    .userops
                    .get(&(index as u32))
                    .cloned()
                    .unwrap_or_else(|| format!("__pcodeop_{index}"))
            })
            .unwrap_or_else(|| "__pcodeop_unknown".to_string());
        let args = if let Some(recovered_args) = recovered_args {
            recovered_args
        } else {
            op.inputs
                .iter()
                .skip(1)
                .map(|input| self.lower_varnode(input, visiting))
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(HirExpr::Call {
            target,
            args,
            ty: op
                .output
                .as_ref()
                .map(|out| type_from_size(out.size, false))
                .unwrap_or(NirType::Unknown),
        })
    }

    fn resolve_relocation_call_target_name(&mut self, op: &PcodeOp) -> Option<String> {
        if !matches!(op.opcode, PcodeOpcode::Call) {
            return None;
        }
        self.options.relocation_names.get(&op.address).cloned()
    }

    fn resolve_constant_call_target_name(
        &mut self,
        op: &PcodeOp,
        target: &Varnode,
    ) -> Option<String> {
        if !target.is_constant {
            return None;
        }
        let addr = if target.offset != 0 {
            target.offset
        } else if target.constant_val >= 0 {
            target.constant_val as u64
        } else {
            return None;
        };
        if let Some(name) = self.resolve_call_target_by_address(addr) {
            if matches!(op.opcode, PcodeOpcode::CallInd) {
                self.telemetry
                    .call_targets
                    .call_target_indirect_const_resolved_count += 1;
            }
            Some(name)
        } else {
            self.telemetry
                .call_targets
                .call_target_unresolved_sub_fallback_count += 1;
            Some(format!("sub_{addr:x}"))
        }
    }

    pub(in crate::midend::builder) fn resolve_address_like_call_target_name(
        &mut self,
        target: &str,
    ) -> Option<String> {
        let raw = target
            .strip_prefix("tmp_")
            .or_else(|| target.strip_prefix("DAT_"))?;
        let addr = u64::from_str_radix(raw.trim_start_matches("0x"), 16).ok()?;
        if let Some(name) = self.resolve_call_target_by_address(addr) {
            Some(name)
        } else {
            self.telemetry
                .call_targets
                .call_target_unresolved_sub_fallback_count += 1;
            Some(format!("sub_{addr:x}"))
        }
    }

    fn resolve_indirect_scalar_call_target_name(&mut self, target: &Varnode) -> Option<String> {
        let scope = self.current_lowering_site?;
        let addr = self
            .resolve_exact_scalar_const_for_call_target(
                target,
                scope,
                CALL_TARGET_CONST_FOLD_BUDGET,
            )
            .ok()?;
        if let Some(name) = self.resolve_call_target_by_address(addr) {
            self.telemetry
                .call_targets
                .call_target_indirect_const_resolved_count += 1;
            return Some(name);
        }
        if self.pcode_has_instruction_address(addr)
            && let Some(name) = self.current_function_name.clone()
        {
            self.telemetry
                .call_targets
                .call_target_indirect_const_resolved_count += 1;
            return Some(name);
        }
        self.telemetry
            .call_targets
            .call_target_unresolved_sub_fallback_count += 1;
        Some(format!("sub_{addr:x}"))
    }

    fn pcode_has_instruction_address(&self, addr: u64) -> bool {
        self.pcode
            .blocks
            .iter()
            .flat_map(|block| block.ops.iter())
            .any(|op| op.address == addr)
    }

    pub(in crate::midend) fn lower_intrinsic_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
        target: &str,
        ty: NirType,
    ) -> Result<HirExpr, MlilPreviewError> {
        let args = op
            .inputs
            .iter()
            .map(|input| self.lower_varnode(input, visiting))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HirExpr::Call {
            target: target.to_string(),
            args,
            ty,
        })
    }

    fn recover_opaque_callind_target(&self, target: &Varnode) -> Option<String> {
        let (_, producer) = self.lookup_def_site(target)?;
        let mnemonic = producer.asm_mnemonic.as_deref()?.trim();
        if !mnemonic.eq_ignore_ascii_case("INT3") {
            self.debug_callind_target_recovery("callind_target_recovery_rejected_unknown_producer");
            return None;
        }

        let swi_num = producer
            .inputs
            .iter()
            .rev()
            .find(|input| input.is_constant)
            .map(|input| input.constant_val)
            .unwrap_or(3);
        let target = format!("((code *)swi({swi_num}))");
        self.debug_callind_target_recovery("callind_target_recovered_trap_stub");
        Some(target)
    }

    fn resolve_copy_only_constant_chain(&self, target: &Varnode) -> Option<u64> {
        let mut current = target.clone();
        let mut visited = HashSet::default();
        for _ in 0..16 {
            if current.is_constant {
                return Some(current.constant_val as u64);
            }
            if !visited.insert(VarnodeKey::from(&current)) {
                return None;
            }
            let (_, producer) = self.lookup_def_site(&current)?;
            if producer.opcode != PcodeOpcode::Copy {
                return None;
            }
            current = producer.inputs.first()?.clone();
        }
        None
    }

    fn resolve_iat_load_call_target(&mut self, target: &Varnode) -> Option<String> {
        let Some((_, producer)) = self.lookup_def_site(target) else {
            self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            return None;
        };
        // Rust-Sleigh Copy-from-RAM idiom: `CALL qword ptr [IAT_addr]` is lowered as
        //   Copy unique <- v(ram:IAT_addr)
        //   CallInd unique
        // rather than the standard Load + CallInd pcode. Detect by checking whether the
        // Copy source is in a memory space (not constant, not register, not unique) and
        // treat its offset as the IAT slot address.
        if producer.opcode == PcodeOpcode::Copy {
            let Some(output) = producer.output.as_ref() else {
                self.record_call_target_const_reject(CallTargetConstReject::NoDef);
                return None;
            };
            if output.size != self.options.pointer_size {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_width_mismatch_count += 1;
                return None;
            }
            let Some(src) = producer.inputs.first() else {
                self.record_call_target_const_reject(CallTargetConstReject::NoDef);
                return None;
            };
            if !src.is_constant && !is_register_space_id(src.space_id) {
                return self.resolve_call_target_by_iat_slot(src.offset);
            }
            self.record_call_target_const_reject(CallTargetConstReject::UnsupportedOpcode);
            return None;
        }
        if producer.opcode != PcodeOpcode::Load {
            self.record_call_target_const_reject(CallTargetConstReject::UnsupportedOpcode);
            return None;
        }
        let Some(output) = producer.output.as_ref() else {
            self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            return None;
        };
        if output.size != self.options.pointer_size {
            self.telemetry
                .call_targets
                .call_target_indirect_rejected_width_mismatch_count += 1;
            return None;
        }
        let Some(ptr) = producer.inputs.get(1) else {
            self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            return None;
        };
        let ptr_addr = if ptr.is_constant {
            ptr.constant_val as u64
        } else {
            let producer_site = self
                .lookup_def_site(target)
                .map(|(site, _)| site)
                .unwrap_or_else(|| {
                    self.current_lowering_site.unwrap_or(LoweringSite {
                        block_idx: 0,
                        op_idx: usize::MAX,
                    })
                });
            match self.resolve_exact_scalar_const_for_call_target(
                ptr,
                producer_site,
                CALL_TARGET_CONST_FOLD_BUDGET,
            ) {
                Ok(addr) => {
                    self.telemetry
                        .call_targets
                        .call_target_indirect_ptr_const_folded_count += 1;
                    addr
                }
                Err(reason) => {
                    self.record_call_target_const_reject(reason);
                    return None;
                }
            }
        };
        self.resolve_call_target_by_iat_slot(ptr_addr)
    }

    fn recover_powerpc64_descriptor_call_target(&mut self, target: &Varnode) -> Option<String> {
        if self.options.calling_convention != CallingConvention::PowerPc64 {
            return None;
        }
        let scope = self.current_lowering_site?;
        self.debug_callind_target_recovery("powerpc64_descriptor_recovery_attempt");
        self.recover_powerpc64_descriptor_call_target_at(
            target,
            scope,
            CALL_TARGET_DESCRIPTOR_RECOVERY_BUDGET,
        )
    }

    fn recover_powerpc64_descriptor_call_target_at(
        &mut self,
        target: &Varnode,
        scope: LoweringSite,
        budget: usize,
    ) -> Option<String> {
        if budget == 0 {
            return None;
        }
        let site = match self.exact_def_site_for_call_target(target, scope) {
            Ok(site) => site,
            Err(_) if is_register_space_id(target.space_id) => {
                return self
                    .register_namer()
                    .register_name_with_param_owned(target.offset, target.size)
                    .and_then(|(name, param_index)| {
                        param_index
                            .filter(|&idx| idx < self.entry_arity)
                            .map(|_| name.to_string())
                    });
            }
            Err(_) => return None,
        };
        let producer = self.pcode.blocks[site.block_idx].ops[site.op_idx].clone();
        match producer.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                self.recover_powerpc64_descriptor_call_target_at(
                    producer.inputs.first()?,
                    site,
                    budget - 1,
                )
            }
            PcodeOpcode::Load => {
                let ptr = producer.inputs.get(1)?;
                self.recover_zero_offset_param_pointer(ptr, site, budget - 1)
            }
            _ => None,
        }
    }

    fn recover_zero_offset_param_pointer(
        &self,
        ptr: &Varnode,
        scope: LoweringSite,
        budget: usize,
    ) -> Option<String> {
        if budget == 0 {
            return None;
        }
        if is_register_space_id(ptr.space_id) {
            return self
                .register_namer()
                .register_name_with_param_owned(ptr.offset, ptr.size)
                .and_then(|(name, param_index)| {
                    param_index
                        .filter(|&idx| idx < self.entry_arity)
                        .map(|_| name.to_string())
                });
        }
        if ptr.is_constant {
            return None;
        }
        let site = self.exact_def_site_for_call_target(ptr, scope).ok()?;
        let producer = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
        let input = |idx: usize| producer.inputs.get(idx);
        match producer.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                self.recover_zero_offset_param_pointer(input(0)?, site, budget - 1)
            }
            PcodeOpcode::IntAdd => {
                if input(1)?.is_constant && input(1)?.constant_val == 0 {
                    self.recover_zero_offset_param_pointer(input(0)?, site, budget - 1)
                } else if input(0)?.is_constant && input(0)?.constant_val == 0 {
                    self.recover_zero_offset_param_pointer(input(1)?, site, budget - 1)
                } else {
                    None
                }
            }
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => {
                if input(1)?.is_constant
                    && input(1)?.constant_val == 0
                    && input(2).is_none_or(|scale| scale.is_constant && scale.constant_val <= 1)
                {
                    self.recover_zero_offset_param_pointer(input(0)?, site, budget - 1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn record_call_target_const_reject(&mut self, reason: CallTargetConstReject) {
        self.telemetry
            .call_targets
            .call_target_indirect_rejected_non_const_ptr_count += 1;
        match reason {
            CallTargetConstReject::UnsupportedOpcode => {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_unsupported_ptr_opcode_count += 1;
            }
            CallTargetConstReject::AmbiguousDef => {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_ambiguous_def_count += 1;
            }
            CallTargetConstReject::NonDominatingDef => {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_non_dominating_def_count += 1;
            }
            CallTargetConstReject::NoDef => {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_no_def_count += 1;
            }
        }
    }

    fn exact_def_site_for_call_target(
        &self,
        vn: &Varnode,
        scope: LoweringSite,
    ) -> Result<LoweringSite, CallTargetConstReject> {
        let key = VarnodeKey::from(vn);
        let Some(sites) = self.def_sites.get(&key) else {
            return Err(CallTargetConstReject::NoDef);
        };
        if sites.is_empty() {
            return Err(CallTargetConstReject::NoDef);
        }
        if let Some(defs_in_block) = self.block_defs.get(scope.block_idx)
            && let Some(def_indices) = defs_in_block.get(&key)
        {
            let prior_count = def_indices.partition_point(|idx| *idx < scope.op_idx);
            if prior_count > 0 {
                return Ok(LoweringSite {
                    block_idx: scope.block_idx,
                    op_idx: def_indices[prior_count - 1],
                });
            }
        }

        let mut candidates = sites
            .iter()
            .filter_map(|site| {
                if site.block_idx == scope.block_idx {
                    return (site.op_idx < scope.op_idx).then_some(LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: site.op_idx,
                    });
                }
                self.dom_tree
                    .dominates(site.block_idx, scope.block_idx)
                    .then_some(LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: site.op_idx,
                    })
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|site| (site.block_idx, site.op_idx));
        candidates.dedup();
        match candidates.as_slice() {
            [site] => Ok(*site),
            [] => Err(CallTargetConstReject::NonDominatingDef),
            _ => Err(CallTargetConstReject::AmbiguousDef),
        }
    }

    fn resolve_exact_scalar_const_for_call_target(
        &self,
        vn: &Varnode,
        scope: LoweringSite,
        budget: usize,
    ) -> Result<u64, CallTargetConstReject> {
        if vn.is_constant {
            return Ok(vn.constant_val as u64);
        }
        if budget == 0 {
            return Err(CallTargetConstReject::UnsupportedOpcode);
        }
        let site = self.exact_def_site_for_call_target(vn, scope)?;
        let producer = &self.pcode.blocks[site.block_idx].ops[site.op_idx];
        let input_const = |idx: usize| {
            producer
                .inputs
                .get(idx)
                .ok_or(CallTargetConstReject::NoDef)
                .and_then(|input| {
                    self.resolve_exact_scalar_const_for_call_target(input, site, budget - 1)
                })
        };
        match producer.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                input_const(0)
            }
            PcodeOpcode::IntAdd => Ok(input_const(0)?.wrapping_add(input_const(1)?)),
            PcodeOpcode::IntSub => Ok(input_const(0)?.wrapping_sub(input_const(1)?)),
            PcodeOpcode::IntLeft => {
                let value = input_const(0)?;
                let shift = u32::try_from(input_const(1)?)
                    .map_err(|_| CallTargetConstReject::UnsupportedOpcode)?;
                Ok(value.checked_shl(shift).unwrap_or(0))
            }
            PcodeOpcode::IntRight | PcodeOpcode::IntSRight => {
                let value = input_const(0)?;
                let shift = u32::try_from(input_const(1)?)
                    .map_err(|_| CallTargetConstReject::UnsupportedOpcode)?;
                Ok(value.checked_shr(shift).unwrap_or(0))
            }
            PcodeOpcode::PtrAdd => {
                let base = input_const(0)?;
                let index = input_const(1)?;
                let scale = input_const(2)?;
                Ok(base.wrapping_add(index.wrapping_mul(scale)))
            }
            PcodeOpcode::PtrSub => Ok(input_const(0)?.wrapping_add(input_const(1)?)),
            _ => Err(CallTargetConstReject::UnsupportedOpcode),
        }
    }

    fn debug_callind_target_recovery(&self, label: &str) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage={label}");
        }
    }

    pub(in crate::midend) fn lower_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
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
    ) -> Result<HirExpr, MlilPreviewError> {
        if vn.is_constant {
            if let Some(name) = self.options.global_names.get(&(vn.constant_val as u64)) {
                return Ok(HirExpr::AddressOfGlobal(name.clone()));
            }
            return Ok(HirExpr::Const(
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
        if let Some(site) = self.current_lowering_site {
            if !self.has_prior_local_def_for_varnode(vn, site) {
                if let Some(name) = self
                    .explicit_merge_bindings
                    .get(&(site.block_idx, key.clone()))
                {
                    return Ok(HirExpr::Var(name.clone()));
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
                    let expr = HirExpr::Var(name.clone());
                    if candidate_key.size == key.size {
                        return Ok(expr);
                    }
                    return Ok(HirExpr::Cast {
                        ty: type_from_size(vn.size, false),
                        expr: Box::new(expr),
                    });
                }
            }
        }
        let def_site = self.lookup_def_site(vn);
        if let Some(name) = self.live_call_result_binding_for_return_register(vn) {
            return Ok(HirExpr::Var(name));
        }
        // Loop body: LOAD/use of a loop-carried register must share the binding
        // that the loop's self-update (e.g. INT_ADD stride) will use — not a
        // frozen preheader snapshot vs a distinct bare hardware name.
        if let Some(name) = self.loop_body_carried_register_read_name(vn) {
            let name = self.ensure_live_register_binding(&name, vn.size);
            return Ok(HirExpr::Var(name));
        }
        if let Some(expr) = self.loop_exit_materialized_register_binding(vn) {
            return Ok(expr);
        }
        if let Some(expr) = self.try_lower_zero_extended_partial_register(vn, visiting)? {
            return Ok(expr);
        }
        if let Some(expr) = self.try_lower_diamond_select_for_varnode(vn, visiting)? {
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
                return Ok(HirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id)
                && self.current_store_value_read_at_join(vn)
                && let Some(name) = self.live_register_name_for_join_register_read(vn)
            {
                self.ensure_live_register_binding(&name, vn.size);
                return Ok(HirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id)
                && self.current_join_register_update_reads_live_register(vn)
                && let Some(name) = self.live_register_name_for_join_register_read(vn)
            {
                self.ensure_live_register_binding(&name, vn.size);
                return Ok(HirExpr::Var(name));
            }
            if let Some(param) = self.register_param(vn) {
                return Ok(HirExpr::Var(param));
            }
            if is_unique_space_id(vn.space_id)
                && let Some(name) = crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
            {
                return Ok(HirExpr::Var(name.to_string()));
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
                return Ok(HirExpr::Var(name));
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
                return Ok(HirExpr::Var(name));
            }
        }
        let stack_reg_name = self.stack_pointer_register_name(vn);
        if let Some(name) = stack_reg_name
            && matches!(name.as_str(), "rsp" | "esp" | "sp")
        {
            return Ok(HirExpr::Var(name));
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
                return Ok(HirExpr::Var(name.clone()));
            }
            let materialized_key = MaterializedVarnodeKey::new(vn, op);
            if let Some(name) = self.materialized_vns.get(&materialized_key) {
                return Ok(HirExpr::Var(name.clone()));
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
                                        return Ok(HirExpr::Var(name.clone()));
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
                    return Ok(self.project_alias_def_expr(vn, op, HirExpr::Var(name.clone())));
                }
            }
        }
        if !visiting.insert(key.clone()) {
            if let Some((site, op)) = def_site
                && Some(site) != self.current_lowering_site
            {
                let mut prior_visiting = visiting.clone();
                return self
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
            return Ok(HirExpr::Var(cycle_name));
        }

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
            None if self.options.global_names.contains_key(&vn.offset) => Ok(HirExpr::Var(
                self.options
                    .global_names
                    .get(&vn.offset)
                    .expect("global name exists after contains_key")
                    .clone(),
            )),
            None if is_unique_space_id(vn.space_id) => {
                let name = format!("tmp_{:x}", vn.offset);
                let name = self.ensure_live_register_binding(&name, vn.size);
                Ok(HirExpr::Var(name))
            }
            None if self.options.is_mapped_global(vn.offset) => {
                Ok(HirExpr::Var(format!("DAT_{:x}", vn.offset)))
            }
            None => {
                let name = format!("var_{:x}", vn.offset);
                let name = self.ensure_live_register_binding(&name, vn.size);
                Ok(HirExpr::Var(name))
            }
        };
        visiting.remove(&key);
        result
    }

    fn current_store_value_read_at_join(&self, vn: &Varnode) -> bool {
        let Some(site) = self.current_lowering_site else {
            return false;
        };
        if self
            .predecessors
            .get(site.block_idx)
            .is_none_or(|preds| preds.len() < 2)
        {
            return false;
        }
        let Some(op) = self
            .pcode
            .blocks
            .get(site.block_idx)
            .and_then(|block| block.ops.get(site.op_idx))
        else {
            return false;
        };
        op.opcode == PcodeOpcode::Store
            && op
                .inputs
                .get(2)
                .is_some_and(|input| self.varnode_aliases_value(input, vn))
    }

    fn current_join_register_update_reads_live_register(&self, vn: &Varnode) -> bool {
        let Some(site) = self.current_lowering_site else {
            return false;
        };
        if self
            .predecessors
            .get(site.block_idx)
            .is_none_or(|preds| preds.len() < 2)
        {
            return false;
        }
        let Some(block) = self.pcode.blocks.get(site.block_idx) else {
            return false;
        };
        let Some(op) = block.ops.get(site.op_idx) else {
            return false;
        };
        if !op
            .inputs
            .iter()
            .any(|input| self.varnode_aliases_value(input, vn))
        {
            return false;
        }
        block.ops.iter().skip(site.op_idx + 1).any(|candidate| {
            candidate
                .output
                .as_ref()
                .is_some_and(|output| self.varnode_aliases_value(output, vn))
        })
    }

    fn live_register_name_for_join_register_read(&self, vn: &Varnode) -> Option<String> {
        if !is_register_space_id(vn.space_id) {
            return None;
        }
        if self.options.calling_convention == CallingConvention::AArch64 && vn.size == 8 {
            return self.sla_hw_name(vn.offset, 4);
        }
        self.sla_hw_name(vn.offset, vn.size)
    }

    fn try_lower_diamond_select_for_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if !is_register_space_id(vn.space_id) {
            return Ok(None);
        }
        let key = VarnodeKey::from(vn);
        if visiting.contains(&key) {
            return Ok(None);
        }
        let Some(site) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(preds) = self.predecessors.get(site.block_idx).cloned() else {
            return Ok(None);
        };
        let [pred_a, pred_b] = preds.as_slice() else {
            return Ok(None);
        };
        let Some((branch_idx, branch_term_idx)) =
            self.find_diamond_branch_for_predecessors(*pred_a, *pred_b)
        else {
            return Ok(None);
        };
        let branch_block = self.pcode.blocks[branch_idx].clone();
        let branch_op = branch_block.ops[branch_term_idx].clone();
        if branch_op.opcode != PcodeOpcode::CBranch {
            return Ok(None);
        }
        let Some(cond_vn) = branch_op.inputs.last().cloned() else {
            return Ok(None);
        };
        let Some(target_vn) = branch_op.inputs.first() else {
            return Ok(None);
        };
        let Some(true_succ_idx) = resolve_branch_target_index(
            self.pcode,
            &self.address_to_index,
            branch_idx,
            &branch_op,
            target_vn,
        ) else {
            return Ok(None);
        };
        let false_succ_idx = if true_succ_idx == *pred_a {
            *pred_b
        } else if true_succ_idx == *pred_b {
            *pred_a
        } else {
            return Ok(None);
        };

        visiting.insert(key.clone());
        let true_res = self.lower_predecessor_incoming_value(true_succ_idx, vn, visiting);
        let false_res = if true_res.is_ok() {
            self.lower_predecessor_incoming_value(false_succ_idx, vn, visiting)
        } else {
            Err(MlilPreviewError::LoweringFailed)
        };
        visiting.remove(&key);

        let Some(true_expr) = true_res? else {
            return Ok(None);
        };
        let Some(false_expr) = false_res? else {
            return Ok(None);
        };
        if strip_casts(&true_expr) == strip_casts(&false_expr) {
            return Ok(Some(true_expr));
        }
        let cond = self.with_lowering_site(
            LoweringSite {
                block_idx: branch_idx,
                op_idx: branch_term_idx,
            },
            |this| this.lower_varnode(&cond_vn, visiting),
        )?;
        Ok(Some(HirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(true_expr),
            else_expr: Box::new(false_expr),
            ty: type_from_size(vn.size, false),
        }))
    }

    pub(in crate::midend::builder) fn find_diamond_branch_for_predecessors(
        &self,
        pred_a: usize,
        pred_b: usize,
    ) -> Option<(usize, usize)> {
        for (block_idx, succs) in self.successors.iter().enumerate() {
            if succs.len() != 2 || !succs.contains(&pred_a) || !succs.contains(&pred_b) {
                continue;
            }
            let block = self.pcode.blocks.get(block_idx)?;
            let term_idx = self.block_terminator_index(block)?;
            if block.ops.get(term_idx)?.opcode == PcodeOpcode::CBranch {
                return Some((block_idx, term_idx));
            }
        }
        None
    }

    fn lower_predecessor_incoming_value(
        &mut self,
        pred_idx: usize,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let Some(pred_block) = self.pcode.blocks.get(pred_idx).cloned() else {
            return Ok(None);
        };
        let term_idx = self
            .block_terminator_index(&pred_block)
            .unwrap_or(pred_block.ops.len());
        // CallInd/Call leave the ABI return value in the primary return register
        // without a p-code write. Prefer the materialize-time call-result binding
        // over a pre-call EAX/RAX write that only staged arguments
        // (apply_binop: `rax = (*(fp))(a,b)` then join must not re-read `a`).
        if self.register_namer().is_primary_return_register(vn)
            && let Some(name) =
                self.live_call_result_binding_in_block_for_return_register(vn, pred_idx, term_idx)
        {
            return Ok(Some(HirExpr::Var(name)));
        }
        let Some(def_idx) = self.last_alias_def_in_block(&pred_block, term_idx, vn) else {
            return Ok(None);
        };
        // If a Call/CallInd occurs after the last register write, that call is the
        // true producer of the return value (p-code does not model ABI out regs).
        if self.register_namer().is_primary_return_register(vn)
            && pred_block.ops[def_idx + 1..term_idx.min(pred_block.ops.len())]
                .iter()
                .any(|op| {
                    op.output.is_none()
                        && matches!(
                            op.opcode,
                            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                        )
                })
            && let Some(name) =
                self.live_call_result_binding_in_block_for_return_register(vn, pred_idx, term_idx)
        {
            return Ok(Some(HirExpr::Var(name)));
        }
        let op = pred_block.ops[def_idx].clone();
        let expr = self.with_lowering_site(
            LoweringSite {
                block_idx: pred_idx,
                op_idx: def_idx,
            },
            |this| this.lower_def_op(&op, visiting),
        )?;
        Ok(Some(self.project_alias_def_expr(vn, &op, expr)))
    }

    fn try_lower_zero_extended_partial_register(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size <= 1 {
            return Ok(None);
        }
        let Some(site) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(block) = self.pcode.blocks.get(self.pcode_block_idx(site.block_idx)) else {
            return Ok(None);
        };
        let scan_end = site.op_idx.min(block.ops.len());
        let requested_start = vn.offset;
        let requested_end = requested_start.saturating_add(u64::from(vn.size));
        let mut zeroed_ranges = Vec::new();
        // x86 `xor reg,reg; setcc low` zeros *before* the partial write. Reverse
        // scan therefore sees the setcc first; keep it pending until an older
        // clear covers the upper bytes (AArch64-style zero-*after*-partial still
        // resolves immediately when the clear is already in `zeroed_ranges`).
        let mut pending_partial_idx: Option<usize> = None;

        for idx in (0..scan_end).rev() {
            let op = &block.ops[idx];
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if output.is_constant
                || output.space_id != vn.space_id
                || !is_register_space_id(output.space_id)
                || !Self::varnode_ranges_overlap(output.offset, output.size, vn.offset, vn.size)
            {
                continue;
            }

            if Self::is_register_clear_def(op, output) {
                zeroed_ranges.push((
                    output.offset.max(requested_start),
                    output
                        .offset
                        .saturating_add(u64::from(output.size))
                        .min(requested_end),
                ));
                if let Some(partial_idx) = pending_partial_idx {
                    if let Some(expr) = self.try_finish_zero_extended_partial(
                        block,
                        site.block_idx,
                        partial_idx,
                        vn,
                        requested_start,
                        requested_end,
                        &zeroed_ranges,
                        visiting,
                    )? {
                        return Ok(Some(expr));
                    }
                }
                continue;
            }

            // Same-register low→wide IntZExt. Two roles:
            // 1) After `xor eax,eax`, SLEIGH emits `IntZExt rax ← eax`. That
            //    only zeros above EAX and must not block composing a later
            //    `setnz al` with the xor clear when reading EAX.
            // 2) `movzx eax, al` is value-defining for the requested width
            //    (input narrower than the read). Treat as a real def — do not
            //    skip (checksum byte accumulator: add al; movzx eax,al).
            if Self::is_same_register_low_zext(op, output) {
                let input = &op.inputs[0];
                let low_end = output.offset.saturating_add(u64::from(input.size));
                let out_end = output.offset.saturating_add(u64::from(output.size));
                // Upper bytes introduced by the zext are zero.
                let z_start = low_end.max(requested_start);
                let z_end = out_end.min(requested_end);
                if z_start < z_end {
                    zeroed_ranges.push((z_start, z_end));
                }
                // Transparent only when the zext input already covers the full
                // requested width (extension is strictly above what we read).
                let input_covers_request =
                    input.offset <= requested_start && low_end >= requested_end;
                if input_covers_request {
                    if let Some(partial_idx) = pending_partial_idx {
                        if let Some(expr) = self.try_finish_zero_extended_partial(
                            block,
                            site.block_idx,
                            partial_idx,
                            vn,
                            requested_start,
                            requested_end,
                            &zeroed_ranges,
                            visiting,
                        )? {
                            return Ok(Some(expr));
                        }
                    }
                    continue;
                }
                // Value-defining movzx into/within the requested range: stop.
                return Ok(None);
            }

            if output.offset == requested_start && output.size < vn.size {
                if let Some(expr) = self.try_finish_zero_extended_partial(
                    block,
                    site.block_idx,
                    idx,
                    vn,
                    requested_start,
                    requested_end,
                    &zeroed_ranges,
                    visiting,
                )? {
                    return Ok(Some(expr));
                }
                // Upper not covered yet — wait for an older clear (xor-before-setcc).
                if pending_partial_idx.is_none() {
                    pending_partial_idx = Some(idx);
                }
                continue;
            }

            // Other overlapping def: cannot compose through it.
            return Ok(None);
        }

        Ok(None)
    }

    fn try_finish_zero_extended_partial(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        block_idx: usize,
        partial_idx: usize,
        vn: &Varnode,
        requested_start: u64,
        requested_end: u64,
        zeroed_ranges: &[(u64, u64)],
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let op = &block.ops[partial_idx];
        let Some(output) = op.output.as_ref() else {
            return Ok(None);
        };
        if output.offset != requested_start || output.size >= vn.size {
            return Ok(None);
        }
        let upper_start = requested_start.saturating_add(u64::from(output.size));
        if !Self::ranges_cover(upper_start, requested_end, zeroed_ranges) {
            return Ok(None);
        }
        let expr = self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: partial_idx,
            },
            |this| this.lower_def_op(op, visiting),
        )?;
        Ok(Some(HirExpr::Cast {
            ty: type_from_size(vn.size, false),
            expr: Box::new(expr),
        }))
    }

    fn is_zero_copy(op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::Copy
            && op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0)
    }

    /// True when `op` definitively clears `output`'s storage (upper-byte zeroing
    /// for partial-register composition).
    ///
    /// Covers:
    /// - `mov reg, 0`
    /// - `xor reg, reg` / `sub reg, reg` (x86 zeroing idioms used before setcc)
    /// - `and reg, 0`
    fn is_register_clear_def(op: &PcodeOp, output: &Varnode) -> bool {
        if Self::is_zero_copy(op) {
            return true;
        }
        if !is_register_space_id(output.space_id) {
            return false;
        }
        match op.opcode {
            PcodeOpcode::IntXor | PcodeOpcode::IntSub if op.inputs.len() >= 2 => {
                let a = &op.inputs[0];
                let b = &op.inputs[1];
                !a.is_constant
                    && !b.is_constant
                    && a.space_id == output.space_id
                    && b.space_id == output.space_id
                    && a.offset == output.offset
                    && b.offset == output.offset
                    && a.size == output.size
                    && b.size == output.size
            }
            PcodeOpcode::IntAnd if op.inputs.len() >= 2 => op
                .inputs
                .iter()
                .any(|input| input.is_constant && input.constant_val == 0),
            _ => false,
        }
    }

    /// `IntZExt wide ← low` on the same register family (same space + offset,
    /// low.size < wide.size). Low lane is copied; upper bytes become zero.
    fn is_same_register_low_zext(op: &PcodeOp, output: &Varnode) -> bool {
        if op.opcode != PcodeOpcode::IntZExt || op.inputs.len() != 1 {
            return false;
        }
        if !is_register_space_id(output.space_id) {
            return false;
        }
        let input = &op.inputs[0];
        !input.is_constant
            && input.space_id == output.space_id
            && input.offset == output.offset
            && input.size < output.size
    }

    fn varnode_ranges_overlap(
        lhs_offset: u64,
        lhs_size: u32,
        rhs_offset: u64,
        rhs_size: u32,
    ) -> bool {
        let Some(lhs_end) = lhs_offset.checked_add(u64::from(lhs_size)) else {
            return false;
        };
        let Some(rhs_end) = rhs_offset.checked_add(u64::from(rhs_size)) else {
            return false;
        };
        lhs_offset < rhs_end && rhs_offset < lhs_end
    }

    fn ranges_cover(start: u64, end: u64, ranges: &[(u64, u64)]) -> bool {
        if start >= end {
            return true;
        }
        let mut covered_until = start;
        let mut sorted = ranges.to_vec();
        sorted.sort_unstable();
        for (range_start, range_end) in sorted {
            if range_end <= covered_until {
                continue;
            }
            if range_start > covered_until {
                return false;
            }
            covered_until = range_end;
            if covered_until >= end {
                return true;
            }
        }
        false
    }

    fn last_alias_def_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        vn: &Varnode,
    ) -> Option<usize> {
        block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .rev()
            .find_map(|(op_idx, op)| {
                op.output
                    .as_ref()
                    .is_some_and(|output| self.varnode_aliases_value(output, vn))
                    .then_some(op_idx)
            })
    }

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

    pub(in crate::midend) fn lower_def_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let created_trace = if self.active_trace_id.is_none() {
            let trace_id = self.next_trace_id();
            self.active_trace_id = Some(trace_id);
            true
        } else {
            false
        };
        let result = self.lower_def_op_inner(op, visiting);
        if created_trace {
            self.last_trace_id = self.active_trace_id;
            self.active_trace_id = None;
        }
        result
    }

    fn lower_def_op_inner(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match op.opcode {
            PcodeOpcode::Copy => self.lower_varnode(&op.inputs[0], visiting),
            PcodeOpcode::IntZExt => {
                // Zero-extend from a narrower source must first keep only the source
                // width. Classic x86 `movzx r32/r64, r8` after a wider ADD (e.g. RC4
                // keystream `(s[i]+s[j]) % 256`) is exactly this: the p-code is
                // `INT_ZEXT edx <- al` after `INT_ADD eax, …`. If the low-byte
                // truncation is lost, the sum is used as a pointer index and can
                // go out of bounds of a 256-byte table.
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let input = op
                    .inputs
                    .first()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let expr = self.lower_varnode(input, visiting)?;
                // Partial-register ZExt (`movzx r32, al` / `movzx r32, ax`) must keep a
                // source-width truncation before widening. Prefer an explicit narrow cast
                // plus AND mask so normalize cannot drop the low-byte lane when the parent
                // was a wider ADD (RC4 keystream index: `(s[i]+s[j]) % 256`).
                // Do not apply this to full-width 4→8 ZExt (ordinary zero-extend).
                if input.size > 0 && input.size <= 2 && input.size < output.size {
                    let narrow_ty = type_from_size(input.size, false);
                    let out_ty = type_from_size(output.size, false);
                    let bits = (input.size as u32).saturating_mul(8);
                    let mask = (1i64 << bits) - 1;
                    let truncated = HirExpr::Cast {
                        ty: narrow_ty,
                        expr: Box::new(expr),
                    };
                    return Ok(HirExpr::Binary {
                        op: HirBinaryOp::And,
                        lhs: Box::new(truncated),
                        rhs: Box::new(HirExpr::Const(mask, out_ty.clone())),
                        ty: out_ty,
                    });
                }
                Ok(HirExpr::Cast {
                    ty: type_from_size(output.size, false),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Cast | PcodeOpcode::IntSExt => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprAddressMaterialization)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(HirExpr::Cast {
                    ty: type_from_size(output.size, matches!(op.opcode, PcodeOpcode::IntSExt)),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Load => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::UnsupportedExprMemoryBackedVarnode);
                }
                let out = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprMemoryBackedVarnode)?;
                if let Some((slot_name, _)) = self.try_stack_slot_lvalue_for_memory_op(
                    op,
                    &op.inputs[1],
                    type_from_size(out.size, false),
                ) {
                    Ok(HirExpr::Var(slot_name))
                } else if let Some(global) = self.resolve_relocated_load_pointer(op, 16) {
                    Ok(if global.byte_offset == 0 {
                        HirExpr::AddressOfGlobal(global.name)
                    } else {
                        HirExpr::PtrOffset {
                            base: Box::new(HirExpr::AddressOfGlobal(global.name)),
                            offset: global.byte_offset,
                        }
                    })
                } else if let Some(addr) = self.resolve_global_address(&op.inputs[1], 16)
                    && let Some(value) = self.read_readonly_scalar_from_binary(addr, out.size)
                {
                    Ok(HirExpr::Const(
                        value as i64,
                        type_from_size(out.size, false),
                    ))
                } else {
                    Ok(HirExpr::Load {
                        ptr: Box::new(self.lower_varnode(&op.inputs[1], visiting)?),
                        ty: type_from_size(out.size, false),
                    })
                }
            }
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => self.lower_ptr_op(op, visiting),
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::FloatAdd
            | PcodeOpcode::FloatDiv
            | PcodeOpcode::FloatMult
            | PcodeOpcode::FloatSub
            | PcodeOpcode::FloatEqual
            | PcodeOpcode::FloatNotEqual
            | PcodeOpcode::FloatLess
            | PcodeOpcode::FloatLessEqual => self.lower_binary_op(op, visiting),
            PcodeOpcode::FloatInt2Float => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(HirExpr::Cast {
                    ty: float_type_from_size(output.size),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::FloatNan => {
                self.lower_intrinsic_call(op, visiting, "__isnan", NirType::Bool)
            }
            PcodeOpcode::IntNegate | PcodeOpcode::BoolNegate | PcodeOpcode::Int2Comp => {
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                let ty = type_from_size(output.size, false);
                let op = match op.opcode {
                    PcodeOpcode::IntNegate => HirUnaryOp::BitNot,
                    PcodeOpcode::BoolNegate => HirUnaryOp::Not,
                    PcodeOpcode::Int2Comp => HirUnaryOp::Neg,
                    _ => return Err(MlilPreviewError::UnsupportedExprVarnodeLowering),
                };
                Ok(HirExpr::Unary {
                    op,
                    expr: Box::new(expr),
                    ty,
                })
            }
            PcodeOpcode::IntCarry => {
                self.lower_intrinsic_call(op, visiting, "__carry", NirType::Bool)
            }
            PcodeOpcode::IntSCarry => {
                self.lower_intrinsic_call(op, visiting, "__scarry", NirType::Bool)
            }
            PcodeOpcode::IntSBorrow => {
                self.lower_intrinsic_call(op, visiting, "__sborrow", NirType::Bool)
            }
            PcodeOpcode::PopCount => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__popcount",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::LzCount => {
                let output = op
                    .output
                    .as_ref()
                    .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__lzcnt",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                self.lower_call(op, None, visiting)
            }
            PcodeOpcode::Piece => self.lower_piece_op(op, visiting),
            PcodeOpcode::SubPiece => self.lower_subpiece_op(op, visiting),
            PcodeOpcode::MultiEqual => self.lower_multiequal(op, visiting),
            PcodeOpcode::Indirect => {
                if let Some(input) = op.inputs.first() {
                    self.lower_varnode(input, visiting)
                } else {
                    Err(MlilPreviewError::UnsupportedExprIndirectValueSource)
                }
            }
            _ => {
                self.record_unsupported_inventory_event(
                    "lower_def_op_unsupported",
                    op.output.as_ref(),
                    Some(op),
                    Some(op.opcode),
                    self.current_lowering_site
                        .map(|site| self.pcode.blocks[site.block_idx].start_address),
                    Some(u64::from(op.seq_num)),
                    false,
                    "opcode_not_lowered",
                );
                self.debug_preview_log(&format!(
                    "[mlil-preview] stage=lower_def_op_unsupported opcode={:?} asm={}\n",
                    op.opcode,
                    op.asm_mnemonic.as_deref().unwrap_or("<none>")
                ));
                Err(MlilPreviewError::UnsupportedPattern("opcode"))
            }
        }
    }

    pub(in crate::midend) fn lower_multiequal(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let mut lowered: Vec<Option<HirExpr>> = Vec::with_capacity(op.inputs.len());
        for input in &op.inputs {
            match self.lower_varnode(input, visiting) {
                Ok(expr) => lowered.push(Some(expr)),
                Err(_) => lowered.push(None),
            }
        }

        // Collect only the successfully-lowered expressions.
        let resolved: Vec<&HirExpr> = lowered.iter().filter_map(Option::as_ref).collect();

        if resolved.is_empty() {
            // All inputs failed — nothing to coalesce.
            return Err(MlilPreviewError::UnsupportedExprMultiequal);
        }

        // Check whether all successfully-resolved inputs have the same
        // canonical expression (ignoring cast wrappers).  If so, that value
        // is the definitive join — this covers both the "all-same" case and
        // the "partial failure with a unique surviving value" case (e.g. one
        // predecessor is a loop back-edge whose def-chain failed because the
        // back-edge varnode traces to the same MultiEqual, and the other
        // predecessor resolves to the function-entry value).
        let canonical = strip_casts(resolved[0]);
        if resolved.iter().all(|e| strip_casts(e) == canonical) {
            return Ok(resolved[0].clone());
        }

        Err(MlilPreviewError::UnsupportedExprMultiequal)
    }

    pub(in crate::midend) fn lower_ptr_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let base = self.lower_varnode(&op.inputs[0], visiting)?;
        let offset = if op.inputs.len() > 1 && op.inputs[1].is_constant {
            op.inputs[1].constant_val
        } else {
            0
        };
        if op.opcode == PcodeOpcode::PtrAdd && op.inputs.len() > 2 && op.inputs[2].is_constant {
            let index = self.lower_varnode(&op.inputs[1], visiting)?;
            let elem_ty = type_from_size(op.inputs[2].constant_val as u32, false);
            return Ok(HirExpr::Index {
                base: Box::new(base),
                index: Box::new(index),
                elem_ty,
            });
        }
        if (op.opcode == PcodeOpcode::PtrAdd || op.opcode == PcodeOpcode::PtrSub)
            && op.inputs.len() > 1
            && !op.inputs[1].is_constant
        {
            let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprPtrArithmetic)?;
            let arith_op = if op.opcode == PcodeOpcode::PtrAdd {
                HirBinaryOp::Add
            } else {
                HirBinaryOp::Sub
            };
            return Ok(HirExpr::Binary {
                op: arith_op,
                lhs: Box::new(base),
                rhs: Box::new(rhs),
                ty: type_from_size(output.size, false),
            });
        }
        Ok(HirExpr::PtrOffset {
            base: Box::new(base),
            offset,
        })
    }

    pub(in crate::midend) fn lower_binary_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::UnsupportedExprVarnodeLowering);
        }
        if op.opcode == PcodeOpcode::IntXor
            && VarnodeKey::from(&op.inputs[0]) == VarnodeKey::from(&op.inputs[1])
        {
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
            return Ok(HirExpr::Const(0, type_from_size(output.size, false)));
        }
        // x86 CDQ + IDIV: dividend is Piece(sign_fill(L), L). Use signed L alone.
        if matches!(op.opcode, PcodeOpcode::IntSRem | PcodeOpcode::IntSDiv)
            && let Some(low) = self.try_cdq_signed_dividend_low(&op.inputs[0])
        {
            let lhs = self.lower_varnode(&low, visiting)?;
            let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
            let output = op
                .output
                .as_ref()
                .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
            // Remainder/quotient lane is the machine width of the low half (EAX),
            // not the 64-bit piece — match signed C `%` / `/` on that width.
            let bits = low
                .size
                .saturating_mul(8)
                .max(output.size.saturating_mul(8));
            let ty = NirType::Int { bits, signed: true };
            let lhs = HirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(lhs),
            };
            let rhs = HirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(rhs),
            };
            return Ok(HirExpr::Binary {
                op: map_binary_op(op.opcode)?,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                ty,
            });
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        let output = op
            .output
            .as_ref()
            .ok_or(MlilPreviewError::UnsupportedExprVarnodeLowering)?;
        let ty = if is_comparison(op.opcode) {
            NirType::Bool
        } else if matches!(
            op.opcode,
            PcodeOpcode::FloatAdd
                | PcodeOpcode::FloatDiv
                | PcodeOpcode::FloatMult
                | PcodeOpcode::FloatSub
        ) {
            float_type_from_size(output.size)
        } else {
            pcode_output_type_from_size(op.opcode, output.size)
        };
        Ok(HirExpr::Binary {
            op: map_binary_op(op.opcode)?,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        })
    }

    /// CDQ-class dividend low half for signed rem/div.
    ///
    /// Recognized forms (x86 `cdq; idiv` / SLEIGH):
    /// - `Piece(H, L)` with `H` arithmetic sign-fill of `L`
    /// - direct `IntSExt(L)` as the wide dividend
    /// - `IntOr(IntLeft(H, k), L')` with `k == 8*sizeof(L)`, `L'` zero/copy of `L`,
    ///   and `H` the high half of `IntSExt(L)` (SubPiece) or other CDQ sign-fill
    ///
    /// Returns `L` so SRem/SDiv lower as C signed `%`/`/` on that half-width.
    fn try_cdq_signed_dividend_low(&self, dividend: &Varnode) -> Option<Varnode> {
        // Peel Copy/Cast wrappers on the dividend itself (some templates stage
        // the wide Or into a unique via Copy before SRem).
        let mut current = dividend.clone();
        for _ in 0..6 {
            let (_, def) = self.lookup_def_site(&current)?;
            match def.opcode {
                PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    current = def.inputs.first()?.clone();
                    continue;
                }
                PcodeOpcode::IntSExt => return def.inputs.first().cloned(),
                PcodeOpcode::Piece if def.inputs.len() >= 2 => {
                    let high = &def.inputs[0];
                    let low = &def.inputs[1];
                    return if self.varnode_is_sign_fill_of(high, low) {
                        Some(low.clone())
                    } else {
                        None
                    };
                }
                // SLEIGH idiv form: (ZExt(hi) << 32) | ZExt(lo) via IntOr/IntLeft.
                PcodeOpcode::IntOr if def.inputs.len() >= 2 => {
                    return self
                        .try_cdq_low_from_or_shl_dividend(&def.inputs[0], &def.inputs[1])
                        .or_else(|| {
                            self.try_cdq_low_from_or_shl_dividend(&def.inputs[1], &def.inputs[0])
                        });
                }
                _ => return None,
            }
        }
        None
    }

    /// Match `shifted_high | low_ext` where `shifted_high` is `IntLeft(H, k)`
    /// (after peeling ZExt/Copy on the left arm) with `k` equal to the low half
    /// width in bits, and `H` is CDQ sign-fill of the core of `low_ext`.
    fn try_cdq_low_from_or_shl_dividend(
        &self,
        shifted_high: &Varnode,
        low_ext: &Varnode,
    ) -> Option<Varnode> {
        // High arm may be ZExt/Copy of the IntLeft result in some SLEIGH paths.
        let left_vn = self.peel_cdq_width_ext(shifted_high)?;
        let (_, left_def) = self.lookup_def_site(&left_vn)?;
        if left_def.opcode != PcodeOpcode::IntLeft || left_def.inputs.len() < 2 {
            return None;
        }
        let high = &left_def.inputs[0];
        let shift = &left_def.inputs[1];
        if !shift.is_constant {
            return None;
        }
        let shift_amt = shift.constant_val as u32;
        // Peel ZExt/Copy/Cast on the low side to the machine-width half.
        let low = self.peel_cdq_width_ext(low_ext)?;
        let low_bits = u32::from(low.size.saturating_mul(8));
        if shift_amt != low_bits && shift_amt != 32 && shift_amt != 64 {
            return None;
        }
        if !self.varnode_is_sign_fill_of(high, &low) {
            return None;
        }
        Some(low)
    }

    /// Peel ZExt / Copy / Cast wrappers (width promotion of the low/high half).
    fn peel_cdq_width_ext(&self, vn: &Varnode) -> Option<Varnode> {
        let mut current = vn.clone();
        for _ in 0..6 {
            let Some((_, op)) = self.lookup_def_site(&current) else {
                return Some(current);
            };
            match op.opcode {
                PcodeOpcode::IntZExt | PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    current = op.inputs.first()?.clone();
                }
                _ => return Some(current),
            }
        }
        Some(current)
    }

    /// True when `high` is CDQ-class **arithmetic** sign-fill of `low`.
    ///
    /// Accept:
    /// - `IntSRight` of `low` (or short ZExt/SExt/Copy/Cast chain to `low`)
    /// - `IntSExt` of `low` (after peeling Copy/Cast wrappers)
    /// - `SubPiece(IntSExt(low), offset == low.size)` — SLEIGH CDQ high half
    /// - `IntZExt` / `Copy` / `Cast` wrappers **around the above only**
    ///
    /// Reject bare `Copy(low)`, bare `IntZExt(low)`, or logical `IntRight` alone —
    /// peeling wrappers must land on SAR/SExt/SubPiece(SExt), never on `low` itself.
    fn varnode_is_sign_fill_of(&self, high: &Varnode, low: &Varnode) -> bool {
        let low_key = VarnodeKey::from(low);
        let mut current = high.clone();
        for _ in 0..8 {
            // High reduced to exact `low` without an intervening SAR/SExt/
            // SubPiece(SExt) is not sign-fill (e.g. Piece(Copy(L), L)).
            // Use exact key match only — register alias helpers can be broader
            // than "same value" and must not reject SubPiece high halves.
            if VarnodeKey::from(&current) == low_key {
                return false;
            }
            let Some((_, hop)) = self.lookup_def_site(&current) else {
                return false;
            };
            match hop.opcode {
                // Only arithmetic right-shift is CDQ-class sign fill via shift.
                PcodeOpcode::IntSRight => {
                    let Some(base) = hop.inputs.first() else {
                        return false;
                    };
                    if !self.varnode_related_to_cdq_low(base, low) {
                        return false;
                    }
                    let Some(shift) = hop.inputs.get(1) else {
                        return false;
                    };
                    if !shift.is_constant {
                        return false;
                    }
                    let shift_amt = shift.constant_val as i64;
                    // CDQ: SAR L, 31  or SAR (sext L), 32  (word-width shift).
                    let bits = i64::from(low.size.saturating_mul(8));
                    return shift_amt == bits - 1
                        || shift_amt == bits
                        || shift_amt == 31
                        || shift_amt == 63;
                }
                // Full signed widen of low is a valid wide-dividend form.
                PcodeOpcode::IntSExt => {
                    return hop
                        .inputs
                        .first()
                        .is_some_and(|base| self.varnode_related_to_cdq_low(base, low));
                }
                // High half of sign-extend: SubPiece(SExt(L), offset == |L|).
                PcodeOpcode::SubPiece if hop.inputs.len() >= 2 => {
                    let base = &hop.inputs[0];
                    let offset = &hop.inputs[1];
                    if !offset.is_constant {
                        return false;
                    }
                    let off = offset.constant_val as u64;
                    if off != u64::from(low.size) {
                        return false;
                    }
                    // base must be IntSExt(low) (possibly through Copy/Cast).
                    let mut sext_vn = base.clone();
                    for _ in 0..4 {
                        let Some((_, sop)) = self.lookup_def_site(&sext_vn) else {
                            return false;
                        };
                        match sop.opcode {
                            PcodeOpcode::IntSExt => {
                                return sop
                                    .inputs
                                    .first()
                                    .is_some_and(|b| self.varnode_related_to_cdq_low(b, low));
                            }
                            PcodeOpcode::Copy | PcodeOpcode::Cast => {
                                let Some(input) = sop.inputs.first() else {
                                    return false;
                                };
                                sext_vn = input.clone();
                            }
                            _ => return false,
                        }
                    }
                    return false;
                }
                // Peel width/copy wrappers on the high side only — never treat
                // the peeled-to `low` as fill (checked at loop head).
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt => {
                    let Some(input) = hop.inputs.first() else {
                        return false;
                    };
                    current = input.clone();
                }
                // Logical right-shift alone is not sign-fill.
                PcodeOpcode::IntRight => return false,
                _ => return false,
            }
        }
        false
    }

    /// `base` is `low`, an alias, or a short ZExt/SExt/Copy/Cast chain from `low`.
    /// Used only as the *input* of IntSRight / IntSExt / SubPiece(SExt), not as high itself.
    fn varnode_related_to_cdq_low(&self, base: &Varnode, low: &Varnode) -> bool {
        if self.varnode_aliases_value(base, low) || VarnodeKey::from(base) == VarnodeKey::from(low)
        {
            return true;
        }
        let mut current = base.clone();
        for _ in 0..4 {
            let Some((_, op)) = self.lookup_def_site(&current) else {
                return false;
            };
            match op.opcode {
                PcodeOpcode::IntSExt
                | PcodeOpcode::IntZExt
                | PcodeOpcode::Copy
                | PcodeOpcode::Cast => {
                    let Some(input) = op.inputs.first() else {
                        return false;
                    };
                    if self.varnode_aliases_value(input, low)
                        || VarnodeKey::from(input) == VarnodeKey::from(low)
                    {
                        return true;
                    }
                    current = input.clone();
                }
                _ => return false,
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "lower_expr_tests.rs"]
mod tests;
