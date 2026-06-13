use super::super::cleanup::{finalize_structured_body, has_orphan_goto_labels, orphan_goto_labels};
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn find_block_index_by_label(&self, label: &str) -> Option<usize> {
        for idx in 0..self.block_count() {
            if block_label(self.block_target_key(idx)) == label {
                return Some(idx);
            }
        }
        None
    }

    fn emit_orphan_target_block(&mut self, block_idx: usize) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let label = block_label(self.block_target_key(block_idx));
        let mut stmts = vec![HirStmt::Label(label)];
        let block = self.pcode_block(block_idx).clone();
        stmts.extend(self.lower_block_stmts(&block)?);
        match self.lower_block_terminator(block_idx)? {
            LoweredTerminator::Return(expr) => stmts.push(HirStmt::Return(expr)),
            LoweredTerminator::Goto(target) => {
                if self.next_block_address(block_idx) != Some(target) {
                    stmts.push(HirStmt::Goto(block_label(target)));
                }
            }
            LoweredTerminator::Fallthrough(Some(target)) => {
                if let Some(target_idx) = self.find_block_index_by_address(target)
                    && let Some(expr) =
                        self.lower_return_join_expr_for_predecessor(block_idx, target_idx)?
                {
                    stmts.push(HirStmt::Return(Some(expr)));
                } else if self.next_block_address(block_idx) != Some(target) {
                    stmts.push(HirStmt::Goto(block_label(target)));
                }
            }
            LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } => {
                let then_body = if let Some(true_idx) = self.find_block_index_by_address(true_target)
                    && let Some(expr) =
                        self.lower_return_join_expr_for_predecessor(block_idx, true_idx)?
                {
                    vec![HirStmt::Return(Some(expr))]
                } else {
                    vec![HirStmt::Goto(block_label(true_target))]
                };
                let else_body = if let Some(false_target) = false_target {
                    if let Some(false_idx) = self.find_block_index_by_address(false_target)
                        && let Some(expr) =
                            self.lower_return_join_expr_for_predecessor(block_idx, false_idx)?
                    {
                        vec![HirStmt::Return(Some(expr))]
                    } else {
                        vec![HirStmt::Goto(block_label(false_target))]
                    }
                } else {
                    Vec::new()
                };
                stmts.push(HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                });
            }
            LoweredTerminator::Fallthrough(None) => {}
            LoweredTerminator::Unsupported {
                evidence,
                target_expr,
            } => {
                stmts.push(self.emit_unsupported_control_surface(evidence, target_expr));
            }
            LoweredTerminator::Switch { .. } => {
                return Err(MlilPreviewError::UnsupportedCfgRegionShape);
            }
        }
        Ok(stmts)
    }

    /// Ghidra `ruleBlockGoto` analog: keep structured SESE output and localize orphan goto
    /// targets by appending missing block labels/bodies instead of rebuilding the whole
    /// function as flat goto-linear.
    pub(super) fn try_repair_orphan_gotos(&mut self, body: Vec<HirStmt>) -> Option<Vec<HirStmt>> {
        if !has_orphan_goto_labels(&body) {
            return Some(body);
        }

        let mut body = body;
        for _ in 0..self.block_count().saturating_add(8) {
            let orphans = orphan_goto_labels(&body);
            if orphans.is_empty() {
                return Some(finalize_structured_body(body));
            }

            let mut repaired_any = false;
            for label in orphans {
                let Some(block_idx) = self.find_block_index_by_label(&label) else {
                    return None;
                };
                if let Ok(fragment) = self.emit_orphan_target_block(block_idx) {
                    body.extend(fragment);
                    repaired_any = true;
                } else {
                    return None;
                }
            }

            if !repaired_any {
                return None;
            }
            body = finalize_structured_body(body);
            if !has_orphan_goto_labels(&body) {
                return Some(body);
            }
        }

        if has_orphan_goto_labels(&body) {
            None
        } else {
            Some(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::cleanup::{has_orphan_goto_labels, orphan_goto_labels};
    use crate::PcodeFunction;
    use crate::nir::types::{HirStmt, MlilPreviewOptions, StructuringEngineKind};
    use crate::nir::PreviewBuilder;

    #[test]
    fn try_repair_orphan_gotos_returns_none_for_unknown_label() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_return_target: None,
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: Default::default(),
            is_data_ref_origin: false,
        };
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![HirStmt::Goto("block_deadbeef".to_string())];
        assert!(orphan_goto_labels(&body).contains(&"block_deadbeef".to_string()));
        assert!(builder.try_repair_orphan_gotos(body).is_none());
    }

    #[test]
    fn try_repair_orphan_gotos_noop_when_already_valid() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_return_target: None,
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: Default::default(),
            is_data_ref_origin: false,
        };
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![
            HirStmt::Label("block_100".to_string()),
            HirStmt::Return(None),
        ];
        assert!(!has_orphan_goto_labels(&body));
        let repaired = builder.try_repair_orphan_gotos(body.clone()).expect("noop repair");
        assert!(!has_orphan_goto_labels(&repaired));
    }
}
