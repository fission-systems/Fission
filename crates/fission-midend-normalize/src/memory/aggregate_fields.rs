use super::partition::type_byte_size;
use super::typed_facts::{
    TypedAccessFacts, collect_typed_fact_inventory,
    inferred_aggregate_size as inferred_size_from_facts,
    should_infer_aggregate as should_infer_aggregate_from_facts,
};
/// Aggregate field layout recovery pass.
///
/// After pointer-arithmetic recovery (`ptr_arith.rs`) has converted raw
/// `IntAdd(ptr, k)` expressions into `PtrOffset { base: Var(x), offset: k }`
/// nodes, this pass examines every `PtrOffset` whose base variable has type
/// `Ptr(Aggregate { .. })` and accumulates the complete set of byte offsets
/// that are actually accessed.  It then:
///
/// 1. **Builds an offset → field-type map** for each aggregate variable by
///    scanning `Load { ptr: PtrOffset }` and store-lvalue `Deref { ptr:
///    PtrOffset }` sites.
/// 2. **Annotates** the `NirType::Aggregate` with a sorted `Vec<StructField>`,
///    giving each field the name `field_{offset:x}` (e.g. `field_8`).
/// 3. **Updates the printer** indirectly: `printer.rs` checks for a non-empty
///    `fields` vec and emits `base->field_8` instead of the raw byte-offset form.
///
/// ### Algorithm
///
/// The algorithm is purely data-flow / use-site driven:
///
/// - Only constant-offset `PtrOffset` nodes are considered (variable offsets
///   produce `Index` nodes, handled separately).
/// - When two accesses at the same offset have different type widths, the
///   wider type wins (conservative union-field model; no Rust-level union is
///   emitted, the smaller access simply becomes a nested cast at the use-site).
/// - The pass is monotone: it only *adds* fields to a previously-empty
///   `fields` vec.  Re-running is safe.
///
/// This pass is architecture-agnostic and has no binary-specific thresholds.
use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::wave_stats::{
    add_object_root_recoveries, add_object_shape_recoveries, add_surface_binding_promotions,
    add_typed_object_shape_refinements,
};

fn can_upgrade_binding_to_aggregate(
    binding: &PreHirBinding,
    accesses: &std::collections::BTreeMap<u32, TypedAccessFacts>,
) -> bool {
    let NirType::Ptr(inner) = &binding.ty else {
        return false;
    };
    if matches!(
        inner.as_ref(),
        NirType::Unknown | NirType::Aggregate { .. } | NirType::Int { bits: 8 | 16, .. }
    ) {
        return true;
    }
    // A homogeneous scalar array can expose several constant offsets and
    // must remain `T *`. Distinct access widths at those offsets cannot be
    // explained by that homogeneous pointee, however, and are strong record
    // evidence (for example a 32-bit value followed by a pointer-width link).
    // This lets prior scalar inference yield to stronger object-shape facts
    // without turning ordinary `int[2]`-style accesses into structures.
    if !matches!(inner.as_ref(), NirType::Int { .. }) {
        return false;
    }
    let widths = accesses
        .values()
        .filter_map(|facts| type_byte_size(&facts.ty))
        .filter(|width| *width > 0)
        .collect::<std::collections::BTreeSet<_>>();
    widths.len() >= 2
}

fn infer_storage_class(binding: &PreHirBinding) -> StorageClass {
    match binding.origin {
        Some(NirBindingOrigin::ParamIndex(_)) => StorageClass::Param,
        Some(
            NirBindingOrigin::StackOffset(_)
            | NirBindingOrigin::DerivedFromStackOffset(_)
            | NirBindingOrigin::HomeSlot(_)
            | NirBindingOrigin::OutgoingArgSlot(_)
            | NirBindingOrigin::ReturnScaffold,
        ) => StorageClass::StackLocal,
        _ => StorageClass::Unknown,
    }
}

fn should_emit_surface_binding(
    binding: &PreHirBinding,
    offset_count: usize,
    has_stores: bool,
) -> bool {
    if matches!(&binding.ty, NirType::Ptr(inner) if matches!(inner.as_ref(), NirType::Unknown)) {
        return offset_count > 0;
    }
    match infer_storage_class(binding) {
        StorageClass::Param => offset_count > 0,
        StorageClass::StackLocal => offset_count >= 2 || has_stores,
        _ => offset_count >= 2,
    }
}

