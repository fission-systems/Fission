use super::*;

const CALL_TARGET_CONST_FOLD_BUDGET: usize = 16;

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
    pub(in crate::nir::builder) fn stack_pointer_register_name(
        &self,
        vn: &Varnode,
    ) -> Option<&'static str> {
        match vn.space_id {
            UNIQUE_SPACE_ID => unique_register_name(vn.offset, vn.size),
            space_id if is_register_space_id(space_id) => {
                register_name_with_param(vn.offset, vn.size, self.options.calling_convention)
                    .map(|(name, _)| name)
                    .or_else(|| Some(register_name(vn.offset, vn.size)))
            }
            _ => None,
        }
    }

    pub(in crate::nir::builder) fn live_call_result_binding_for_return_register(
        &self,
        vn: &Varnode,
    ) -> Option<String> {
        if !is_primary_return_register_for_abi(vn, self.options.calling_convention) {
            return None;
        }
        let site = self.current_lowering_site?;
        let block = self.pcode.blocks.get(site.block_idx)?;
        for (prior_idx, op) in block.ops.iter().enumerate().take(site.op_idx).rev() {
            let prior_site = LoweringSite {
                block_idx: site.block_idx,
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

    pub(in crate::nir) fn lookup_def_site(
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
                for candidate_key in &candidate_keys {
                    if let Some(def_indices) = defs_in_block.get(candidate_key) {
                        let mut prior_count = def_indices.partition_point(|idx| *idx < site.op_idx);
                        while prior_count > 0 {
                            let def_idx = def_indices[prior_count - 1];
                            let candidate_op = &self.pcode.blocks[site.block_idx].ops[def_idx];
                            if Self::is_identity_copy_def(candidate_op) {
                                prior_count -= 1;
                                continue;
                            }
                            let candidate = LoweringSite {
                                block_idx: site.block_idx,
                                op_idx: def_idx,
                            };
                            if resolved_site
                                .is_none_or(|resolved| candidate.op_idx > resolved.op_idx)
                            {
                                resolved_site = Some(candidate);
                            }
                            break;
                        }
                    }
                }
            }
        }

        if resolved_site.is_none() {
            if let Some(scope_site) = scope {
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

    fn is_identity_copy_def(op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::Copy
            && op.output.as_ref().is_some_and(|output| {
                op.inputs
                    .first()
                    .is_some_and(|input| VarnodeKey::from(output) == VarnodeKey::from(input))
            })
    }

    fn register_key_covers(candidate: &VarnodeKey, requested: &VarnodeKey) -> bool {
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

    fn register_key_zero_extends(&self, candidate: &VarnodeKey, requested: &VarnodeKey) -> bool {
        self.options.is_64bit
            && !candidate.is_constant
            && !requested.is_constant
            && candidate.space_id == requested.space_id
            && is_register_space_id(candidate.space_id)
            && candidate.offset == requested.offset
            && candidate.size == 4
            && requested.size == 8
            && (x64_ghidra_reg_name(candidate.offset).is_some()
                || aarch64_ghidra_reg_name(candidate.offset, candidate.size).is_some())
    }

    fn register_key_cross_space_covers(
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

    fn register_key_cross_space_zero_extends(
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

    fn gpr_family_index_for_key(&self, key: &VarnodeKey) -> Option<usize> {
        if key.is_constant {
            return None;
        }
        if is_register_space_id(key.space_id) {
            return match self.options.calling_convention {
                CallingConvention::AArch64 => {
                    aarch64_ghidra_reg_name(key.offset, key.size).and_then(aarch64_gpr_family_index)
                }
                CallingConvention::Arm32 => {
                    arm32_ghidra_reg_name(key.offset, key.size).and_then(arm32_gpr_family_index)
                }
                CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
                    powerpc_ghidra_reg_name_for_abi(
                        key.offset,
                        key.size,
                        self.options.calling_convention,
                    )
                    .and_then(powerpc_gpr_family_index)
                }
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                    x64_ghidra_reg_name(key.offset).and_then(crate::arch::x86::x86_gpr_family_index)
                }
            };
        }
        if is_unique_space_id(key.space_id) {
            let name = unique_register_name(key.offset, key.size)?;
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

    pub(in crate::nir::builder) fn varnode_aliases_value(
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
            && aarch64_ghidra_reg_name(output.offset, output.size)
                .and_then(aarch64_gpr_family_index)
                .is_some_and(|output_family| {
                    aarch64_ghidra_reg_name(requested.offset, requested.size)
                        .and_then(aarch64_gpr_family_index)
                        == Some(output_family)
                })
    }

    fn loop_exit_materialized_register_binding(&mut self, vn: &Varnode) -> Option<HirExpr> {
        if vn.is_constant || !is_register_space_id(vn.space_id) || vn.size < 4 {
            return None;
        }
        let site = self.current_lowering_site?;
        let predecessor_idxs = self.predecessors.get(site.block_idx)?.clone();
        if predecessor_idxs.len() < 2 || predecessor_idxs.contains(&site.block_idx) {
            return None;
        }

        let mut materialized_name = None;
        let mut materialized_expr = None;
        let mut zero_incoming = false;
        for pred_idx in predecessor_idxs {
            let pred_block = self.pcode.blocks.get(pred_idx)?;
            let term_idx = self
                .block_terminator_index(pred_block)
                .unwrap_or(pred_block.ops.len());
            let (_, pred_op) = self.last_register_redefinition_before(pred_block, term_idx, vn)?;
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

    fn last_register_redefinition_before<'b>(
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

    pub(in crate::nir) fn lower_call(
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
                        } else if let Some(name) = self.resolve_iat_load_call_target(target) {
                            name
                        } else {
                            name
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
                        } else if matches!(other, HirExpr::Load { .. }) {
                            if let Some(name) = self.resolve_iat_load_call_target(target) {
                                name
                            } else {
                                print_expr(&other)
                            }
                        } else {
                            print_expr(&other)
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
            .map(|index| format!("__pcodeop_{index}"))
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

    pub(in crate::nir::builder) fn resolve_address_like_call_target_name(
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

    pub(in crate::nir) fn lower_intrinsic_call(
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
        let mut visited = HashSet::new();
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

    pub(in crate::nir) fn lower_varnode(
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
                return Ok(HirExpr::Var(name.clone()));
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
                                || self.register_key_cross_space_zero_extends(candidate_key, &key))
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
        let def_site = self.lookup_def_site(vn);
        if let Some(name) = self.live_call_result_binding_for_return_register(vn) {
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
            if let Some(param) = self.register_param(vn) {
                return Ok(HirExpr::Var(param));
            }
            if is_unique_space_id(vn.space_id)
                && let Some(name) = unique_register_name(vn.offset, vn.size)
            {
                return Ok(HirExpr::Var(name.to_string()));
            }
            if self.options.calling_convention == CallingConvention::Arm32
                && is_register_space_id(vn.space_id)
            {
                let name =
                    register_name_with_param(vn.offset, vn.size, self.options.calling_convention)
                        .map(|(name, _)| name)
                        .or_else(|| arm32_ghidra_reg_name(vn.offset, vn.size));
                if let Some(name) = name {
                    let name = self.ensure_live_register_binding(name, vn.size);
                    return Ok(HirExpr::Var(name));
                }
            }
            if !self.options.is_64bit
                && is_register_space_id(vn.space_id)
                && matches!(
                    self.options.calling_convention,
                    CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                )
                && let Some(name) = register_name_32(vn.offset, vn.size)
            {
                let name = self.ensure_live_register_binding(name, vn.size);
                return Ok(HirExpr::Var(name));
            }
            if is_register_space_id(vn.space_id) {
                let name = if (!self.options.is_64bit
                    && matches!(
                        self.options.calling_convention,
                        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                    ))
                    || self.suppress_entry_register_params
                {
                    register_name(vn.offset, vn.size)
                } else {
                    register_name_with_param(vn.offset, vn.size, self.options.calling_convention)
                        .map(|(name, _)| name)
                        .unwrap_or_else(|| register_name(vn.offset, vn.size))
                };
                let name = self.ensure_live_register_binding(name, vn.size);
                return Ok(HirExpr::Var(name));
            }
        }
        let stack_reg_name = self.stack_pointer_register_name(vn);
        if let Some(name) = stack_reg_name
            && matches!(name, "rsp" | "esp" | "sp")
        {
            return Ok(HirExpr::Var(name.to_string()));
        }
        if let Some((_, op)) = def_site {
            if op.output.is_none()
                && matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
                && is_primary_return_register_for_abi(vn, self.options.calling_convention)
                && let Some((site, _)) = def_site
                && let Some(name) = self.call_result_bindings.get(&site)
            {
                return Ok(HirExpr::Var(name.clone()));
            }
            let materialized_key = MaterializedVarnodeKey::new(vn, op);
            if let Some(name) = self.materialized_vns.get(&materialized_key) {
                return Ok(HirExpr::Var(name.clone()));
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
                prior_visiting.remove(&key);
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
                unique_register_name(vn.offset, vn.size)
                    .map_or_else(|| format!("tmp_{:x}", vn.offset), ToString::to_string)
            } else {
                format!("tmp_{:x}", vn.offset)
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
                Ok(HirExpr::Var(format!("tmp_{:x}", vn.offset)))
            }
            None if self.options.is_mapped_global(vn.offset) => {
                Ok(HirExpr::Var(format!("DAT_{:x}", vn.offset)))
            }
            None => Ok(HirExpr::Var(format!("var_{:x}", vn.offset))),
        };
        visiting.remove(&key);
        result
    }

    fn try_lower_diamond_select_for_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if !is_register_space_id(vn.space_id) {
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
        let Some(true_expr) = self.lower_predecessor_incoming_value(true_succ_idx, vn, visiting)?
        else {
            return Ok(None);
        };
        let Some(false_expr) =
            self.lower_predecessor_incoming_value(false_succ_idx, vn, visiting)?
        else {
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

    fn find_diamond_branch_for_predecessors(
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
        let Some(def_idx) = self.last_alias_def_in_block(&pred_block, term_idx, vn) else {
            return Ok(None);
        };
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

            if Self::is_zero_copy(op) {
                zeroed_ranges.push((
                    output.offset.max(requested_start),
                    output
                        .offset
                        .saturating_add(u64::from(output.size))
                        .min(requested_end),
                ));
                continue;
            }

            if output.offset == requested_start && output.size < vn.size {
                let upper_start = requested_start.saturating_add(u64::from(output.size));
                if !Self::ranges_cover(upper_start, requested_end, &zeroed_ranges) {
                    return Ok(None);
                }
                let expr = self.with_lowering_site(
                    LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: idx,
                    },
                    |this| this.lower_def_op(op, visiting),
                )?;
                return Ok(Some(HirExpr::Cast {
                    ty: type_from_size(vn.size, false),
                    expr: Box::new(expr),
                }));
            }

            return Ok(None);
        }

        Ok(None)
    }

    fn is_zero_copy(op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::Copy
            && op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0)
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

    pub(in crate::nir) fn lower_def_op(
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
            PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
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
            | PcodeOpcode::FloatSub => self.lower_binary_op(op, visiting),
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

    pub(in crate::nir) fn lower_multiequal(
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

    pub(in crate::nir) fn lower_ptr_op(
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

    pub(in crate::nir) fn lower_binary_op(
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
            type_from_size(
                output.size,
                matches!(
                    op.opcode,
                    PcodeOpcode::IntSDiv
                        | PcodeOpcode::IntSRem
                        | PcodeOpcode::IntSLess
                        | PcodeOpcode::IntSLessEqual
                ),
            )
        };
        Ok(HirExpr::Binary {
            op: map_binary_op(op.opcode)?,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::render_mlil_preview;

    fn varnode(offset: u64) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size: 8,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn register(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn constant(value: i64) -> Varnode {
        Varnode::constant(value, 8)
    }

    fn constant_sized(value: i64, size: u32) -> Varnode {
        Varnode::constant(value, size)
    }

    fn op(
        seq_num: u32,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x1000 + u64::from(seq_num),
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    fn block_at(
        start_address: u64,
        index: u32,
        ops: Vec<PcodeOp>,
    ) -> crate::pcode::PcodeBasicBlock {
        crate::pcode::PcodeBasicBlock {
            index,
            start_address,
            successors: Vec::new(),
            ops,
        }
    }

    fn pcode_function(blocks: Vec<crate::pcode::PcodeBasicBlock>) -> crate::pcode::PcodeFunction {
        crate::pcode::PcodeFunction { blocks }
    }

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0x1400_0000,
            sections: vec![(0x1400_1000, 0x1400_2000)],
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
        }
    }

    #[test]
    fn diamond_join_lowers_branch_local_register_defs_as_select() {
        let cond = varnode(0x80);
        let rax = register(0, 8);
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    1,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1020), cond],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(2, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(10)]),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(4, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(20)]),
                    op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(0x1030, 3, vec![op(6, PcodeOpcode::Return, None, vec![rax])]),
        ]);
        let options = test_options();

        let code = render_mlil_preview(&pcode, "diamond_select", 0x1000, &options).expect("render");

        assert!(
            code.contains("return tmp_80 ? 20 : 10;"),
            "expected branch-target arm to be the true select arm:\n{code}"
        );
    }

    #[test]
    fn diamond_join_lowers_copy_through_join_read_as_select() {
        let cond = varnode(0x80);
        let rax = register(0, 8);
        let rcx = register(8, 8);
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(10)]),
                    op(2, PcodeOpcode::CBranch, None, vec![constant(0x1020), cond]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(3, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(20)]),
                    op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(5, PcodeOpcode::Copy, Some(rcx.clone()), vec![rax]),
                    op(6, PcodeOpcode::Return, None, vec![rcx]),
                ],
            ),
        ]);
        let options = test_options();

        let code =
            render_mlil_preview(&pcode, "diamond_copy_select", 0x1000, &options).expect("render");

        assert!(
            code.contains("return tmp_80 ? 10 : 20;"),
            "expected copy-through join read to use the synthesized select:\n{code}"
        );
    }

    #[test]
    fn same_block_partial_register_write_with_zeroed_upper_replaces_stale_wide_def() {
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;

        let s0 = register(0x5000, 4);
        let h0 = register(0x5000, 2);
        let upper_h0 = register(0x5002, 2);
        let w8 = register(0x4040, 4);
        let pcode = pcode_function(vec![block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::Copy,
                    Some(s0.clone()),
                    vec![constant(0x1234_5678)],
                ),
                op(1, PcodeOpcode::Copy, Some(h0.clone()), vec![h0.clone()]),
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(h0.clone()),
                    vec![constant_sized(1, 2), constant_sized(2, 2)],
                ),
                op(
                    3,
                    PcodeOpcode::Copy,
                    Some(upper_h0),
                    vec![constant_sized(0, 2)],
                ),
                op(4, PcodeOpcode::Copy, Some(w8.clone()), vec![s0]),
                op(5, PcodeOpcode::Return, None, vec![w8]),
            ],
        )]);

        let code = render_mlil_preview(&pcode, "partial_zero_extend", 0x1000, &options)
            .expect("render partial zero-extend");

        assert!(
            code.contains("return 3;"),
            "expected low partial write to replace stale wide definition:\n{code}"
        );
        assert!(
            !code.contains("305419896"),
            "stale full-width definition should not feed the return:\n{code}"
        );
    }

    #[test]
    fn partial_register_zero_extend_ignores_stale_virtual_lowering_site_bound() {
        let mut options = test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;

        let w0 = register(0x5000, 4);
        let pcode = pcode_function(vec![block_at(
            0x1000,
            0,
            vec![op(
                0,
                PcodeOpcode::Copy,
                Some(w0.clone()),
                vec![constant(1)],
            )],
        )]);
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 0,
            op_idx: 12,
        });
        let mut visiting = HashSet::new();

        let lowered = builder
            .try_lower_zero_extended_partial_register(&w0, &mut visiting)
            .expect("stale lowering-site op index should not panic");

        assert!(lowered.is_none());
    }
}
