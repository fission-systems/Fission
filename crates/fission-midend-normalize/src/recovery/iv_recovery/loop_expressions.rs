use crate::HashSet;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

pub(super) fn invert_condition(expr: PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            let inverted = match op {
                PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Ne),
                PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Eq),
                PreHirBinaryOp::Lt => Some(PreHirBinaryOp::Ge),
                PreHirBinaryOp::Le => Some(PreHirBinaryOp::Gt),
                PreHirBinaryOp::Gt => Some(PreHirBinaryOp::Le),
                PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Lt),
                PreHirBinaryOp::SLt => Some(PreHirBinaryOp::SGe),
                PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGt),
                PreHirBinaryOp::SGt => Some(PreHirBinaryOp::SLe),
                PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLt),
                _ => None,
            };
            if let Some(op) = inverted {
                PreHirExpr::Binary { op, lhs, rhs, ty }
            } else {
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Binary { op, lhs, rhs, ty }),
                    ty: NirType::Bool,
                }
            }
        }
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

/// Collect all variable names mentioned in an expression.
pub(super) fn expr_vars(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => expr_vars(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_vars(lhs, out);
            expr_vars(rhs, out);
        }
        PreHirExpr::Load { ptr, .. } => expr_vars(ptr, out),
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            expr_vars(base, out)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args {
                expr_vars(a, out);
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_vars(src, out),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_vars(cond, out);
            expr_vars(then_expr, out);
            expr_vars(else_expr, out);
        }
        PreHirExpr::Const(_, _) => {}
    }
}

pub(super) fn strip_casts(expr: &PreHirExpr) -> &PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr, .. } => strip_casts(expr),
        _ => expr,
    }
}

pub(super) fn stripped_var_name(expr: &PreHirExpr) -> Option<&str> {
    match strip_casts(expr) {
        PreHirExpr::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

pub(super) fn vars_equivalent_after_casts(a: &PreHirExpr, b: &PreHirExpr) -> bool {
    matches!((stripped_var_name(a), stripped_var_name(b)), (Some(x), Some(y)) if x == y)
}

pub(super) fn is_zero(expr: &PreHirExpr) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Const(0, _))
}

fn is_one(expr: &PreHirExpr) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Const(1, _))
}

pub(super) fn type_size_bytes(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } | NirType::Float { bits } => (*bits / 8).max(1),
        NirType::Ptr(_) => 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Unknown => 1,
    }
}

pub(super) fn is_const_val(expr: &PreHirExpr, val: i64) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Const(v, _) if *v == val)
}

pub(super) fn substitute_var_in_expr(
    expr: &mut PreHirExpr,
    name: &str,
    replacement: &PreHirExpr,
) -> bool {
    match expr {
        PreHirExpr::Var(var) if var == name => {
            *expr = replacement.clone();
            true
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            substitute_var_in_expr(expr, name, replacement)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            substitute_var_in_expr(lhs, name, replacement)
                | substitute_var_in_expr(rhs, name, replacement)
        }
        // Not `.any()`: it stops at the first argument that named the
        // variable and left the old name in every argument after it.
        PreHirExpr::Call { args, .. } => args.iter_mut().fold(false, |changed, arg| {
            substitute_var_in_expr(arg, name, replacement) | changed
        }),
        PreHirExpr::PtrOffset { base, .. } => substitute_var_in_expr(base, name, replacement),
        PreHirExpr::Index { base, index, .. } => {
            substitute_var_in_expr(base, name, replacement)
                | substitute_var_in_expr(index, name, replacement)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_var_in_expr(cond, name, replacement)
                | substitute_var_in_expr(then_expr, name, replacement)
                | substitute_var_in_expr(else_expr, name, replacement)
        }
        _ => false,
    }
}

pub(super) fn index_type_for_count(count_expr: &PreHirExpr) -> NirType {
    match expr_type(count_expr) {
        NirType::Int { bits, signed } if bits >= 32 => NirType::Int { bits, signed },
        _ => NirType::Int {
            bits: 64,
            signed: true,
        },
    }
}
