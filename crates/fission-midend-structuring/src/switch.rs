//! Switch structuring free functions (`try_lower_switch`, compare-chain parse).
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`.

use crate::helpers::{
    block_label, detect_and_patch_case_fallthrough, extract_eq_const_for_case,
    extract_range_guard_for_chain, merge_equivalent_switch_cases, recovered_switch_case_values,
};
use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, LoweredTerminator, structuring_diag_enabled};
use crate::regions::EmitReadyDecision;
use fission_midend_dir::util::format_expr_key;
use fission_midend_core::ir::{DispatcherCaseMapSource, DispatcherLegality, DispatcherProofScope, DispatcherProofUnit, MlilPreviewError, SelectorNormalization};
use fission_midend_dir::{DirExpr, DirStmt, DirSwitchCase};
use fission_midend_core::wave_stats;
use fission_midend_dir::util::strip_casts;
use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
use crate::HashSet;

/// Soft budget for compare-chain switch parsing steps.
pub const SWITCH_CHAIN_PARSE_BUDGET_MAX: usize = 16;

pub fn try_lower_switch(host: &mut impl StructuringHost, 
        idx: usize,
    ) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
        if let Some(direct) = try_lower_direct_dispatcher_switch(host, idx)? {
            return Ok(Some(direct));
        }

        let Some(parsed) = parse_switch_chain(host, idx)? else {
            return Ok(None);
        };
        if !compare_chain_switch_candidate(&parsed) {
            return Ok(None);
        }
        let emit_ready = EmitReadyDecision::from_dispatcher_proof(Some(&parsed.proof));
        if !emit_ready.emit_ready {
            if structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] switch chain at {} emit_ready failed: {:?}",
                    host.block_target_key(idx),
                    emit_ready.failure
                );
            }
            host.bump_switch_emit_ready_failed();
            return Ok(None);
        }
        let mut seen_case_values = HashSet::default();
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
        let Some(exit) = host.shared_exit_for_indices(&exits)? else {
            return Ok(None);
        };

        let mut case_targets = std::collections::HashSet::default();
        for case_idx in &exits {
            case_targets.insert(*case_idx);
        }
        let old_targets = std::mem::replace(host.active_switch_targets_mut(), case_targets);

        let mut cases = Vec::new();
        let mut max_skip = 0usize;
        for (value, case_idx) in parsed.cases {
            let Some((mut case_body, skip_to)) = host.lower_linear_body(case_idx, exit)? else {
                 *host.active_switch_targets_mut() = old_targets;
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to).max(case_idx + 1);
            if !case_body.iter().any(|s| matches!(s, DirStmt::Label(_))) {
                let target_addr = host.block_start_address(case_idx);
                case_body.insert(
                    0,
                    DirStmt::Label(block_label(target_addr)),
                );
            }
            cases.push(DirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        merge_equivalent_switch_cases(&mut cases);
        let ft_count = detect_and_patch_case_fallthrough(&mut cases);
        let Some((mut default_body, default_skip)) =
            host.lower_linear_body(parsed.default_idx, exit)?
        else {
             *host.active_switch_targets_mut() = old_targets;
            return Ok(None);
        };
        if !default_body.iter().any(|s| matches!(s, DirStmt::Label(_))) {
            let target_addr = host.block_start_address(parsed.default_idx);
            default_body.insert(
                0,
                DirStmt::Label(block_label(target_addr)),
            );
        }
        max_skip = max_skip.max(default_skip).max(parsed.default_idx + 1);

         *host.active_switch_targets_mut() = old_targets;
        host.bump_switch_fallthrough_detected(ft_count);

        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => max_skip,
        };
        wave_stats::add_compare_chain_dispatcher_count(1);
        wave_stats::add_dispatcher_proof_units(1);
        wave_stats::add_dispatcher_proof_completed(1);
        wave_stats::add_dispatcher_shape_recoveries(1);
        Ok(Some((
            DirStmt::Switch {
                expr: parsed.selector,
                cases,
                default: default_body,
            },
            skip_to,
        )))
    }

pub fn try_lower_direct_dispatcher_switch(host: &mut impl StructuringHost, 
        idx: usize,
    ) -> Result<Option<(DirStmt, usize)>, MlilPreviewError> {
        let LoweredTerminator::Switch {
            expr,
            targets,
            default_target,
            min_val,
            proof,
        } = host.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let emit_ready = EmitReadyDecision::from_dispatcher_proof(proof.as_ref());
        if !emit_ready.emit_ready {
            if structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] direct switch at {} emit_ready failed: {:?}",
                    host.block_target_key(idx),
                    emit_ready.failure
                );
            }
            host.bump_switch_emit_ready_failed();
            return Ok(None);
        }

        let (case_values, used_proof_payload) =
            recovered_switch_case_values(&targets, default_target, min_val, proof.as_ref());
        if case_values.len() < 2 {
            return Ok(None);
        }

        let mut seen_case_values = HashSet::default();
        if !case_values
            .iter()
            .all(|(value, _)| seen_case_values.insert(*value))
        {
            return Ok(None);
        }

        let mut exits = Vec::new();
        for (_, target) in &case_values {
            let Some(case_idx) = host.find_block_index_by_address(*target) else {
                return Ok(None);
            };
            let canon = canonicalize_switch_target(host, case_idx);
            if !exits.contains(&canon) {
                exits.push(canon);
            }
        }
        if let Some(default_target) = default_target {
            let Some(default_idx) = host.find_block_index_by_address(default_target) else {
                return Ok(None);
            };
            let canon = canonicalize_switch_target(host, default_idx);
            if !exits.contains(&canon) {
                exits.push(canon);
            }
        }
        let Some(exit) = host.shared_exit_for_indices(&exits)? else {
            return Ok(None);
        };

        let mut case_targets = std::collections::HashSet::default();
        for case_idx in &exits {
            case_targets.insert(*case_idx);
        }
        let old_targets = std::mem::replace(host.active_switch_targets_mut(), case_targets);

        let mut cases = Vec::new();
        let mut max_skip = idx + 1;
        let mut success = true;
        for (value, target) in case_values {
            if Some(target) == default_target {
                continue;
            }
            let Some(case_idx) = host.find_block_index_by_address(target) else {
                success = false;
                break;
            };
            let case_idx = canonicalize_switch_target(host, case_idx);
            let Some((case_body, skip_to)) = host.lower_linear_body(case_idx, exit)? else {
                success = false;
                break;
            };
            max_skip = max_skip.max(skip_to).max(case_idx + 1);
            cases.push(DirSwitchCase {
                values: vec![value],
                body: case_body,
            });
        }
        if !success || cases.len() < 2 {
             *host.active_switch_targets_mut() = old_targets;
            return Ok(None);
        }
        merge_equivalent_switch_cases(&mut cases);
        let ft_count = detect_and_patch_case_fallthrough(&mut cases);

        let default = if let Some(default_target) = default_target {
            let Some(default_idx) = host.find_block_index_by_address(default_target) else {
                 *host.active_switch_targets_mut() = old_targets;
                return Ok(None);
            };
            let default_idx = canonicalize_switch_target(host, default_idx);
            let Some((default_body, skip_to)) = host.lower_linear_body(default_idx, exit)? else {
                 *host.active_switch_targets_mut() = old_targets;
                return Ok(None);
            };
            max_skip = max_skip.max(skip_to).max(default_idx + 1);
            default_body
        } else {
            Vec::new()
        };

         *host.active_switch_targets_mut() = old_targets;
        host.bump_switch_fallthrough_detected(ft_count);

        let skip_to = match exit {
            LinearExit::Join(join_idx) => join_idx,
            LinearExit::Return | LinearExit::End => max_skip,
        };

        if used_proof_payload {
            host.bump_proof_payload_direct_emit();
        }
        wave_stats::add_dispatcher_proof_units(1);
        wave_stats::add_dispatcher_proof_completed(1);
        wave_stats::add_dispatcher_shape_recoveries(1);
        Ok(Some((
            DirStmt::Switch {
                expr,
                cases,
                default,
            },
            skip_to,
        )))
    }

pub fn parse_switch_chain(host: &mut impl StructuringHost, 
        start_idx: usize,
    ) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
        let mut current_idx = start_idx;
        let mut current_term = host.lower_block_terminator(current_idx)?;
        let mut selector: Option<DirExpr> = None;
        let mut cases = Vec::new();
        let mut guarded_default_idx: Option<usize> = None;
        let mut saw_range_guard = false;
        let mut visited = HashSet::default();
        let max_chain_steps = host
            .successors()
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

            let mut case_target = None;
            let mut next_compare_target = None;
            let mut case_on_true = true;
            let mut matched_case = None;

            if let Some((case_selector, value)) = extract_eq_const_for_case(&cond, true) {
                case_target = Some(true_target);
                next_compare_target = false_target;
                case_on_true = true;
                matched_case = Some((case_selector, value));
            } else if let Some((case_selector, value)) = extract_eq_const_for_case(&cond, false) {
                case_target = false_target;
                next_compare_target = Some(true_target);
                case_on_true = false;
                matched_case = Some((case_selector, value));
            } else if !saw_range_guard {
                if let Some(range_selector) = extract_range_guard_for_chain(&cond, true) {
                    case_target = false_target;
                    next_compare_target = Some(true_target);
                    case_on_true = false;

                    if let Some(existing) = &selector {
                        if strip_casts(existing) != strip_casts(&range_selector) {
                            return Ok(None);
                        }
                    } else {
                        selector = Some(range_selector);
                    }
                    let Some(case_target_addr) = case_target else {
                        return Ok(None);
                    };
                    let Some(default_idx) = host.find_block_index_by_address(case_target_addr)
                    else {
                        return Ok(None);
                    };
                    guarded_default_idx = Some(canonicalize_switch_target(host, default_idx));
                    saw_range_guard = true;
                } else if let Some(range_selector) = extract_range_guard_for_chain(&cond, false) {
                    case_target = Some(true_target);
                    next_compare_target = false_target;
                    case_on_true = true;

                    if let Some(existing) = &selector {
                        if strip_casts(existing) != strip_casts(&range_selector) {
                            return Ok(None);
                        }
                    } else {
                        selector = Some(range_selector);
                    }
                    let Some(case_target_addr) = case_target else {
                        return Ok(None);
                    };
                    let Some(default_idx) = host.find_block_index_by_address(case_target_addr)
                    else {
                        return Ok(None);
                    };
                    guarded_default_idx = Some(canonicalize_switch_target(host, default_idx));
                    saw_range_guard = true;
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }

            if let Some((case_selector, value)) = matched_case {
                let Some(case_target_addr) = case_target else {
                    return Ok(None);
                };
                let Some(case_idx) = host.find_block_index_by_address(case_target_addr) else {
                    return Ok(None);
                };
                let case_idx = canonicalize_switch_target(host, case_idx);
                if let Some(existing) = &selector {
                    if strip_casts(existing) != strip_casts(&case_selector) {
                        return Ok(None);
                    }
                } else {
                    selector = Some(case_selector);
                }
                cases.push((value, case_idx));
            }

            let Some(next_compare_addr) = next_compare_target else {
                return Ok(None);
            };
            let Some(next_idx) = host.find_block_index_by_address(next_compare_addr) else {
                return Ok(None);
            };

            let next_term = host.lower_block_terminator(next_idx)?;
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
                    let default_idx = canonicalize_switch_target(host, next_idx);
                    if let Some(guarded_default_idx) = guarded_default_idx
                        && guarded_default_idx != default_idx
                    {
                        return Ok(None);
                    }
                    let default_idx = guarded_default_idx.unwrap_or(default_idx);
                    if !compare_chain_switch_candidate_values(&cases, default_idx) {
                        return Ok(None);
                    }
                    let proof =
                        build_compare_chain_proof(host, start_idx, &selector, &cases, default_idx);
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

pub fn canonicalize_switch_target(host: &impl StructuringHost, start_idx: usize) -> usize {
        const MAX_SWITCH_TARGET_CANON_STEPS: usize = 32;
        let mut current = start_idx;
        let mut visited = HashSet::default();
        for _ in 0..MAX_SWITCH_TARGET_CANON_STEPS {
            if !visited.insert(current) {
                break;
            }
            if host.successors()[current].len() != 1 {
                break;
            }
            let next_idx = host.successors()[current][0];
            if !host.is_trivial_forwarding_block(current, next_idx) {
                break;
            }
            current = next_idx;
        }
        current
}

pub fn compare_chain_switch_candidate_values(cases: &[(i64, usize)], default_idx: usize) -> bool {
    if cases.len() < 2 {
        return false;
    }
    let mut targets: HashSet<usize> = cases.iter().map(|(_, block_idx)| *block_idx).collect();
    targets.insert(default_idx);
    targets.len() >= 2
}

pub fn compare_chain_switch_candidate(parsed: &ParsedSwitch) -> bool {
    compare_chain_switch_candidate_values(&parsed.cases, parsed.default_idx)
}

#[derive(Debug, Clone)]
pub struct ParsedSwitch {
    pub selector: DirExpr,
    pub cases: Vec<(i64, usize)>,
    pub default_idx: usize,
    pub proof: DispatcherProofUnit,
}

pub fn build_compare_chain_proof(host: &impl StructuringHost, 
        start_idx: usize,
        selector: &DirExpr,
        cases: &[(i64, usize)],
        default_idx: usize,
    ) -> DispatcherProofUnit {
        let recovered_cases = cases
            .iter()
            .map(|(value, block_idx)| (*value, host.block_target_key(*block_idx)))
            .collect::<Vec<_>>();
        let mut guard_bounds = Vec::new();
        if !cases.is_empty() {
            let min_case = cases.iter().map(|(value, _)| *value).min();
            let max_case = cases.iter().map(|(value, _)| *value).max();
            guard_bounds.push((min_case, max_case));
        }
        DispatcherProofUnit {
            selector_expr: format_expr_key(selector),
            rendered_selector_expr: Some(format_expr_key(selector)),
            candidate_targets: recovered_cases.iter().map(|(_, target)| *target).collect(),
            recovered_cases,
            selector_cardinality: cases.len(),
            target_cardinality: cases
                .iter()
                .map(|(_, block_idx)| host.block_target_key(*block_idx))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            case_map_source: DispatcherCaseMapSource::CompareChainRecovered,
            default_target: Some(host.block_target_key(default_idx)),
            guard_set: vec!["compare_chain".to_string(), "shared_selector".to_string()],
            follow_block: Some(host.block_target_key(default_idx)),
            normalization: Some(SelectorNormalization {
                base_subtract: None,
                mask: None,
                stride: None,
                width: None,
                address_space: None,
                guard_bounds,
            }),
            legality_witness: Some(DispatcherLegality {
                follow_block: Some(host.block_target_key(default_idx)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::extract_eq_const_operands;
    use fission_midend_core::ir::{NirType};
use fission_midend_dir::{DirBinaryOp, DirUnaryOp};

    #[test]
    fn merge_equivalent_switch_cases_merges_non_adjacent_equal_bodies() {
        let mut cases = vec![
            DirSwitchCase {
                values: vec![1],
                body: vec![DirStmt::Return(Some(DirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))],
            },
            DirSwitchCase {
                values: vec![2],
                body: vec![DirStmt::Return(Some(DirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))],
            },
            DirSwitchCase {
                values: vec![3],
                body: vec![DirStmt::Return(Some(DirExpr::Const(
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
        let selector = DirExpr::Var("msg".to_string());
        let shifted = DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(DirExpr::Const(
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
            &DirExpr::Const(
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
        let selector = DirExpr::Var("msg".to_string());
        let shifted = DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(DirExpr::Const(
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
        let cond = DirExpr::Binary {
            op: DirBinaryOp::Le,
            lhs: Box::new(shifted),
            rhs: Box::new(DirExpr::Const(
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

    /// A switch with two cases where case 0 ends with a goto targeting case 1's label.
    /// The goto should be replaced with the __fallthrough sentinel.
    #[test]
    fn test_switch_fallthrough_detection_patches_goto_to_next_label() {
        let next_label = "block_0x1000".to_string();
        let mut cases = vec![
            DirSwitchCase {
                values: vec![0],
                body: vec![
                    DirStmt::Label("block_0x0000".to_string()),
                    DirStmt::Goto(next_label.clone()),
                ],
            },
            DirSwitchCase {
                values: vec![1],
                body: vec![DirStmt::Label(next_label.clone()), DirStmt::Return(None)],
            },
        ];

        let patched = detect_and_patch_case_fallthrough(&mut cases);
        assert_eq!(patched, 1, "Expected 1 fallthrough patched");
        // The goto in case[0] should now be the sentinel.
        assert!(
            matches!(&cases[0].body[1], DirStmt::Goto(lbl) if lbl == SWITCH_FALLTHROUGH_SENTINEL),
            "Expected __fallthrough sentinel, got: {:?}",
            &cases[0].body[1]
        );
        // Case[1] should be unchanged.
        assert!(matches!(&cases[1].body[0], DirStmt::Label(lbl) if lbl == &next_label),);
    }

    /// A switch where case 0's goto targets a label NOT in case 1 — must not be patched.
    #[test]
    fn test_switch_fallthrough_detection_ignores_non_adjacent_goto() {
        let mut cases = vec![
            DirSwitchCase {
                values: vec![0],
                body: vec![
                    DirStmt::Label("block_a".to_string()),
                    DirStmt::Goto("block_x".to_string()), // points somewhere else
                ],
            },
            DirSwitchCase {
                values: vec![1],
                body: vec![DirStmt::Label("block_b".to_string()), DirStmt::Return(None)],
            },
        ];
        let patched = detect_and_patch_case_fallthrough(&mut cases);
        assert_eq!(patched, 0, "Expected 0 fallthroughs for non-adjacent goto");
    }

    #[test]
    pub fn compare_chain_switch_candidate_requires_two_cases_and_distinct_targets() {
        assert!(!compare_chain_switch_candidate_values(&[(1, 3)], 4));
        assert!(!compare_chain_switch_candidate_values(&[(1, 3), (2, 3)], 3));
        assert!(compare_chain_switch_candidate_values(&[(1, 3), (2, 4)], 5));
    }
}
