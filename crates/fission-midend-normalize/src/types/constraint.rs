use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Algebraic Type Constraint Propagator pass.
///
/// Discovers struct and array structures by propagating constraints algebraically
/// (using a fixed-point solver) forward and backward along the dataflow graph.
pub fn apply_type_constraint_propagation(func: &mut PreHirFunction) -> bool {
    let mut changed = scalarize_fieldless_aggregate_values(&mut func.body);

    // 1. Initialize constraints for each local and parameter variable
    let mut var_types = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        if binding.ty != NirType::Unknown {
            var_types.insert(binding.name.clone(), binding.ty.clone());
        }
    }

    // 2. Scan the body to collect constraints from memory/pointer accesses
    // and assignments (dataflow edges)
    let mut field_accesses = HashMap::<String, HashMap<u32, NirType>>::default();
    let mut assignments = Vec::new(); // Pairs of (lhs_var, rhs_expr)

    collect_constraints(&func.body, &mut field_accesses, &mut assignments);

    // Initial upgrade of variables to Ptr(Aggregate) if they have field accesses
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

            let unified = if is_fieldless_aggregate(&lhs_ty) && rhs_ty != NirType::Unknown {
                Some(rhs_ty.clone())
            } else {
                unify_types(&lhs_ty, &rhs_ty)
            };
            if let Some(unified) = unified {
                if unified != lhs_ty {
                    var_types.insert(lhs.clone(), unified.clone());
                    loop_changed = true;
                }
                // Back-propagation to RHS variable if RHS is a variable
                if let PreHirExpr::Var(rhs_name) = rhs {
                    let prev_rhs_ty = var_types.get(rhs_name).cloned().unwrap_or(NirType::Unknown);
                    if unified != prev_rhs_ty {
                        var_types.insert(rhs_name.clone(), unified.clone());
                        loop_changed = true;
                    }
                }
                // Back-propagation to Deref pointer variable if RHS is a Load
                if let PreHirExpr::Load { ptr, .. } = rhs {
                    if let PreHirExpr::Var(ptr_var) = ptr.as_ref() {
                        let prev_ptr_ty =
                            var_types.get(ptr_var).cloned().unwrap_or(NirType::Unknown);
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
    let update_binding = |binding: &mut PreHirBinding| -> bool {
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

    changed |= reconcile_assignment_access_types(&mut func.body, &var_types);
    let surface_overrides = func
        .params
        .iter()
        .chain(func.locals.iter())
        .filter_map(|binding| {
            binding
                .surface_type_name
                .as_ref()
                .map(|_| binding.name.clone())
        })
        .collect::<HashSet<_>>();
    changed |= cast_mismatched_field_access_bases(
        &mut func.body,
        &var_types,
        &field_accesses,
        &surface_overrides,
    );

    changed
}

fn is_fieldless_aggregate(ty: &NirType) -> bool {
    matches!(ty, NirType::Aggregate { fields, .. } if fields.is_empty())
}

fn fieldless_aggregate_int(ty: &NirType) -> Option<NirType> {
    let NirType::Aggregate { size, fields } = ty else {
        return None;
    };
    fields.is_empty().then(|| NirType::Int {
        bits: size.saturating_mul(8),
        signed: false,
    })
}

fn scalarize_fieldless_aggregate_values(stmts: &mut [PreHirStmt]) -> bool {
    fn expr(e: &mut PreHirExpr) -> bool {
        let mut changed = false;
        match e {
            PreHirExpr::Const(_, ty) => {
                if let Some(int_ty) = fieldless_aggregate_int(ty) {
                    *ty = int_ty;
                    changed = true;
                }
            }
            PreHirExpr::Cast { ty, expr: inner }
            | PreHirExpr::Unary {
                ty, expr: inner, ..
            } => {
                changed |= expr(inner);
                if let Some(int_ty) = fieldless_aggregate_int(ty) {
                    *ty = int_ty;
                    changed = true;
                }
            }
            PreHirExpr::Binary { lhs, rhs, ty, .. } => {
                changed |= expr(lhs);
                changed |= expr(rhs);
                if let Some(int_ty) = fieldless_aggregate_int(ty) {
                    *ty = int_ty;
                    changed = true;
                }
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                changed |= expr(cond);
                changed |= expr(then_expr);
                changed |= expr(else_expr);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    changed |= expr(arg);
                }
            }
            PreHirExpr::Load { ptr, .. }
            | PreHirExpr::PtrOffset { base: ptr, .. }
            | PreHirExpr::FieldAccess { base: ptr, .. }
            | PreHirExpr::AggregateCopy { src: ptr, .. } => changed |= expr(ptr),
            PreHirExpr::Index { base, index, .. } => {
                changed |= expr(base);
                changed |= expr(index);
            }
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) => {}
        }
        changed
    }

    fn lvalue(lhs: &mut PreHirLValue) -> bool {
        match lhs {
            PreHirLValue::Var(_) => false,
            PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
                expr(ptr)
            }
            PreHirLValue::Index { base, index, .. } => expr(base) | expr(index),
        }
    }

    let mut changed = false;
    for stmt in stmts {
        changed |= match stmt {
            PreHirStmt::Assign { lhs, rhs } => lvalue(lhs) | expr(rhs),
            PreHirStmt::VaStart { va_list, .. }
            | PreHirStmt::Expr(va_list)
            | PreHirStmt::Return(Some(va_list)) => expr(va_list),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                scalarize_fieldless_aggregate_values(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body))
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                expr(cond)
                    | scalarize_fieldless_aggregate_values(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    )
                    | scalarize_fieldless_aggregate_values(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    )
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.as_mut().is_some_and(|stmt| {
                    scalarize_fieldless_aggregate_values(std::slice::from_mut(stmt.as_mut()))
                }) | cond.as_mut().is_some_and(expr)
                    | update.as_mut().is_some_and(|stmt| {
                        scalarize_fieldless_aggregate_values(std::slice::from_mut(stmt.as_mut()))
                    })
                    | scalarize_fieldless_aggregate_values(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    )
            }
            PreHirStmt::Switch {
                expr: switch_expr,
                cases,
                default,
            } => {
                let mut inner_changed = expr(switch_expr);
                for case in cases {
                    inner_changed |=
                        scalarize_fieldless_aggregate_values(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        );
                }
                inner_changed
                    | scalarize_fieldless_aggregate_values(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    )
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => false,
        };
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
    stmts: &[PreHirStmt],
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
    assignments: &mut Vec<(String, PreHirExpr)>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Var(lhs_name) = lhs {
                    assignments.push((lhs_name.clone(), rhs.clone()));
                }
                collect_constraints_expr(rhs, field_accesses);
                collect_constraints_lvalue(lhs, field_accesses);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_constraints_expr(expr, field_accesses);
            }
            PreHirStmt::Block(body) => {
                collect_constraints(body, field_accesses, assignments);
            }
            PreHirStmt::While { cond, body } => {
                collect_constraints_expr(cond, field_accesses);
                collect_constraints(body, field_accesses, assignments);
            }
            PreHirStmt::DoWhile { body, cond } => {
                collect_constraints(body, field_accesses, assignments);
                collect_constraints_expr(cond, field_accesses);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    collect_constraints(
                        std::slice::from_ref(init_stmt.as_ref()),
                        field_accesses,
                        assignments,
                    );
                }
                if let Some(cond_expr) = cond {
                    collect_constraints_expr(cond_expr, field_accesses);
                }
                if let Some(update_stmt) = update {
                    collect_constraints(
                        std::slice::from_ref(update_stmt.as_ref()),
                        field_accesses,
                        assignments,
                    );
                }
                collect_constraints(body, field_accesses, assignments);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_constraints_expr(cond, field_accesses);
                collect_constraints(then_body, field_accesses, assignments);
                collect_constraints(else_body, field_accesses, assignments);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
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
    lhs: &PreHirLValue,
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
) {
    match lhs {
        PreHirLValue::Deref { ptr, ty } => {
            if let PreHirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let PreHirExpr::Var(base_name) = base.as_ref() {
                    field_accesses
                        .entry(base_name.clone())
                        .or_default()
                        .insert(*offset as u32, ty.clone());
                }
            }
            collect_constraints_expr(ptr, field_accesses);
        }
        PreHirLValue::Index {
            base,
            index,
            elem_ty: _,
        } => {
            collect_constraints_expr(base, field_accesses);
            collect_constraints_expr(index, field_accesses);
        }
        PreHirLValue::FieldAccess {
            base, offset, ty, ..
        } => {
            if let PreHirExpr::Var(base_name) = base.as_ref() {
                field_accesses
                    .entry(base_name.clone())
                    .or_default()
                    .insert(*offset, ty.clone());
            }
            collect_constraints_expr(base, field_accesses);
        }
        PreHirLValue::Var(_) => {}
    }
}

