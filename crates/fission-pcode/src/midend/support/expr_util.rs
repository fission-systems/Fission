use super::*;

pub(crate) fn expr_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::AddressOfGlobal(_) => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::Var(_) => NirType::Unknown,
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
            size: *size,
            fields: vec![],
        },
    }
}

pub(crate) fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

/// True when `lhs = rhs` is a pure variable identity (`x = x`).
///
/// Distinct p-code ops often collapse onto one binding name; emitting the
/// resulting self-assign adds noise without evaluation order or value change.
/// Cast / load / call RHS are never treated as identity.
pub(crate) fn is_identity_var_assign(lhs: &HirLValue, rhs: &HirExpr) -> bool {
    matches!(
        (lhs, rhs),
        (HirLValue::Var(a), HirExpr::Var(b)) if a == b
    )
}

pub(crate) fn is_identity_var_assign_stmt(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign { lhs, rhs } if is_identity_var_assign(lhs, rhs)
    )
}

pub(crate) fn expr_has_side_effecting_call(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { target, args, .. } => {
            !is_pure_intrinsic_call(target) || args.iter().any(expr_has_side_effecting_call)
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            expr_has_side_effecting_call(expr)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effecting_call(lhs) || expr_has_side_effecting_call(rhs)
        }
        HirExpr::Load { ptr, .. } => expr_has_side_effecting_call(ptr),
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            expr_has_side_effecting_call(base)
        }
        HirExpr::Index { base, index, .. } => {
            expr_has_side_effecting_call(base) || expr_has_side_effecting_call(index)
        }
        HirExpr::AggregateCopy { src, .. } => expr_has_side_effecting_call(src),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effecting_call(cond)
                || expr_has_side_effecting_call(then_expr)
                || expr_has_side_effecting_call(else_expr)
        }
        HirExpr::Var(_, ..) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, ..) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_var_assign_detects_pure_self_copy_only() {
        let lhs = HirLValue::Var("uVar1".into());
        assert!(is_identity_var_assign(
            &lhs,
            &HirExpr::Var("uVar1".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &HirExpr::Var("uVar2".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &HirExpr::Binary {
                op: crate::midend::HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("uVar1".into())),
                rhs: Box::new(HirExpr::Const(1, NirType::Int {
                    bits: 32,
                    signed: true
                })),
                ty: NirType::Int {
                    bits: 32,
                    signed: true
                },
            }
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &HirExpr::Call {
                target: "f".into(),
                args: vec![],
                ty: NirType::Unknown,
            }
        ));
        assert!(is_identity_var_assign_stmt(&HirStmt::Assign {
            lhs: HirLValue::Var("rbx".into()),
            rhs: HirExpr::Var("rbx".into()),
        }));
    }
}

