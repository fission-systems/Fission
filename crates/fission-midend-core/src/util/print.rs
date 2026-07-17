//! Deterministic expression rendering for normalize **keys** / **diagnostics**.
//!
//! # Not the dual-layer C printer
//!
//! Human-readable NIR/HIR C presentation lives in `fission-pcode` `render/`
//! ([ADR 0011](../../../../../docs/adr/0011-hir-presentation-contract.md),
//! [ADR 0013](../../../../../docs/adr/0013-print-expr-vs-dual-layer-printer.md)).
//! This helper exists so midend-normalize/structuring can format expressions
//! without depending on the render crate (and without creating crate cycles).

use crate::ir::{HirBinaryOp, HirExpr, HirLValue, HirUnaryOp};

pub fn print_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Var(name) => name.clone(),
        HirExpr::AddressOfGlobal(name) => format!("&{name}"),
        HirExpr::Const(v, _) => v.to_string(),
        HirExpr::Cast { ty, expr } => format!("({ty:?}){}", print_expr(expr)),
        HirExpr::Unary { op, expr, .. } => match op {
            HirUnaryOp::Not => format!("!{}", print_expr(expr)),
            HirUnaryOp::Neg => format!("-{}", print_expr(expr)),
            HirUnaryOp::BitNot => format!("~{}", print_expr(expr)),
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
            format!("({} {} {})", print_expr(lhs), op_s, print_expr(rhs))
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => format!(
            "({} ? {} : {})",
            print_expr(cond),
            print_expr(then_expr),
            print_expr(else_expr)
        ),
        HirExpr::Call { target, args, .. } => {
            let args_s = args.iter().map(print_expr).collect::<Vec<_>>().join(", ");
            format!("{target}({args_s})")
        }
        HirExpr::Load { ptr, .. } => format!("*{}", print_expr(ptr)),
        HirExpr::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", print_expr(base), field_name),
        HirExpr::Index { base, index, .. } => {
            format!("{}[{}]", print_expr(base), print_expr(index))
        }
        HirExpr::PtrOffset { base, offset } => {
            format!("({} + {offset})", print_expr(base))
        }
        HirExpr::AggregateCopy { src, size } => {
            format!("memcpy({}, {size})", print_expr(src))
        }
    }
}

pub fn print_lvalue(lv: &HirLValue) -> String {
    match lv {
        HirLValue::Var(n) => n.clone(),
        HirLValue::Deref { ptr, .. } => format!("*{}", print_expr(ptr)),
        HirLValue::Index { base, index, .. } => {
            format!("{}[{}]", print_expr(base), print_expr(index))
        }
        HirLValue::FieldAccess {
            base,
            field_name,
            ..
        } => format!("{}.{}", print_expr(base), field_name),
    }
}
