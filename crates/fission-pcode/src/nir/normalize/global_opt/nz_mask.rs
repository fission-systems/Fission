use super::super::*;
use std::collections::HashMap;
use crate::nir::support::expr_type;

pub(crate) fn type_mask(ty: &NirType) -> u64 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => {
            if *bits >= 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            }
        }
        _ => u64::MAX,
    }
}

fn type_bits(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        _ => 64,
    }
}

fn get_expr_type(expr: &HirExpr, type_map: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => type_map.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

pub(crate) fn eval_expr_nz_mask(
    expr: &HirExpr,
    var_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> u64 {
    match expr {
        HirExpr::Const(val, _) => *val as u64,
        HirExpr::Var(name) => *var_masks.get(name).unwrap_or(&u64::MAX),
        HirExpr::Cast { ty, expr } => {
            let inner_mask = eval_expr_nz_mask(expr, var_masks, type_map);
            let outer_mask = type_mask(ty);
            let inner_ty = get_expr_type(expr, type_map);
            if let NirType::Int { bits: inner_bits, signed: true } = inner_ty {
                if let NirType::Int { bits: outer_bits, .. } = ty {
                    if *outer_bits > inner_bits {
                        // Sign extension check
                        let sign_bit = 1u64 << (inner_bits - 1);
                        if (inner_mask & sign_bit) != 0 {
                            return inner_mask | (outer_mask & !type_mask(&inner_ty));
                        }
                    }
                }
            }
            inner_mask & outer_mask
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let left_mask = eval_expr_nz_mask(lhs, var_masks, type_map);
            let right_mask = eval_expr_nz_mask(rhs, var_masks, type_map);
            let out_mask = type_mask(ty);
            match op {
                HirBinaryOp::And => left_mask & right_mask & out_mask,
                HirBinaryOp::Or | HirBinaryOp::Xor => (left_mask | right_mask) & out_mask,
                HirBinaryOp::Add => {
                    let h1 = 64 - left_mask.leading_zeros();
                    let h2 = 64 - right_mask.leading_zeros();
                    let max_h = std::cmp::max(h1, h2);
                    let sum_h = std::cmp::min(max_h + 1, type_bits(ty));
                    if sum_h >= 64 {
                        out_mask
                    } else {
                        ((1u64 << sum_h) - 1) & out_mask
                    }
                }
                HirBinaryOp::Shl => {
                    if let HirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa >= 64 {
                            0
                        } else {
                            (left_mask << sa) & out_mask
                        }
                    } else {
                        out_mask
                    }
                }
                HirBinaryOp::Shr => {
                    if let HirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa >= 64 {
                            0
                        } else {
                            (left_mask >> sa) & out_mask
                        }
                    } else {
                        out_mask
                    }
                }
                HirBinaryOp::Sar => {
                    if let HirExpr::Const(sa, _) = &**rhs {
                        let sa = *sa as u32;
                        if sa >= 64 {
                            out_mask
                        } else {
                            let shifted = left_mask >> sa;
                            let sign_bit = 1u64 << (type_bits(ty) - 1);
                            if (left_mask & sign_bit) != 0 {
                                (shifted | (out_mask & !(out_mask >> sa))) & out_mask
                            } else {
                                shifted & out_mask
                            }
                        }
                    } else {
                        out_mask
                    }
                }
                HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr |
                HirBinaryOp::Eq | HirBinaryOp::Ne |
                HirBinaryOp::Lt | HirBinaryOp::Le | HirBinaryOp::Gt | HirBinaryOp::Ge |
                HirBinaryOp::SLt | HirBinaryOp::SLe | HirBinaryOp::SGt | HirBinaryOp::SGe => {
                    1
                }
                _ => out_mask,
            }
        }
        HirExpr::Select { then_expr, else_expr, ty, .. } => {
            let t_mask = eval_expr_nz_mask(then_expr, var_masks, type_map);
            let e_mask = eval_expr_nz_mask(else_expr, var_masks, type_map);
            (t_mask | e_mask) & type_mask(ty)
        }
        HirExpr::Unary { op, expr, ty } => {
            let out_mask = type_mask(ty);
            match op {
                HirUnaryOp::Not => 1,
                _ => out_mask,
            }
        }
        _ => {
            let ty = expr_type(expr);
            type_mask(&ty)
        }
    }
}

