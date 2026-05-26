use std::collections::HashSet;

use super::*;

impl<'a> PreviewBuilder<'a> {
    /// Follows a linear chain of single-predecessor goto/fallthrough blocks
    /// starting at `start_idx`, staying within `[start_idx, follow_idx)`,
    /// and accepts the chain only if it terminates in a `Return`.
    ///
    /// Returns `Some((body_stmts, follow_idx))` on success or `None` if the
    /// chain exits the range, has multiple predecessors, or doesn't end in Return.
    fn try_lower_return_chain_arm(
        &mut self,
        start_idx: usize,
        follow_idx: usize,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        let mut body: Vec<HirStmt> = Vec::new();
        let mut visited: HashSet<usize> = HashSet::new();
        let mut idx = start_idx;
        loop {
            if idx >= follow_idx || !visited.insert(idx) {
                return Ok(None);
            }
            let block = self.pcode_block(idx).clone();
            body.extend(self.lower_block_stmts(&block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => {
                    body.push(HirStmt::Return(expr));
                    return Ok(Some((body, follow_idx)));
                }
                LoweredTerminator::Fallthrough(Some(target))
                | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if next_idx == follow_idx {
                        return Ok(None);
                    }
                    if next_idx >= follow_idx {
                        return Ok(None);
                    }
                    if !self.can_inline_linear_successor(idx, next_idx, &visited) {
                        return Ok(None);
                    }
                    idx = next_idx;
                }
                _ => return Ok(None),
            }
        }
    }

    pub(in crate::nir::structuring) fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let cond_block = self.pcode_block(idx).clone();
        let cond_prefix = self.lower_block_stmts(&cond_block)?;
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

        let (then_body, _) = match self.lower_linear_body(then_idx, exit)? {
            Some(result) => result,
            None => match self.try_lower_return_chain_arm(then_idx, follow_idx)? {
                Some(result) => result,
                None => return Ok(None),
            },
        };
        let (else_body, _) = match self.lower_linear_body(else_idx, exit)? {
            Some(result) => result,
            None => match self.try_lower_return_chain_arm(else_idx, follow_idx)? {
                Some(result) => result,
                None => return Ok(None),
            },
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
