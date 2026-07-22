use crate::prelude::*;
use fission_midend_core::util_dir::expr_type;
use crate::HashMap;

pub fn type_mask(ty: &NirType) -> u64 {
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

fn get_expr_type(expr: &DirExpr, type_map: &HashMap<String, NirType>) -> NirType {
    match expr {
        DirExpr::Var(name) => type_map.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

pub fn eval_expr_nz_mask(
    expr: &DirExpr,
    var_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> u64 {
    match expr {
        DirExpr::Const(val, _) => *val as u64,
        DirExpr::Var(name) => *var_masks.get(name).unwrap_or(&u64::MAX),
        DirExpr::Cast { ty, expr } => {
            let inner_mask = eval_expr_nz_mask(expr, var_masks, type_map);
            let outer_mask = type_mask(ty);
            let inner_ty = get_expr_type(expr, type_map);
            if let NirType::Int {
                bits: inner_bits,
                signed: true,
            } = inner_ty
            {
                if let NirType::Int {
                    bits: outer_bits, ..
                } = ty
                {
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
        DirExpr::Binary { op, lhs, rhs, ty } => {
            let left_mask = eval_expr_nz_mask(lhs, var_masks, type_map);
            let right_mask = eval_expr_nz_mask(rhs, var_masks, type_map);
            let out_mask = type_mask(ty);
            match op {
                DirBinaryOp::And => left_mask & right_mask & out_mask,
                DirBinaryOp::Or | DirBinaryOp::Xor => (left_mask | right_mask) & out_mask,
                DirBinaryOp::Add => {
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
                DirBinaryOp::Shl => {
                    if let DirExpr::Const(sa, _) = &**rhs {
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
                DirBinaryOp::Shr => {
                    if let DirExpr::Const(sa, _) = &**rhs {
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
                DirBinaryOp::Sar => {
                    if let DirExpr::Const(sa, _) = &**rhs {
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
                DirBinaryOp::LogicalAnd
                | DirBinaryOp::LogicalOr
                | DirBinaryOp::Eq
                | DirBinaryOp::Ne
                | DirBinaryOp::Lt
                | DirBinaryOp::Le
                | DirBinaryOp::Gt
                | DirBinaryOp::Ge
                | DirBinaryOp::SLt
                | DirBinaryOp::SLe
                | DirBinaryOp::SGt
                | DirBinaryOp::SGe => 1,
                _ => out_mask,
            }
        }
        DirExpr::Select {
            then_expr,
            else_expr,
            ty,
            ..
        } => {
            let t_mask = eval_expr_nz_mask(then_expr, var_masks, type_map);
            let e_mask = eval_expr_nz_mask(else_expr, var_masks, type_map);
            (t_mask | e_mask) & type_mask(ty)
        }
        DirExpr::Unary { op, expr, ty } => {
            let out_mask = type_mask(ty);
            match op {
                DirUnaryOp::Not => 1,
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
    stmt: &DirStmt,
    var_masks: &mut HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            let new_mask = eval_expr_nz_mask(rhs, var_masks, type_map);
            let old_mask = var_masks.get(name).copied().unwrap_or(0);
            let merged = old_mask | new_mask;
            if merged != old_mask {
                var_masks.insert(name.clone(), merged);
                changed = true;
            }
        }
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. } => {
            for s in stmts {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
            for s in else_body {
                changed |= collect_assignments_in_stmt(s, var_masks, type_map);
            }
        }
        DirStmt::For {
            init, update, body, ..
        } => {
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
        DirStmt::Switch { cases, default, .. } => {
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

pub fn compute_nz_masks(func: &DirFunction) -> HashMap<String, u64> {
    let mut type_map = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut var_masks = HashMap::default();
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
    expr: &mut DirExpr,
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. } => {
            changed |= simplify_expr(inner, nz_masks, type_map);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs, nz_masks, type_map);
            changed |= simplify_expr(rhs, nz_masks, type_map);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg, nz_masks, type_map);
            }
        }
        DirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base, nz_masks, type_map);
            changed |= simplify_expr(index, nz_masks, type_map);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= simplify_expr(cond, nz_masks, type_map);
            changed |= simplify_expr(then_expr, nz_masks, type_map);
            changed |= simplify_expr(else_expr, nz_masks, type_map);
        }
        _ => {}
    }

    if let DirExpr::Binary {
        op: DirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let DirExpr::Const(mask, _) = &**rhs {
            let active = eval_expr_nz_mask(lhs, nz_masks, type_map);
            if (active & !(*mask as u64)) == 0 {
                *expr = (**lhs).clone();
                return true;
            }
        } else if let DirExpr::Const(mask, _) = &**lhs {
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
    stmt: &mut DirStmt,
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            match lhs {
                DirLValue::Deref { ptr, .. } => {
                    changed |= simplify_expr(ptr, nz_masks, type_map);
                }
                DirLValue::Index { base, index, .. } => {
                    changed |= simplify_expr(base, nz_masks, type_map);
                    changed |= simplify_expr(index, nz_masks, type_map);
                }
                _ => {}
            }
            changed |= simplify_expr(rhs, nz_masks, type_map);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr, nz_masks, type_map);
        }
        DirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list, nz_masks, type_map);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body, nz_masks, type_map);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond, nz_masks, type_map);
            changed |= simplify_stmts(then_body, nz_masks, type_map);
            changed |= simplify_stmts(else_body, nz_masks, type_map);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
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
    stmts: &mut [DirStmt],
    nz_masks: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt, nz_masks, type_map);
    }
    changed
}

pub fn apply_nz_mask_simplification_pass(func: &mut DirFunction) -> bool {
    let nz_masks = compute_nz_masks(func);
    let mut type_map = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }
    simplify_stmts(&mut func.body, &nz_masks, &type_map)
}
