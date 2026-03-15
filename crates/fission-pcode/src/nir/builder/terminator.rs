use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir) fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        let block = &self.pcode.blocks[idx];
        let Some(term_idx) = self.block_terminator_index(block) else {
            return Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx)));
        };
        let op = &block.ops[term_idx];
        self.with_lowering_site(
            LoweringSite {
                block_idx: idx,
                op_idx: term_idx,
            },
            |this| match op.opcode {
                PcodeOpcode::Return => {
                    let expr = op
                        .inputs
                        .last()
                        .map(|input| this.lower_wrapped_varnode(input, &mut HashSet::new()))
                        .transpose()?;
                    Ok(LoweredTerminator::Return(expr))
                }
                PcodeOpcode::Branch if op.inputs.len() == 1 => {
                    let Some(target) = op.inputs.first().and_then(branch_target_address) else {
                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                    };
                    Ok(LoweredTerminator::Goto(target))
                }
                PcodeOpcode::CBranch | PcodeOpcode::Branch if op.inputs.len() >= 2 => {
                    if op.inputs.len() < 2 {
                        return Err(MlilPreviewError::UnsupportedExprVarnodeLowering);
                    }
                    let Some(true_target) = branch_target_address(&op.inputs[0]) else {
                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                    };
                    let cond = this
                        .lower_wrapped_varnode(&op.inputs[1], &mut HashSet::new())
                        .map_err(|err| {
                            this.debug_lowering_error(
                                "terminator_cond",
                                block.start_address,
                                u64::from(op.seq_num),
                                op.opcode,
                                &err,
                            );
                            err
                        })?;
                    Ok(LoweredTerminator::Cond {
                        cond,
                        true_target,
                        false_target: this.next_block_address(idx),
                    })
                }
                PcodeOpcode::BranchInd => Ok(LoweredTerminator::Unsupported),
                _ => Ok(LoweredTerminator::Fallthrough(this.next_block_address(idx))),
            },
        )
    }

    fn lower_wrapped_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match self.lower_varnode(vn, visiting) {
            Ok(expr) => Ok(expr),
            Err(err) => {
                let Some((_, op)) = self.lookup_def_site(vn) else {
                    return Err(err);
                };
                match op.opcode {
                    PcodeOpcode::Copy
                    | PcodeOpcode::Cast
                    | PcodeOpcode::IntZExt
                    | PcodeOpcode::IntSExt
                        if op.inputs.len() == 1 =>
                    {
                        self.lower_wrapped_varnode(&op.inputs[0], visiting)
                    }
                    PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                        if const_offset(&op.inputs[0]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[1], visiting)
                        } else if const_offset(&op.inputs[1]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[0], visiting)
                        } else {
                            Err(err)
                        }
                    }
                    _ => Err(err),
                }
            }
        }
    }
}
