//! Shared pure helpers for residual structuring free functions.

use fission_midend_core::ir::{
    DispatcherProofUnit, DirBinaryOp, DirExpr, DirStmt, DirSwitchCase, DirUnaryOp,
};
use fission_midend_core::util_dir::strip_casts;
use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
use crate::regions::EmitReadyDecision;

/// Label name for a block target key / address (matches pcode midend cfg helper).
const DUPLICATE_BLOCK_KEY_TAG: u64 = 0x8000_0000_0000_0000;

pub fn block_label(address: u64) -> String {
    if address & DUPLICATE_BLOCK_KEY_TAG != 0 {
        let ordinal = (address >> 48) & 0x7fff;
        let raw = address & 0x0000_ffff_ffff_ffff;
        format!("block_{raw:x}_dup{ordinal}")
    } else {
        format!("block_{:x}", address)
    }
}

pub fn recovered_switch_case_values(
    targets: &[u64],
    default_target: Option<u64>,
    min_val: i64,
    proof: Option<&DispatcherProofUnit>,
) -> (Vec<(i64, u64)>, bool) {
    if let Some(proof) = proof
        && proof_supports_direct_emit(proof)
    {
        let recovered = proof
            .recovered_cases
            .iter()
            .copied()
            .filter(|(_, target)| Some(*target) != default_target)
            .collect::<Vec<_>>();
        if !recovered.is_empty() {
            return (recovered, true);
        }
    }

    (
        targets
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(ordinal, target)| {
                (Some(target) != default_target).then_some((min_val + ordinal as i64, target))
            })
            .collect(),
        false,
    )
}

pub fn proof_supports_direct_emit(proof: &DispatcherProofUnit) -> bool {
    EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready
        && proof.recovered_cases.len() >= proof.selector_cardinality
}

pub fn merge_equivalent_switch_cases(cases: &mut Vec<DirSwitchCase>) {
    let mut merged: Vec<DirSwitchCase> = Vec::with_capacity(cases.len());
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

pub fn detect_and_patch_case_fallthrough(cases: &mut Vec<DirSwitchCase>) -> usize {
    let mut patched = 0usize;
    let n = cases.len();
    if n < 2 {
        return 0;
    }
    let next_labels: Vec<Option<String>> = (0..n)
        .map(|i| {
            cases[i].body.iter().find_map(|s| {
                if let DirStmt::Label(l) = s {
                    Some(l.clone())
                } else {
                    None
                }
            })
        })
        .collect();
    for i in 0..(n - 1) {
        let Some(ref next_label) = next_labels[i + 1] else {
            continue;
        };
        let last_stmt = cases[i]
            .body
            .iter_mut()
            .rev()
            .find(|s| !matches!(s, DirStmt::Label(_)));
        if let Some(DirStmt::Goto(label)) = last_stmt {
            if label == next_label {
                *label = SWITCH_FALLTHROUGH_SENTINEL.to_string();
                patched += 1;
            }
        }
    }
    patched
}

pub fn extract_eq_const_for_case(expr: &DirExpr, case_on_true: bool) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if !case_on_true => extract_eq_const_operands(lhs.as_ref(), rhs.as_ref()),
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => extract_eq_const_for_case(expr.as_ref(), !case_on_true),
        _ => None,
    }
}

pub fn extract_eq_const_operands(lhs: &DirExpr, rhs: &DirExpr) -> Option<(DirExpr, i64)> {
    match (strip_casts(lhs), strip_casts(rhs)) {
        (DirExpr::Const(value, _), other) => normalize_affine_case_expr(&other, value),
        (other, DirExpr::Const(value, _)) => normalize_affine_case_expr(&other, value),
        _ => None,
    }
}

pub fn extract_range_guard_for_chain(expr: &DirExpr, chain_on_true: bool) -> Option<DirExpr> {
    let expr = strip_casts(expr);
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Lt | DirBinaryOp::Le | DirBinaryOp::SLt | DirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => match (strip_casts(lhs.as_ref()), strip_casts(rhs.as_ref())) {
            (other, DirExpr::Const(_, _)) if chain_on_true => normalize_affine_bound_expr(&other),
            (DirExpr::Const(_, _), other) if !chain_on_true => normalize_affine_bound_expr(&other),
            _ => None,
        },
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => extract_range_guard_for_chain(expr.as_ref(), !chain_on_true),
        _ => None,
    }
}

fn normalize_affine_bound_expr(expr: &DirExpr) -> Option<DirExpr> {
    let expr = strip_casts(expr);
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        }
        | DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if matches!(strip_casts(rhs.as_ref()), DirExpr::Const(_, _)) => Some(*lhs.clone()),
        _ => Some(expr.clone()),
    }
}

fn normalize_affine_case_expr(expr: &DirExpr, value: i64) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            ref lhs,
            ref rhs,
            ..
        } => {
            let DirExpr::Const(offset, _) = strip_casts(rhs.as_ref()) else {
                return Some((expr.clone(), value));
            };
            value
                .checked_add(offset)
                .map(|normalized| ((*lhs.clone()), normalized))
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            ref lhs,
            ref rhs,
            ..
        } => {
            let DirExpr::Const(offset, _) = strip_casts(rhs.as_ref()) else {
                return Some((expr.clone(), value));
            };
            value
                .checked_sub(offset)
                .map(|normalized| ((*lhs.clone()), normalized))
        }
        _ => Some((expr.clone(), value)),
    }
}
