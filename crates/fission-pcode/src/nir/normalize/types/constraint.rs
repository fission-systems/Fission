use super::super::super::*;
use std::collections::{HashMap, HashSet};

/// Algebraic Type Constraint Propagator pass.
///
/// Discovers struct and array structures by propagating constraints algebraically
/// (using a fixed-point solver) forward and backward along the dataflow graph.
pub(crate) fn apply_type_constraint_propagation(func: &mut HirFunction) -> bool {
    // 1. Initialize constraints for each local and parameter variable
    let mut var_types = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        if binding.ty != NirType::Unknown {
            var_types.insert(binding.name.clone(), binding.ty.clone());
        }
    }

    // 2. Scan the body to collect constraints from memory/pointer accesses
    // and assignments (dataflow edges)
    let mut field_accesses = HashMap::<String, HashMap<u32, NirType>>::new();
    let mut assignments = Vec::new(); // Pairs of (lhs_var, rhs_expr)

    collect_constraints(&func.body, &mut field_accesses, &mut assignments);

    // Initial upgrade of variables to Ptr(Aggregate) if they have field accesses
    let mut changed = false;
    for (var_name, fields) in &field_accesses {
        let current_ty = var_types.get(var_name).cloned().unwrap_or(NirType::Unknown);
        if let NirType::Unknown = current_ty {
            // Find max offset to determine aggregate size
            let mut max_offset = 0;
            let mut struct_fields = Vec::new();
            for (&offset, ty) in fields {
                let size = type_byte_size(ty).unwrap_or(1).max(1);
                max_offset = max_offset.max(offset + size);
                struct_fields.push(StructField {
                    offset,
                    ty: ty.clone(),
                    name: format!("field_{:x}", offset),
                });
            }
            struct_fields.sort_by_key(|f| f.offset);

            let new_ty = NirType::Ptr(Box::new(NirType::Aggregate {
                size: max_offset,
                fields: struct_fields,
            }));
            var_types.insert(var_name.clone(), new_ty);
            changed = true;
        }
    }

    // 3. Fixed-point propagation loop
    let mut loop_changed = true;
    let mut rounds = 0;
    while loop_changed && rounds < 10 {
        loop_changed = false;
        rounds += 1;

        // Propagate across assignments: lhs = rhs
        for (lhs, rhs) in &assignments {
            let lhs_ty = var_types.get(lhs).cloned().unwrap_or(NirType::Unknown);
            let rhs_ty = get_expr_type(rhs, &var_types);

            if let Some(unified) = unify_types(&lhs_ty, &rhs_ty) {
                if unified != lhs_ty {
                    var_types.insert(lhs.clone(), unified.clone());
                    loop_changed = true;
                }
                // Back-propagation to RHS variable if RHS is a variable
                if let HirExpr::Var(rhs_name) = rhs {
                    let prev_rhs_ty = var_types.get(rhs_name).cloned().unwrap_or(NirType::Unknown);
                    if unified != prev_rhs_ty {
                        var_types.insert(rhs_name.clone(), unified.clone());
                        loop_changed = true;
                    }
                }
                // Back-propagation to Deref pointer variable if RHS is a Load
                if let HirExpr::Load { ptr, .. } = rhs {
                    if let HirExpr::Var(ptr_var) = ptr.as_ref() {
                        let prev_ptr_ty = var_types.get(ptr_var).cloned().unwrap_or(NirType::Unknown);
                        let ptr_constraint = NirType::Ptr(Box::new(unified.clone()));
                        if let Some(unified_ptr) = unify_types(&prev_ptr_ty, &ptr_constraint) {
                            if unified_ptr != prev_ptr_ty {
                                var_types.insert(ptr_var.clone(), unified_ptr);
                                loop_changed = true;
                            }
                        }
                    }
                }
            }
        }
    }

    changed |= loop_changed;

    // 4. Update types of local and parameter bindings
    let update_binding = |binding: &mut NirBinding| -> bool {
        if let Some(solved_ty) = var_types.get(&binding.name) {
            if *solved_ty != NirType::Unknown && binding.ty != *solved_ty {
                binding.ty = solved_ty.clone();
                return true;
            }
        }
        false
    };

    for binding in &mut func.locals {
        changed |= update_binding(binding);
    }
    for binding in &mut func.params {
        changed |= update_binding(binding);
    }

    // 5. Walk AST and update expression/statement type annotations where necessary
    if changed {
        update_ast_types(&mut func.body, &var_types);
    }

    changed
}

fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

fn collect_constraints(
    stmts: &[HirStmt],
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
    assignments: &mut Vec<(String, HirExpr)>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Var(lhs_name) = lhs {
                    assignments.push((lhs_name.clone(), rhs.clone()));
                }
                collect_constraints_expr(rhs, field_accesses);
                collect_constraints_lvalue(lhs, field_accesses);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_constraints_expr(expr, field_accesses);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                collect_constraints(body, field_accesses, assignments);
            }
            HirStmt::If { cond, then_body, else_body } => {
                collect_constraints_expr(cond, field_accesses);
                collect_constraints(then_body, field_accesses, assignments);
                collect_constraints(else_body, field_accesses, assignments);
            }
            HirStmt::Switch { expr, cases, default } => {
                collect_constraints_expr(expr, field_accesses);
                for case in cases {
                    collect_constraints(&case.body, field_accesses, assignments);
                }
                collect_constraints(default, field_accesses, assignments);
            }
            _ => {}
        }
    }
}

