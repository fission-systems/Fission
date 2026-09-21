//! Branch-indirect target decoding and selector provenance helpers.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct InferredJumpTableTargets {
    pub(super) unique_targets: Vec<u64>,
    pub(super) recovered_cases: Vec<(i64, u64)>,
    pub(super) selector_cardinality: usize,
    pub(super) decode_mode: &'static str,
}

pub(super) fn merge_inferred_branchind_targets(
    targets: &mut Vec<u64>,
    recovered_targets: InferredJumpTableTargets,
    recovered_case_map: &mut Option<Vec<(i64, u64)>>,
    recovered_selector_cardinality: &mut Option<usize>,
) {
    *recovered_selector_cardinality = Some(recovered_targets.selector_cardinality);
    *recovered_case_map = Some(recovered_targets.recovered_cases);
    let mut seen = targets.iter().copied().collect::<BTreeSet<_>>();
    for target in recovered_targets.unique_targets {
        if seen.insert(target) {
            targets.push(target);
        }
    }
}

pub(super) fn decode_jump_table_target(
    bytes: &[u8],
    little_endian: bool,
    relative_entries: bool,
    target_base: Option<u64>,
) -> Option<u64> {
    if relative_entries {
        let base = i128::from(target_base?);
        let displacement = match bytes.len() {
            4 => {
                let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
                i128::from(if little_endian {
                    i32::from_le_bytes(raw)
                } else {
                    i32::from_be_bytes(raw)
                })
            }
            8 => {
                let raw = [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ];
                i128::from(if little_endian {
                    i64::from_le_bytes(raw)
                } else {
                    i64::from_be_bytes(raw)
                })
            }
            _ => return None,
        };
        let target = base + displacement;
        return (0..=i128::from(u64::MAX))
            .contains(&target)
            .then_some(target as u64);
    }

    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(if little_endian {
                u32::from_le_bytes(raw) as u64
            } else {
                u32::from_be_bytes(raw) as u64
            })
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(if little_endian {
                u64::from_le_bytes(raw)
            } else {
                u64::from_be_bytes(raw)
            })
        }
        _ => None,
    }
}

pub(super) fn branchind_decode_modes(
    relative_entries: bool,
    table_base: u64,
    target_base: Option<u64>,
    image_base: u64,
    sections: &[(u64, u64)],
) -> Vec<(&'static str, bool, Option<u64>)> {
    if relative_entries {
        return vec![(
            "relative_target_base",
            true,
            target_base.or(Some(table_base)),
        )];
    }
    let mut modes = vec![
        ("absolute", false, None),
        ("relative_table_base", true, Some(table_base)),
    ];
    if let Some(section_base) = containing_section_start(sections, table_base) {
        if section_base != table_base {
            modes.push(("section_base_relative", true, Some(section_base)));
        }
    }
    if image_base != 0 {
        modes.push(("image_base_relative", true, Some(image_base)));
    }
    modes
}

fn containing_section_start(sections: &[(u64, u64)], address: u64) -> Option<u64> {
    sections
        .iter()
        .find_map(|(start, end)| (address >= *start && address < *end).then_some(*start))
}

