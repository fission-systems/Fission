use super::*;

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
    pub(in crate::midend) fn lower_call(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<PreHirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let created_trace = if self.active_trace_id.is_none() {
            let trace_id = self.next_trace_id();
            self.active_trace_id = Some(trace_id);
            self.varnode_lowering_work = 0;
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
        recovered_args: Option<Vec<PreHirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
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
                    Ok(PreHirExpr::Const(val, _)) => {
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
                    Ok(PreHirExpr::Var(name)) if matches!(op.opcode, PcodeOpcode::CallInd) => {
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
                    Ok(PreHirExpr::Var(name)) => self
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
                        } else if matches!(other, PreHirExpr::Load { .. }) {
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
                    Ok(other) => print_prehir_expr(&other),
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
        let ty = self
            .current_lowering_site
            .and_then(|site| self.call_result_types.get(&site).cloned())
            .or_else(|| {
                op.output
                    .as_ref()
                    .map(|out| type_from_size(out.size, false))
            })
            .unwrap_or(NirType::Unknown);
        Ok(PreHirExpr::Call { target, args, ty })
    }

    /// True when a recovered call argument expression is the same surface as
    /// the CallInd target (function pointer used as both target and "arg").
    fn call_arg_is_callind_target_carrier(arg: &PreHirExpr, target: &PreHirExpr) -> bool {
        match (arg, target) {
            (PreHirExpr::Var(a), PreHirExpr::Var(b)) => a == b,
            (PreHirExpr::Cast { expr: a_inner, .. }, _) => {
                Self::call_arg_is_callind_target_carrier(a_inner, target)
            }
            (_, PreHirExpr::Cast { expr: t_inner, .. }) => {
                Self::call_arg_is_callind_target_carrier(arg, t_inner)
            }
            _ => false,
        }
    }

    fn lower_callother(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<PreHirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
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
        Ok(PreHirExpr::Call {
            target,
            args,
            ty: op
                .output
                .as_ref()
                .map(|out| type_from_size(out.size, false))
                .unwrap_or(NirType::Unknown),
        })
    }

    pub(in crate::midend) fn lower_intrinsic_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
        target: &str,
        ty: NirType,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let args = op
            .inputs
            .iter()
            .map(|input| self.lower_varnode(input, visiting))
            .collect::<Result<Vec<_>, _>>()?;
        // fabs/sqrt/ceil/floor/round/__isnan reach here rather than the unary
        // arm, and their operand metatype is just as real.
        let refs: Vec<&PreHirExpr> = args.iter().collect();
        self.note_operand_metatypes(op.opcode, &refs);
        Ok(PreHirExpr::Call {
            target: target.to_string(),
            args,
            ty,
        })
    }
}
