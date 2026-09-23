use super::*;
use fission_midend_core::ir::sanitize_c_identifier;

const CALL_TARGET_CONST_FOLD_BUDGET: usize = 16;
const CALL_TARGET_DESCRIPTOR_RECOVERY_BUDGET: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallTargetConstReject {
    UnsupportedOpcode,
    AmbiguousDef,
    NonDominatingDef,
    NoDef,
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn resolve_call_target_by_address(&mut self, addr: u64) -> Option<String> {
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
            return Some(sanitize_c_identifier(&target_ref.symbol));
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
            return Some(sanitize_c_identifier(&target_ref.symbol));
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

    pub(super) fn resolve_call_target_by_iat_slot(&mut self, addr: u64) -> Option<String> {
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
        Some(sanitize_c_identifier(&target_ref.symbol))
    }

    fn resolve_call_target_by_iat_slot_without_telemetry(&self, addr: u64) -> Option<String> {
        self.type_context?
            .iat_target_refs
            .get(&addr)
            .map(|target_ref| sanitize_c_identifier(&target_ref.symbol))
    }

    pub(super) fn resolve_relocation_call_target_name(&mut self, op: &PcodeOp) -> Option<String> {
        if !matches!(op.opcode, PcodeOpcode::Call) {
            return None;
        }
        self.options
            .relocation_names
            .get(&op.address)
            .filter(|name| !name.is_empty())
            .map(|name| sanitize_c_identifier(name))
    }

    pub(super) fn resolve_constant_call_target_name(
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

    pub(super) fn resolve_indirect_scalar_call_target_name(
        &mut self,
        target: &Varnode,
    ) -> Option<String> {
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
            && let Some(name) = self.current_function_name.as_deref()
        {
            self.telemetry
                .call_targets
                .call_target_indirect_const_resolved_count += 1;
            return Some(sanitize_c_identifier(name));
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

    pub(super) fn recover_opaque_callind_target(&self, target: &Varnode) -> Option<String> {
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

    pub(super) fn resolve_copy_only_constant_chain(&self, target: &Varnode) -> Option<u64> {
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

    pub(in crate::midend::builder) fn resolve_iat_load_call_target(
        &mut self,
        target: &Varnode,
    ) -> Option<String> {
        self.resolve_iat_load_call_target_with_telemetry(target, true)
    }

    pub(in crate::midend::builder) fn resolve_iat_load_call_target_without_telemetry(
        &mut self,
        target: &Varnode,
    ) -> Option<String> {
        self.resolve_iat_load_call_target_with_telemetry(target, false)
    }

    fn resolve_iat_load_call_target_with_telemetry(
        &mut self,
        target: &Varnode,
        record_telemetry: bool,
    ) -> Option<String> {
        let Some((_, producer)) = self.lookup_def_site(target) else {
            if record_telemetry {
                self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            }
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
                if record_telemetry {
                    self.record_call_target_const_reject(CallTargetConstReject::NoDef);
                }
                return None;
            };
            if output.size != self.options.pointer_size {
                if record_telemetry {
                    self.telemetry
                        .call_targets
                        .call_target_indirect_rejected_width_mismatch_count += 1;
                }
                return None;
            }
            let Some(src) = producer.inputs.first() else {
                if record_telemetry {
                    self.record_call_target_const_reject(CallTargetConstReject::NoDef);
                }
                return None;
            };
            if !src.is_constant && !is_register_space_id(src.space_id) {
                return if record_telemetry {
                    self.resolve_call_target_by_iat_slot(src.offset)
                } else {
                    self.resolve_call_target_by_iat_slot_without_telemetry(src.offset)
                };
            }
            if record_telemetry {
                self.record_call_target_const_reject(CallTargetConstReject::UnsupportedOpcode);
            }
            return None;
        }
        if producer.opcode != PcodeOpcode::Load {
            if record_telemetry {
                self.record_call_target_const_reject(CallTargetConstReject::UnsupportedOpcode);
            }
            return None;
        }
        let Some(output) = producer.output.as_ref() else {
            if record_telemetry {
                self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            }
            return None;
        };
        if output.size != self.options.pointer_size {
            if record_telemetry {
                self.telemetry
                    .call_targets
                    .call_target_indirect_rejected_width_mismatch_count += 1;
            }
            return None;
        }
        let Some(ptr) = producer.inputs.get(1) else {
            if record_telemetry {
                self.record_call_target_const_reject(CallTargetConstReject::NoDef);
            }
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
                    if record_telemetry {
                        self.telemetry
                            .call_targets
                            .call_target_indirect_ptr_const_folded_count += 1;
                    }
                    addr
                }
                Err(reason) => {
                    if record_telemetry {
                        self.record_call_target_const_reject(reason);
                    }
                    return None;
                }
            }
        };
        if record_telemetry {
            self.resolve_call_target_by_iat_slot(ptr_addr)
        } else {
            self.resolve_call_target_by_iat_slot_without_telemetry(ptr_addr)
        }
    }

    pub(super) fn recover_powerpc64_descriptor_call_target(
        &mut self,
        target: &Varnode,
    ) -> Option<String> {
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
        if preview_debug_enabled() {
            eprintln!("[mlil-preview] stage={label}");
        }
    }
}
