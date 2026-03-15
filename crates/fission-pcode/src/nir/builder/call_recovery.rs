use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn recover_call_args_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        call_idx: usize,
    ) -> Result<Option<Vec<HirExpr>>, MlilPreviewError> {
        if !self.options.is_64bit || call_idx == 0 {
            return Ok(None);
        }

        const WIN64_PARAM_REGS: &[(u64, u32)] = &[(0x08, 8), (0x10, 8), (0x80, 8), (0x88, 8)];
        let mut recovered: Vec<Option<HirExpr>> = vec![None; WIN64_PARAM_REGS.len()];

        for prev_idx in (0..call_idx).rev() {
            let prev = &block.ops[prev_idx];
            if prev.opcode.is_control_flow() {
                break;
            }
            let Some(output) = &prev.output else {
                continue;
            };
            if output.space_id != REGISTER_SPACE_ID {
                continue;
            }
            let Some((_, Some(param_index))) = register_name_with_param(output.offset, output.size)
            else {
                continue;
            };
            if param_index >= recovered.len() || recovered[param_index].is_some() {
                continue;
            }
            let expr = self
                .lower_varnode(output, &mut HashSet::new())
                .map_err(|err| {
                    self.debug_lowering_error(
                        "call_arg_recovery",
                        block.start_address,
                        u64::from(prev.seq_num),
                        prev.opcode,
                        &err,
                    );
                    err
                })?;
            recovered[param_index] = Some(expr);
        }

        let Some(highest_recovered) = recovered.iter().rposition(Option::is_some) else {
            return Ok(None);
        };

        for (param_index, (offset, size)) in WIN64_PARAM_REGS.iter().enumerate() {
            if param_index > highest_recovered || recovered[param_index].is_some() {
                continue;
            }
            let expr = self.lower_varnode(
                &Varnode {
                    space_id: REGISTER_SPACE_ID,
                    offset: *offset,
                    size: *size,
                    is_constant: false,
                    constant_val: 0,
                },
                &mut HashSet::new(),
            )?;
            recovered[param_index] = Some(expr);
        }

        if recovered[..=highest_recovered].iter().any(Option::is_none) {
            return Ok(None);
        }

        Ok(Some(
            recovered
                .into_iter()
                .take(highest_recovered + 1)
                .map(|expr| expr.expect("validated recovered call arg"))
                .collect(),
        ))
    }
}
