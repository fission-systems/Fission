use super::constraints::UseConstraint;
use super::expression_types::{expr_int_bits, nir_type_bits};
use crate::HashMap;
use crate::prelude::*;

pub(super) fn collect_compare_constraints(
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    signed: bool,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let lhs_bits = expr_int_bits(lhs, known_binding_types)
        .or_else(|| expr_int_bits(rhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));
    let rhs_bits = expr_int_bits(rhs, known_binding_types)
        .or_else(|| expr_int_bits(lhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));

    if let (PreHirExpr::Var(name), Some(bits)) = (lhs, lhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
    if let (PreHirExpr::Var(name), Some(bits)) = (rhs, rhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
}

fn compare_constraint(bits: u32, signed: bool) -> UseConstraint {
    if signed {
        UseConstraint::Signed { bits }
    } else {
        UseConstraint::Unsigned { bits }
    }
}

pub(super) fn collect_arithmetic_result_constraints(
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    if let NirType::Float { bits } = result_ty {
        // FLOAT_MULT/ADD-class binary results force float operands and
        // float* bases for Index/Load that produced those operands.
        collect_float_operand_constraint(lhs, *bits, out);
        collect_float_operand_constraint(rhs, *bits, out);
        return;
    }
    let NirType::Int {
        bits: result_bits,
        signed,
    } = result_ty
    else {
        return;
    };
    collect_arithmetic_operand_constraint(lhs, *result_bits, *signed, known_binding_types, out);
    collect_arithmetic_operand_constraint(rhs, *result_bits, *signed, known_binding_types, out);
}

fn collect_float_operand_constraint(
    expr: &PreHirExpr,
    bits: u32,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let float_ty = NirType::Float { bits };
    match expr {
        PreHirExpr::Var(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Exact(float_ty));
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_float_operand_constraint(expr, bits, out);
        }
        PreHirExpr::Load { ptr, .. } => {
            if let PreHirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(float_ty));
            }
            collect_float_operand_constraint(ptr, bits, out);
        }
        PreHirExpr::Index { base, .. } => {
            if let PreHirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(float_ty));
            }
            collect_float_operand_constraint(base, bits, out);
        }
        _ => {}
    }
}

fn collect_arithmetic_operand_constraint(
    expr: &PreHirExpr,
    result_bits: u32,
    signed: bool,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let PreHirExpr::Var(name) = expr else {
        return;
    };
    if expr_int_bits(expr, known_binding_types) != Some(result_bits) {
        return;
    }
    out.entry(name.clone())
        .or_default()
        .push(compare_constraint(result_bits, signed));
}