fn collect_constraints_expr(
    expr: &PreHirExpr,
    field_accesses: &mut HashMap<String, HashMap<u32, NirType>>,
) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            if let PreHirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let PreHirExpr::Var(base_name) = base.as_ref() {
                    field_accesses
                        .entry(base_name.clone())
                        .or_default()
                        .insert(*offset as u32, ty.clone());
                }
            }
            collect_constraints_expr(ptr, field_accesses);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            collect_constraints_expr(expr, field_accesses);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_constraints_expr(lhs, field_accesses);
            collect_constraints_expr(rhs, field_accesses);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_constraints_expr(cond, field_accesses);
            collect_constraints_expr(then_expr, field_accesses);
            collect_constraints_expr(else_expr, field_accesses);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_constraints_expr(arg, field_accesses);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_constraints_expr(base, field_accesses);
            collect_constraints_expr(index, field_accesses);
        }
        PreHirExpr::FieldAccess {
            base, offset, ty, ..
        } => {
            if let PreHirExpr::Var(base_name) = base.as_ref() {
                field_accesses
                    .entry(base_name.clone())
                    .or_default()
                    .insert(*offset, ty.clone());
            }
            collect_constraints_expr(base, field_accesses);
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn get_expr_type(expr: &PreHirExpr, var_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        PreHirExpr::Var(name) => var_types.get(name).cloned().unwrap_or(NirType::Unknown),
        PreHirExpr::Const(_, ty) => ty.clone(),
        PreHirExpr::Cast { ty, .. } => ty.clone(),
        PreHirExpr::Unary { ty, .. } => ty.clone(),
        PreHirExpr::Binary { ty, .. } => ty.clone(),
        PreHirExpr::Select { ty, .. } => ty.clone(),
        PreHirExpr::Call { ty, .. } => ty.clone(),
        PreHirExpr::Load { ty, .. } => ty.clone(),
        PreHirExpr::Index { elem_ty, .. } => elem_ty.clone(),
        PreHirExpr::FieldAccess { ty, .. } => ty.clone(),
        PreHirExpr::AddressOfGlobal(name) if name.starts_with('"') => {
            NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: true,
            }))
        }
        PreHirExpr::AddressOfGlobal(_) => NirType::Ptr(Box::new(NirType::Unknown)),
        PreHirExpr::PtrOffset { base, .. } => {
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
        (
            NirType::Aggregate {
                size: s1,
                fields: f1,
            },
            NirType::Aggregate {
                size: s2,
                fields: f2,
            },
        ) => {
            let mut merged_fields = HashMap::default();
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

fn is_nonaggregate_value_type(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Bool | NirType::Int { .. } | NirType::Float { .. } | NirType::Ptr(_)
    )
}

fn reconcile_assignment_access_types(
    stmts: &mut [PreHirStmt],
    var_types: &HashMap<String, NirType>,
) -> bool {
    fn assignment(
        lhs: &mut PreHirLValue,
        rhs: &mut PreHirExpr,
        var_types: &HashMap<String, NirType>,
    ) -> bool {
        let mut changed = false;
        if let PreHirLValue::Var(name) = lhs {
            let expected = var_types.get(name).cloned().unwrap_or(NirType::Unknown);
            if is_nonaggregate_value_type(&expected) {
                match rhs {
                    PreHirExpr::Load { ty, .. } if matches!(ty, NirType::Aggregate { .. }) => {
                        *ty = expected;
                        changed = true;
                    }
                    PreHirExpr::Index { elem_ty, .. }
                        if matches!(elem_ty, NirType::Aggregate { .. }) =>
                    {
                        *elem_ty = expected;
                        changed = true;
                    }
                    _ => {}
                }
            }
            return changed;
        }

        if matches!(rhs, PreHirExpr::Const(0, _)) {
            let aggregate_size = match lhs {
                PreHirLValue::Deref {
                    ty: NirType::Aggregate { size, .. },
                    ..
                }
                | PreHirLValue::Index {
                    elem_ty: NirType::Aggregate { size, .. },
                    ..
                } => Some(*size),
                _ => None,
            };
            if let Some(size) = aggregate_size {
                let scalar = NirType::Int {
                    bits: size.saturating_mul(8),
                    signed: false,
                };
                match lhs {
                    PreHirLValue::Deref { ty, .. } => *ty = scalar,
                    PreHirLValue::Index { elem_ty, .. } => *elem_ty = scalar,
                    _ => unreachable!(),
                }
                return true;
            }
        }

        let rhs_ty = get_expr_type(rhs, var_types);
        if !is_nonaggregate_value_type(&rhs_ty) {
            return false;
        }
        match lhs {
            PreHirLValue::Deref { ty, .. } if matches!(ty, NirType::Aggregate { .. }) => {
                let same_width = type_byte_size(ty) == type_byte_size(&rhs_ty);
                if same_width {
                    *ty = rhs_ty;
                    changed = true;
                }
            }
            PreHirLValue::Index { elem_ty, .. } if matches!(elem_ty, NirType::Aggregate { .. }) => {
                let same_width = type_byte_size(elem_ty) == type_byte_size(&rhs_ty);
                if same_width {
                    *elem_ty = rhs_ty;
                    changed = true;
                }
            }
            _ => {}
        }
        changed
    }

    let mut changed = false;
    for stmt in stmts {
        changed |= match stmt {
            PreHirStmt::Assign { lhs, rhs } => assignment(lhs, rhs, var_types),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => reconcile_assignment_access_types(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                var_types,
            ),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                reconcile_assignment_access_types(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    var_types,
                ) | reconcile_assignment_access_types(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    var_types,
                )
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                init.as_mut().is_some_and(|stmt| {
                    reconcile_assignment_access_types(
                        std::slice::from_mut(stmt.as_mut()),
                        var_types,
                    )
                }) | update.as_mut().is_some_and(|stmt| {
                    reconcile_assignment_access_types(
                        std::slice::from_mut(stmt.as_mut()),
                        var_types,
                    )
                }) | reconcile_assignment_access_types(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    var_types,
                )
            }
            PreHirStmt::Switch { cases, default, .. } => {
                let mut inner_changed = false;
                for case in cases {
                    inner_changed |= reconcile_assignment_access_types(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        var_types,
                    );
                }
                inner_changed
                    | reconcile_assignment_access_types(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                        var_types,
                    )
            }
            _ => false,
        };
    }
    changed
}