fn should_emit_surface_binding_from_facts(
    binding: &PreHirBinding,
    offsets: &std::collections::BTreeMap<u32, TypedAccessFacts>,
) -> bool {
    should_emit_surface_binding(
        binding,
        offsets.len(),
        offsets.values().any(|facts| facts.stores > 0),
    )
}

/// Apply aggregate field layout recovery to a function.
///
/// Returns `true` if any `NirType::Aggregate` had fields added to it.
pub fn apply_aggregate_fields_pass(func: &mut PreHirFunction) -> bool {
    let inventory = collect_typed_fact_inventory(func, true);
    if inventory.objects.is_empty() {
        return false;
    }

    let mut changed = false;
    let object_root_count = inventory.objects.len();
    if object_root_count > 0 {
        add_object_root_recoveries(object_root_count);
    }

    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        let Some(facts) = inventory.objects.get(&binding.name) else {
            continue;
        };
        if !can_upgrade_binding_to_aggregate(binding, &facts.accesses)
            || !should_infer_aggregate_from_facts(&facts.accesses)
        {
            continue;
        }
        let Some(size) = inferred_size_from_facts(&facts.accesses) else {
            continue;
        };
        binding.ty = NirType::Ptr(Box::new(NirType::Aggregate {
            size,
            fields: Vec::new(),
        }));
        changed = true;
        add_object_shape_recoveries(1);
    }

    // Update each PreHirBinding that is Ptr(Aggregate { .. }) with discovered fields.
    let update_binding = |binding: &mut PreHirBinding| -> bool {
        let Some(object_facts) = inventory.objects.get(&binding.name) else {
            return false;
        };
        let can_surface = should_emit_surface_binding_from_facts(binding, &object_facts.accesses);
        if !can_surface {
            return false;
        }
        if object_facts.shape.fields.is_empty() {
            return false;
        }
        {
            let NirType::Ptr(inner) = &mut binding.ty else {
                return false;
            };
            let NirType::Aggregate { fields, .. } = inner.as_mut() else {
                return false;
            };
            if !fields.is_empty() {
                return false; // already populated
            }
            *fields = object_facts.shape.fields.clone();
        }
        let named_fields = object_facts
            .shape
            .fields
            .iter()
            .any(|field| !field.name.starts_with("field_"));
        if named_fields {
            add_typed_object_shape_refinements(1);
        }
        if binding.surface_type_name.is_none()
            && let Some(struct_name) = object_facts.resolved_struct_name.as_ref()
        {
            binding.surface_type_name = Some(format!("{struct_name} *"));
        }
        true
    };

    for binding in func.locals.iter_mut() {
        if update_binding(binding) {
            add_surface_binding_promotions(1);
            changed = true;
        }
    }
    for binding in func.params.iter_mut() {
        if update_binding(binding) {
            add_surface_binding_promotions(1);
            changed = true;
        }
    }

    changed
}

#[derive(Debug, Clone)]
struct AggregateAlias {
    root: PreHirExpr,
    base_offset: i64,
    elem_ty: NirType,
}

