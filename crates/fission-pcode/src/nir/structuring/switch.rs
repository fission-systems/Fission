use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_switch(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let Some(parsed) = self.parse_switch_chain(idx)? else {
            return Ok(None);
        };
        if parsed.cases.len() < 2 {
            return Ok(None);
        }
        let mut seen_case_values = HashSet::new();
        if !parsed
            .cases
            .iter()
            .all(|(value, _)| seen_case_values.insert(*value))
        {
            return Ok(None);
        }

        let mut exits = parsed
            .cases
            .iter()
            .map(|(_, block_idx)| *block_idx)
            .collect::<Vec<_>>();
        exits.push(parsed.default_idx);
        let Some(exit) = self.shared_exit_for_indices(&exits)? else {
            return Ok(None);
        };

        let mut cases = Vec::new();
        let mut max_skip = 0usize;
        for (value, case_idx) in parsed.cases {
            let Some((case_body, skip_to)) = self.lower_linear_body(case_idx, exit)? else {
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to);
            cases.push(HirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        merge_adjacent_switch_cases(&mut cases);
        let Some((default_body, default_skip)) =
            self.lower_linear_body(parsed.default_idx, exit)?
        else {
            return Ok(None);
        };
        max_skip = max_skip.max(default_skip);

        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => max_skip,
        };
        Ok(Some((
            HirStmt::Switch {
                expr: parsed.selector,
                cases,
                default: default_body,
            },
            skip_to,
        )))
    }

    fn parse_switch_chain(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
        let mut current_idx = start_idx;
        let mut selector: Option<HirExpr> = None;
        let mut cases = Vec::new();

        loop {
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(current_idx)?
            else {
                return Ok(None);
            };
            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let next_addr = self.pcode.blocks[next_idx].start_address;
            let (case_target, case_on_true) = if false_target == Some(next_addr) {
                (true_target, true)
            } else if true_target == next_addr {
                let Some(false_target) = false_target else {
                    return Ok(None);
                };
                (false_target, false)
            } else {
                return Ok(None);
            };
            let Some(case_idx) = self.find_block_index_by_address(case_target) else {
                return Ok(None);
            };
            let Some((case_selector, value)) = extract_eq_const_for_case(&cond, case_on_true)
            else {
                return Ok(None);
            };
            if let Some(existing) = &selector {
                if strip_casts(existing) != strip_casts(&case_selector) {
                    return Ok(None);
                }
            } else {
                selector = Some(case_selector);
            }
            cases.push((value, case_idx));

            match self.lower_block_terminator(next_idx)? {
                LoweredTerminator::Cond { .. } => {
                    current_idx = next_idx;
                    continue;
                }
                _ => {
                    let Some(selector) = selector else {
                        return Ok(None);
                    };
                    return Ok(Some(ParsedSwitch {
                        selector,
                        cases,
                        default_idx: next_idx,
                    }));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ParsedSwitch {
    selector: HirExpr,
    cases: Vec<(i64, usize)>,
    default_idx: usize,
}

fn extract_eq_const_for_case(expr: &HirExpr, case_on_true: bool) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if !case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => extract_eq_const_for_case(expr.as_ref(), !case_on_true),
        _ => None,
    }
}

fn extract_eq_const_operands(lhs: &HirExpr, rhs: &HirExpr) -> Option<(HirExpr, i64)> {
    match (strip_casts(lhs), strip_casts(rhs)) {
        (HirExpr::Const(value, _), other) => Some((other, value)),
        (other, HirExpr::Const(value, _)) => Some((other, value)),
        _ => None,
    }
}

fn merge_adjacent_switch_cases(cases: &mut Vec<HirSwitchCase>) {
    let mut merged: Vec<HirSwitchCase> = Vec::with_capacity(cases.len());
    for case in cases.drain(..) {
        if let Some(prev) = merged.last_mut()
            && prev.body == case.body
        {
            prev.values.extend(case.values);
            continue;
        }
        merged.push(case);
    }
    *cases = merged;
}