pub(super) fn extract_selector_upper_bound_from_cond(
    cond: &PreHirExpr,
    selector_match: &impl Fn(&PreHirExpr) -> bool,
    current_on_true: bool,
) -> Option<u64> {
    let cond = strip_casts(cond);
    if let PreHirExpr::Unary {
        op: PreHirUnaryOp::Not,
        expr,
        ..
    } = cond
    {
        return extract_selector_upper_bound_from_cond(&expr, selector_match, !current_on_true);
    }

    let PreHirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };

    let lhs = strip_casts(&lhs);
    let rhs = strip_casts(&rhs);
    let const_u64 = |expr: &PreHirExpr| match strip_casts(expr) {
        PreHirExpr::Const(value, _) if value >= 0 => Some(value as u64),
        _ => None,
    };
    let selector_sub_const_eq_zero = |expr: &PreHirExpr, zero_side: &PreHirExpr| {
        if const_u64(zero_side) != Some(0) {
            return None;
        }
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } = strip_casts(expr)
        else {
            return None;
        };
        selector_match(&lhs).then(|| const_u64(&rhs)).flatten()
    };

    match op {
        PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::And => {
            let lhs_bound =
                extract_selector_upper_bound_from_cond(&lhs, selector_match, current_on_true);
            let rhs_bound =
                extract_selector_upper_bound_from_cond(&rhs, selector_match, current_on_true);
            return if current_on_true {
                match (lhs_bound, rhs_bound) {
                    (Some(lhs), Some(rhs)) => Some(lhs.min(rhs)),
                    (Some(bound), None) | (None, Some(bound)) => Some(bound),
                    (None, None) => None,
                }
            } else {
                match (lhs_bound, rhs_bound) {
                    (Some(lhs), Some(rhs)) => Some(lhs.max(rhs)),
                    (Some(bound), None) | (None, Some(bound)) => Some(bound),
                    (None, None) => None,
                }
            };
        }
        PreHirBinaryOp::LogicalOr | PreHirBinaryOp::Or => {
            let lhs_bound =
                extract_selector_upper_bound_from_cond(&lhs, selector_match, current_on_true);
            let rhs_bound =
                extract_selector_upper_bound_from_cond(&rhs, selector_match, current_on_true);
            return if current_on_true {
                match (lhs_bound, rhs_bound) {
                    (Some(lhs), Some(rhs)) => Some(lhs.max(rhs)),
                    (Some(bound), None) | (None, Some(bound)) => Some(bound),
                    (None, None) => None,
                }
            } else {
                match (lhs_bound, rhs_bound) {
                    (Some(lhs), Some(rhs)) => Some(lhs.min(rhs)),
                    (Some(bound), None) | (None, Some(bound)) => Some(bound),
                    (None, None) => None,
                }
            };
        }
        _ => {}
    }

    match (op, selector_match(&lhs), selector_match(&rhs)) {
        (PreHirBinaryOp::Eq, true, false) if current_on_true => const_u64(&rhs),
        (PreHirBinaryOp::Eq, false, true) if current_on_true => const_u64(&lhs),
        (PreHirBinaryOp::Ne, true, false) if !current_on_true => const_u64(&rhs),
        (PreHirBinaryOp::Ne, false, true) if !current_on_true => const_u64(&lhs),
        (PreHirBinaryOp::Eq, false, false) if current_on_true => {
            selector_sub_const_eq_zero(&lhs, &rhs)
                .or_else(|| selector_sub_const_eq_zero(&rhs, &lhs))
        }
        (PreHirBinaryOp::Ne, false, false) if !current_on_true => {
            selector_sub_const_eq_zero(&lhs, &rhs)
                .or_else(|| selector_sub_const_eq_zero(&rhs, &lhs))
        }
        (PreHirBinaryOp::Le | PreHirBinaryOp::SLe, true, false) if current_on_true => {
            const_u64(&rhs)
        }
        (PreHirBinaryOp::Lt | PreHirBinaryOp::SLt, true, false) if current_on_true => {
            const_u64(&rhs)?.checked_sub(1)
        }
        (PreHirBinaryOp::Le | PreHirBinaryOp::SLe, false, true) if !current_on_true => {
            const_u64(&lhs)?.checked_sub(1)
        }
        (PreHirBinaryOp::Lt | PreHirBinaryOp::SLt, false, true) if !current_on_true => {
            const_u64(&lhs)
        }
        (PreHirBinaryOp::Gt | PreHirBinaryOp::SGt, true, false) if !current_on_true => {
            const_u64(&rhs)
        }
        (PreHirBinaryOp::Ge | PreHirBinaryOp::SGe, true, false) if !current_on_true => {
            const_u64(&rhs)?.checked_sub(1)
        }
        (PreHirBinaryOp::Gt | PreHirBinaryOp::SGt, false, true) if current_on_true => {
            const_u64(&lhs)?.checked_sub(1)
        }
        (PreHirBinaryOp::Ge | PreHirBinaryOp::SGe, false, true) if current_on_true => {
            const_u64(&lhs)
        }
        _ => None,
    }
}

pub(super) fn is_safe_selector_provenance_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
    )
}

pub(super) fn same_family_varnode(vn: &Varnode, selector_family: (u64, u64)) -> bool {
    (vn.space_id, vn.offset) == selector_family
}
