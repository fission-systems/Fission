use super::*;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PopOutcome {
    Success(HirExpr),
    Solid,
    FailedDef,
}

impl<'a> PreviewBuilder<'a> {
    fn check_ancestor_realistic(
        &self,
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
                if op.opcode.is_call() {
                    return false;
                }
            }
        }

        true
    }

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
        let mut outcomes: Vec<PopOutcome> = vec![PopOutcome::FailedDef; WIN64_PARAM_REGS.len()];

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

            let expr = match self.lower_varnode(output, &mut HashSet::new()) {
                Ok(expr) => expr,
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
            outcomes[param_index] = PopOutcome::Success(expr);
        }

        let Some(highest_recovered) = recovered.iter().rposition(Option::is_some) else {
            return Ok(None);
        };

        for (param_index, (offset, size)) in WIN64_PARAM_REGS.iter().enumerate() {
            if param_index > highest_recovered || recovered[param_index].is_some() {
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
                if self.check_ancestor_realistic(def_site, block.index as usize, call_idx) {
                    recovered[param_index] = Some(self.lower_varnode(&vn, &mut HashSet::new())?);
                    outcomes[param_index] = PopOutcome::Solid;
                    continue;
                }
            }

            return Ok(None);
        }

        Ok(Some(
            recovered
                .into_iter()
                .take(highest_recovered + 1)
                .map(|expr| expr.unwrap())
                .collect(),
        ))
    }
}