fn collect_assignments_in_stmt(
    stmt: &HirStmt,
    var_masks: &mut HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs: HirLValue::Var(name), rhs } => {
            let new_mask = eval_expr_nz_mask(rhs, var_masks, type_map);
            let old_mask = var_masks.get(name).copied().unwrap_or(0);
            let merged = old_mask | new_mask;
            if merged != old_mask {
                var_masks.insert(name.clone(), merged);
                changed = true;
            }
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => {
            for s in stmts {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        HirStmt::If { then_body, else_body, .. } => {
            for s in then_body {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
            for s in else_body {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        HirStmt::For { init, update, body, .. } => {
            if let Some(i) = init {
                changed |= collect_assignments_in_stmt(i, var_masks, type_map);
            }
            if let Some(u) = update {
                changed |= collect_assignments_in_stmt(u, var_masks, type_map);
            }
            for s in body {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    changed |= collect_assignments_in_stmt(s, var_masks, type_map);
                }
            }
            for s in default {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        _ => {}
    }
    changed
}

pub(crate) fn compute_nz_masks(func: &HirFunction) -> HashMap<String, u64> {
    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut var_masks = HashMap::new();
    for param in &func.params {
        var_masks.insert(param.name.clone(), type_mask(&param.ty));
    }
    for local in &func.locals {
        var_masks.insert(local.name.clone(), 0);
    }

    let mut iterations = 0;
    while iterations < 20 {
        let mut changed = false;
        for stmt in &func.body {
            changed |= collect_assignments_in_stmt(stmt, &mut var_masks, &type_map);
        }
        if !changed {
            break;
        }
        iterations += 1;
    }

    for (name, ty) in &type_map {
        if !var_masks.contains_key(name) {
            var_masks.insert(name.clone(), type_mask(ty));
        }
    }

    var_masks
}

fn simplify_expr(
    expr: &mut HirExpr,
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            changed |= simplify_expr(inner, nz_masks, type_map);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs, nz_masks, type_map);
            changed |= simplify_expr(rhs, nz_masks, type_map);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg, nz_masks, type_map);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base, nz_masks, type_map);
            changed |= simplify_expr(index, nz_masks, type_map);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            changed |= simplify_expr(cond, nz_masks, type_map);
            changed |= simplify_expr(then_expr, nz_masks, type_map);
            changed |= simplify_expr(else_expr, nz_masks, type_map);
        }
        _ => {}
    }

    if let HirExpr::Binary { op: HirBinaryOp::And, lhs, rhs, .. } = expr {
        if let HirExpr::Const(mask, _) = &**rhs {
            let active = eval_expr_nz_mask(lhs, nz_masks, type_map);
            if (active & !(*mask as u64)) == 0 {
                *expr = (**lhs).clone();
                return true;
            }
        } else if let HirExpr::Const(mask, _) = &**lhs {
            let active = eval_expr_nz_mask(rhs, nz_masks, type_map);
            if (active & !(*mask as u64)) == 0 {
                *expr = (**rhs).clone();
                return true;
            }
        }
    }

    changed
}

fn simplify_stmt(
    stmt: &mut HirStmt,
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            match lhs {
                HirLValue::Deref { ptr, .. } => {
                    changed |= simplify_expr(ptr, nz_masks, type_map);
                }
                HirLValue::Index { base, index, .. } => {
                    changed |= simplify_expr(base, nz_masks, type_map);
                    changed |= simplify_expr(index, nz_masks, type_map);
                }
                _ => {}
            }
            changed |= simplify_expr(rhs, nz_masks, type_map);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr, nz_masks, type_map);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list, nz_masks, type_map);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body, nz_masks, type_map);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                changed |= simplify_stmt(i, nz_masks, type_map);
            }
            if let Some(c) = cond {
                changed |= simplify_expr(c, nz_masks, type_map);
            }
            if let Some(u) = update {
                changed |= simplify_stmt(u, nz_masks, type_map);
            }
            changed |= simplify_stmts(body, nz_masks, type_map);
        }
        HirStmt::If { cond, then_body, else_body } => {
            changed |= simplify_expr(cond, nz_masks, type_map);
            changed |= simplify_stmts(then_body, nz_masks, type_map);
            changed |= simplify_stmts(else_body, nz_masks, type_map);
        }
        HirStmt::Switch { expr, cases, default } => {
            changed |= simplify_expr(expr, nz_masks, type_map);
            for case in cases {
                changed |= simplify_stmts(&mut case.body, nz_masks, type_map);
            }
            changed |= simplify_stmts(default, nz_masks, type_map);
        }
        _ => {}
    }
    changed
}

fn simplify_stmts(
    stmts: &mut [HirStmt],
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt, nz_masks, type_map);
    }
    changed
}

pub(crate) fn apply_nz_mask_simplification_pass(func: &mut HirFunction) -> bool {
    let nz_masks = compute_nz_masks(func);
    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }
    simplify_stmts(&mut func.body, &nz_masks, &type_map)
}
