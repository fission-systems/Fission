use crate::ir::{NirType, PreHirExpr};

pub fn expr_type(expr: &PreHirExpr) -> NirType {
    match expr {
        PreHirExpr::AddressOfGlobal(_) => NirType::Ptr(Box::new(NirType::Unknown)),
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

pub fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

pub fn expr_has_side_effecting_call(expr: &PreHirExpr) -> bool {
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
        PreHirExpr::Var(_, ..) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, ..) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::expr_has_side_effecting_call;
    use crate::ir::{NirType, PreHirExpr};

    fn call(target: &str) -> PreHirExpr {
        PreHirExpr::Call {
            target: target.to_string(),
            args: vec![PreHirExpr::Const(
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
