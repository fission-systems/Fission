use super::*;
use std::time::Instant;

mod conditionals;
mod linear;
mod loops;
mod switch;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let total_start = Instant::now();
        if diag {
            eprintln!(
                "[DIAG] structuring start: blocks={} edges={} force_linear={}",
                self.pcode.blocks.len(),
                self.successors.iter().map(Vec::len).sum::<usize>(),
                self.should_force_linear_structuring()
            );
        }
        if self.should_force_linear_structuring() {
            let result = self.build_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success={}",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok()
                );
            }
            return result;
        }

        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if diag && idx > 0 && idx % 32 == 0 {
                eprintln!(
                    "[DIAG] structuring progress: idx={} elapsed={:.3}s",
                    idx,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=switch elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_switch(idx))? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=dowhile elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_dowhile(idx))? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=while elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_while(idx))? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=short_if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_short_circuit_if(idx))?
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if_else elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_if_else(idx))? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_if(idx))? {
                body.push(stmt);
                idx = skip_to;
                continue;
            }

            let block = &self.pcode.blocks[idx];
            if idx == 0 || targeted.contains(&block.start_address) {
                body.push(HirStmt::Label(block_label(block.start_address)));
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_stmts elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            body.extend(self.lower_block_stmts(block)?);
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_terminator elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
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
        if diag {
            eprintln!(
                "[DIAG] structuring done: elapsed={:.3}s stmts={}",
                total_start.elapsed().as_secs_f64(),
                body.len()
            );
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
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);

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

fn structuring_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}
