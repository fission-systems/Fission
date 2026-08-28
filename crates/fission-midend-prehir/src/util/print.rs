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

use crate::ir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirUnaryOp};

/// Deterministic stringization of `PreHirExpr` for sort keys, GVN ties, and diagnostics.
pub fn format_expr_key(expr: &PreHirExpr) -> String {
    match expr {
        PreHirExpr::Var(name) => name.clone(),
        PreHirExpr::AddressOfGlobal(name) | PreHirExpr::AddressOfLocal(name) => {
            format!("&{name}")
        }
        PreHirExpr::Const(v, _) => v.to_string(),
        PreHirExpr::Cast { ty, expr } => format!("({ty:?}){}", format_expr_key(expr)),
        PreHirExpr::Unary { op, expr, .. } => match op {
            PreHirUnaryOp::Not => format!("!{}", format_expr_key(expr)),
            PreHirUnaryOp::Neg => format!("-{}", format_expr_key(expr)),
            PreHirUnaryOp::BitNot => format!("~{}", format_expr_key(expr)),
        },
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            let op_s = match op {
                PreHirBinaryOp::Add => "+",
                PreHirBinaryOp::Sub => "-",
                PreHirBinaryOp::Mul => "*",
                PreHirBinaryOp::Div => "/",
                PreHirBinaryOp::Mod => "%",
                PreHirBinaryOp::And => "&",
                PreHirBinaryOp::Or => "|",
                PreHirBinaryOp::Xor => "^",
                PreHirBinaryOp::Shl => "<<",
                PreHirBinaryOp::Shr | PreHirBinaryOp::Sar => ">>",
                PreHirBinaryOp::Eq => "==",
                PreHirBinaryOp::Ne => "!=",
                PreHirBinaryOp::Lt | PreHirBinaryOp::SLt => "<",
                PreHirBinaryOp::Le | PreHirBinaryOp::SLe => "<=",
                PreHirBinaryOp::Gt | PreHirBinaryOp::SGt => ">",
                PreHirBinaryOp::Ge | PreHirBinaryOp::SGe => ">=",
                PreHirBinaryOp::LogicalAnd => "&&",
                PreHirBinaryOp::LogicalOr => "||",
            };
            format!(
                "({} {} {})",
                format_expr_key(lhs),
                op_s,
                format_expr_key(rhs)
            )
        }
        PreHirExpr::Select {
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
        PreHirExpr::Call { target, args, .. } => {
            let args_s = args
                .iter()
                .map(format_expr_key)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{target}({args_s})")
        }
        PreHirExpr::Load { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        PreHirExpr::FieldAccess {
            base, field_name, ..
        } => format!("{}.{}", format_expr_key(base), field_name),
        PreHirExpr::Index { base, index, .. } => {
            format!("{}[{}]", format_expr_key(base), format_expr_key(index))
        }
        PreHirExpr::PtrOffset { base, offset } => {
            format!("({} + {offset})", format_expr_key(base))
        }
        PreHirExpr::AggregateCopy { src, size } => {
            format!("memcpy({}, {size})", format_expr_key(src))
        }
    }
}

/// Deterministic stringization of `PreHirLValue` for keys / diagnostics.
pub fn format_lvalue_key(lv: &PreHirLValue) -> String {
    match lv {
        PreHirLValue::Var(n) => n.clone(),
        PreHirLValue::Deref { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        PreHirLValue::Index { base, index, .. } => {
            format!("{}[{}]", format_expr_key(base), format_expr_key(index))
        }
        PreHirLValue::FieldAccess {
            base, field_name, ..
        } => format!("{}.{}", format_expr_key(base), field_name),
    }
}
