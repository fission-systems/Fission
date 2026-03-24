use super::*;

fn rewrite_loop_control_gotos_in_stmts(
    stmts: &mut [HirStmt],
    continue_label: Option<&str>,
    break_label: Option<&str>,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(label) => {
                if break_label.is_some_and(|target| label == target) {
                    *stmt = HirStmt::Break;
                    continue;
                }
                if continue_label.is_some_and(|target| label == target) {
                    *stmt = HirStmt::Continue;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                rewrite_loop_control_gotos_in_stmts(then_body, continue_label, break_label);
                rewrite_loop_control_gotos_in_stmts(else_body, continue_label, break_label);
            }
            HirStmt::Block(body) => {
                rewrite_loop_control_gotos_in_stmts(body, continue_label, break_label);
            }
            HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::Switch { .. }
            | HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_infloop(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if self.successors[idx].len() != 1 {
            return Ok(None);
        }
        let block = &self.pcode.blocks[idx];
        let block_addr = block.start_address;
        let terminator = self.lower_block_terminator(idx)?;
        let loops_to_self = matches!(
            terminator,
            LoweredTerminator::Goto(target) if target == block_addr
        ) || matches!(
            terminator,
            LoweredTerminator::Fallthrough(Some(target)) if target == block_addr
        );
        if !loops_to_self {
            return Ok(None);
        }

        let body = self.lower_block_stmts(block)?;
        let mut body = body;
        let continue_label = block_label(block_addr);
        rewrite_loop_control_gotos_in_stmts(&mut body, Some(&continue_label), None);
        Ok(Some((
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body,
            },
            idx + 1,
        )))
    }

    pub(super) fn try_lower_dowhile(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some((mut body, cond, cond_idx, skip_to)) = self.lower_do_while_region(idx)? else {
            return Ok(None);
        };
        let continue_label = block_label(self.block_target_key(cond_idx));
        let break_label = block_label(self.block_target_key(skip_to));
        rewrite_loop_control_gotos_in_stmts(&mut body, Some(&continue_label), Some(&break_label));
        Ok(Some((HirStmt::DoWhile { body, cond }, skip_to)))
    }

    pub(super) fn try_lower_while(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let block_addr = self.pcode.blocks[idx].start_address;
        let mut budget = IfLoweringBudget::new(self.options, idx, block_addr, "try_lower_while");
        if diag {
            eprintln!(
                "[DIAG] try_lower_while start: idx={} block=0x{:x} x86_guard={}",
                idx, block_addr, budget.enabled
            );
        }

        let result = (|| {
            if budget.checkpoint("terminator_pre") {
                return Ok(None);
            }
            let cond_block = &self.pcode.blocks[idx];
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(idx)?
            else {
                return Ok(None);
            };
            if budget.checkpoint("terminator_post") {
                return Ok(None);
            }

            if budget.checkpoint("cond_prefix_pre") {
                return Ok(None);
            }
            let cond_prefix = self.lower_block_stmts(cond_block)?;
            if budget.checkpoint("cond_prefix_post") {
                return Ok(None);
            }
            if !cond_prefix.iter().all(Self::is_trivial_structuring_stmt) {
                return Ok(None);
            }

            let Some(body_idx) = self.fallthrough_index(idx) else {
                return Ok(None);
            };
            let body_addr = self.block_target_key(body_idx);

            let (cond, exit_idx) = if false_target == Some(body_addr) {
                let exit_idx = self
                    .find_block_index_by_address(true_target)
                    .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
                (negate_expr(cond), exit_idx)
            } else if true_target == body_addr {
                let Some(exit_addr) = false_target else {
                    return Ok(None);
                };
                let exit_idx = self
                    .find_block_index_by_address(exit_addr)
                    .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;
                (cond, exit_idx)
            } else {
                return Ok(None);
            };

            if budget.checkpoint("body_pre") {
                return Ok(None);
            }
            let Some((body, loop_join_idx)) = self.lower_linear_body_with_budget(
                body_idx,
                LinearExit::Join(idx),
                Some(&mut budget),
            )?
            else {
                return Ok(None);
            };
            if budget.checkpoint("body_post") {
                return Ok(None);
            }
            if loop_join_idx != idx {
                return Ok(None);
            }
            let continue_label = block_label(self.block_target_key(idx));
            let break_label = block_label(self.block_target_key(exit_idx));
            let mut body = body;
            rewrite_loop_control_gotos_in_stmts(
                &mut body,
                Some(&continue_label),
                Some(&break_label),
            );
            if cond_prefix.is_empty() {
                return Ok(Some((HirStmt::While { cond, body }, exit_idx)));
            }

            let mut guarded_body = cond_prefix;
            guarded_body.push(HirStmt::If {
                cond: negate_expr(cond),
                then_body: vec![HirStmt::Break],
                else_body: Vec::new(),
            });
            guarded_body.extend(body);
            Ok(Some((
                HirStmt::While {
                    cond: HirExpr::Const(1, NirType::Bool),
                    body: guarded_body,
                },
                exit_idx,
            )))
        })();

        if diag {
            eprintln!(
                "[DIAG] try_lower_while done: idx={} block=0x{:x} elapsed={:.3}s success={} budget_tripped={}",
                idx,
                block_addr,
                budget.start.elapsed().as_secs_f64(),
                matches!(result, Ok(Some(_))),
                budget.tripped
            );
        }
        result
    }

    pub(super) fn lower_do_while_region(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<(Vec<HirStmt>, HirExpr, usize, usize)>, MlilPreviewError> {
        let mut idx = start_idx;
        let mut visited = HashSet::new();
        let mut body = Vec::new();

        loop {
            if !visited.insert(idx) {
                return Ok(None);
            }

            let block = &self.pcode.blocks[idx];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    if self.region_has_external_entry(&visited, start_idx) {
                        return Ok(None);
                    }
                    let start_addr = self.block_target_key(start_idx);
                    if true_target == start_addr {
                        let Some(exit_addr) = false_target else {
                            return Ok(None);
                        };
                        let exit_idx = self
                            .find_block_index_by_address(exit_addr)
                            .ok_or(MlilPreviewError::UnsupportedCfgPhiJoin)?;
                        return Ok(Some((body, cond, idx, exit_idx)));
                    }
                    if false_target == Some(start_addr) {
                        let exit_idx = self
                            .find_block_index_by_address(true_target)
                            .ok_or(MlilPreviewError::UnsupportedCfgPhiJoin)?;
                        return Ok(Some((body, negate_expr(cond), idx, exit_idx)));
                    }
                    return Ok(None);
                }
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    let Some(next_idx) = self.find_block_index_by_address(target) else {
                        return Ok(None);
                    };
                    if self.can_inline_linear_successor(idx, next_idx, &visited) {
                        idx = next_idx;
                        continue;
                    }
                    return Ok(None);
                }
                _ => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_loop_control_gotos_converts_break_and_continue_targets() {
        let mut body = vec![
            HirStmt::Goto("block_header".to_string()),
            HirStmt::Goto("block_exit".to_string()),
            HirStmt::If {
                cond: HirExpr::Const(1, NirType::Bool),
                then_body: vec![HirStmt::Goto("block_header".to_string())],
                else_body: vec![HirStmt::Goto("block_exit".to_string())],
            },
        ];

        rewrite_loop_control_gotos_in_stmts(&mut body, Some("block_header"), Some("block_exit"));

        assert!(matches!(body[0], HirStmt::Continue));
        assert!(matches!(body[1], HirStmt::Break));
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[2]
        else {
            panic!("expected if statement in rewritten loop body");
        };
        assert!(matches!(then_body.as_slice(), [HirStmt::Continue]));
        assert!(matches!(else_body.as_slice(), [HirStmt::Break]));
    }

    #[test]
    fn rewrite_loop_control_gotos_does_not_rewrite_inside_nested_loop_or_switch() {
        let mut body = vec![
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: vec![HirStmt::Goto("block_header".to_string())],
            },
            HirStmt::Switch {
                expr: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
                cases: vec![HirSwitchCase {
                    values: vec![1],
                    body: vec![HirStmt::Goto("block_exit".to_string())],
                }],
                default: vec![HirStmt::Goto("block_header".to_string())],
            },
        ];

        rewrite_loop_control_gotos_in_stmts(&mut body, Some("block_header"), Some("block_exit"));

        let HirStmt::While {
            body: nested_while_body,
            ..
        } = &body[0]
        else {
            panic!("expected nested while");
        };
        assert!(
            matches!(nested_while_body.as_slice(), [HirStmt::Goto(label)] if label == "block_header")
        );

        let HirStmt::Switch { cases, default, .. } = &body[1] else {
            panic!("expected switch statement");
        };
        assert!(
            matches!(cases[0].body.as_slice(), [HirStmt::Goto(label)] if label == "block_exit")
        );
        assert!(matches!(default.as_slice(), [HirStmt::Goto(label)] if label == "block_header"));
    }
}
