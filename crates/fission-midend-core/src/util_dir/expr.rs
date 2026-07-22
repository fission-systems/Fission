use crate::ir::{DirExpr, NirType};

pub fn expr_type(expr: &DirExpr) -> NirType {
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

pub fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

pub fn expr_has_side_effecting_call(expr: &DirExpr) -> bool {
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
    use super::expr_has_side_effecting_call;
    use crate::ir::{DirExpr, NirType};

    fn call(target: &str) -> DirExpr {
        DirExpr::Call {
            target: target.to_string(),
            args: vec![DirExpr::Const(
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
