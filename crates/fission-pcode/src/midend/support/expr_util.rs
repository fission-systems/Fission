use super::*;

pub(crate) fn expr_type(expr: &DirExpr) -> NirType {
    match expr {
        DirExpr::AddressOfGlobal(_) => NirType::Ptr(Box::new(NirType::Unknown)),
        DirExpr::Var(_) => NirType::Unknown,
        DirExpr::Const(_, ty)
        | DirExpr::Unary { ty, .. }
        | DirExpr::Binary { ty, .. }
        | DirExpr::Select { ty, .. }
        | DirExpr::Call { ty, .. }
        | DirExpr::Load { ty, .. }
        | DirExpr::FieldAccess { ty, .. }
        | DirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        DirExpr::Cast { ty, .. } => ty.clone(),
        DirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        DirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
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
pub(crate) fn is_identity_var_assign(lhs: &DirLValue, rhs: &DirExpr) -> bool {
    matches!(
        (lhs, rhs),
        (DirLValue::Var(a), DirExpr::Var(b)) if a == b
    )
}

pub(crate) fn is_identity_var_assign_stmt(stmt: &DirStmt) -> bool {
    matches!(
        stmt,
        DirStmt::Assign { lhs, rhs } if is_identity_var_assign(lhs, rhs)
    )
}

pub(crate) fn expr_has_side_effecting_call(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Call { target, args, .. } => {
            !is_pure_intrinsic_call(target) || args.iter().any(expr_has_side_effecting_call)
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            expr_has_side_effecting_call(expr)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effecting_call(lhs) || expr_has_side_effecting_call(rhs)
        }
        DirExpr::Load { ptr, .. } => expr_has_side_effecting_call(ptr),
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
            expr_has_side_effecting_call(base)
        }
        DirExpr::Index { base, index, .. } => {
            expr_has_side_effecting_call(base) || expr_has_side_effecting_call(index)
        }
        DirExpr::AggregateCopy { src, .. } => expr_has_side_effecting_call(src),
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effecting_call(cond)
                || expr_has_side_effecting_call(then_expr)
                || expr_has_side_effecting_call(else_expr)
        }
        DirExpr::Var(_, ..) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, ..) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_var_assign_detects_pure_self_copy_only() {
        let lhs = DirLValue::Var("uVar1".into());
        assert!(is_identity_var_assign(
            &lhs,
            &DirExpr::Var("uVar1".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &DirExpr::Var("uVar2".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &DirExpr::Binary {
                op: crate::midend::DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("uVar1".into())),
                rhs: Box::new(DirExpr::Const(1, NirType::Int {
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
            &DirExpr::Call {
                target: "f".into(),
                args: vec![],
                ty: NirType::Unknown,
            }
        ));
        assert!(is_identity_var_assign_stmt(&DirStmt::Assign {
            lhs: DirLValue::Var("rbx".into()),
            rhs: DirExpr::Var("rbx".into()),
        }));
    }
}