fn collect_constraints_lvalue(
    lhs: &HirLValue,
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
) {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            if let HirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let HirExpr::Var(base_name) = base.as_ref() {
                    field_accesses
                        .entry(base_name.clone())
                        .or_default()
                        .insert(*offset as u32, ty.clone());
                }
            }
            collect_constraints_expr(ptr, field_accesses);
        }
        HirLValue::Index { base, index, elem_ty: _ } => {
            collect_constraints_expr(base, field_accesses);
            collect_constraints_expr(index, field_accesses);
        }
        _ => {}
    }
}

fn collect_constraints_expr(
    expr: &HirExpr,
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            if let HirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let HirExpr::Var(base_name) = base.as_ref() {
                    field_accesses
                        .entry(base_name.clone())
                        .or_default()
                        .insert(*offset as u32, ty.clone());
                }
            }
            collect_constraints_expr(ptr, field_accesses);
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_constraints_expr(expr, field_accesses);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_constraints_expr(lhs, field_accesses);
            collect_constraints_expr(rhs, field_accesses);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            collect_constraints_expr(cond, field_accesses);
            collect_constraints_expr(then_expr, field_accesses);
            collect_constraints_expr(else_expr, field_accesses);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_constraints_expr(arg, field_accesses);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_constraints_expr(base, field_accesses);
            collect_constraints_expr(index, field_accesses);
        }
        _ => {}
    }
}

fn get_expr_type(expr: &HirExpr, var_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => var_types.get(name).cloned().unwrap_or(NirType::Unknown),
        HirExpr::Const(_, ty) => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::Unary { ty, .. } => ty.clone(),
        HirExpr::Binary { ty, .. } => ty.clone(),
        HirExpr::Select { ty, .. } => ty.clone(),
        HirExpr::Call { ty, .. } => ty.clone(),
        HirExpr::Load { ty, .. } => ty.clone(),
        HirExpr::Index { elem_ty, .. } => elem_ty.clone(),
        HirExpr::PtrOffset { base, .. } => {
            let base_ty = get_expr_type(base, var_types);
            if let NirType::Ptr(inner) = base_ty {
                NirType::Ptr(inner)
            } else {
                NirType::Unknown
            }
        }
        _ => NirType::Unknown,
    }
}

fn unify_types(t1: &NirType, t2: &NirType) -> Option<NirType> {
    if *t1 == NirType::Unknown {
        return Some(t2.clone());
    }
    if *t2 == NirType::Unknown {
        return Some(t1.clone());
    }

    match (t1, t2) {
        (NirType::Ptr(i1), NirType::Ptr(i2)) => {
            let unified_inner = unify_types(i1, i2)?;
            Some(NirType::Ptr(Box::new(unified_inner)))
        }
        (NirType::Aggregate { size: s1, fields: f1 }, NirType::Aggregate { size: s2, fields: f2 }) => {
            let mut merged_fields = HashMap::new();
            for field in f1 {
                merged_fields.insert(field.offset, field.clone());
            }
            for field in f2 {
                merged_fields
                    .entry(field.offset)
                    .and_modify(|existing| {
                        if let Some(unified) = unify_types(&existing.ty, &field.ty) {
                            existing.ty = unified;
                        }
                    })
                    .or_insert(field.clone());
            }
            let mut fields_vec: Vec<StructField> = merged_fields.into_values().collect();
            fields_vec.sort_by_key(|f| f.offset);

            Some(NirType::Aggregate {
                size: (*s1).max(*s2),
                fields: fields_vec,
            })
        }
        _ => {
            if t1 == t2 {
                Some(t1.clone())
            } else {
                None
            }
        }
    }
}

fn update_ast_types(stmts: &mut [HirStmt], var_types: &HashMap<String, NirType>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                update_ast_lvalue(lhs, var_types);
                update_ast_expr(rhs, var_types);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                update_ast_expr(expr, var_types);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                update_ast_types(body, var_types);
            }
            HirStmt::If { cond, then_body, else_body } => {
                update_ast_expr(cond, var_types);
                update_ast_types(then_body, var_types);
                update_ast_types(else_body, var_types);
            }
            HirStmt::Switch { expr, cases, default } => {
                update_ast_expr(expr, var_types);
                for case in cases {
                    update_ast_types(&mut case.body, var_types);
                }
                update_ast_types(default, var_types);
            }
            _ => {}
        }
    }
}

fn update_ast_lvalue(lhs: &mut HirLValue, var_types: &HashMap<String, NirType>) {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            update_ast_expr(ptr, var_types);
            let ptr_ty = get_expr_type(ptr, var_types);
            if let NirType::Ptr(inner) = ptr_ty {
                *ty = *inner;
            }
        }
        HirLValue::Index { base, index, elem_ty } => {
            update_ast_expr(base, var_types);
            update_ast_expr(index, var_types);
            let base_ty = get_expr_type(base, var_types);
            if let NirType::Ptr(inner) = base_ty {
                *elem_ty = *inner;
            }
        }
        _ => {}
    }
}

fn update_ast_expr(expr: &mut HirExpr, var_types: &HashMap<String, NirType>) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            update_ast_expr(ptr, var_types);
            let ptr_ty = get_expr_type(ptr, var_types);
            if let NirType::Ptr(inner) = ptr_ty {
                *ty = *inner;
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            update_ast_expr(expr, var_types);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            update_ast_expr(lhs, var_types);
            update_ast_expr(rhs, var_types);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            update_ast_expr(cond, var_types);
            update_ast_expr(then_expr, var_types);
            update_ast_expr(else_expr, var_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                update_ast_expr(arg, var_types);
            }
        }
        HirExpr::Index { base, index, elem_ty } => {
            update_ast_expr(base, var_types);
            update_ast_expr(index, var_types);
            let base_ty = get_expr_type(base, var_types);
            if let NirType::Ptr(inner) = base_ty {
                *elem_ty = *inner;
            }
        }
        _ => {}
    }
}
