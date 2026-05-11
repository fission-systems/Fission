use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::structuring) fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_block = self.pcode_block(idx).clone();
        let cond_prefix = self.lower_block_stmts(&cond_block)?;
        if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
            return Ok(None);
        }
        if idx + 2 >= self.block_count() {
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
        let next_addr = self.block_target_key(next_idx);

        let (cond, then_idx, else_idx) = if true_target == next_addr {
            let Some(else_idx) = self.forward_join_idx_from_address(idx, false_target) else {
                return Ok(None);
            };
            (cond, next_idx, else_idx)
        } else if false_target == next_addr {
            let Some(then_idx) = self.forward_join_idx_from_address(idx, true_target) else {
                return Ok(None);
            };
            (negate_expr(cond), next_idx, then_idx)
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

    /// Postdominance-guided if-then-else structuring.
    ///
    /// Unlike `try_lower_if_else`, this variant uses the precomputed
    /// `follow_block` (nearest common postdominator of the two branch arms)
    /// as the authoritative join point, bypassing `shared_forward_linear_exit`.
    ///
    /// This handles cases where `shared_linear_exit` fails because one or both
    /// arms do not form a simple linear chain to the follow block (e.g., they
    /// contain nested conditionals that were already structured).
    pub(in crate::nir::structuring) fn try_reduce_if_else_with_follow(
        &mut self,
        idx: usize,
        follow: Option<usize>,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some(follow_idx) = follow else {
            return Ok(None);
        };
        // follow_idx must be strictly after idx (forward edge) and reachable.
        if follow_idx <= idx || follow_idx >= self.block_count() {
            return Ok(None);
        }

        let cond_block = self.pcode_block(idx).clone();
        let cond_prefix = self.lower_block_stmts(&cond_block)?;
        if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
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
        let next_addr = self.block_target_key(next_idx);

        let (cond, then_idx, else_idx) = if true_target == next_addr {
            let Some(else_idx) = self.forward_join_idx_from_address(idx, false_target) else {
                return Ok(None);
            };
            (cond, next_idx, else_idx)
        } else if false_target == next_addr {
            let Some(then_idx) = self.forward_join_idx_from_address(idx, true_target) else {
                return Ok(None);
            };
            (negate_expr(cond), next_idx, then_idx)
        } else {
            return Ok(None);
        };

        // Use the postdom-computed follow block as the exit.
        let exit = LinearExit::Join(follow_idx);

        // Both arms must be within [idx+1, follow_idx).
        if then_idx >= follow_idx || else_idx >= follow_idx {
            return Ok(None);
        }

        let Some((then_body, _)) = self.lower_linear_body(then_idx, exit)? else {
            return Ok(None);
        };
        let Some((else_body, _)) = self.lower_linear_body(else_idx, exit)? else {
            return Ok(None);
        };

        let stmt = HirStmt::If {
            cond,
            then_body,
            else_body,
        };
        if cond_prefix.is_empty() {
            Ok(Some((stmt, follow_idx)))
        } else {
            let mut wrapped = cond_prefix;
            wrapped.push(stmt);
            Ok(Some((HirStmt::Block(wrapped), follow_idx)))
        }
    }
}
