use crate::prelude::*;
use crate::analysis::defuse::DefUseMap;
use fission_midend_dir::util::expr_type;
use crate::HashMap;

pub fn apply_subvar_trim_pass(func: &mut DirFunction) -> bool {
    let mut assignments = HashMap::default();
    find_all_assignments(&func.body, &mut assignments);

    let defuse = DefUseMap::build(&func.body);

    let mut local_types = HashMap::default();
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

fn get_expr_type(expr: &DirExpr, local_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        DirExpr::Var(name) => {
            if let Some(ty) = local_types.get(name) {
                ty.clone()
            } else {
                NirType::Unknown
            }
        }
        DirExpr::Cast { ty, .. } => ty.clone(),
        _ => expr_type(expr),
    }
}

fn find_all_assignments(stmts: &[DirStmt], assignments: &mut HashMap<String, Vec<DirExpr>>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } => {
                assignments
                    .entry(name.clone())
                    .or_default()
                    .push(rhs.clone());
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                find_all_assignments(body, assignments);
            }
            DirStmt::For {
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
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                find_all_assignments(then_body, assignments);
                find_all_assignments(else_body, assignments);
            }
            DirStmt::Switch { cases, default, .. } => {
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
    stmts: &mut [DirStmt],
    assignments: &HashMap<String, Vec<DirExpr>>,
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
    stmt: &mut DirStmt,
    assignments: &HashMap<String, Vec<DirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            changed |= simplify_expr(rhs, assignments, defuse, local_types);
            changed |= simplify_lvalue(lhs, assignments, defuse, local_types);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr, assignments, defuse, local_types);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body, assignments, defuse, local_types);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond, assignments, defuse, local_types);
            changed |= simplify_stmts(then_body, assignments, defuse, local_types);
            changed |= simplify_stmts(else_body, assignments, defuse, local_types);
        }
        DirStmt::Switch {
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
        DirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list, assignments, defuse, local_types);
        }
        _ => {}
    }
    changed
}

fn simplify_lvalue(
    lval: &mut DirLValue,
    assignments: &HashMap<String, Vec<DirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match lval {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => {
            changed |= simplify_expr(ptr, assignments, defuse, local_types);
        }
        DirLValue::Index { base, index, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
            changed |= simplify_expr(index, assignments, defuse, local_types);
        }
        DirLValue::FieldAccess { base, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
        }
    }
    changed
}

fn simplify_expr(
    expr: &mut DirExpr,
    assignments: &HashMap<String, Vec<DirExpr>>,
    defuse: &DefUseMap,
    local_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    // Recurse first bottom-up
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            changed |= simplify_expr(inner, assignments, defuse, local_types);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs, assignments, defuse, local_types);
            changed |= simplify_expr(rhs, assignments, defuse, local_types);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg, assignments, defuse, local_types);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= simplify_expr(cond, assignments, defuse, local_types);
            changed |= simplify_expr(then_expr, assignments, defuse, local_types);
            changed |= simplify_expr(else_expr, assignments, defuse, local_types);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base, assignments, defuse, local_types);
            changed |= simplify_expr(index, assignments, defuse, local_types);
        }
        _ => {}
    }

    if let DirExpr::Cast {
        ty: target_ty,
        expr: inner_cast_expr,
    } = expr
    {
        if let DirExpr::Var(name) = inner_cast_expr.as_ref() {
            if let Some(exprs) = assignments.get(name) {
                if exprs.len() == 1 {
                    let def_expr = &exprs[0];
                    let use_count = defuse.use_count.get(name).copied().unwrap_or(0);
                    let is_safe_to_dup = matches!(def_expr, DirExpr::Var(_) | DirExpr::Const(_, _))
                        || use_count <= 1;

                    if is_safe_to_dup {
                        // Pattern 1: (target_ty)(intermediate_ty)inner_expr  where inner_expr: target_ty
                        if let DirExpr::Cast {
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
                        if let DirExpr::Binary {
                            op: DirBinaryOp::And,
                            lhs: and_lhs,
                            rhs: and_rhs,
                            ..
                        } = def_expr
                        {
                            let target_width = int_type_bits(target_ty);
                            if let Some(w) = target_width {
                                let expected_mask = (1_i64 << w).wrapping_sub(1);
                                if let DirExpr::Const(mask_val, _) = and_rhs.as_ref() {
                                    if *mask_val == expected_mask {
                                        *expr = DirExpr::Cast {
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
