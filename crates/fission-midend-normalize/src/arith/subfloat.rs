use crate::prelude::*;
use fission_midend_core::expr_type;
use std::collections::HashMap;

/// Identifies redundant floating-point widening and narrowing cast chains,
/// narrowing intermediate double-precision calculations to single-precision float
/// operations (Ghidra's SubfloatFlow/RuleSubfloatConvert equivalent).
pub fn apply_subfloat_flow_pass(func: &mut HirFunction) -> bool {
    // 1. Build type map of variables from parameters and locals
    let mut var_types = HashMap::new();
    for binding in &func.params {
        var_types.insert(binding.name.clone(), binding.ty.clone());
    }
    for binding in &func.locals {
        var_types.insert(binding.name.clone(), binding.ty.clone());
    }

    // 2. Walk statements recursively to narrow float expressions
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= visit_stmt(stmt, &var_types);
    }

    changed
}

fn resolve_expr_type(expr: &HirExpr, var_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => var_types.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

fn narrow_float_expression(
    expr: &HirExpr,
    var_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    match expr {
        // A widening cast from float(32) to float(64): if we are narrowing, we can elide the cast!
        HirExpr::Cast {
            ty: NirType::Float { bits: 64 },
            expr: inner,
        } => {
            if let NirType::Float { bits: 32 } = resolve_expr_type(inner, var_types) {
                return Some((**inner).clone());
            }
        }
        // Floating-point binary operations (+, -, *, /) on double-precision inputs
        HirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: NirType::Float { bits: 64 },
        } => {
            if matches!(
                op,
                HirBinaryOp::Add | HirBinaryOp::Sub | HirBinaryOp::Mul | HirBinaryOp::Div
            ) {
                if let (Some(narrowed_lhs), Some(narrowed_rhs)) = (
                    narrow_float_expression(lhs, var_types),
                    narrow_float_expression(rhs, var_types),
                ) {
                    return Some(HirExpr::Binary {
                        op: *op,
                        lhs: Box::new(narrowed_lhs),
                        rhs: Box::new(narrowed_rhs),
                        ty: NirType::Float { bits: 32 },
                    });
                }
            }
        }
        // Double-precision constants narrowed to single-precision
        HirExpr::Const(val, NirType::Float { bits: 64 }) => {
            return Some(HirExpr::Const(*val, NirType::Float { bits: 32 }));
        }
        _ => {}
    }
    None
}

fn visit_expr(expr: &mut HirExpr, var_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // First recurse into subexpressions
    match expr {
        HirExpr::Cast { expr: inner, .. } => {
            changed |= visit_expr(inner, var_types);
        }
        HirExpr::Unary { expr: inner, .. } => {
            changed |= visit_expr(inner, var_types);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs, var_types);
            changed |= visit_expr(rhs, var_types);
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
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg, var_types);
            }
        }
        HirExpr::Load { ptr, .. } => {
            changed |= visit_expr(ptr, var_types);
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= visit_expr(base, var_types);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base, var_types);
            changed |= visit_expr(index, var_types);
        }
        HirExpr::AggregateCopy { src, .. } => {
            changed |= visit_expr(src, var_types);
        }
        _ => {}
    }

    // Attempt narrowing on the current expression if it's a Cast to single-precision float(32)
    if let HirExpr::Cast {
        ty: NirType::Float { bits: 32 },
        expr: inner,
    } = expr
    {
        if let Some(narrowed) = narrow_float_expression(inner, var_types) {
            *expr = narrowed;
            changed = true;
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
