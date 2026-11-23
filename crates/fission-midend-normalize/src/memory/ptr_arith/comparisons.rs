use super::address_terms::typed_pointer_base;
use crate::HashMap;
use crate::prelude::*;

fn condition_pointer_operand(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Cast { ty, expr } if matches!(ty, NirType::Int { .. }) => {
            typed_pointer_base(expr, binding_types).map(|(base, _, _)| base)
        }
        _ => typed_pointer_base(expr, binding_types).map(|(base, _, _)| base),
    }
}

pub(super) fn recover_pointer_difference_condition(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let lhs = condition_pointer_operand(lhs, binding_types)?;
    let rhs = condition_pointer_operand(rhs, binding_types)?;
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Ne,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    })
}

pub(super) fn recover_pointer_difference_zero_compare(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: op @ (PreHirBinaryOp::Eq | PreHirBinaryOp::Ne),
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if matches!(rhs.as_ref(), PreHirExpr::Const(0, _)) {
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: diff_lhs,
            rhs: diff_rhs,
            ..
        } = lhs.as_ref()
        else {
            return None;
        };
        let diff_lhs = condition_pointer_operand(diff_lhs, binding_types)?;
        let diff_rhs = condition_pointer_operand(diff_rhs, binding_types)?;
        return Some(PreHirExpr::Binary {
            op: *op,
            lhs: Box::new(diff_lhs),
            rhs: Box::new(diff_rhs),
            ty: NirType::Bool,
        });
    }
    if matches!(lhs.as_ref(), PreHirExpr::Const(0, _)) {
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: diff_lhs,
            rhs: diff_rhs,
            ..
        } = rhs.as_ref()
        else {
            return None;
        };
        let diff_lhs = condition_pointer_operand(diff_lhs, binding_types)?;
        let diff_rhs = condition_pointer_operand(diff_rhs, binding_types)?;
        return Some(PreHirExpr::Binary {
            op: *op,
            lhs: Box::new(diff_lhs),
            rhs: Box::new(diff_rhs),
            ty: NirType::Bool,
        });
    }
    None
}

pub(super) fn recover_negated_pointer_difference_condition(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<PreHirExpr> {
    let PreHirExpr::Unary {
        op: PreHirUnaryOp::Not,
        expr,
        ..
    } = expr
    else {
        return None;
    };
    recover_pointer_difference_zero_compare(expr, binding_types).map(negate_expr)
}