/// Build the struct a set of recorded field accesses describes, if it is one.
///
/// A single access at offset 0 that spans the whole width is not a struct --
/// it is the value itself. Wrapping it anyway produces an aggregate whose one
/// field has the aggregate's own size, and the printer names aggregates by
/// size, so the emitted typedef refers to itself:
///
/// ```text
/// typedef struct fission_agg16 { fission_agg16 field_0; } fission_agg16;
/// ```
///
/// That type has no finite size and no compiler accepts it.
fn aggregate_type_from_fields(fields: &HashMap<u32, NirType>) -> Option<NirType> {
    if fields.len() == 1
        && fields
            .get(&0)
            .is_some_and(|only| matches!(only, NirType::Aggregate { .. }))
    {
        return None;
    }
    let mut size = 0u32;
    let mut struct_fields = fields
        .iter()
        .map(|(&offset, ty)| {
            size = size.max(offset.saturating_add(type_byte_size(ty).unwrap_or(1).max(1)));
            StructField {
                offset,
                ty: ty.clone(),
                name: format!("field_{offset:x}"),
            }
        })
        .collect::<Vec<_>>();
    struct_fields.sort_by_key(|field| field.offset);
    (!struct_fields.is_empty()).then_some(NirType::Aggregate {
        size,
        fields: struct_fields,
    })
}