pub fn apply_aggregate_alias_access_rewrite_pass(func: &mut PreHirFunction) -> bool {
    let assigned_vars = assigned_var_names(&func.body);
    let aliases = func
        .locals
        .iter()
        .filter(|binding| !assigned_vars.contains(binding.name.as_str()))
        .filter_map(|binding| {
            let initializer = binding.initializer.as_ref()?;
            let (root, base_offset) = aggregate_alias_root(initializer)?;
            if !root_is_typed_object_carrier(func, &root) {
                return None;
            }
            let NirType::Ptr(elem_ty) = &binding.ty else {
                return None;
            };
            Some((
                binding.name.clone(),
                AggregateAlias {
                    root,
                    base_offset,
                    elem_ty: elem_ty.as_ref().clone(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();
    if aliases.is_empty() {
        return false;
    }
    let mut changed = rewrite_alias_stmts(&mut func.body, &aliases);
    if changed {
        func.locals.retain(|binding| {
            let remove = aliases.contains_key(binding.name.as_str())
                && !stmt_list_uses_var(&func.body, &binding.name);
            changed |= remove;
            !remove
        });
    }
    changed
}

fn assigned_var_names(stmts: &[PreHirStmt]) -> HashSet<String> {
    fn visit(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    ..
                } => {
                    out.insert(name.clone());
                }
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => {
                    visit(body, out);
                }
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    visit(then_body, out);
                    visit(else_body, out);
                }
                PreHirStmt::For {
                    init, update, body, ..
                } => {
                    if let Some(init) = init.as_deref() {
                        visit(std::slice::from_ref(init), out);
                    }
                    if let Some(update) = update.as_deref() {
                        visit(std::slice::from_ref(update), out);
                    }
                    visit(body, out);
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(&case.body, out);
                    }
                    visit(default, out);
                }
                _ => {}
            }
        }
    }

    let mut out = HashSet::default();
    visit(stmts, &mut out);
    out
}

fn aggregate_alias_root(expr: &PreHirExpr) -> Option<(PreHirExpr, i64)> {
    match expr {
        PreHirExpr::Cast { expr, .. } => aggregate_alias_root(expr),
        PreHirExpr::PtrOffset { base, offset } => Some(((**base).clone(), *offset)),
        _ => None,
    }
}

fn root_is_typed_object_carrier(func: &PreHirFunction, root: &PreHirExpr) -> bool {
    match root {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => func
            .params
            .iter()
            .chain(func.locals.iter())
            .find(|binding| binding.name == *name)
            .is_some_and(|binding| {
                matches!(
                    binding.origin,
                    Some(
                        NirBindingOrigin::ParamIndex(_)
                            | NirBindingOrigin::StackOffset(_)
                            | NirBindingOrigin::DerivedFromStackOffset(_)
                            | NirBindingOrigin::HomeSlot(_)
                            | NirBindingOrigin::OutgoingArgSlot(_)
                    )
                )
            }),
        PreHirExpr::Cast { expr, .. } | PreHirExpr::PtrOffset { base: expr, .. } => {
            root_is_typed_object_carrier(func, expr)
        }
        _ => false,
    }
}

fn stmt_list_uses_var(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| stmt_uses_var(stmt, name))
}

fn stmt_uses_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => lvalue_uses_var(lhs, name) || expr_uses_var(rhs, name),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_uses_var(expr, name),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var(cond, name)
                || stmt_list_uses_var(then_body, name)
                || stmt_list_uses_var(else_body, name)
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            expr_uses_var(cond, name) || stmt_list_uses_var(body, name)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_uses_var(stmt, name))
                || cond.as_ref().is_some_and(|expr| expr_uses_var(expr, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_uses_var(stmt, name))
                || stmt_list_uses_var(body, name)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_var(expr, name)
                || cases
                    .iter()
                    .any(|case| stmt_list_uses_var(&case.body, name))
                || stmt_list_uses_var(default, name)
        }
        PreHirStmt::Block(body) => stmt_list_uses_var(body, name),
        PreHirStmt::Return(None)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn lvalue_uses_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(var) => var == name,
        PreHirLValue::Deref { ptr, .. } => expr_uses_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_uses_var(base, name) || expr_uses_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_uses_var(base, name),
    }
}

fn expr_uses_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(var) | PreHirExpr::AddressOfGlobal(var) => var == name,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_uses_var(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_uses_var(lhs, name) || expr_uses_var(rhs, name),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var(cond, name)
                || expr_uses_var(then_expr, name)
                || expr_uses_var(else_expr, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_uses_var(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_uses_var(base, name) || expr_uses_var(index, name)
        }
        PreHirExpr::Const(_, _) => false,
    }
}

fn alias_const_index_offset(alias: &AggregateAlias, index: &PreHirExpr) -> Option<i64> {
    let PreHirExpr::Const(index, _) = index else {
        return None;
    };
    let elem_size = i64::from(type_byte_size(&alias.elem_ty)?);
    alias.base_offset.checked_add(index.checked_mul(elem_size)?)
}

fn alias_ptr_offset(alias: &AggregateAlias, offset: i64) -> PreHirExpr {
    if offset == 0 {
        alias.root.clone()
    } else {
        PreHirExpr::PtrOffset {
            base: Box::new(alias.root.clone()),
            offset,
        }
    }
}

