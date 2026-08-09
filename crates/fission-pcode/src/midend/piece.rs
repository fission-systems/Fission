use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn lower_piece_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::UnsupportedExprPieceShape);
        }
        let output = op
            .output
            .as_ref()
            .ok_or(MlilPreviewError::UnsupportedExprPieceShape)?;
        let output_ty = type_from_size(output.size, false);
        if let Some(expr) = self.try_recombine_piece(op, &output_ty, visiting)? {
            return Ok(expr);
        }
        let lhs = self.lower_varnode(&op.inputs[0], visiting)?;
        let rhs = self.lower_varnode(&op.inputs[1], visiting)?;
        let shift_bits = i64::from(op.inputs[1].size) * 8;
        let shifted = PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs: Box::new(PreHirExpr::Cast {
                ty: output_ty.clone(),
                expr: Box::new(lhs),
            }),
            rhs: Box::new(PreHirExpr::Const(
                shift_bits,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: output_ty.clone(),
        };
        Ok(PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs: Box::new(shifted),
            rhs: Box::new(PreHirExpr::Cast {
                ty: output_ty.clone(),
                expr: Box::new(rhs),
            }),
            ty: output_ty,
        })
    }

    fn try_recombine_piece(
        &mut self,
        op: &PcodeOp,
        _output_ty: &NirType,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Ok(None);
        }
        let Some(lhs_origin) = self.extract_subpiece_origin(&op.inputs[0]) else {
            return Ok(None);
        };
        let Some(rhs_origin) = self.extract_subpiece_origin(&op.inputs[1]) else {
            return Ok(None);
        };
        if lhs_origin.base != rhs_origin.base {
            return Ok(None);
        }
        if rhs_origin.byte_offset != 0 {
            return Ok(None);
        }
        if lhs_origin.byte_offset != i64::from(rhs_origin.piece_size) {
            return Ok(None);
        }
        if lhs_origin.base_size != op.output.as_ref().map(|out| out.size).unwrap_or(0) {
            return Ok(None);
        }
        let base_expr = self.lower_varnode(&lhs_origin.base_vn, visiting)?;
        Ok(Some(base_expr))
    }

    pub(super) fn lower_subpiece_op(
        &mut self,
        op: &PcodeOp,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if op.inputs.len() < 2 {
            return Err(MlilPreviewError::UnsupportedExprPieceShape);
        }
        let output = op
            .output
            .as_ref()
            .ok_or(MlilPreviewError::UnsupportedExprPieceShape)?;
        let base = self.lower_varnode(&op.inputs[0], visiting)?;
        let base_expr_signed = matches!(expr_type(&base), NirType::Int { signed: true, .. });
        let base_def_signed = self.lookup_def_site(&op.inputs[0]).is_some_and(|(_, def)| {
            matches!(
                def.opcode,
                PcodeOpcode::IntSExt
                    | PcodeOpcode::IntSRight
                    | PcodeOpcode::IntSDiv
                    | PcodeOpcode::IntSRem
            ) || matches!(
                pcode_output_type_from_size(def.opcode, op.inputs[0].size),
                NirType::Int { signed: true, .. }
            )
        });
        let output_signed = base_expr_signed || base_def_signed;
        let output_ty = type_from_size(output.size, output_signed);
        let byte_offset =
            const_offset(&op.inputs[1]).ok_or(MlilPreviewError::UnsupportedExprPieceShape)?;
        if let Some(lane) =
            self.lower_register_subpiece_lane(&op.inputs[0], output, byte_offset, visiting)?
        {
            return Ok(PreHirExpr::Cast {
                ty: output_ty,
                expr: Box::new(lane),
            });
        }
        let shifted = if byte_offset == 0 {
            base
        } else {
            // High half of a signed base (e.g. SubPiece(IntSExt(L), |L|) CDQ)
            // is arithmetic sign-fill — use Sar. Logical Shr would mis-model
            // negative lows and block CDQ residual matching.
            let shift_op = if output_signed {
                PreHirBinaryOp::Sar
            } else {
                PreHirBinaryOp::Shr
            };
            PreHirExpr::Binary {
                op: shift_op,
                lhs: Box::new(base),
                rhs: Box::new(PreHirExpr::Const(
                    byte_offset * 8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: type_from_size(op.inputs[0].size, output_signed),
            }
        };
        Ok(PreHirExpr::Cast {
            ty: output_ty,
            expr: Box::new(shifted),
        })
    }

    fn lower_register_subpiece_lane(
        &mut self,
        wide: &Varnode,
        output: &Varnode,
        byte_offset: i64,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if !is_register_space_id(wide.space_id) || wide.size <= output.size || byte_offset < 0 {
            return Ok(None);
        }
        let byte_offset =
            u64::try_from(byte_offset).map_err(|_| MlilPreviewError::UnsupportedExprPieceShape)?;
        let output_size = u64::from(output.size);
        let wide_size = u64::from(wide.size);
        let Some(lane_end_from_lsb) = byte_offset.checked_add(output_size) else {
            return Ok(None);
        };
        if lane_end_from_lsb > wide_size {
            return Ok(None);
        }
        let lane_storage_offset = if self.options.is_big_endian {
            wide_size - lane_end_from_lsb
        } else {
            byte_offset
        };
        let requested = Varnode {
            space_id: wide.space_id,
            offset: wide.offset.saturating_add(lane_storage_offset),
            size: output.size,
            is_constant: false,
            constant_val: 0,
        };
        let Some(scope) = self.current_lowering_site else {
            return Ok(None);
        };
        let Some(block) = self.pcode.blocks.get(scope.block_idx) else {
            return Ok(None);
        };
        let requested_start = requested.offset;
        let requested_end = requested_start.saturating_add(output_size);
        let Some((def_idx, def)) = block
            .ops
            .iter()
            .enumerate()
            .take(scope.op_idx.min(block.ops.len()))
            .rev()
            .find(|(_, candidate)| {
                candidate.output.as_ref().is_some_and(|candidate_output| {
                    if candidate_output.is_constant
                        || candidate_output.space_id != requested.space_id
                    {
                        return false;
                    }
                    let candidate_start = candidate_output.offset;
                    let candidate_end =
                        candidate_start.saturating_add(u64::from(candidate_output.size));
                    candidate_start < requested_end && requested_start < candidate_end
                })
            })
            .map(|(idx, def)| (idx, def.clone()))
        else {
            return Ok(None);
        };
        let def_output = def
            .output
            .as_ref()
            .ok_or(MlilPreviewError::UnsupportedExprPieceShape)?;
        let def_site = LoweringSite {
            block_idx: scope.block_idx,
            op_idx: def_idx,
        };

        if VarnodeKey::from(def_output) == VarnodeKey::from(&requested) {
            let expr =
                self.with_lowering_site(def_site, |this| this.lower_def_op(&def, visiting))?;
            return Ok(Some(expr));
        }

        self.lower_covering_passthrough_register_lane(&requested, def_site, &def, visiting)
    }

    fn extract_subpiece_origin(&self, vn: &Varnode) -> Option<SubpieceOrigin> {
        self.extract_subpiece_origin_inner(vn, &mut HashSet::default())
    }

    fn extract_subpiece_origin_inner(
        &self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<SubpieceOrigin> {
        let key = VarnodeKey::from(vn);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let result = match self.lookup_def_site(vn).map(|(_, op)| op) {
            Some(op)
                if matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                ) && op.inputs.len() == 1
                    && op.inputs[0].size == vn.size =>
            {
                self.extract_subpiece_origin_inner(&op.inputs[0], visiting)
            }
            Some(op) if op.opcode == PcodeOpcode::SubPiece && op.inputs.len() >= 2 => {
                let base_vn = op.inputs[0].clone();
                Some(SubpieceOrigin {
                    base: VarnodeKey::from(&base_vn),
                    base_vn,
                    base_size: op.inputs[0].size,
                    byte_offset: const_offset(&op.inputs[1])?,
                    piece_size: vn.size,
                })
            }
            None => Some(SubpieceOrigin {
                base: VarnodeKey::from(vn),
                base_vn: vn.clone(),
                base_size: vn.size,
                byte_offset: 0,
                piece_size: vn.size,
            }),
            _ => None,
        };
        visiting.remove(&key);
        result
    }
}
