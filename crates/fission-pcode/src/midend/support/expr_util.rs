use super::*;

pub(crate) fn expr_type(expr: &PreHirExpr) -> NirType {
    match expr {
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
            NirType::Ptr(Box::new(NirType::Unknown))
        }
        PreHirExpr::Var(_) => NirType::Unknown,
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. }
        | PreHirExpr::FieldAccess { ty, .. }
        | PreHirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        PreHirExpr::Cast { ty, .. } => ty.clone(),
        PreHirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        PreHirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
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
pub(crate) fn is_identity_var_assign(lhs: &PreHirLValue, rhs: &PreHirExpr) -> bool {
    matches!(
        (lhs, rhs),
        (PreHirLValue::Var(a), PreHirExpr::Var(b)) if a == b
    )
}

pub(crate) fn is_identity_var_assign_stmt(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign { lhs, rhs } if is_identity_var_assign(lhs, rhs)
    )
}

pub(crate) fn expr_has_side_effecting_call(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            !is_pure_intrinsic_call(target) || args.iter().any(expr_has_side_effecting_call)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_has_side_effecting_call(expr)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effecting_call(lhs) || expr_has_side_effecting_call(rhs)
        }
        PreHirExpr::Load { ptr, .. } => expr_has_side_effecting_call(ptr),
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            expr_has_side_effecting_call(base)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_has_side_effecting_call(base) || expr_has_side_effecting_call(index)
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_has_side_effecting_call(src),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effecting_call(cond)
                || expr_has_side_effecting_call(then_expr)
                || expr_has_side_effecting_call(else_expr)
        }
        PreHirExpr::Var(_, ..)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, ..) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_var_assign_detects_pure_self_copy_only() {
        let lhs = PreHirLValue::Var("uVar1".into());
        assert!(is_identity_var_assign(
            &lhs,
            &PreHirExpr::Var("uVar1".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &PreHirExpr::Var("uVar2".into())
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &PreHirExpr::Binary {
                op: crate::midend::PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("uVar1".into())),
                rhs: Box::new(PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: true
                    }
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: true
                },
            }
        ));
        assert!(!is_identity_var_assign(
            &lhs,
            &PreHirExpr::Call {
                target: "f".into(),
                args: vec![],
                ty: NirType::Unknown,
            }
        ));
        assert!(is_identity_var_assign_stmt(&PreHirStmt::Assign {
            lhs: PreHirLValue::Var("rbx".into()),
            rhs: PreHirExpr::Var("rbx".into()),
        }));
    }
}
