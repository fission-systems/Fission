use super::*;

const CALL_TARGET_CONST_FOLD_BUDGET: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallTargetConstReject {
    UnsupportedOpcode,
    AmbiguousDef,
    NonDominatingDef,
    NoDef,
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
            self.call_target_context_missing_count += 1;
            return None;
        };
        if let Some(target_ref) = ctx.call_target_refs.get(&addr) {
            self.call_target_exact_index_hit_count += 1;
            match target_ref.provenance {
                CallTargetProvenance::Import => {
                    self.call_target_import_resolved_count += 1;
                }
                CallTargetProvenance::ExportThunkTarget => {
                    self.call_target_direct_symbol_resolved_count += 1;
                    self.call_target_export_thunk_target_resolved_count += 1;
                }
                _ => {
                    self.call_target_direct_symbol_resolved_count += 1;
                }
            }
            return Some(target_ref.symbol.clone());
        }
        if ctx.ambiguous_call_targets.contains(&addr) {
            self.call_target_exact_index_ambiguous_count += 1;
        } else {
            self.call_target_unresolved_no_exact_identity_count += 1;
        }
        None
    }

    fn resolve_call_target_by_iat_slot(&mut self, addr: u64) -> Option<String> {
        let Some(ctx) = self.type_context else {
            self.call_target_context_missing_count += 1;
            return None;
        };
        let Some(target_ref) = ctx.iat_target_refs.get(&addr) else {
            self.call_target_indirect_rejected_non_iat_load_count += 1;
            return None;
        };
        self.call_target_iat_slot_resolved_count += 1;
        self.call_target_indirect_load_resolved_count += 1;
        self.call_target_import_resolved_count += 1;
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
                        let prior_count = def_indices.partition_point(|idx| *idx < site.op_idx);
                        if prior_count > 0 {
                            let def_idx = def_indices[prior_count - 1];
                            let candidate = LoweringSite {
                                block_idx: site.block_idx,
                                op_idx: def_idx,
                            };
                            if resolved_site
                                .is_none_or(|resolved| candidate.op_idx > resolved.op_idx)
                            {
                                resolved_site = Some(candidate);
                            }
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
                    .map(|def| LoweringSite {
                        block_idx: def.block_idx,
                        op_idx: def.op_idx,
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
        if !is_register_space_id(key.space_id) || key.is_constant {
            return candidates;
        }
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
        candidates
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
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                    x64_ghidra_reg_name(key.offset).and_then(crate::arch::x86::x86_gpr_family_index)
                }
            };
        }
        if key.space_id == UNIQUE_SPACE_ID {
            let name = unique_register_name(key.offset, key.size)?;
            return crate::arch::x86::x86_gpr_family_index(name);
        }
        None
    }

    fn varnode_covers(candidate: &Varnode, requested: &Varnode) -> bool {
        Self::register_key_covers(&VarnodeKey::from(candidate), &VarnodeKey::from(requested))
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
        if !Self::varnode_covers(output, requested) {
            return expr;
        }
        let byte_offset = requested.offset.saturating_sub(output.offset);
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
        let target = if let Some(target) = op.inputs.first() {
            if let Some(name) = self.resolve_constant_call_target_name(op, target) {
                name
            } else {
                match self.lower_varnode(target, visiting) {
                    Ok(HirExpr::Const(val, _)) => {
                        let addr = val as u64;
                        if let Some(name) = self.resolve_call_target_by_address(addr) {
                            if matches!(op.opcode, PcodeOpcode::CallInd) {
                                self.call_target_indirect_const_resolved_count += 1;
                            }
                            name
                        } else {
                            self.call_target_unresolved_sub_fallback_count += 1;
                            format!("sub_{addr:x}")
                        }
                    }
                    Ok(HirExpr::Var(name)) if matches!(op.opcode, PcodeOpcode::CallInd) => {
                        if let Some(addr) = self.resolve_copy_only_constant_chain(target) {
                            if let Some(name) = self.resolve_call_target_by_address(addr) {
                                self.call_target_indirect_const_resolved_count += 1;
                                name
                            } else {
                                self.call_target_unresolved_sub_fallback_count += 1;
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
                                self.call_target_indirect_const_resolved_count += 1;
                                name
                            } else {
                                self.call_target_unresolved_sub_fallback_count += 1;
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
        if addr == 0 {
            return None;
        }
        if let Some(name) = self.resolve_call_target_by_address(addr) {
            if matches!(op.opcode, PcodeOpcode::CallInd) {
                self.call_target_indirect_const_resolved_count += 1;
            }
            Some(name)
        } else {
            self.call_target_unresolved_sub_fallback_count += 1;
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
            self.call_target_unresolved_sub_fallback_count += 1;
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
            self.call_target_indirect_rejected_width_mismatch_count += 1;
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
                    self.call_target_indirect_ptr_const_folded_count += 1;
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
        self.call_target_indirect_rejected_non_const_ptr_count += 1;
        match reason {
            CallTargetConstReject::UnsupportedOpcode => {
                self.call_target_indirect_rejected_unsupported_ptr_opcode_count += 1;
            }
            CallTargetConstReject::AmbiguousDef => {
                self.call_target_indirect_rejected_ambiguous_def_count += 1;
            }
            CallTargetConstReject::NonDominatingDef => {
                self.call_target_indirect_rejected_non_dominating_def_count += 1;
            }
            CallTargetConstReject::NoDef => {
                self.call_target_indirect_rejected_no_def_count += 1;
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
        if def_site.is_none() {
            if let Some(param) = self.register_param(vn) {
                return Ok(HirExpr::Var(param));
            }
            if vn.space_id == UNIQUE_SPACE_ID
                && let Some(name) = unique_register_name(vn.offset, vn.size)
            {
                return Ok(HirExpr::Var(name.to_string()));
            }
            if !self.options.is_64bit
                && is_register_space_id(vn.space_id)
                && let Some(name) = register_name_32(vn.offset, vn.size)
            {
                return Ok(HirExpr::Var(name.to_string()));
            }
            if is_register_space_id(vn.space_id) {
                let name = if !self.options.is_64bit || self.suppress_entry_register_params {
                    register_name(vn.offset, vn.size)
                } else {
                    register_name_with_param(vn.offset, vn.size, self.options.calling_convention)
                        .map(|(name, _)| name)
                        .unwrap_or_else(|| register_name(vn.offset, vn.size))
                };
                return Ok(HirExpr::Var(name.to_string()));
            }
        }
        let stack_reg_name = self.stack_pointer_register_name(vn);
        if let Some(name) = stack_reg_name
            && matches!(name, "rsp" | "esp" | "sp")
        {
            return Ok(HirExpr::Var(name.to_string()));
        }
        if let Some(name) = self.live_call_result_binding_for_return_register(vn) {
            return Ok(HirExpr::Var(name));
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
            let cycle_name = if vn.space_id == UNIQUE_SPACE_ID {
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
            None if vn.space_id == UNIQUE_SPACE_ID => {
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
            | PcodeOpcode::BoolXor => self.lower_binary_op(op, visiting),
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
