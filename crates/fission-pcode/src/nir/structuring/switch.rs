use super::*;
use crate::nir::normalize::wave_stats;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn try_lower_switch(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        if let Some(direct) = self.try_lower_direct_dispatcher_switch(idx)? {
            return Ok(Some(direct));
        }

        let Some(parsed) = self.parse_switch_chain(idx)? else {
            return Ok(None);
        };
        let emit_ready = EmitReadyDecision::from_dispatcher_proof(Some(&parsed.proof));
        if !emit_ready.emit_ready {
            if structuring_diag_enabled() {
                eprintln!("[DIAG] switch chain at {} emit_ready failed: {:?}", self.block_target_key(idx), emit_ready.failure);
            }
            self.telemetry.dispatcher.switch_emit_ready_failed_count += 1;
            self.telemetry.structuring.region_proof_candidate_count += 1;
            self.telemetry.structuring.region_emit_ready_failed_count += 1;
            return Ok(None);
        }
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
        merge_equivalent_switch_cases(&mut cases);
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
        wave_stats::add_compare_chain_dispatcher_count(1);
        wave_stats::add_dispatcher_proof_units(1);
        wave_stats::add_dispatcher_proof_completed(1);
        wave_stats::add_dispatcher_shape_recoveries(1);
        Ok(Some((
            HirStmt::Switch {
                expr: parsed.selector,
                cases,
                default: default_body,
            },
            skip_to,
        )))
    }

    fn try_lower_direct_dispatcher_switch(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        let LoweredTerminator::Switch {
            expr,
            targets,
            default_target,
            min_val,
            proof,
        } = self.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let emit_ready = EmitReadyDecision::from_dispatcher_proof(proof.as_ref());
        if !emit_ready.emit_ready {
            if structuring_diag_enabled() {
                eprintln!("[DIAG] direct switch at {} emit_ready failed: {:?}", self.block_target_key(idx), emit_ready.failure);
            }
            self.telemetry.dispatcher.switch_emit_ready_failed_count += 1;
            self.telemetry.structuring.region_proof_candidate_count += 1;
            self.telemetry.structuring.region_emit_ready_failed_count += 1;
            return Ok(None);
        }

        let (case_values, used_proof_payload) =
            recovered_switch_case_values(&targets, default_target, min_val, proof.as_ref());
        if case_values.len() < 2 {
            return Ok(None);
        }

        let mut seen_case_values = HashSet::new();
        if !case_values
            .iter()
            .all(|(value, _)| seen_case_values.insert(*value))
        {
            return Ok(None);
        }

        let mut cases = Vec::new();
        let mut max_skip = idx + 1;
        for (value, target) in case_values {
            if Some(target) == default_target {
                continue;
            }
            let Some(case_idx) = self.find_block_index_by_address(target) else {
                return Ok(None);
            };
            let case_idx = self.canonicalize_switch_target(case_idx);
            let Some((case_body, skip_to)) =
                self.lower_linear_body(case_idx, LinearExit::Return)?
            else {
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to);
            cases.push(HirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        if cases.len() < 2 {
            return Ok(None);
        }
        merge_equivalent_switch_cases(&mut cases);

        let default = if let Some(default_target) = default_target {
            let Some(default_idx) = self.find_block_index_by_address(default_target) else {
                return Ok(None);
            };
            let default_idx = self.canonicalize_switch_target(default_idx);
            let Some((default_body, skip_to)) =
                self.lower_linear_body(default_idx, LinearExit::Return)?
            else {
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to);
            default_body
        } else {
            Vec::new()
        };

        if used_proof_payload {
            self.telemetry.dispatcher.proof_payload_direct_emit_count += 1;
        }
        wave_stats::add_dispatcher_proof_units(1);
        wave_stats::add_dispatcher_proof_completed(1);
        wave_stats::add_dispatcher_shape_recoveries(1);
        Ok(Some((
            HirStmt::Switch {
                expr,
                cases,
                default,
            },
            max_skip,
        )))
    }

    pub(super) fn parse_switch_chain(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
        let mut current_idx = start_idx;
        let mut current_term = self.lower_block_terminator(current_idx)?;
        let mut selector: Option<HirExpr> = None;
        let mut cases = Vec::new();
        let mut guarded_default_idx: Option<usize> = None;
        let mut saw_range_guard = false;
        let mut visited = HashSet::new();
        let max_chain_steps = self
            .successors
            .len()
            .min(SWITCH_CHAIN_PARSE_BUDGET_MAX)
            .max(1);

        for _ in 0..max_chain_steps {
            if !visited.insert(current_idx) {
                return Ok(None);
            }

            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = current_term
            else {
                return Ok(None);
            };
            let Some(next_idx) = self.fallthrough_index(current_idx) else {
                return Ok(None);
            };
            let next_addr = self.block_target_key(next_idx);
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
            if let Some((case_selector, value)) = extract_eq_const_for_case(&cond, case_on_true) {
                let Some(case_idx) = self.find_block_index_by_address(case_target) else {
                    return Ok(None);
                };
                let case_idx = self.canonicalize_switch_target(case_idx);
                if let Some(existing) = &selector {
                    if strip_casts(existing) != strip_casts(&case_selector) {
                        return Ok(None);
                    }
                } else {
                    selector = Some(case_selector);
                }
                cases.push((value, case_idx));
            } else if cases.is_empty() && !saw_range_guard {
                let Some(range_selector) = extract_range_guard_for_chain(&cond, !case_on_true)
                else {
                    return Ok(None);
                };
                if let Some(existing) = &selector {
                    if strip_casts(existing) != strip_casts(&range_selector) {
                        return Ok(None);
                    }
                } else {
                    selector = Some(range_selector);
                }
                let Some(default_idx) = self.find_block_index_by_address(case_target) else {
                    return Ok(None);
                };
                guarded_default_idx = Some(self.canonicalize_switch_target(default_idx));
                saw_range_guard = true;
            } else {
                return Ok(None);
            }

            let next_term = self.lower_block_terminator(next_idx)?;
            match next_term {
                LoweredTerminator::Cond { .. } => {
                    current_idx = next_idx;
                    current_term = next_term;
                    continue;
                }
                _ => {
                    let Some(selector) = selector else {
                        return Ok(None);
                    };
                    let default_idx = self.canonicalize_switch_target(next_idx);
                    if let Some(guarded_default_idx) = guarded_default_idx
                        && guarded_default_idx != default_idx
                    {
                        return Ok(None);
                    }
                    let default_idx = guarded_default_idx.unwrap_or(default_idx);
                    let proof =
                        self.build_compare_chain_proof(start_idx, &selector, &cases, default_idx);
                    return Ok(Some(ParsedSwitch {
                        selector,
                        cases,
                        default_idx,
                        proof,
                    }));
                }
            }
        }

        Ok(None)
    }

    pub(super) fn canonicalize_switch_target(&self, start_idx: usize) -> usize {
        const MAX_SWITCH_TARGET_CANON_STEPS: usize = 32;
        let mut current = start_idx;
        let mut visited = HashSet::new();
        for _ in 0..MAX_SWITCH_TARGET_CANON_STEPS {
            if !visited.insert(current) {
                break;
            }
            if self.successors[current].len() != 1 {
                break;
            }
            let next_idx = self.successors[current][0];
            if !self.is_trivial_forwarding_block(current, next_idx) {
                break;
            }
            current = next_idx;
        }
        current
    }
}

#[derive(Debug, Clone)]
pub(super) struct ParsedSwitch {
    pub(super) selector: HirExpr,
    pub(super) cases: Vec<(i64, usize)>,
    pub(super) default_idx: usize,
    pub(super) proof: DispatcherProofUnit,
}

impl<'a> PreviewBuilder<'a> {
    fn build_compare_chain_proof(
        &self,
        start_idx: usize,
        selector: &HirExpr,
        cases: &[(i64, usize)],
        default_idx: usize,
    ) -> DispatcherProofUnit {
        let recovered_cases = cases
            .iter()
            .map(|(value, block_idx)| (*value, self.block_target_key(*block_idx)))
            .collect::<Vec<_>>();
        let mut guard_bounds = Vec::new();
        if !cases.is_empty() {
            let min_case = cases.iter().map(|(value, _)| *value).min();
            let max_case = cases.iter().map(|(value, _)| *value).max();
            guard_bounds.push((min_case, max_case));
        }
        DispatcherProofUnit {
            selector_expr: print_expr(selector),
            rendered_selector_expr: Some(print_expr(selector)),
            candidate_targets: recovered_cases.iter().map(|(_, target)| *target).collect(),
            recovered_cases,
            selector_cardinality: cases.len(),
            target_cardinality: cases
                .iter()
                .map(|(_, block_idx)| self.block_target_key(*block_idx))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            case_map_source: DispatcherCaseMapSource::CompareChainRecovered,
            default_target: Some(self.block_target_key(default_idx)),
            guard_set: vec!["compare_chain".to_string(), "shared_selector".to_string()],
            follow_block: Some(self.block_target_key(default_idx)),
            normalization: Some(SelectorNormalization {
                base_subtract: None,
                mask: None,
                stride: None,
                width: None,
                address_space: None,
                guard_bounds,
            }),
            legality_witness: Some(DispatcherLegality {
                follow_block: Some(self.block_target_key(default_idx)),
                postdom_ok: true,
                side_effect_free_selector: true,
                ordinal_domain_complete: true,
                shared_tail_conflict: false,
                valid: true,
            }),
            proof_scope: if start_idx == 0 {
                DispatcherProofScope::OuterDispatch
            } else {
                DispatcherProofScope::NestedDispatch
            },
            proof_complete: true,
            failure_family: None,
        }
    }
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
        (HirExpr::Const(value, _), other) => normalize_affine_case_expr(&other, value),
        (other, HirExpr::Const(value, _)) => normalize_affine_case_expr(&other, value),
        _ => None,
    }
}

