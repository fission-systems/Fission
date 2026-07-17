use crate::ir::{HirExpr, NirType};

pub fn expr_type(expr: &HirExpr) -> NirType {
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

pub fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

pub fn expr_has_side_effecting_call(expr: &HirExpr) -> bool {
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
    use super::expr_has_side_effecting_call;
    use crate::ir::{HirExpr, NirType};

    fn call(target: &str) -> HirExpr {
        HirExpr::Call {
            target: target.to_string(),
            args: vec![HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )],
            ty: NirType::Bool,
        }
    }

    #[test]
    fn side_effecting_call_fact_distinguishes_pcode_intrinsics_from_regular_calls() {
        for target in ["__carry", "__scarry", "__sborrow", "__popcount"] {
            assert!(!expr_has_side_effecting_call(&call(target)), "{target}");
        }
        assert!(expr_has_side_effecting_call(&call("callee")));
    }
}
