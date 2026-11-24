//! Canonical string keys for pure HIR expressions.
//!
//! Shared by the CSE pass and the if-else common-prefix hoisting pass.
//! **Not** used for `Load`/`Call`/`Index` leaves — those return `None` from
//! `pure_expr_key` (same contract as the original CSE-only `expr_key`).

use crate::HashMap;
use crate::prelude::*;

pub type PureExprMap = HashMap<PureExprKey, String>;
pub type PureExprKey = String;

/// Canonical key for a **pure** expression tree (no Load, Call, AggregateCopy, Index).
pub fn pure_expr_key(expr: &PreHirExpr) -> Option<PureExprKey> {
    match expr {
        PreHirExpr::Const(v, ty) => Some(format!("K({},{})", v, type_key(ty))),
        PreHirExpr::Var(name) => Some(format!("V({})", name)),
        PreHirExpr::AddressOfGlobal(name) => Some(format!("A({})", name)),
        PreHirExpr::AddressOfLocal(name) => Some(format!("L({})", name)),
        PreHirExpr::Cast { ty, expr: inner } => {
            let ik = pure_expr_key(inner)?;
            Some(format!("C({},{})", type_key(ty), ik))
        }
        PreHirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
            let ik = pure_expr_key(inner)?;
            Some(format!("U({:?},{},{})", op, type_key(ty), ik))
        }
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            let lk = pure_expr_key(lhs)?;
            let rk = pure_expr_key(rhs)?;
            let (lk, rk) = if is_commutative(*op) && lk > rk {
                (rk, lk)
            } else {
                (lk, rk)
            };
            Some(format!("B({:?},{},{},{})", op, type_key(ty), lk, rk))
        }
        PreHirExpr::PtrOffset { base, offset } => {
            let bk = pure_expr_key(base)?;
            Some(format!("P({},{})", offset, bk))
        }
        PreHirExpr::Load { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::AggregateCopy { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::FieldAccess { .. } => None,
    }
}

pub fn type_key(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "?".to_string(),
        NirType::Bool => "b".to_string(),
        NirType::Int { bits, signed } => format!("i{}s{}", bits, if *signed { 1 } else { 0 }),
        NirType::Ptr(inner) => format!("p({})", type_key(inner)),
        NirType::Aggregate { size, .. } => format!("a{}", size),
        NirType::Float { bits } => format!("f{}", bits),
    }
}

pub fn is_commutative(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Add
            | PreHirBinaryOp::Mul
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
            | PreHirBinaryOp::Eq
            | PreHirBinaryOp::Ne
            | PreHirBinaryOp::LogicalAnd
            | PreHirBinaryOp::LogicalOr
    )
}

/// Remove map entries whose key string embeds `V(name)` for `name`.
pub fn invalidate_pure_map(map: &mut PureExprMap, defined_var: &str) {
    let marker = format!("V({})", defined_var);
    map.retain(|k, _| !k.contains(&marker));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_key_distinguishes_pointer_pointee_types() {
        let ptr_u8 = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let ptr_u64 = NirType::Ptr(Box::new(NirType::Int {
            bits: 64,
            signed: false,
        }));
        let expr_u8 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Cast {
                ty: ptr_u8.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }),
            rhs: Box::new(PreHirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: ptr_u8,
        };
        let expr_u64 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Cast {
                ty: ptr_u64.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }),
            rhs: Box::new(PreHirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: ptr_u64,
        };
        assert_ne!(pure_expr_key(&expr_u8), pure_expr_key(&expr_u64));
    }
}