fn rewrite_alias_stmts(
    stmts: &mut [PreHirStmt],
    aliases: &HashMap<String, AggregateAlias>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_alias_lvalue(lhs, aliases);
                changed |= rewrite_alias_expr(rhs, aliases);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                changed |= rewrite_alias_expr(expr, aliases);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_alias_expr(cond, aliases);
                changed |= rewrite_alias_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    aliases,
                );
                changed |= rewrite_alias_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    aliases,
                );
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                changed |= rewrite_alias_expr(cond, aliases);
                changed |=
                    rewrite_alias_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), aliases);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init.as_deref_mut() {
                    changed |= rewrite_alias_stmts(std::slice::from_mut(init), aliases);
                }
                if let Some(cond) = cond {
                    changed |= rewrite_alias_expr(cond, aliases);
                }
                if let Some(update) = update.as_deref_mut() {
                    changed |= rewrite_alias_stmts(std::slice::from_mut(update), aliases);
                }
                changed |=
                    rewrite_alias_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), aliases);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_alias_expr(expr, aliases);
                for case in cases {
                    changed |= rewrite_alias_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        aliases,
                    );
                }
                changed |=
                    rewrite_alias_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), aliases);
            }
            PreHirStmt::Block(body) => {
                changed |=
                    rewrite_alias_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), aliases);
            }
            PreHirStmt::Return(None)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    changed
}

fn rewrite_alias_lvalue(lhs: &mut PreHirLValue, aliases: &HashMap<String, AggregateAlias>) -> bool {
    match lhs {
        PreHirLValue::Deref { ptr, .. } => rewrite_alias_expr(ptr, aliases),
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let mut changed =
                rewrite_alias_expr(base, aliases) | rewrite_alias_expr(index, aliases);
            if let PreHirExpr::Var(name) = base.as_ref()
                && let Some(alias) = aliases.get(name)
                && let Some(offset) = alias_const_index_offset(alias, index)
            {
                *lhs = PreHirLValue::Deref {
                    ptr: Box::new(alias_ptr_offset(alias, offset)),
                    ty: elem_ty.clone(),
                };
                changed = true;
            }
            changed
        }
        PreHirLValue::Var(_) => false,
        PreHirLValue::FieldAccess { base, .. } => rewrite_alias_expr(base, aliases),
    }
}

