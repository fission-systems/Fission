use super::*;

mod conditionals;
mod linear;
mod loops;
mod switch;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        if self.should_force_linear_structuring() {
            return self.build_linear_multiblock_body();
        }

        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_switch(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_dowhile(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_while(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_short_circuit_if(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_if_else(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_if(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }

            let block = &self.pcode.blocks[idx];
            if idx == 0 || targeted.contains(&block.start_address) {
                body.push(HirStmt::Label(block_label(block.start_address)));
            }
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
            idx += 1;
        }
        Ok(body)
    }

    fn should_force_linear_structuring(&self) -> bool {
        if self.pcode.blocks.len() > 80 {
            return true;
        }

        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self
            .predecessors
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0);

        self.pcode.blocks.len() > 32
            && (edge_count > self.pcode.blocks.len().saturating_mul(2)
                || multi_pred_blocks > 8
                || max_predecessors >= 4)
    }

    fn ignore_unsupported<T>(
        result: Result<Option<T>, MlilPreviewError>,
    ) -> Result<Option<T>, MlilPreviewError> {
        match result {
            Ok(result) => Ok(result),
            Err(MlilPreviewError::UnsupportedControlFlow)
            | Err(MlilPreviewError::UnsupportedCfgRegionShape)
            | Err(MlilPreviewError::UnsupportedCfgPhiJoin)
            | Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion)
            | Err(MlilPreviewError::UnsupportedCfgBranchTarget) => Ok(None),
            Err(err) => Err(err),
        }
    }
}
