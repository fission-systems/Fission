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
use super::super::*;
use super::typed_facts::{
    TypedAccessFacts, collect_typed_fact_inventory,
    inferred_aggregate_size as inferred_size_from_facts,
    should_infer_aggregate as should_infer_aggregate_from_facts,
};
use crate::nir::normalize::wave_stats::{
    add_object_root_recoveries, add_object_shape_recoveries, add_surface_binding_promotions,
    add_typed_object_shape_refinements,
};

fn can_upgrade_binding_to_aggregate(binding: &NirBinding) -> bool {
    matches!(
        &binding.ty,
        NirType::Ptr(inner)
            if matches!(
                inner.as_ref(),
                NirType::Unknown
                    | NirType::Aggregate { .. }
                    | NirType::Int { bits: 8 | 16, .. }
            )
    )
}

fn infer_storage_class(binding: &NirBinding) -> StorageClass {
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
    binding: &NirBinding,
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
    binding: &NirBinding,
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
pub(crate) fn apply_aggregate_fields_pass(func: &mut HirFunction) -> bool {
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
        if !can_upgrade_binding_to_aggregate(binding)
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

    // Update each NirBinding that is Ptr(Aggregate { .. }) with discovered fields.
    let update_binding = |binding: &mut NirBinding| -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut func = HirFunction {
            name: "test".to_string(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: ptr_unknown(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("param_1".to_string())),
                    offset: 8,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }))],
            calling_convention: Default::default(),
            is_64bit: true,
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
        let mut func = HirFunction {
            name: "shape".to_string(),
            params: vec![NirBinding {
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
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 4,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::PtrOffset {
                            base: Box::new(HirExpr::Var("param_1".to_string())),
                            offset: 8,
                        }),
                        ty: NirType::Int {
                            bits: 16,
                            signed: false,
                        },
                    },
                    rhs: HirExpr::Const(
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
    fn aggregate_fields_uses_windows_struct_field_names_when_surface_type_known() {
        let mut func = HirFunction {
            name: "rect_shape".to_string(),
            params: vec![NirBinding {
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
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 4,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
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
    fn aggregate_fields_infers_struct_pointer_surface_when_shape_is_unique() {
        let mut func = HirFunction {
            name: "process_info_infer".to_string(),
            params: vec![NirBinding {
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
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 16,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                HirStmt::Expr(HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
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
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_aggregate_fields_pass(&mut func));
        assert_eq!(
            func.params[0].surface_type_name.as_deref(),
            Some("PROCESS_INFORMATION *")
        );
    }
}