fn extract_range_guard_for_chain(expr: &HirExpr, chain_on_true: bool) -> Option<HirExpr> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Lt | HirBinaryOp::Le | HirBinaryOp::SLt | HirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => match (strip_casts(lhs.as_ref()), strip_casts(rhs.as_ref())) {
            (other, HirExpr::Const(_, _)) if chain_on_true => normalize_affine_bound_expr(&other),
            (HirExpr::Const(_, _), other) if !chain_on_true => normalize_affine_bound_expr(&other),
            _ => None,
        },
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => extract_range_guard_for_chain(expr.as_ref(), !chain_on_true),
        _ => None,
    }
}

fn normalize_affine_bound_expr(expr: &HirExpr) -> Option<HirExpr> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        }
        | HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if matches!(strip_casts(rhs.as_ref()), HirExpr::Const(_, _)) => Some(*lhs.clone()),
        _ => Some(expr.clone()),
    }
}

fn normalize_affine_case_expr(expr: &HirExpr, value: i64) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            ref lhs,
            ref rhs,
            ..
        } => {
            let HirExpr::Const(offset, _) = strip_casts(rhs.as_ref()) else {
                return Some((expr.clone(), value));
            };
            value
                .checked_add(offset)
                .map(|normalized| ((*lhs.clone()), normalized))
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            ref lhs,
            ref rhs,
            ..
        } => {
            let HirExpr::Const(offset, _) = strip_casts(rhs.as_ref()) else {
                return Some((expr.clone(), value));
            };
            value
                .checked_sub(offset)
                .map(|normalized| ((*lhs.clone()), normalized))
        }
        _ => Some((expr.clone(), value)),
    }
}

