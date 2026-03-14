use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir) fn lower_call(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let target = if let Some(target) = op.inputs.first() {
            match self.lower_varnode(target, visiting)? {
                HirExpr::Const(val, _) => self
                    .type_context
                    .and_then(|ctx| ctx.call_targets.get(&(val as u64)).cloned())
                    .unwrap_or_else(|| format!("sub_{:x}", val as u64)),
                HirExpr::Var(name) => name,
                other => print_expr(&other),
            }
        } else {
            "callee".to_string()
        };
        let args = op
            .inputs
            .iter()
            .skip(1)
            .map(|input| self.lower_varnode(input, &mut HashSet::new()))
            .collect::<Result<Vec<_>, _>>()?;
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

        if vn.space_id == REGISTER_SPACE_ID {
            return Ok(HirExpr::Var(register_name(vn.offset, vn.size).to_string()));
        }

        let key = VarnodeKey::from(vn);
        if let Some(name) = self.materialized_vns.get(&key) {
            return Ok(HirExpr::Var(name.clone()));
        }
        if !visiting.insert(key.clone()) {
            return Ok(HirExpr::Var(format!("tmp_{:x}", vn.offset)));
        }

        let result = match self.defs.get(&key).copied() {
            Some(op) => self.lower_def_op(op, visiting),
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

    pub(in crate::nir) fn lower_def_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match op.opcode {
            PcodeOpcode::Copy => self.lower_varnode(&op.inputs[0], visiting),
            PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                let expr = self.lower_varnode(&op.inputs[0], visiting)?;
                Ok(HirExpr::Cast {
                    ty: type_from_size(output.size, matches!(op.opcode, PcodeOpcode::IntSExt)),
                    expr: Box::new(expr),
                })
            }
            PcodeOpcode::Load => {
                if op.inputs.len() < 2 {
                    return Err(MlilPreviewError::LoweringFailed);
                }
                let out = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                if let Some((slot_name, _)) =
                    self.try_stack_slot_lvalue(&op.inputs[1], type_from_size(out.size, false))
                {
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
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                let ty = type_from_size(output.size, false);
                let op = match op.opcode {
                    PcodeOpcode::IntNegate => HirUnaryOp::BitNot,
                    PcodeOpcode::BoolNegate => HirUnaryOp::Not,
                    PcodeOpcode::Int2Comp => HirUnaryOp::Neg,
                    _ => return Err(MlilPreviewError::LoweringFailed),
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
                let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
                self.lower_intrinsic_call(
                    op,
                    visiting,
                    "__popcount",
                    type_from_size(output.size, false),
                )
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                self.lower_call(op, visiting)
            }
            PcodeOpcode::Piece => self.lower_piece_op(op, visiting),
            PcodeOpcode::SubPiece => self.lower_subpiece_op(op, visiting),
            PcodeOpcode::MultiEqual => self.lower_multiequal(op, visiting),
            PcodeOpcode::Indirect => {
                if let Some(input) = op.inputs.first() {
                    self.lower_varnode(input, visiting)
                } else {
                    Err(MlilPreviewError::LoweringFailed)
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
            return Err(MlilPreviewError::LoweringFailed);
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        let output = op.output.as_ref().ok_or(MlilPreviewError::LoweringFailed)?;
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
