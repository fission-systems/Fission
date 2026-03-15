use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::structuring) fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_prefix = self.lower_block_stmts(&self.pcode.blocks[idx])?;
        if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
            return Ok(None);
        }
        if idx + 2 >= self.pcode.blocks.len() {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target: Some(false_target),
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };

        let Some(next_idx) = self.fallthrough_index(idx) else {
            return Ok(None);
        };
        let next_addr = self.pcode.blocks[next_idx].start_address;

        let (cond, then_idx, else_idx) = if true_target == next_addr {
            let Some(else_idx) = self.forward_join_idx_from_address(idx, false_target) else {
                return Ok(None);
            };
            (cond, next_idx, else_idx)
        } else if false_target == next_addr {
            let Some(then_idx) = self.forward_join_idx_from_address(idx, true_target) else {
                return Ok(None);
            };
            (negate_expr(cond), then_idx, next_idx)
        } else {
            return Ok(None);
        };

        let Some(exit) = self.shared_forward_linear_exit(idx, then_idx, else_idx)? else {
            return Ok(None);
        };
        let Some((then_body, then_skip)) = self.lower_linear_body(then_idx, exit)? else {
            return Ok(None);
        };
        let Some((else_body, else_skip)) = self.lower_linear_body(else_idx, exit)? else {
            return Ok(None);
        };
        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
        };
        let stmt = HirStmt::If {
            cond,
            then_body,
            else_body,
        };
        if cond_prefix.is_empty() {
            Ok(Some((stmt, skip_to)))
        } else {
            let mut wrapped = cond_prefix;
            wrapped.push(stmt);
            Ok(Some((HirStmt::Block(wrapped), skip_to)))
        }
    }
}