fn cast_mismatched_field_access_bases(
    stmts: &mut [PreHirStmt],
    var_types: &HashMap<String, NirType>,
    field_accesses: &HashMap<String, HashMap<u32, NirType>>,
    surface_overrides: &HashSet<String>,
) -> bool {
    fn expr(
        e: &mut PreHirExpr,
        var_types: &HashMap<String, NirType>,
        field_accesses: &HashMap<String, HashMap<u32, NirType>>,
        surface_overrides: &HashSet<String>,
    ) -> bool {
        let mut changed = false;
        match e {
            PreHirExpr::FieldAccess {
                base, field_name, ..
            } => {
                changed |= expr(base, var_types, field_accesses, surface_overrides);
                let PreHirExpr::Var(name) = base.as_ref() else {
                    return changed;
                };
                let Some(aggregate) = field_accesses
                    .get(name)
                    .and_then(aggregate_type_from_fields)
                else {
                    return changed;
                };
                let expected = NirType::Ptr(Box::new(aggregate));
                let actual = var_types.get(name).cloned().unwrap_or(NirType::Unknown);
                let surface_mismatch =
                    field_name.starts_with("field_") && surface_overrides.contains(name);
                if actual != expected || surface_mismatch {
                    let inner =
                        std::mem::replace(base, Box::new(PreHirExpr::Const(0, NirType::Unknown)));
                    *base = Box::new(PreHirExpr::Cast {
                        ty: expected,
                        expr: inner,
                    });
                    changed = true;
                }
            }
            PreHirExpr::Cast { expr: inner, .. }
            | PreHirExpr::Unary { expr: inner, .. }
            | PreHirExpr::Load { ptr: inner, .. }
            | PreHirExpr::PtrOffset { base: inner, .. }
            | PreHirExpr::AggregateCopy { src: inner, .. } => {
                changed |= expr(inner, var_types, field_accesses, surface_overrides)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                changed |= expr(lhs, var_types, field_accesses, surface_overrides);
                changed |= expr(rhs, var_types, field_accesses, surface_overrides);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                changed |= expr(cond, var_types, field_accesses, surface_overrides);
                changed |= expr(then_expr, var_types, field_accesses, surface_overrides);
                changed |= expr(else_expr, var_types, field_accesses, surface_overrides);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    changed |= expr(arg, var_types, field_accesses, surface_overrides);
                }
            }
            PreHirExpr::Index { base, index, .. } => {
                changed |= expr(base, var_types, field_accesses, surface_overrides);
                changed |= expr(index, var_types, field_accesses, surface_overrides);
            }
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        }
        changed
    }

    fn lvalue(
        lhs: &mut PreHirLValue,
        var_types: &HashMap<String, NirType>,
        field_accesses: &HashMap<String, HashMap<u32, NirType>>,
        surface_overrides: &HashSet<String>,
    ) -> bool {
        match lhs {
            PreHirLValue::FieldAccess {
                base, field_name, ..
            } => {
                let mut wrapper = PreHirExpr::FieldAccess {
                    base: std::mem::replace(base, Box::new(PreHirExpr::Const(0, NirType::Unknown))),
                    field_name: field_name.clone(),
                    offset: 0,
                    ty: NirType::Unknown,
                };
                let changed = expr(&mut wrapper, var_types, field_accesses, surface_overrides);
                let PreHirExpr::FieldAccess {
                    base: rewritten, ..
                } = wrapper
                else {
                    unreachable!();
                };
                *base = rewritten;
                changed
            }
            PreHirLValue::Deref { ptr, .. } => {
                expr(ptr, var_types, field_accesses, surface_overrides)
            }
            PreHirLValue::Index { base, index, .. } => {
                expr(base, var_types, field_accesses, surface_overrides)
                    | expr(index, var_types, field_accesses, surface_overrides)
            }
            PreHirLValue::Var(_) => false,
        }
    }

    let mut changed = false;
    for stmt in stmts {
        changed |=
            match stmt {
                PreHirStmt::Assign { lhs, rhs } => {
                    lvalue(lhs, var_types, field_accesses, surface_overrides)
                        | expr(rhs, var_types, field_accesses, surface_overrides)
                }
                PreHirStmt::VaStart { va_list, .. }
                | PreHirStmt::Expr(va_list)
                | PreHirStmt::Return(Some(va_list)) => {
                    expr(va_list, var_types, field_accesses, surface_overrides)
                }
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => cast_mismatched_field_access_bases(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    var_types,
                    field_accesses,
                    surface_overrides,
                ),
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    expr(cond, var_types, field_accesses, surface_overrides)
                        | cast_mismatched_field_access_bases(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        )
                        | cast_mismatched_field_access_bases(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        )
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    init.as_mut().is_some_and(|stmt| {
                        cast_mismatched_field_access_bases(
                            std::slice::from_mut(stmt.as_mut()),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        )
                    }) | cond.as_mut().is_some_and(|cond| {
                        expr(cond, var_types, field_accesses, surface_overrides)
                    }) | update.as_mut().is_some_and(|stmt| {
                        cast_mismatched_field_access_bases(
                            std::slice::from_mut(stmt.as_mut()),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        )
                    }) | cast_mismatched_field_access_bases(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                        var_types,
                        field_accesses,
                        surface_overrides,
                    )
                }
                PreHirStmt::Switch {
                    expr: switch_expr,
                    cases,
                    default,
                } => {
                    let mut inner_changed =
                        expr(switch_expr, var_types, field_accesses, surface_overrides);
                    for case in cases {
                        inner_changed |= cast_mismatched_field_access_bases(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        );
                    }
                    inner_changed
                        | cast_mismatched_field_access_bases(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                            var_types,
                            field_accesses,
                            surface_overrides,
                        )
                }
                _ => false,
            };
    }
    changed
}

