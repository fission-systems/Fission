use super::*;
use std::collections::HashSet;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn abi_state(&self) -> AbiState {
        AbiState::new(
            self.options.calling_convention,
            self.options.is_64bit,
            self.options.pointer_size,
            self.stack_frame_size,
        )
    }

    fn surface_call_carrier_name(&mut self, vn: &Varnode) -> Option<String> {
        if let Some(param) = self.register_param(vn) {
            return Some(param);
        }
        if vn.space_id == UNIQUE_SPACE_ID {
            return unique_register_name(vn.offset, vn.size).map(str::to_string);
        }
        if is_register_varnode(vn) {
            return Some(register_name(vn.offset, vn.size).to_string());
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
            register_name(vn.offset, vn.size)
        } else if vn.space_id == UNIQUE_SPACE_ID {
            unique_register_name(vn.offset, vn.size).unwrap_or("")
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
        let abi = self.abi_state();
        if !self.options.is_64bit {
            return Ok(Vec::new());
        }

        let mut recovered = std::collections::BTreeMap::<usize, HirExpr>::new();
        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
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
                this.resolve_stack_address_from_memory_op(prev)
                    .or_else(|| this.resolve_stack_address(&prev.inputs[1]))
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

    fn recover_call_args_from_block_with_mode(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
        prefer_source_values: bool,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit && self.options.calling_convention != CallingConvention::Arm32 {
            return Ok(None);
        }

        let abi = self.abi_state();
        let param_slots = self.options.calling_convention.param_reg_slots();
        let param_count = param_slots.len();
        let mut recovered: Vec<Option<HirExpr>> = vec![None; param_count];

        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                if self.call_is_terminal_branchind_artifact(block, prev_idx) {
                    continue;
                }
                break;
            }
            let Some(output) = &prev.output else {
                continue;
            };
            let Some(param_index) = self.param_index_for_varnode(output, true) else {
                continue;
            };
            if param_index >= recovered.len() || recovered[param_index].is_some() {
                continue;
            }

            let source = if prefer_source_values {
                prev.inputs.first().unwrap_or(output)
            } else {
                output
            };
            let expr = if prefer_source_values && let Some(name) = self.surface_call_carrier_name(source) {
                HirExpr::Var(name)
            } else {
                match self.lower_varnode(source, &mut HashSet::new()) {
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
                }
            };
            recovered[param_index] = Some(self.normalize_recovered_call_arg(expr));
        }

        let assignments = self.call_arg_carrier_assignments(block, call_idx, &abi);
        for assignment in assignments {
            let param_index = assignment.resource.slot;
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
