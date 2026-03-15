use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir) fn lookup_def_site(
        &self,
        vn: &Varnode,
    ) -> Option<(LoweringSite, &'a PcodeOp)> {
        if let Some(site) = self.current_lowering_site {
            let block = &self.pcode.blocks[site.block_idx];
            if let Some((def_idx, op)) =
                aggregate_recovery::find_prior_def_in_block(block, site.op_idx, vn)
            {
                return Some((
                    LoweringSite {
                        block_idx: site.block_idx,
                        op_idx: def_idx,
                    },
                    op,
                ));
            }
        }

        let key = VarnodeKey::from(vn);
        self.defs.get(&key).map(|def| {
            (
                LoweringSite {
                    block_idx: def.block_idx,
                    op_idx: def.op_idx,
                },
                def.op,
            )
        })
    }

    pub(in crate::nir) fn lower_call(
        &mut self,
        op: &PcodeOp,
        recovered_args: Option<Vec<HirExpr>>,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let target = if let Some(target) = self.resolve_call_target_from_asm(op) {
            target
        } else if let Some(target) = op.inputs.first() {
            match self.lower_varnode(target, visiting) {
                Ok(HirExpr::Const(val, _)) => self
                    .type_context
                    .and_then(|ctx| ctx.call_targets.get(&(val as u64)).cloned())
                    .unwrap_or_else(|| format!("sub_{:x}", val as u64)),
                Ok(HirExpr::Var(name)) => name,
                Ok(other) => print_expr(&other),
                Err(MlilPreviewError::UnsupportedPattern("opcode"))
                    if matches!(op.opcode, PcodeOpcode::CallInd) =>
                {
                    if let Some(target) = self.recover_opaque_callind_target(target) {
                        target
                    } else {
                        return Err(MlilPreviewError::UnsupportedPattern("opcode"));
                    }
                }
                Err(err) => return Err(err),
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
                .map(|input| self.lower_varnode(input, &mut HashSet::new()))
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

    fn resolve_call_target_from_asm(&self, op: &PcodeOp) -> Option<String> {
        let asm = op.asm_mnemonic.as_deref()?;
        let addr = parse_call_target_address(asm)?;
        if let Some(name) = self
            .type_context
            .and_then(|ctx| ctx.call_targets.get(&addr))
            .cloned()
        {
            return Some(name);
        }
        if matches!(op.opcode, PcodeOpcode::Call) {
            return Some(format!("sub_{addr:x}"));
        }
        None
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
        if vn.is_constant {
            return Ok(HirExpr::Const(
                vn.constant_val,
                type_from_size(vn.size, false),
            ));
        }

        if let Some(param) = self.register_param(vn) {
            return Ok(HirExpr::Var(param));
        }

        if !self.options.is_64bit
            && vn.space_id == REGISTER_SPACE_ID
            && let Some(name) = x86_register_name(vn.offset, vn.size)
        {
            return Ok(HirExpr::Var(name.to_string()));
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

        if vn.space_id == REGISTER_SPACE_ID {
            return Ok(HirExpr::Var(register_name(vn.offset, vn.size).to_string()));
        }

        let key = VarnodeKey::from(vn);
        let def_site = self.lookup_def_site(vn);
        if let Some((_, op)) = def_site {
            let materialized_key = MaterializedVarnodeKey::new(vn, op);
            if let Some(name) = self.materialized_vns.get(&materialized_key) {
                return Ok(HirExpr::Var(name.clone()));
            }
        }
        if !visiting.insert(key.clone()) {
            return Ok(HirExpr::Var(format!("tmp_{:x}", vn.offset)));
        }

        let result = match def_site {
            Some((site, op)) => self
                .with_lowering_site(site, |this| this.lower_def_op(op, visiting))
                .map_err(|err| self.classify_varnode_lowering_error(op, err)),
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
            _ => Err(MlilPreviewError::UnsupportedPattern("opcode")),
        }
    }

    pub(in crate::nir) fn lower_multiequal(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let mut lowered = Vec::new();
        for input in &op.inputs {
            lowered.push(self.lower_varnode(input, visiting)?);
        }
        if let Some(first) = lowered.first() {
            let canonical = strip_casts(first);
            if lowered.iter().all(|expr| strip_casts(expr) == canonical) {
                return Ok(first.clone());
            }
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

fn parse_call_target_address(asm: &str) -> Option<u64> {
    let start = asm.find("0x")?;
    let hex = asm[start + 2..]
        .chars()
        .take_while(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    if hex.is_empty() {
        return None;
    }
    u64::from_str_radix(&hex, 16).ok()
}
