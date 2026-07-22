use crate::prelude::*;
use fission_midend_dir::util::expr_type;
use crate::HashMap;

/// Identifies redundant floating-point widening and narrowing cast chains,
/// narrowing intermediate double-precision calculations to single-precision float
/// operations (Ghidra's SubfloatFlow/RuleSubfloatConvert equivalent).
pub fn apply_subfloat_flow_pass(func: &mut DirFunction) -> bool {
    // 1. Build type map of variables from parameters and locals
    let mut var_types = HashMap::default();
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

fn resolve_expr_type(expr: &DirExpr, var_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        DirExpr::Var(name) => var_types.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

fn narrow_float_expression(
    expr: &DirExpr,
    var_types: &HashMap<String, NirType>,
) -> Option<DirExpr> {
    match expr {
        // A widening cast from float(32) to float(64): if we are narrowing, we can elide the cast!
        DirExpr::Cast {
            ty: NirType::Float { bits: 64 },
            expr: inner,
        } => {
            if let NirType::Float { bits: 32 } = resolve_expr_type(inner, var_types) {
                return Some((**inner).clone());
            }
        }
        // Floating-point binary operations (+, -, *, /) on double-precision inputs
        DirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: NirType::Float { bits: 64 },
        } => {
            if matches!(
                op,
                DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Mul | DirBinaryOp::Div
            ) {
                if let (Some(narrowed_lhs), Some(narrowed_rhs)) = (
                    narrow_float_expression(lhs, var_types),
                    narrow_float_expression(rhs, var_types),
                ) {
                    return Some(DirExpr::Binary {
                        op: *op,
                        lhs: Box::new(narrowed_lhs),
                        rhs: Box::new(narrowed_rhs),
                        ty: NirType::Float { bits: 32 },
                    });
                }
            }
        }
        // Double-precision constants narrowed to single-precision
        DirExpr::Const(val, NirType::Float { bits: 64 }) => {
            return Some(DirExpr::Const(*val, NirType::Float { bits: 32 }));
        }
        _ => {}
    }
    None
}

fn visit_expr(expr: &mut DirExpr, var_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // First recurse into subexpressions
    match expr {
        DirExpr::Cast { expr: inner, .. } => {
            changed |= visit_expr(inner, var_types);
        }
        DirExpr::Unary { expr: inner, .. } => {
            changed |= visit_expr(inner, var_types);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs, var_types);
            changed |= visit_expr(rhs, var_types);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= visit_expr(cond, var_types);
            changed |= visit_expr(then_expr, var_types);
            changed |= visit_expr(else_expr, var_types);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg, var_types);
            }
        }
        DirExpr::Load { ptr, .. } => {
            changed |= visit_expr(ptr, var_types);
        }
        DirExpr::PtrOffset { base, .. } => {
            changed |= visit_expr(base, var_types);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base, var_types);
            changed |= visit_expr(index, var_types);
        }
        DirExpr::AggregateCopy { src, .. } => {
            changed |= visit_expr(src, var_types);
        }
        _ => {}
    }

    // Attempt narrowing on the current expression if it's a Cast to single-precision float(32)
    if let DirExpr::Cast {
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

fn visit_stmt(stmt: &mut DirStmt, var_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            changed |= visit_expr(rhs, var_types);
            match lhs {
                DirLValue::Deref { ptr, .. } => {
                    changed |= visit_expr(ptr, var_types);
                }
                DirLValue::Index { base, index, .. } => {
                    changed |= visit_expr(base, var_types);
                    changed |= visit_expr(index, var_types);
                }
                _ => {}
            }
        }
        DirStmt::Expr(expr) => {
            changed |= visit_expr(expr, var_types);
        }
        DirStmt::VaStart { va_list, .. } => {
            changed |= visit_expr(va_list, var_types);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for s in body {
                changed |= visit_stmt(s, var_types);
            }
        }
        DirStmt::If {
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
        DirStmt::Switch {
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
        DirStmt::Return(Some(expr)) => {
            changed |= visit_expr(expr, var_types);
        }
        _ => {}
    }
    changed
}
