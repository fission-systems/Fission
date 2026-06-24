use super::super::*;
use crate::nir::normalize::analysis::defuse::DefUseMap;
use crate::nir::support::expr_type;
use std::collections::HashMap;

pub(crate) fn apply_subvar_trim_pass(func: &mut HirFunction) -> bool {
    let mut assignments = HashMap::new();
    find_all_assignments(&func.body, &mut assignments);

    let defuse = DefUseMap::build(&func.body);

    let mut local_types = HashMap::new();
    for local in &func.locals {
        local_types.insert(local.name.clone(), local.ty.clone());
    }
    for param in &func.params {
        local_types.insert(param.name.clone(), param.ty.clone());
    }

    let mut changed = false;
    changed |= simplify_stmts(&mut func.body, &assignments, &defuse, &local_types);
    changed
}

fn int_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn get_expr_type(expr: &HirExpr, local_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => {
            if let Some(ty) = local_types.get(name) {
                ty.clone()
            } else {
                NirType::Unknown
            }
        }
        HirExpr::Cast { ty, .. } => ty.clone(),
        _ => expr_type(expr),
    }
}

fn find_all_assignments(stmts: &[HirStmt], assignments: &mut HashMap<String, Vec<HirExpr>>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                assignments
                    .entry(name.clone())
                    .or_default()
                    .push(rhs.clone());
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                find_all_assignments(body, assignments);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    find_all_assignments(std::slice::from_ref(init.as_ref()), assignments);
                }
                if let Some(update) = update {
                    find_all_assignments(std::slice::from_ref(update.as_ref()), assignments);
                }
                find_all_assignments(body, assignments);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                find_all_assignments(then_body, assignments);
                find_all_assignments(else_body, assignments);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    find_all_assignments(&case.body, assignments);
                }
                find_all_assignments(default, assignments);
            }
            _ => {}
        }
    }
}

fn simplify_stmts(
    stmts: &mut [HirStmt],
    assignments: &HashMap<String, Vec<HirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt, assignments, defuse, local_types);
    }
    changed
}

fn simplify_stmt(
    stmt: &mut HirStmt,
    assignments: &HashMap<String, Vec<HirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= simplify_expr(rhs, assignments, defuse, local_types);
            changed |= simplify_lvalue(lhs, assignments, defuse, local_types);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr, assignments, defuse, local_types);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body, assignments, defuse, local_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= simplify_stmt(i.as_mut(), assignments, defuse, local_types);
            }
            if let Some(c) = cond {
                changed |= simplify_expr(c, assignments, defuse, local_types);
            }
            if let Some(u) = update {
                changed |= simplify_stmt(u.as_mut(), assignments, defuse, local_types);
            }
            changed |= simplify_stmts(body, assignments, defuse, local_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond, assignments, defuse, local_types);
            changed |= simplify_stmts(then_body, assignments, defuse, local_types);
            changed |= simplify_stmts(else_body, assignments, defuse, local_types);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= simplify_expr(expr, assignments, defuse, local_types);
            for case in cases {
                changed |= simplify_stmts(&mut case.body, assignments, defuse, local_types);
            }
            changed |= simplify_stmts(default, assignments, defuse, local_types);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list, assignments, defuse, local_types);
        }
        _ => {}
    }
    changed
}

fn simplify_lvalue(
    lval: &mut HirLValue,
    assignments: &HashMap<String, Vec<HirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match lval {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            changed |= simplify_expr(ptr, assignments, defuse, local_types);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
            changed |= simplify_expr(index, assignments, defuse, local_types);
        }
        HirLValue::FieldAccess { base, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
        }
    }
    changed
}

fn simplify_expr(
    expr: &mut HirExpr,
    assignments: &HashMap<String, Vec<HirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    // Recurse first bottom-up
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= simplify_expr(inner, assignments, defuse, local_types);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs, assignments, defuse, local_types);
            changed |= simplify_expr(rhs, assignments, defuse, local_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg, assignments, defuse, local_types);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= simplify_expr(cond, assignments, defuse, local_types);
            changed |= simplify_expr(then_expr, assignments, defuse, local_types);
            changed |= simplify_expr(else_expr, assignments, defuse, local_types);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
            changed |= simplify_expr(index, assignments, defuse, local_types);
        }
        _ => {}
    }

    if let HirExpr::Cast {
        ty: target_ty,
        expr: inner_cast_expr,
    } = expr
    {
        if let HirExpr::Var(name) = inner_cast_expr.as_ref() {
            if let Some(exprs) = assignments.get(name) {
                if exprs.len() == 1 {
                    let def_expr = &exprs[0];
                    let use_count = defuse.use_count.get(name).copied().unwrap_or(0);
                    let is_safe_to_dup = matches!(def_expr, HirExpr::Var(_) | HirExpr::Const(_, _))
                        || use_count <= 1;

                    if is_safe_to_dup {
                        // Pattern 1: (target_ty)(intermediate_ty)inner_expr  where inner_expr: target_ty
                        if let HirExpr::Cast {
                            expr: inner_val, ..
                        } = def_expr
                        {
                            let inner_val_ty = get_expr_type(inner_val, local_types);
                            if &inner_val_ty == target_ty {
                                *expr = inner_val.as_ref().clone();
                                return true;
                            }
                        }

                        // Pattern 2: (target_ty)(val & mask)  where mask == full_mask(target_ty)
                        if let HirExpr::Binary {
                            op: HirBinaryOp::And,
                            lhs: and_lhs,
                            rhs: and_rhs,
                            ..
                        } = def_expr
                        {
                            let target_width = int_type_bits(target_ty);
                            if let Some(w) = target_width {
                                let expected_mask = (1_i64 << w).wrapping_sub(1);
                                if let HirExpr::Const(mask_val, _) = and_rhs.as_ref() {
                                    if *mask_val == expected_mask {
                                        *expr = HirExpr::Cast {
                                            ty: target_ty.clone(),
                                            expr: and_lhs.clone(),
                                        };
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    changed
}
