//! Deterministic expression formatting for normalize **keys** / **diagnostics**.
//!
//! # Not the dual-layer C printer
//!
//! Human-readable NIR/HIR C presentation lives in `fission-pcode` `render/`
//! ([ADR 0011](../../../../../docs/adr/0011-hir-presentation-contract.md),
//! [ADR 0013](../../../../../docs/adr/0013-print-expr-vs-dual-layer-printer.md)).
//! This helper exists so midend-normalize/structuring can format expressions
//! without depending on the render crate (and without creating crate cycles).
//!
//! Named `format_expr_key` (not `print_expr`) to avoid confusion with the dual-layer
//! presentation surface in `fission-pcode::render`.

use crate::ir::{HirBinaryOp, HirExpr, HirLValue, HirUnaryOp};

/// Deterministic stringization of `HirExpr` for sort keys, GVN ties, and diagnostics.
pub fn format_expr_key(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Var(name) => name.clone(),
        HirExpr::AddressOfGlobal(name) => format!("&{name}"),
        HirExpr::Const(v, _) => v.to_string(),
        HirExpr::Cast { ty, expr } => format!("({ty:?}){}", format_expr_key(expr)),
        HirExpr::Unary { op, expr, .. } => match op {
            HirUnaryOp::Not => format!("!{}", format_expr_key(expr)),
            HirUnaryOp::Neg => format!("-{}", format_expr_key(expr)),
            HirUnaryOp::BitNot => format!("~{}", format_expr_key(expr)),
        },
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let op_s = match op {
                HirBinaryOp::Add => "+",
                HirBinaryOp::Sub => "-",
                HirBinaryOp::Mul => "*",
                HirBinaryOp::Div => "/",
                HirBinaryOp::Mod => "%",
                HirBinaryOp::And => "&",
                HirBinaryOp::Or => "|",
                HirBinaryOp::Xor => "^",
                HirBinaryOp::Shl => "<<",
                HirBinaryOp::Shr | HirBinaryOp::Sar => ">>",
                HirBinaryOp::Eq => "==",
                HirBinaryOp::Ne => "!=",
                HirBinaryOp::Lt | HirBinaryOp::SLt => "<",
                HirBinaryOp::Le | HirBinaryOp::SLe => "<=",
                HirBinaryOp::Gt | HirBinaryOp::SGt => ">",
                HirBinaryOp::Ge | HirBinaryOp::SGe => ">=",
                HirBinaryOp::LogicalAnd => "&&",
                HirBinaryOp::LogicalOr => "||",
            };
            format!(
                "({} {} {})",
                format_expr_key(lhs),
                op_s,
                format_expr_key(rhs)
            )
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => format!(
            "({} ? {} : {})",
            format_expr_key(cond),
            format_expr_key(then_expr),
            format_expr_key(else_expr)
        ),
        HirExpr::Call { target, args, .. } => {
            let args_s = args
                .iter()
                .map(format_expr_key)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{target}({args_s})")
        }
        HirExpr::Load { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        HirExpr::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", format_expr_key(base), field_name),
        HirExpr::Index { base, index, .. } => {
            format!(
                "{}[{}]",
                format_expr_key(base),
                format_expr_key(index)
            )
        }
        HirExpr::PtrOffset { base, offset } => {
            format!("({} + {offset})", format_expr_key(base))
        }
        HirExpr::AggregateCopy { src, size } => {
            format!("memcpy({}, {size})", format_expr_key(src))
        }
    }
}

/// Deterministic stringization of `HirLValue` for keys / diagnostics.
pub fn format_lvalue_key(lv: &HirLValue) -> String {
    match lv {
        HirLValue::Var(n) => n.clone(),
        HirLValue::Deref { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        HirLValue::Index { base, index, .. } => {
            format!(
                "{}[{}]",
                format_expr_key(base),
                format_expr_key(index)
            )
        }
        HirLValue::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", format_expr_key(base), field_name),
    }
}

