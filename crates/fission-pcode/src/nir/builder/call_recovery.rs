use super::*;
use std::collections::HashSet;

impl<'a> PreviewBuilder<'a> {
    fn surface_call_carrier_name(&mut self, vn: &Varnode) -> Option<String> {
        if let Some(param) = self.register_param(vn) {
            return Some(param);
        }
        if vn.space_id == UNIQUE_SPACE_ID {
            return crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
                .map(str::to_string);
        }
        if vn.space_id == REGISTER_SPACE_ID {
            return Some(register_name(vn.offset, vn.size).to_string());
        }
        None
    }

    fn param_index_for_varnode(&self, vn: &Varnode) -> Option<usize> {
        if vn.space_id == REGISTER_SPACE_ID {
            return register_name_with_param(vn.offset, vn.size, self.options.calling_convention)
                .and_then(|(_, index)| index);
        }
        if vn.space_id == UNIQUE_SPACE_ID
            && let Some(name) = crate::arch::x86::unique_x86_register_name(vn.offset, vn.size)
        {
            return self
                .options
                .calling_convention
                .param_offsets()
                .iter()
                .position(|&off| x64_ghidra_reg_name(off).is_some_and(|hw| hw.eq_ignore_ascii_case(name)));
        }
        None
    }

    fn debug_call_recovery(&self, message: &str) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=call_arg_recovery {message}");
        }
    }

    fn is_callee_saved_register_varnode(&self, vn: &Varnode) -> bool {
        let reg_name = if vn.space_id == REGISTER_SPACE_ID {
            register_name(vn.offset, vn.size)
        } else if vn.space_id == UNIQUE_SPACE_ID {
            crate::arch::x86::unique_x86_register_name(vn.offset, vn.size).unwrap_or("")
        } else {
            ""
        };
        matches!(
            reg_name,
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

    fn recover_call_stack_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Vec<HirExpr>, MlilPreviewError> {
        if !self.options.is_64bit
            || self.options.calling_convention != CallingConvention::WindowsX64
        {
            return Ok(Vec::new());
        }

        let mut recovered = std::collections::BTreeMap::<usize, HirExpr>::new();
        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                break;
            }
            if prev.opcode != PcodeOpcode::Store || prev.inputs.len() < 3 {
                continue;
            }
            let Some((StackBase::Rsp, offset)) = self
                .resolve_stack_address_from_memory_op(prev)
                .or_else(|| self.resolve_stack_address(&prev.inputs[1]))
            else {
                continue;
            };
            if offset < 0x20 || (offset - 0x20) % i64::from(self.options.pointer_size) != 0 {
                continue;
            }
            let stack_index = ((offset - 0x20) / i64::from(self.options.pointer_size)) as usize;
            if recovered.contains_key(&stack_index) {
                continue;
            }
            let value = self.lower_varnode(prev.inputs.last().expect("store rhs"), &mut HashSet::new())?;
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

    pub(in crate::nir::builder) fn recover_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit || call_idx == 0 {
            return Ok(None);
        }

        let param_regs = self.options.calling_convention.param_reg_slots_64();
        let mut recovered: Vec<Option<HirExpr>> = vec![None; param_regs.len()];

        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                break;
            }
            let Some(output) = &prev.output else {
                continue;
            };
            let Some(param_index) = self.param_index_for_varnode(output) else {
                continue;
            };
            if param_index >= recovered.len() || recovered[param_index].is_some() {
                continue;
            }

            let expr = match self.lower_varnode(output, &mut HashSet::new()) {
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
                        block.start_address,
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
                            Some(block.start_address),
                            Some(u64::from(prev.seq_num)),
                            false,
                            "call_arg_recovery_lowering_failed",
                        );
                    }
                    continue;
                }
            };
            recovered[param_index] = Some(expr.clone());
        }

        for (param_index, (offset, size)) in param_regs.iter().enumerate() {
            if recovered[param_index].is_some() {
                continue;
            }

            let vn = Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: *offset,
                size: *size,
                is_constant: false,
                constant_val: 0,
            };

            let key = VarnodeKey::from(&vn);
            if let Some(def_site) = self.defs.get(&key) {
                if self.check_ancestor_realistic(&vn, def_site, block.index as usize, call_idx) {
                    recovered[param_index] = Some(self.lower_varnode(&vn, &mut HashSet::new())?);
                    continue;
                }
            }
        }

        let contiguous_reg_count = recovered.iter().take_while(|expr| expr.is_some()).count();
        if contiguous_reg_count == 0 {
            self.debug_call_recovery("no_contiguous_reg_args");
            return Ok(None);
        }

        let mut args = recovered
            .into_iter()
            .take(contiguous_reg_count)
            .map(|expr| expr.expect("contiguous recovered reg arg"))
            .collect::<Vec<_>>();
        let stack_args = self.recover_call_stack_args_from_block(block, call_idx)?;
        args.extend(stack_args);
        self.debug_call_recovery(&format!("reg_args={} total_args={}", contiguous_reg_count, args.len()));
        Ok(Some(args))
    }
}
