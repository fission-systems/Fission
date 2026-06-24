use super::super::*;
use crate::nir::support::expr_type;
use std::collections::HashMap;

/// Normalizes floating-point sign bit manipulation patterns (equivalent to Ghidra's RuleFloatSign):
/// - `x & 0x7fffffff` -> `fabsf(x)`
/// - `x ^ 0x80000000` -> `-x`
/// - `y & 0x7fffffffffffffff` -> `fabs(y)`
/// - `y ^ 0x8000000000000000` -> `-y`
pub(crate) fn apply_float_sign_pass(func: &mut HirFunction) -> bool {
    let mut var_types = HashMap::new();
    for binding in &func.params {
        var_types.insert(binding.name.clone(), binding.ty.clone());
    }
    for binding in &func.locals {
        var_types.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut changed = false;
    for stmt in &mut func.body {
        changed |= visit_stmt(stmt, &var_types);
    }
    changed
}

fn resolve_float_expr(
    expr: &HirExpr,
    var_types: &HashMap<String, NirType>,
) -> Option<(HirExpr, u32)> {
    match expr {
        HirExpr::Cast { expr: inner, .. } => resolve_float_expr(inner, var_types),
        HirExpr::Var(name) => {
            if let Some(NirType::Float { bits }) = var_types.get(name) {
                Some((expr.clone(), *bits))
            } else {
                None
            }
        }
        _ => {
            if let NirType::Float { bits } = expr_type(expr) {
                Some((expr.clone(), bits))
            } else {
                None
            }
        }
    }
}

fn matches_abs_mask(val: i64, bits: u32) -> bool {
    match bits {
        32 => val == 0x7fffffff || val == 2147483647,
        64 => val == 0x7fffffffffffffff || val == i64::MAX,
        _ => false,
    }
}

fn matches_neg_mask(val: i64, bits: u32) -> bool {
    match bits {
        32 => val == 0x80000000 || val == 2147483648 || val == -2147483648,
        64 => val == i64::MIN || val as u64 == 0x8000000000000000_u64,
        _ => false,
    }
}

fn visit_expr(expr: &mut HirExpr, var_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // Recurse first
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            changed |= visit_expr(inner, var_types);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs, var_types);
            changed |= visit_expr(rhs, var_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg, var_types);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= visit_expr(cond, var_types);
            changed |= visit_expr(then_expr, var_types);
            changed |= visit_expr(else_expr, var_types);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base, var_types);
            changed |= visit_expr(index, var_types);
        }
        _ => {}
    }

    // Try simplifying FLOAT_ABS / FLOAT_NEG patterns
    if let HirExpr::Binary { op, lhs, rhs, .. } = expr {
        if *op == HirBinaryOp::And {
            if let Some((inner, bits)) = resolve_float_expr(lhs, var_types) {
                if let HirExpr::Const(val, _) = rhs.as_ref() {
                    if matches_abs_mask(*val, bits) {
                        let fn_name = if bits == 32 { "fabsf" } else { "fabs" };
                        *expr = HirExpr::Call {
                            target: fn_name.to_string(),
                            args: vec![inner],
                            ty: NirType::Float { bits },
                        };
                        return true;
                    }
                }
            }
            if let Some((inner, bits)) = resolve_float_expr(rhs, var_types) {
                if let HirExpr::Const(val, _) = lhs.as_ref() {
                    if matches_abs_mask(*val, bits) {
                        let fn_name = if bits == 32 { "fabsf" } else { "fabs" };
                        *expr = HirExpr::Call {
                            target: fn_name.to_string(),
                            args: vec![inner],
                            ty: NirType::Float { bits },
                        };
                        return true;
                    }
                }
            }
        } else if *op == HirBinaryOp::Xor {
            if let Some((inner, bits)) = resolve_float_expr(lhs, var_types) {
                if let HirExpr::Const(val, _) = rhs.as_ref() {
                    if matches_neg_mask(*val, bits) {
                        *expr = HirExpr::Unary {
                            op: HirUnaryOp::Neg,
                            expr: Box::new(inner),
                            ty: NirType::Float { bits },
                        };
                        return true;
                    }
                }
            }
            if let Some((inner, bits)) = resolve_float_expr(rhs, var_types) {
                if let HirExpr::Const(val, _) = lhs.as_ref() {
                    if matches_neg_mask(*val, bits) {
                        *expr = HirExpr::Unary {
                            op: HirUnaryOp::Neg,
                            expr: Box::new(inner),
                            ty: NirType::Float { bits },
                        };
                        return true;
                    }
                }
            }
        }
    }

    changed
}

fn visit_stmt(stmt: &mut HirStmt, var_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= visit_expr(rhs, var_types);
            match lhs {
                HirLValue::Deref { ptr, .. } => {
                    changed |= visit_expr(ptr, var_types);
                }
                HirLValue::Index { base, index, .. } => {
                    changed |= visit_expr(base, var_types);
                    changed |= visit_expr(index, var_types);
                }
                _ => {}
            }
        }
        HirStmt::Expr(expr) => {
            changed |= visit_expr(expr, var_types);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= visit_expr(va_list, var_types);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for s in body {
                changed |= visit_stmt(s, var_types);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= visit_expr(cond, var_types);
            for s in then_body {
                changed |= visit_stmt(s, var_types);
            }
            for s in else_body {
                changed |= visit_stmt(s, var_types);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= visit_expr(expr, var_types);
            for case in cases {
                for s in &mut case.body {
                    changed |= visit_stmt(s, var_types);
                }
            }
            for s in default {
                changed |= visit_stmt(s, var_types);
            }
        }
        HirStmt::Return(Some(expr)) => {
            changed |= visit_expr(expr, var_types);
        }
        _ => {}
    }
    changed
}