fn update_ast_types(stmts: &mut [PreHirStmt], var_types: &HashMap<String, NirType>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                update_ast_lvalue(lhs, var_types);
                update_ast_expr(rhs, var_types);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                update_ast_expr(expr, var_types);
            }
            PreHirStmt::Block(body) => {
                update_ast_types(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), var_types);
            }
            PreHirStmt::While { cond, body } => {
                update_ast_expr(cond, var_types);
                update_ast_types(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), var_types);
            }
            PreHirStmt::DoWhile { body, cond } => {
                update_ast_types(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), var_types);
                update_ast_expr(cond, var_types);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    update_ast_types(std::slice::from_mut(init_stmt.as_mut()), var_types);
                }
                if let Some(cond_expr) = cond {
                    update_ast_expr(cond_expr, var_types);
                }
                if let Some(update_stmt) = update {
                    update_ast_types(std::slice::from_mut(update_stmt.as_mut()), var_types);
                }
                update_ast_types(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), var_types);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                update_ast_expr(cond, var_types);
                update_ast_types(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    var_types,
                );
                update_ast_types(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    var_types,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                update_ast_expr(expr, var_types);
                for case in cases {
                    update_ast_types(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        var_types,
                    );
                }
                update_ast_types(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), var_types);
            }
            _ => {}
        }
    }
}

