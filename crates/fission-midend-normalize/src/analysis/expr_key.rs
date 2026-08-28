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
        NirType::Ptr(_) => "p".to_string(),
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
