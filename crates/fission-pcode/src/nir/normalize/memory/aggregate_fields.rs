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
use super::partition::{collect_partitioned_memory_accesses, type_byte_size};
use crate::nir::normalize::wave_stats::{add_object_shape_recoveries, add_surface_binding_promotions};
use std::collections::HashMap;

/// Map: variable name → (offset → best NirType for that field).
type OffsetMap = HashMap<String, HashMap<u32, NirType>>;

#[derive(Debug, Clone)]
struct AccessFacts {
    ty: NirType,
    loads: usize,
    stores: usize,
}

impl Default for AccessFacts {
    fn default() -> Self {
        Self {
            ty: NirType::Unknown,
            loads: 0,
            stores: 0,
        }
    }
}

type AccessFactMap = HashMap<String, HashMap<u32, AccessFacts>>;

/// Return the byte width of a type (0 = unknown).
fn type_bits_bytes(ty: &NirType) -> u32 {
    type_byte_size(ty).unwrap_or(0)
}

/// Merge a new candidate `NirType` into the best-so-far type for an offset.
/// The wider / more-concrete type wins.
fn merge_field_ty(current: &NirType, candidate: &NirType) -> NirType {
    if *current == NirType::Unknown {
        return candidate.clone();
    }
    if *candidate == NirType::Unknown {
        return current.clone();
    }
    let cur_w = type_bits_bytes(current);
    let cand_w = type_bits_bytes(candidate);
    if cand_w > cur_w {
        candidate.clone()
    } else {
        current.clone()
    }
}

fn record_access(var_name: &str, offset: u32, access_ty: NirType, out: &mut OffsetMap) {
    let entry = out.entry(var_name.to_owned()).or_default();
    let best = entry.entry(offset).or_insert(NirType::Unknown);
    *best = merge_field_ty(best, &access_ty);
}

fn record_access_fact(
    var_name: &str,
    offset: u32,
    access_ty: NirType,
    kind: super::partition::MemoryAccessKind,
    out: &mut AccessFactMap,
) {
    let entry = out.entry(var_name.to_owned()).or_default();
    let facts = entry.entry(offset).or_default();
    facts.ty = merge_field_ty(&facts.ty, &access_ty);
    match kind {
        super::partition::MemoryAccessKind::Load => facts.loads += 1,
        super::partition::MemoryAccessKind::Store => facts.stores += 1,
    }
}

fn collect_offsets(func: &HirFunction, tracked_vars: &HashMap<String, NirType>, out: &mut OffsetMap) {
    for access in collect_partitioned_memory_accesses(&func.body) {
        let HirExpr::Var(name) = &access.base else {
            continue;
        };
        if !tracked_vars.contains_key(name.as_str()) || access.const_offset < 0 {
            continue;
        }
        record_access(name, access.const_offset as u32, access.access_ty.clone(), out);
    }
}

fn collect_access_facts(
    func: &HirFunction,
    tracked_vars: &HashMap<String, NirType>,
    out: &mut AccessFactMap,
) {
    for access in collect_partitioned_memory_accesses(&func.body) {
        let HirExpr::Var(name) = &access.base else {
            continue;
        };
        if !tracked_vars.contains_key(name.as_str()) || access.const_offset < 0 {
            continue;
        }
        record_access_fact(
            name,
            access.const_offset as u32,
            access.access_ty.clone(),
            access.kind,
            out,
        );
    }
}

fn collect_pointer_like_vars(func: &HirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter_map(|binding| match &binding.ty {
            NirType::Ptr(_) => Some((binding.name.clone(), binding.ty.clone())),
            _ => None,
        })
        .collect()
}

fn inferred_aggregate_size(offsets: &HashMap<u32, NirType>) -> Option<u32> {
    let mut max_end = 0u32;
    for (&offset, ty) in offsets {
        let width = type_bits_bytes(ty).max(1);
        max_end = max_end.max(offset.saturating_add(width));
    }
    (max_end > 0).then_some(max_end)
}

fn should_infer_aggregate(offsets: &HashMap<u32, NirType>) -> bool {
    if offsets.len() >= 2 {
        return true;
    }
    offsets.keys().any(|offset| *offset != 0)
}

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

fn should_emit_surface_binding(binding: &NirBinding, offsets: &HashMap<u32, AccessFacts>) -> bool {
    if matches!(&binding.ty, NirType::Ptr(inner) if matches!(inner.as_ref(), NirType::Unknown)) {
        return !offsets.is_empty();
    }
    match infer_storage_class(binding) {
        StorageClass::Param => !offsets.is_empty(),
        StorageClass::StackLocal => {
            offsets.len() >= 2 || offsets.values().any(|facts| facts.stores > 0)
        }
        _ => offsets.len() >= 2,
    }
}

/// Apply aggregate field layout recovery to a function.
///
/// Returns `true` if any `NirType::Aggregate` had fields added to it.
pub(crate) fn apply_aggregate_fields_pass(func: &mut HirFunction) -> bool {
    let tracked_ptr_vars = collect_pointer_like_vars(func);
    if tracked_ptr_vars.is_empty() {
        return false;
    }

    let mut offset_map: OffsetMap = HashMap::new();
    let mut access_facts: AccessFactMap = HashMap::new();
    collect_offsets(func, &tracked_ptr_vars, &mut offset_map);
    collect_access_facts(func, &tracked_ptr_vars, &mut access_facts);

    if offset_map.is_empty() {
        return false;
    }

    let mut changed = false;

    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        let Some(offsets) = offset_map.get(&binding.name) else {
            continue;
        };
        if !can_upgrade_binding_to_aggregate(binding) || !should_infer_aggregate(offsets) {
            continue;
        }
        let Some(size) = inferred_aggregate_size(offsets) else {
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
    let update_binding = |binding: &mut NirBinding, offset_map: &OffsetMap| -> bool {
        let can_surface = access_facts
            .get(&binding.name)
            .map(|accesses| should_emit_surface_binding(binding, accesses))
            .unwrap_or(false);
        if !can_surface {
            return false;
        }
        let NirType::Ptr(inner) = &mut binding.ty else { return false; };
        let NirType::Aggregate { fields, .. } = inner.as_mut() else { return false; };
        if !fields.is_empty() {
            return false; // already populated
        }
        let Some(offsets) = offset_map.get(&binding.name) else { return false; };
        if offsets.is_empty() { return false; }

        let mut new_fields: Vec<StructField> = offsets
            .iter()
            .map(|(&offset, ty)| StructField {
                offset,
                ty: ty.clone(),
                name: format!("field_{offset:x}"),
            })
            .collect();
        new_fields.sort_by_key(|f| f.offset);
        *fields = new_fields;
        true
    };

    for binding in func.locals.iter_mut() {
        if update_binding(binding, &offset_map) {
            add_surface_binding_promotions(1);
            changed = true;
        }
    }
    for binding in func.params.iter_mut() {
        if update_binding(binding, &offset_map) {
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
}
