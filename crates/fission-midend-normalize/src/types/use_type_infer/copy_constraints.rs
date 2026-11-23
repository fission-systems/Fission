use super::constraints::UseConstraint;
use crate::HashMap;
use crate::prelude::*;

pub(super) fn collect_assignment_copy_constraints(
    lhs: &PreHirLValue,
    rhs: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match lhs {
        PreHirLValue::Var(lhs_name) => {
            // Reverse-propagate non-pointer legacy constraints only. Pointer
            // equality is owned by the operation-edge TypeFlow solver, which
            // carries evidence strength and lock boundaries.
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if let PreHirExpr::Var(rhs_name) = rhs {
                    match lhs_ty {
                        NirType::Ptr(_) => {}
                        other => {
                            out.entry(rhs_name.clone())
                                .or_default()
                                .push(copy_constraint_from_type(other));
                        }
                    }
                }
            }
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if matches!(lhs_ty, NirType::Ptr(_)) && !matches!(rhs, PreHirExpr::Var(_)) {
                    collect_pointer_assignment_base_constraints(
                        rhs,
                        lhs_ty,
                        known_binding_types,
                        out,
                    );
                }
            }

            if let PreHirExpr::Var(rhs_name) = rhs {
                if let Some(rhs_ty) = known_binding_types.get(rhs_name) {
                    // Pointer COPY equality belongs to TypeFlow's value-lifetime
                    // proof. A global source binding may describe a later value
                    // after redefinition; legacy constraints must not bypass it.
                    if !matches!(rhs_ty, NirType::Ptr(_)) {
                        out.entry(lhs_name.clone())
                            .or_default()
                            .push(copy_constraint_from_type(rhs_ty));
                    }
                }
            }

            if let PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let PreHirExpr::PtrOffset { .. } | PreHirExpr::FieldAccess { .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let PreHirExpr::Load { ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }

            if let PreHirExpr::Index { elem_ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }

            if let PreHirExpr::Cast {
                ty: NirType::Ptr(pointee),
                ..
            } = rhs
            {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        PreHirLValue::Deref { ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
        PreHirLValue::Index { base, elem_ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }
            if let PreHirExpr::Var(base_name) = base.as_ref() {
                if matches!(elem_ty, NirType::Float { .. }) {
                    out.entry(base_name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(elem_ty.clone()));
                }
            }
        }
        PreHirLValue::FieldAccess { ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
    }
}

pub(super) fn collect_pointer_assignment_base_constraints(
    rhs: &PreHirExpr,
    ptr_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let NirType::Ptr(pointee) = ptr_ty else {
        return;
    };
    match rhs {
        // Reverse a plain copy only while the source is unknown. A known source
        // can belong to an earlier scalar role while the destination acquired
        // its pointer type from a later register-reuse definition.
        PreHirExpr::Var(name)
            if matches!(known_binding_types.get(name), None | Some(NirType::Unknown)) =>
        {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        PreHirExpr::Var(_) => {}
        PreHirExpr::AddressOfGlobal(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        PreHirExpr::Cast { expr, .. } => {
            collect_pointer_assignment_base_constraints(expr, ptr_ty, known_binding_types, out);
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            // ptr + integer = pointer. Only promote a Var base when the other
            // side is a *numeric* offset (const / const arithmetic / positive integer evidence).
            // Do not emit bilateral constraints when neither operand is established as an offset.
            let rhs_is_offset =
                expr_has_positive_integer_offset_evidence(rhs.as_ref(), known_binding_types);
            let lhs_is_offset =
                expr_has_positive_integer_offset_evidence(lhs.as_ref(), known_binding_types);
            if rhs_is_offset && !lhs_is_offset {
                if let PreHirExpr::Var(name) = strip_casts_unary(lhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            } else if lhs_is_offset && !rhs_is_offset {
                if let PreHirExpr::Var(name) = strip_casts_unary(rhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            ..
        } => {
            if let PreHirExpr::Var(name) = lhs.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        _ => {}
    }
}

fn strip_casts_unary(expr: &PreHirExpr) -> &PreHirExpr {
    let mut cur = expr;
    while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    cur
}

/// Integer offset in pointer arithmetic: const, index vars, scaled index.
/// An integer cast of a bare variable is not sufficient offset evidence; it can
/// be a pointer base forced through integer ALU.
fn expr_is_numeric_offset(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Var(_) => true,
        PreHirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            PreHirExpr::Const(_, _) => true,
            PreHirExpr::Binary { .. } => expr_is_numeric_offset(inner),
            // Bare var cast to int is ambiguous (often ptr-to-int for end calc).
            PreHirExpr::Var(_) => false,
            other => expr_is_numeric_offset(other),
        },
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_is_numeric_offset(expr)
        }
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => expr_is_numeric_offset(lhs) && expr_is_numeric_offset(rhs),
        _ => false,
    }
}

fn expr_has_positive_integer_offset_evidence(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Var(name) => matches!(
            known_binding_types.get(name),
            Some(NirType::Int { .. } | NirType::Bool)
        ),
        PreHirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            PreHirExpr::Var(name) => matches!(
                known_binding_types.get(name),
                Some(NirType::Int { .. } | NirType::Bool)
            ),
            _ => expr_has_positive_integer_offset_evidence(inner, known_binding_types),
        },
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_has_positive_integer_offset_evidence(expr, known_binding_types)
        }
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Mul | PreHirBinaryOp::Shl | PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
            ..
        } => true,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            expr_has_positive_integer_offset_evidence(lhs, known_binding_types)
                || expr_has_positive_integer_offset_evidence(rhs, known_binding_types)
        }
        _ => false,
    }
}

fn expr_is_pointer_offset_like(expr: &PreHirExpr) -> bool {
    expr_is_numeric_offset(expr)
}

fn copy_constraint_from_type(ty: &NirType) -> UseConstraint {
    match ty {
        NirType::Ptr(pointee) => UseConstraint::Ptr(pointee.as_ref().clone()),
        _ => UseConstraint::Exact(ty.clone()),
    }
}
