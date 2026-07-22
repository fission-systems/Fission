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

use crate::ir::{DirBinaryOp, DirExpr, DirLValue, DirUnaryOp};

/// Deterministic stringization of `DirExpr` for sort keys, GVN ties, and diagnostics.
pub fn format_expr_key(expr: &DirExpr) -> String {
    match expr {
        DirExpr::Var(name) => name.clone(),
        DirExpr::AddressOfGlobal(name) => format!("&{name}"),
        DirExpr::Const(v, _) => v.to_string(),
        DirExpr::Cast { ty, expr } => format!("({ty:?}){}", format_expr_key(expr)),
        DirExpr::Unary { op, expr, .. } => match op {
            DirUnaryOp::Not => format!("!{}", format_expr_key(expr)),
            DirUnaryOp::Neg => format!("-{}", format_expr_key(expr)),
            DirUnaryOp::BitNot => format!("~{}", format_expr_key(expr)),
        },
        DirExpr::Binary { op, lhs, rhs, .. } => {
            let op_s = match op {
                DirBinaryOp::Add => "+",
                DirBinaryOp::Sub => "-",
                DirBinaryOp::Mul => "*",
                DirBinaryOp::Div => "/",
                DirBinaryOp::Mod => "%",
                DirBinaryOp::And => "&",
                DirBinaryOp::Or => "|",
                DirBinaryOp::Xor => "^",
                DirBinaryOp::Shl => "<<",
                DirBinaryOp::Shr | DirBinaryOp::Sar => ">>",
                DirBinaryOp::Eq => "==",
                DirBinaryOp::Ne => "!=",
                DirBinaryOp::Lt | DirBinaryOp::SLt => "<",
                DirBinaryOp::Le | DirBinaryOp::SLe => "<=",
                DirBinaryOp::Gt | DirBinaryOp::SGt => ">",
                DirBinaryOp::Ge | DirBinaryOp::SGe => ">=",
                DirBinaryOp::LogicalAnd => "&&",
                DirBinaryOp::LogicalOr => "||",
            };
            format!(
                "({} {} {})",
                format_expr_key(lhs),
                op_s,
                format_expr_key(rhs)
            )
        }
        DirExpr::Select {
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
        DirExpr::Call { target, args, .. } => {
            let args_s = args
                .iter()
                .map(format_expr_key)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{target}({args_s})")
        }
        DirExpr::Load { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        DirExpr::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", format_expr_key(base), field_name),
        DirExpr::Index { base, index, .. } => {
            format!(
                "{}[{}]",
                format_expr_key(base),
                format_expr_key(index)
            )
        }
        DirExpr::PtrOffset { base, offset } => {
            format!("({} + {offset})", format_expr_key(base))
        }
        DirExpr::AggregateCopy { src, size } => {
            format!("memcpy({}, {size})", format_expr_key(src))
        }
    }
}

/// Deterministic stringization of `DirLValue` for keys / diagnostics.
pub fn format_lvalue_key(lv: &DirLValue) -> String {
    match lv {
        DirLValue::Var(n) => n.clone(),
        DirLValue::Deref { ptr, .. } => format!("*{}", format_expr_key(ptr)),
        DirLValue::Index { base, index, .. } => {
            format!(
                "{}[{}]",
                format_expr_key(base),
                format_expr_key(index)
            )
        }
        DirLValue::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", format_expr_key(base), field_name),
    }
}