fn rewrite_alias_expr(expr: &mut PreHirExpr, aliases: &HashMap<String, AggregateAlias>) -> bool {
    let changed = match expr {
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => rewrite_alias_expr(expr, aliases),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_alias_expr(lhs, aliases) | rewrite_alias_expr(rhs, aliases)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_alias_expr(cond, aliases)
                | rewrite_alias_expr(then_expr, aliases)
                | rewrite_alias_expr(else_expr, aliases)
        }
        PreHirExpr::Call { args, .. } => args
            .iter_mut()
            .fold(false, |acc, arg| rewrite_alias_expr(arg, aliases) | acc),
        PreHirExpr::Load { ptr, .. } => rewrite_alias_expr(ptr, aliases),
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let mut changed =
                rewrite_alias_expr(base, aliases) | rewrite_alias_expr(index, aliases);
            if let PreHirExpr::Var(name) = base.as_ref()
                && let Some(alias) = aliases.get(name)
                && let Some(offset) = alias_const_index_offset(alias, index)
            {
                *expr = PreHirExpr::Load {
                    ptr: Box::new(alias_ptr_offset(alias, offset)),
                    ty: elem_ty.clone(),
                };
                changed = true;
            }
            changed
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => false,
    };
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn ptr_unknown() -> NirType {
        NirType::Ptr(Box::new(NirType::Unknown))
    }

    fn ptr_u8() -> NirType {
        NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }))
    }

    #[test]
    fn aggregate_fields_upgrades_unknown_pointer_to_aggregate() {
        let mut func = PreHirFunction {
            name: "test".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: ptr_unknown(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("param_1".to_string())),
                    offset: 8,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }))],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_aggregate_fields_pass(&mut func));
        let NirType::Ptr(inner) = &func.params[0].ty else {
            panic!("expected pointer param");
        };
        let NirType::Aggregate { size, fields } = inner.as_ref() else {
            panic!("expected inferred aggregate");
        };
        assert_eq!(*size, 12);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].offset, 8);
    }

    #[test]
    fn aggregate_fields_upgrades_byte_pointer_when_shape_is_structured() {
        let mut func = PreHirFunction {
            name: "shape".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: ptr_u8(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 4,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::Var("param_1".to_string())),
                            offset: 8,
                        }),
                        ty: NirType::Int {
                            bits: 16,
                            signed: false,
                        },
                    },
                    rhs: PreHirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 16,
                            signed: false,
                        },
                    ),
                },
            ],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_aggregate_fields_pass(&mut func));
        let NirType::Ptr(inner) = &func.params[0].ty else {
            panic!("expected pointer param");
        };
        let NirType::Aggregate { fields, .. } = inner.as_ref() else {
            panic!("expected inferred aggregate");
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].offset, 4);
        assert_eq!(fields[1].offset, 8);
    }

    #[test]
    fn aggregate_fields_keeps_homogeneous_u32_pointer_array_like() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "homogeneous_scalar_offsets".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "values".to_string(),
                ty: NirType::Ptr(Box::new(u32_ty.clone())),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("values".to_string())),
                    ty: u32_ty.clone(),
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("values".to_string())),
                        offset: 4,
                    }),
                    ty: u32_ty.clone(),
                }),
            ],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(!apply_aggregate_fields_pass(&mut func));
        assert_eq!(func.params[0].ty, NirType::Ptr(Box::new(u32_ty)));
    }

    #[test]
    fn aggregate_fields_uses_windows_struct_field_names_when_surface_type_known() {
        let mut func = PreHirFunction {
            name: "rect_shape".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: ptr_unknown(),
                surface_type_name: Some("LPRECT".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 0,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 4,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 12,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
            ],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_aggregate_fields_pass(&mut func));
        let NirType::Ptr(inner) = &func.params[0].ty else {
            panic!("expected pointer param");
        };
        let NirType::Aggregate { fields, .. } = inner.as_ref() else {
            panic!("expected inferred aggregate");
        };
        let names = fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["left", "top", "right", "bottom"]);
    }

    #[test]
    fn aggregate_fields_infers_aggregate_from_multi_offset_accesses() {
        // Verify the aggregate field pass fires and infers correct field offsets
        // from multiple memory accesses on a pointer parameter.
        let mut func = PreHirFunction {
            name: "process_info_infer".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: ptr_unknown(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 0,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 16,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 20,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
            ],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        // The aggregate fields pass must fire: a pointer is being accessed at
        // multiple offsets so the binding is upgraded to an aggregate.
        assert!(apply_aggregate_fields_pass(&mut func));
        let NirType::Ptr(inner) = &func.params[0].ty else {
            panic!("expected pointer param");
        };
        let NirType::Aggregate { size, fields } = inner.as_ref() else {
            panic!("expected inferred aggregate");
        };
        // Inferred size = offset 20 + 4 bytes (u32) = 24.
        assert_eq!(*size, 24);
        // All four accessed offsets must appear as fields.
        assert_eq!(fields.len(), 4);
        let offsets: std::collections::BTreeSet<u32> = fields.iter().map(|f| f.offset).collect();
        assert_eq!(
            offsets,
            [0u32, 8, 16, 20]
                .into_iter()
                .collect::<std::collections::BTreeSet<u32>>()
        );
    }

    #[test]
    fn aggregate_fields_infers_process_information_surface_from_explicit_hint() {
        // When the caller supplies an explicit surface_type_name matching a known
        // Win32 struct (via LP prefix stripping), the aggregate pass must resolve
        // that name and propagate the surface type correctly.
        let mut func = PreHirFunction {
            name: "process_info_hint".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: ptr_unknown(),
                surface_type_name: Some("LPPROCESS_INFORMATION".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 0,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 16,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                PreHirStmt::Expr(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("param_1".to_string())),
                        offset: 20,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
            ],
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_aggregate_fields_pass(&mut func));
        // With an explicit "LPPROCESS_INFORMATION" hint the aggregate pass resolves
        // "PROCESS_INFORMATION" and assigns canonical field names from that struct.
        let NirType::Ptr(inner) = &func.params[0].ty else {
            panic!("expected pointer param");
        };
        let NirType::Aggregate { fields, .. } = inner.as_ref() else {
            panic!("expected inferred aggregate");
        };
        let names: std::collections::BTreeSet<&str> =
            fields.iter().map(|f| f.name.as_str()).collect();
        // All four PROCESS_INFORMATION field names must be present.
        assert!(
            names.contains("hProcess"),
            "expected hProcess field, got: {names:?}"
        );
        assert!(
            names.contains("hThread"),
            "expected hThread field, got: {names:?}"
        );
        assert!(
            names.contains("dwProcessId"),
            "expected dwProcessId field, got: {names:?}"
        );
        assert!(
            names.contains("dwThreadId"),
            "expected dwThreadId field, got: {names:?}"
        );
    }
}
