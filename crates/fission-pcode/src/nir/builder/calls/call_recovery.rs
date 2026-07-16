use super::*;
use std::collections::HashSet;

fn resolve_add_op_stack_address(
    builder: &PreviewBuilder<'_>,
    add: &PcodeOp,
) -> Option<(StackBase, i64)> {
    if add.inputs.len() < 2 {
        return None;
    }
    if let Some((base, offset)) = builder.resolve_stack_address(&add.inputs[0]) {
        return crate::nir::cfg::const_offset(&add.inputs[1]).map(|delta| (base, offset + delta));
    }
    if let Some((base, offset)) = builder.resolve_stack_address(&add.inputs[1]) {
        return crate::nir::cfg::const_offset(&add.inputs[0]).map(|delta| (base, offset + delta));
    }
    None
}

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn abi_state(&self) -> AbiState {
        AbiState::new_with_cspec(
            self.options.calling_convention,
            self.options.is_64bit,
            self.options.pointer_size,
            self.stack_frame_size,
            self.options.cspec_param_offsets.clone(),
            self.options.cspec_stack_arg_base,
            self.options.cspec_extrapop,
        )
        .with_frame_pointer_established(self.entry_frame_pointer_established)
    }

    fn surface_call_carrier_name(&mut self, vn: &Varnode) -> Option<String> {
        if let Some(param) = self.register_param(vn) {
            return Some(param);
        }
        if vn.space_id == UNIQUE_SPACE_ID {
            return crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                .map(str::to_string);
        }
        if is_register_varnode(vn) {
            return Some(
                self.sla_hw_name(vn.offset, vn.size)
                    .unwrap_or_else(|| "reg".to_string())
                    .to_string(),
            );
        }
        None
    }

    fn param_index_for_varnode(
        &self,
        vn: &Varnode,
        include_rust_sleigh_space: bool,
    ) -> Option<usize> {
        if include_rust_sleigh_space {
            if !is_register_varnode(vn) {
                return None;
            }
        } else if vn.space_id != REGISTER_SPACE_ID {
            return None;
        }
        self.abi_state().param_slot_for_varnode(vn)
    }

    fn debug_call_recovery(&self, message: &str) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=call_arg_recovery {message}");
        }
    }

    fn normalize_recovered_call_arg(&self, expr: HirExpr) -> HirExpr {
        let (value, fallback) = match expr {
            HirExpr::Const(value, ty) => (value, HirExpr::Const(value, ty)),
            HirExpr::Cast { ty, expr } => {
                let HirExpr::Const(value, _) = *expr else {
                    return HirExpr::Cast { ty, expr };
                };
                (value, HirExpr::Const(value, ty))
            }
            expr => return expr,
        };
        if value <= 0 {
            return fallback;
        }
        let Some(ctx) = self.type_context else {
            return fallback;
        };
        ctx.call_target_refs
            .get(&(value as u64))
            .map(|target_ref| HirExpr::Var(target_ref.symbol.clone()))
            .unwrap_or(fallback)
    }

    fn is_callee_saved_register_varnode(&self, vn: &Varnode) -> bool {
        let reg_name = if is_register_varnode(vn) {
            self.sla_hw_name(vn.offset, vn.size)
                .unwrap_or_else(|| "reg".to_string())
        } else if vn.space_id == UNIQUE_SPACE_ID {
            crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                .map(str::to_string)
                .unwrap_or_default()
        } else {
            String::new()
        };
        matches!(
            reg_name.as_str(),
            "rbx" | "rbp" | "rsi" | "rdi" | "r12" | "r13" | "r14" | "r15"
        )
    }

    fn check_ancestor_realistic(
        &self,
        vn: &Varnode,
        def_site: &crate::nir::builder::DefSite,
        call_block_idx: usize,
        call_op_idx: usize,
    ) -> bool {
        if def_site.block_idx == call_block_idx {
            return def_site.op_idx < call_op_idx;
        }

        if !self.dom_tree.dominates(def_site.block_idx, call_block_idx) {
            return false;
        }

        // According to Ghidra 11.4.2 AncestorRealistic:
        for intermediate_idx in def_site.block_idx + 1..call_block_idx {
            if !self
                .dom_tree
                .dominates(def_site.block_idx, intermediate_idx)
            {
                continue;
            }
            if !self.dom_tree.dominates(intermediate_idx, call_block_idx) {
                continue;
            }
            let inter_block = &self.pcode.blocks[intermediate_idx];
            for op in &inter_block.ops {
                if op.opcode.is_call() && !self.is_callee_saved_register_varnode(vn) {
                    return false;
                }
            }
        }

        true
    }

    fn resolve_call_store_stack_address(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        store_idx: usize,
    ) -> Option<(StackBase, i64)> {
        let store = &block.ops[store_idx];
        let site = LoweringSite {
            block_idx: block.index as usize,
            op_idx: store_idx,
        };
        self.with_lowering_site(site, |this| {
            if let Some(addr) = this.resolve_stack_address_from_memory_op(store) {
                return Some(addr);
            }
            let ptr = store.inputs.get(1)?;
            let store_addr = store.address;
            for prev_idx in (0..store_idx).rev() {
                let prev = &block.ops[prev_idx];
                if prev.address != store_addr {
                    break;
                }
                if prev.output.as_ref() != Some(ptr) {
                    continue;
                }
                if matches!(prev.opcode, PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd) {
                    return resolve_add_op_stack_address(this, prev);
                }
            }
            this.resolve_stack_address(ptr)
        })
    }

    fn recover_call_stack_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<HirExpr>, MlilPreviewError> {
        let abi = self.abi_state();
        if !self.options.is_64bit && self.x86_32_stack_call_args_enabled() {
            return self.recover_x86_32_stack_args_from_block(block, call_idx);
        }
        if !self.options.is_64bit {
            return Ok(Vec::new());
        }

        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let mut recovered = std::collections::BTreeMap::<usize, HirExpr>::new();
        for prev_idx in (0..scan_end).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                if prev.opcode == PcodeOpcode::CallOther
                    && prev.output.is_none()
                    && Some(prev.address) == call_address
                {
                    continue;
                }
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            if prev.opcode != PcodeOpcode::Store || prev.inputs.len() < 3 {
                continue;
            }
            let site = LoweringSite {
                block_idx: block.index as usize,
                op_idx: prev_idx,
            };
            let stack_address = self.with_lowering_site(site, |this| {
                this.resolve_call_store_stack_address(block, prev_idx)
            });
            let Some((StackBase::Rsp, offset)) = stack_address else {
                continue;
            };
            let Some(stack_index) = abi.stack_argument_index(offset) else {
                continue;
            };
            if recovered.contains_key(&stack_index) {
                continue;
            }
            let value = self.with_lowering_site(site, |this| {
                this.lower_varnode(prev.inputs.last().expect("store rhs"), &mut HashSet::new())
            })?;
            recovered.insert(stack_index, value);
        }

        let mut out = Vec::new();
        for idx in 0.. {
            let Some(expr) = recovered.remove(&idx) else {
                break;
            };
            out.push(expr);
        }
        self.debug_call_recovery(&format!("stack_args={}", out.len()));
        Ok(out)
    }

    fn x86_32_stack_call_args_enabled(&self) -> bool {
        self.options.pointer_size == 4
            && self.options.calling_convention == CallingConvention::X86_32
    }

    fn recover_x86_32_stack_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<HirExpr>, MlilPreviewError> {
        const MAX_STACK_ARGS: usize = 32;

        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let mut out = Vec::new();
        let mut current_push_address = None;
        for prev_idx in (0..scan_end).rev() {
            if out.len() >= MAX_STACK_ARGS {
                break;
            }
            let prev = &block.ops[prev_idx];
            if Some(prev.address) == call_address {
                continue;
            }
            if prev.opcode.is_control_flow() {
                if prev.opcode == PcodeOpcode::CallOther
                    && prev.output.is_none()
                    && Some(prev.address) == call_address
                {
                    continue;
                }
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            if self.x86_32_stack_push_update(prev) {
                continue;
            }
            if current_push_address.is_some_and(|address| address == prev.address) {
                continue;
            }
            if !self.x86_32_stack_push_store(prev) {
                if !out.is_empty() {
                    break;
                }
                continue;
            }
            let site = LoweringSite {
                block_idx: block.index as usize,
                op_idx: prev_idx,
            };
            let value = self.with_lowering_site(site, |this| {
                this.lower_varnode(prev.inputs.last().expect("store rhs"), &mut HashSet::new())
            })?;
            out.push(self.normalize_recovered_call_arg(value));
            current_push_address = Some(prev.address);
        }

        self.debug_call_recovery(&format!("x86_32_stack_args={}", out.len()));
        Ok(out)
    }

    fn x86_32_stack_push_update(&self, op: &PcodeOp) -> bool {
        op.opcode == PcodeOpcode::IntSub
            && op.inputs.len() >= 2
            && op
                .output
                .as_ref()
                .is_some_and(|output| self.is_x86_32_esp(output))
            && self.is_x86_32_esp(&op.inputs[0])
            && const_offset(&op.inputs[1]) == Some(i64::from(self.options.pointer_size))
    }

    fn x86_32_stack_push_store(&self, op: &PcodeOp) -> bool {
        if op.opcode != PcodeOpcode::Store || op.inputs.len() < 3 {
            return false;
        }
        let is_stack_ptr = self.is_x86_32_esp(&op.inputs[1])
            || self
                .resolve_stack_address_from_memory_op(op)
                .is_some_and(|(base, offset)| base == StackBase::Rsp && offset < 0);
        is_stack_ptr
            && op
                .inputs
                .last()
                .is_some_and(|value| value.size == self.options.pointer_size || value.size == 0)
    }

    pub(in crate::nir::builder) fn x86_32_store_is_recovered_call_arg(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if !self.x86_32_stack_call_args_enabled() || !self.x86_32_stack_push_store(op) {
            return false;
        }
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate.address == op.address && self.x86_32_stack_push_update(candidate) {
                continue;
            }
            if self.x86_32_stack_push_update(candidate) || self.x86_32_stack_push_store(candidate) {
                continue;
            }
            return matches!(
                candidate.opcode,
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
            );
        }
        false
    }

    fn is_x86_32_esp(&self, vn: &Varnode) -> bool {
        self.options.calling_convention == CallingConvention::X86_32
            && is_register_space_id(vn.space_id)
            && vn.offset == 0x10
            && vn.size == 4
    }

    pub(in crate::nir::builder) fn recover_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        self.recover_call_args_from_block_with_mode(block, call_idx, false)
    }

    pub(in crate::nir::builder) fn recover_tail_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        self.recover_call_args_from_block_with_mode(block, call_idx, true)
    }

    /// For CallInd, the register family that holds the callee pointer must not
    /// also appear as a recovered ABI argument (win64 `call r8` writes r8 and
    /// r8 is also param slot 2).
    fn callind_target_param_slot_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Option<usize> {
        let op = block.ops.get(call_idx)?;
        if op.opcode != PcodeOpcode::CallInd {
            return None;
        }
        let mut cursor = op.inputs.first()?.clone();
        // Follow a short copy/zext chain to a register-space carrier.
        for _ in 0..6 {
            if is_register_space_id(cursor.space_id) {
                return self.param_index_for_varnode(&cursor, true);
            }
            let (_site, def_op) = self.lookup_def_site(&cursor)?;
            if !matches!(
                def_op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Cast
            ) {
                return None;
            }
            cursor = def_op.inputs.first()?.clone();
        }
        None
    }

    /// Peel same-block Copy/ZExt/SExt/Cast so tail-call arg recovery sees the
    /// staged source register, not the ABI slot being written.
    ///
    /// Example (x64 O2 `jmp rax` after `mov ecx, edx; movsxd rcx, ecx`):
    /// prefer_source on the zext alone surfaces `ecx`/`param_1`; peeling reaches
    /// `edx`/`param_2`.
    ///
    /// Walks `scan_block.ops` locally (not `lookup_def_site`) so a wider zext of
    /// the same register family is not treated as the seed's own definition.
    fn peel_prefer_source_arg_varnode(
        &self,
        scan_block: &crate::pcode::PcodeBasicBlock,
        before_op_idx: usize,
        seed: &Varnode,
    ) -> Varnode {
        let mut cursor = seed.clone();
        let mut limit = before_op_idx.min(scan_block.ops.len());
        for _ in 0..6 {
            let mut found: Option<(usize, Varnode)> = None;
            for prev_idx in (0..limit).rev() {
                let prev = &scan_block.ops[prev_idx];
                let Some(output) = prev.output.as_ref() else {
                    continue;
                };
                if !Self::varnode_covers_or_aliases_register(output, &cursor) {
                    continue;
                }
                if !matches!(
                    prev.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::Cast
                ) {
                    break;
                }
                let Some(input) = prev.inputs.first() else {
                    break;
                };
                found = Some((prev_idx, input.clone()));
                break;
            }
            let Some((def_idx, next)) = found else {
                break;
            };
            cursor = next;
            limit = def_idx;
            if cursor.is_constant {
                break;
            }
        }
        cursor
    }

    /// True when `def` writes the same register family that `vn` reads
    /// (exact key or overlapping low/high view of the same offset bank).
    fn varnode_covers_or_aliases_register(def: &Varnode, vn: &Varnode) -> bool {
        if !is_register_varnode(def) || !is_register_varnode(vn) {
            return false;
        }
        if def.offset == vn.offset && def.size == vn.size {
            return true;
        }
        // Same base offset family: e.g. rcx (8) covers ecx (4) at offset 0x8.
        if def.offset == vn.offset && def.size >= vn.size {
            return true;
        }
        // Partial write into a wider view: ecx write feeds later rcx read.
        if vn.offset == def.offset && vn.size >= def.size {
            return true;
        }
        false
    }

    fn recover_call_args_from_block_with_mode(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        prefer_source_values: bool,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit
            && !self.x86_32_stack_call_args_enabled()
            && !matches!(
                self.options.calling_convention,
                CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
        {
            return Ok(None);
        }

        let abi = self.abi_state();
        let param_slots = abi.effective_param_reg_slots();
        let param_count = param_slots.len();
        let mut recovered: Vec<Option<HirExpr>> = vec![None; param_count];
        let skip_param = self.callind_target_param_slot_index(block, call_idx);
        // Also skip BranchInd target register if it aliases a param slot (rare).
        let skip_param = skip_param.or_else(|| {
            let op = block.ops.get(call_idx)?;
            if op.opcode != PcodeOpcode::BranchInd {
                return None;
            }
            let target = op.inputs.first()?;
            self.param_index_for_varnode(target, true)
        });
        let scan_end = call_idx.min(block.ops.len());
        let call_address = block.ops.get(call_idx).map(|op| op.address);
        let has_same_instruction_callother_marker = block.ops[..scan_end].iter().any(|prev| {
            prev.opcode == PcodeOpcode::CallOther
                && prev.output.is_none()
                && Some(prev.address) == call_address
        });

        // Scan current block, then (for BranchInd/CallInd tail arms) a single
        // predecessor so args staged before the null-check arm are recovered
        // (x64 O2: rcx/rdx set in parent, jmp in child).
        let mut scan_blocks: Vec<(&crate::pcode::PcodeBasicBlock, usize)> = vec![(block, scan_end)];
        if matches!(
            block.ops.get(call_idx).map(|op| op.opcode),
            Some(PcodeOpcode::BranchInd | PcodeOpcode::CallInd)
        ) {
            if let Some(&block_idx) = self.address_to_index.get(&block.start_address)
                && let Some(preds) = self.predecessors.get(block_idx)
                && let [pred_idx] = preds.as_slice()
                && let Some(pred_block) = self.pcode.blocks.get(*pred_idx)
            {
                scan_blocks.push((pred_block, pred_block.ops.len()));
            }
        }

        for (scan_block, scan_limit) in scan_blocks {
            for prev_idx in (0..scan_limit).rev() {
                let prev = &scan_block.ops[prev_idx];
                if prev.opcode.is_control_flow() {
                    if prev.opcode == PcodeOpcode::CallOther
                        && prev.output.is_none()
                        && Some(prev.address) == call_address
                    {
                        continue;
                    }
                    if self.call_is_terminal_branchind_artifact(scan_block, prev_idx) {
                        continue;
                    }
                    // Only hard-stop control-flow inside the *call* block.
                    if std::ptr::eq(scan_block, block) {
                        break;
                    }
                    continue;
                }
                let Some(output) = &prev.output else {
                    continue;
                };
                if prefer_source_values
                    && self.op_is_terminal_branchind_target_artifact(scan_block, prev_idx)
                {
                    continue;
                }
                let Some(param_index) = self.param_index_for_varnode(output, true) else {
                    continue;
                };
                if skip_param == Some(param_index) {
                    continue;
                }
                if param_index >= recovered.len() || recovered[param_index].is_some() {
                    continue;
                }

                let direct_rhs = if prefer_source_values
                    || self.options.calling_convention != CallingConvention::Arm32
                    || !has_same_instruction_callother_marker
                {
                    None
                } else {
                    self.try_lower_materialized_output_rhs(scan_block.start_address, prev)?
                };
                let source = if prefer_source_values {
                    let seed = prev.inputs.first().unwrap_or(output);
                    // Clone so we can peel without borrowing `prev` across the
                    // subsequent lower/surface calls.
                    self.peel_prefer_source_arg_varnode(scan_block, prev_idx, seed)
                } else {
                    output.clone()
                };
                let expr = if let Some(expr) = direct_rhs {
                    expr
                } else if prefer_source_values
                    && let Some(name) = self.surface_call_carrier_name(&source)
                {
                    HirExpr::Var(name)
                } else {
                    match self.lower_varnode(&source, &mut HashSet::new()) {
                        Ok(expr) => expr,
                        Err(MlilPreviewError::UnsupportedPattern("opcode"))
                            if self.surface_call_carrier_name(output).is_some() =>
                        {
                            HirExpr::Var(
                                self.surface_call_carrier_name(output)
                                    .expect("surface carrier exists after guard"),
                            )
                        }
                        Err(err) => {
                            self.debug_lowering_error(
                                "call_arg_recovery",
                                scan_block.start_address,
                                u64::from(prev.seq_num),
                                prev.opcode,
                                &err,
                            );
                            if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
                                self.record_unsupported_inventory_event(
                                    "call_recovery",
                                    Some(output),
                                    Some(prev),
                                    Some(prev.opcode),
                                    Some(scan_block.start_address),
                                    Some(u64::from(prev.seq_num)),
                                    false,
                                    "call_arg_recovery_lowering_failed",
                                );
                            }
                            continue;
                        }
                    }
                };
                recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
            }
        }

        let assignments = self.call_arg_carrier_assignments(block, call_idx, &abi);
        for assignment in assignments {
            let param_index = assignment.resource.slot;
            if skip_param == Some(param_index) {
                continue;
            }
            if recovered[param_index].is_some() {
                continue;
            }
            let (offset, size) = param_slots[param_index];

            let vn = Varnode {
                space_id: REGISTER_SPACE_ID,
                offset,
                size,
                is_constant: false,
                constant_val: 0,
            };

            let key = VarnodeKey::from(&vn);
            if let Some((site, _)) = self.lookup_def_site(&vn) {
                let def_site = crate::nir::support::DefSite {
                    block_idx: site.block_idx,
                    op_idx: site.op_idx,
                    _marker: std::marker::PhantomData,
                };
                if self.check_ancestor_realistic(&vn, &def_site, block.index as usize, call_idx) {
                    let expr = self.lower_varnode(&vn, &mut HashSet::new())?;
                    recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
                    continue;
                }
            }
        }

        let contiguous_reg_count = recovered.iter().take_while(|expr| expr.is_some()).count();
        let stack_args = self.recover_call_stack_args_from_block(block, call_idx)?;
        if contiguous_reg_count == 0 {
            if !stack_args.is_empty() {
                self.debug_call_recovery(&format!("reg_args=0 total_args={}", stack_args.len()));
                return Ok(Some(stack_args));
            }
            self.debug_call_recovery("no_contiguous_reg_args");
            return Ok(None);
        }

        let mut args = recovered
            .into_iter()
            .take(contiguous_reg_count)
            .map(|expr| expr.expect("contiguous recovered reg arg"))
            .collect::<Vec<_>>();
        args.extend(stack_args);
        self.debug_call_recovery(&format!(
            "reg_args={} total_args={}",
            contiguous_reg_count,
            args.len()
        ));
        Ok(Some(args))
    }

    fn call_arg_carrier_assignments(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        abi: &AbiState,
    ) -> Vec<CarrierAssignment> {
        let same_block_carriers = block.ops[..call_idx]
            .iter()
            .filter_map(|op| op.output.as_ref())
            .collect::<Vec<_>>();
        let same_block = abi.assign_carriers(same_block_carriers.iter().copied());
        if !same_block.is_empty() {
            return same_block;
        }

        let block_idx = block.index as usize;
        let Some(pred_indices) = self.predecessors.get(block_idx) else {
            return same_block;
        };
        let mut predecessor_carriers = Vec::new();
        for pred_idx in pred_indices {
            let pred = self.pcode_block(*pred_idx);
            let end = self.block_terminator_index(pred).unwrap_or(pred.ops.len());
            predecessor_carriers.extend(pred.ops[..end].iter().filter_map(|op| op.output.as_ref()));
        }
        abi.assign_carriers(predecessor_carriers)
    }
}