pub(super) fn merge_equivalent_switch_cases(cases: &mut Vec<HirSwitchCase>) {
    let mut merged: Vec<HirSwitchCase> = Vec::with_capacity(cases.len());
    for case in cases.drain(..) {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.body == case.body)
        {
            existing.values.extend(case.values);
            continue;
        }
        merged.push(case);
    }
    *cases = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_equivalent_switch_cases_merges_non_adjacent_equal_bodies() {
        let mut cases = vec![
            HirSwitchCase {
                values: vec![1],
                body: vec![HirStmt::Return(Some(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))],
            },
            HirSwitchCase {
                values: vec![2],
                body: vec![HirStmt::Return(Some(HirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))],
            },
            HirSwitchCase {
                values: vec![3],
                body: vec![HirStmt::Return(Some(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))],
            },
        ];

        merge_equivalent_switch_cases(&mut cases);

        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].values, vec![1, 3]);
        assert_eq!(cases[1].values, vec![2]);
    }

    #[test]
    fn extract_eq_const_operands_normalizes_subtracted_selector() {
        let selector = HirExpr::Var("msg".to_string());
        let shifted = HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(HirExpr::Const(
                160,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let recovered = extract_eq_const_operands(
            &shifted,
            &HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        )
        .expect("normalized selector");
        assert_eq!(recovered.0, selector);
        assert_eq!(recovered.1, 160);
    }

    #[test]
    fn extract_range_guard_for_chain_normalizes_affine_selector() {
        let selector = HirExpr::Var("msg".to_string());
        let shifted = HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(HirExpr::Const(
                160,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let cond = HirExpr::Binary {
            op: HirBinaryOp::Le,
            lhs: Box::new(shifted),
            rhs: Box::new(HirExpr::Const(
                95,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Bool,
        };

        let recovered =
            extract_range_guard_for_chain(&cond, true).expect("normalized range guard selector");
        assert_eq!(recovered, selector);
    }
}