fn update_ast_lvalue(lhs: &mut PreHirLValue, var_types: &HashMap<String, NirType>) {
    match lhs {
        PreHirLValue::Deref { ptr, ty } => {
            update_ast_expr(ptr, var_types);
            let ptr_ty = get_expr_type(ptr, var_types);
            if let NirType::Ptr(inner) = ptr_ty {
                *ty = *inner;
            }
        }
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
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

fn update_ast_expr(expr: &mut PreHirExpr, var_types: &HashMap<String, NirType>) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            update_ast_expr(ptr, var_types);
            let ptr_ty = get_expr_type(ptr, var_types);
            if let NirType::Ptr(inner) = ptr_ty {
                *ty = *inner;
            }
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            update_ast_expr(expr, var_types);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            update_ast_expr(lhs, var_types);
            update_ast_expr(rhs, var_types);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            update_ast_expr(cond, var_types);
            update_ast_expr(then_expr, var_types);
            update_ast_expr(else_expr, var_types);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                update_ast_expr(arg, var_types);
            }
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn aggregate(size: u32) -> NirType {
        NirType::Aggregate {
            size,
            fields: Vec::new(),
        }
    }

    #[test]
    fn fieldless_aggregate_constant_refines_binding_to_same_width_integer() {
        let agg = aggregate(16);
        let mut func = PreHirFunction {
            locals: vec![binding("wide", agg.clone())],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("wide".to_owned()),
                rhs: PreHirExpr::Const(0, agg),
            }],
            ..Default::default()
        };

        assert!(apply_type_constraint_propagation(&mut func));
        let int128 = NirType::Int {
            bits: 128,
            signed: false,
        };
        assert_eq!(func.locals[0].ty, int128);
        let PreHirStmt::Assign {
            rhs: PreHirExpr::Const(_, rhs_ty),
            ..
        } = &func.body[0]
        else {
            panic!("expected constant assignment");
        };
        assert_eq!(rhs_ty, &int128);
    }

    #[test]
    fn wide_zero_store_uses_scalar_access_without_flattening_pointer_binding() {
        let agg = aggregate(16);
        let mut func = PreHirFunction {
            locals: vec![binding("p", NirType::Ptr(Box::new(agg.clone())))],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                    ty: agg.clone(),
                },
                rhs: PreHirExpr::Const(0, agg.clone()),
            }],
            ..Default::default()
        };

        assert!(apply_type_constraint_propagation(&mut func));
        assert_eq!(func.locals[0].ty, NirType::Ptr(Box::new(agg)));
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { ty, .. },
            ..
        } = &func.body[0]
        else {
            panic!("expected dereference store");
        };
        assert_eq!(
            ty,
            &NirType::Int {
                bits: 128,
                signed: false,
            }
        );
    }

    #[test]
    fn generic_field_access_casts_a_surface_scalar_pointer_to_recovered_owner() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let mut base = binding(
            "text",
            NirType::Ptr(Box::new(NirType::Aggregate {
                size: 3,
                fields: vec![StructField {
                    offset: 1,
                    ty: u8_ty.clone(),
                    name: "field_1".to_owned(),
                }],
            })),
        );
        base.surface_type_name = Some("char*".to_owned());
        let mut func = PreHirFunction {
            locals: vec![base, binding("value", u8_ty.clone())],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("value".to_owned()),
                rhs: PreHirExpr::FieldAccess {
                    base: Box::new(PreHirExpr::Var("text".to_owned())),
                    field_name: "field_1".to_owned(),
                    offset: 1,
                    ty: u8_ty,
                },
            }],
            ..Default::default()
        };

        assert!(apply_type_constraint_propagation(&mut func));
        let PreHirStmt::Assign {
            rhs: PreHirExpr::FieldAccess { base, .. },
            ..
        } = &func.body[0]
        else {
            panic!("expected field access");
        };
        assert!(
            matches!(base.as_ref(), PreHirExpr::Cast { ty: NirType::Ptr(inner), .. } if matches!(inner.as_ref(), NirType::Aggregate { .. }))
        );
    }
}
