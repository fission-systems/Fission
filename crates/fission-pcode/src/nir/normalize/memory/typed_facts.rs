use super::super::*;
use super::partition::{
    MemoryAccessKind, MemoryEscapeClass, collect_partitioned_memory_accesses, type_byte_size,
};
use crate::nir::normalize::wave_stats::{
    add_object_root_fact_promotions, add_surface_fact_promotions, add_typed_fact_conflicts,
    add_typed_fact_evidences,
};
use fission_signatures::win_types::WindowsStructures;
use indexmap::IndexMap;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(super) struct TypedAccessFacts {
    pub(super) ty: NirType,
    pub(super) loads: usize,
    pub(super) stores: usize,
}

impl Default for TypedAccessFacts {
    fn default() -> Self {
        Self {
            ty: NirType::Unknown,
            loads: 0,
            stores: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TypedObjectFacts {
    pub(super) object: ObjectFact,
    pub(super) accesses: BTreeMap<u32, TypedAccessFacts>,
    pub(super) shape: TypedObjectShape,
    pub(super) resolved_struct_name: Option<String>,
}

impl Default for TypedObjectFacts {
    fn default() -> Self {
        Self {
            object: ObjectFact {
                root: 0,
                storage_class: StorageClass::Unknown,
                escaped: false,
                interval_set: Vec::new(),
                type_hint: None,
            },
            accesses: BTreeMap::new(),
            shape: TypedObjectShape {
                fields: Vec::new(),
                array_runs: Vec::new(),
                opaque_ranges: Vec::new(),
                confidence: 0,
            },
            resolved_struct_name: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct TypedFactInventory {
    pub(super) store: TypedFactStore,
    pub(super) objects: IndexMap<String, TypedObjectFacts>,
}

fn merge_field_ty(current: &NirType, candidate: &NirType) -> NirType {
    if *current == NirType::Unknown {
        return candidate.clone();
    }
    if *candidate == NirType::Unknown {
        return current.clone();
    }
    let cur_w = type_byte_size(current).unwrap_or(0);
    let cand_w = type_byte_size(candidate).unwrap_or(0);
    if cand_w > cur_w {
        candidate.clone()
    } else {
        current.clone()
    }
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

fn stable_name_root(name: &str) -> ObjectRootId {
    let mut hash = 0i64;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(131).wrapping_add(i64::from(byte));
    }
    hash.saturating_neg().saturating_sub(1)
}

fn object_root_id(binding: &NirBinding) -> ObjectRootId {
    match binding.origin {
        Some(NirBindingOrigin::ParamIndex(index)) => 1_000_000 + index as i64,
        Some(NirBindingOrigin::StackOffset(offset))
        | Some(NirBindingOrigin::HomeSlot(offset))
        | Some(NirBindingOrigin::OutgoingArgSlot(offset))
        | Some(NirBindingOrigin::DerivedFromStackOffset(offset)) => offset,
        Some(NirBindingOrigin::ReturnScaffold) => 2_000_000,
        Some(NirBindingOrigin::VaRegion) => 3_000_000,
        Some(NirBindingOrigin::Temp) | None => stable_name_root(&binding.name),
    }
}

fn candidate_struct_name(
    surface_type_name: Option<&str>,
    structures: &WindowsStructures,
) -> Option<String> {
    let type_name = surface_type_name?.trim();
    if structures.get(type_name).is_some() {
        return Some(type_name.to_string());
    }
    for prefix in ["LP", "P"] {
        if let Some(candidate) = type_name.strip_prefix(prefix)
            && structures.get(candidate).is_some()
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn infer_struct_name_from_offsets(
    accesses: &BTreeMap<u32, TypedAccessFacts>,
    inferred_size: u32,
    is_64bit: bool,
    structures: &WindowsStructures,
) -> (Option<String>, usize) {
    let mut matches = structures
        .structures
        .iter()
        .filter_map(|(name, def)| {
            let struct_size = if is_64bit {
                def.size_64 as u32
            } else {
                def.size_32 as u32
            };
            if struct_size == 0 || struct_size != inferred_size {
                return None;
            }
            let all_offsets_match = accesses.keys().all(|offset| {
                def.fields.iter().any(|field| {
                    let field_offset = if is_64bit {
                        field.offset_64 as u32
                    } else {
                        field.offset_32 as u32
                    };
                    field_offset == *offset
                })
            });
            all_offsets_match.then(|| name.clone())
        })
        .collect::<Vec<_>>();
    let conflict_count = matches.len().saturating_sub(1);
    if matches.len() == 1 {
        (matches.pop(), conflict_count)
    } else {
        (None, conflict_count)
    }
}

pub(super) fn inferred_aggregate_size(accesses: &BTreeMap<u32, TypedAccessFacts>) -> Option<u32> {
    let mut max_end = 0u32;
    for (&offset, facts) in accesses {
        let width = type_byte_size(&facts.ty).unwrap_or(1).max(1);
        max_end = max_end.max(offset.saturating_add(width));
    }
    (max_end > 0).then_some(max_end)
}

pub(super) fn should_infer_aggregate(accesses: &BTreeMap<u32, TypedAccessFacts>) -> bool {
    if accesses.len() >= 2 {
        return true;
    }
    accesses.keys().any(|offset| *offset != 0)
}

pub(super) fn collect_typed_fact_inventory(
    func: &HirFunction,
    record_stats: bool,
) -> TypedFactInventory {
    let structures = WindowsStructures::new();
    let tracked = func
        .locals
        .iter()
        .chain(func.params.iter())
        .filter(|binding| matches!(binding.ty, NirType::Ptr(_)))
        .map(|binding| (binding.name.clone(), binding.clone()))
        .collect::<IndexMap<_, _>>();

    if tracked.is_empty() {
        return TypedFactInventory::default();
    }

    let mut inventory = TypedFactInventory::default();

    for access in collect_partitioned_memory_accesses(&func.body) {
        let HirExpr::Var(name) = &access.base else {
            continue;
        };
        let Some(binding) = tracked.get(name.as_str()) else {
            continue;
        };
        if access.const_offset < 0 {
            continue;
        }
        let key = name.clone();
        let entry = inventory
            .objects
            .entry(key.clone())
            .or_insert_with(|| TypedObjectFacts {
                object: ObjectFact {
                    root: object_root_id(binding),
                    storage_class: infer_storage_class(binding),
                    escaped: false,
                    interval_set: Vec::new(),
                    type_hint: binding.surface_type_name.clone(),
                },
                accesses: BTreeMap::new(),
                shape: TypedObjectShape {
                    fields: Vec::new(),
                    array_runs: Vec::new(),
                    opaque_ranges: Vec::new(),
                    confidence: 0,
                },
                resolved_struct_name: None,
            });
        let facts = entry
            .accesses
            .entry(access.const_offset as u32)
            .or_default();
        facts.ty = merge_field_ty(&facts.ty, &access.access_ty);
        match access.kind {
            MemoryAccessKind::Load => facts.loads += 1,
            MemoryAccessKind::Store => facts.stores += 1,
        }
        let interval_end = (access.const_offset as u32)
            .saturating_add(type_byte_size(&access.access_ty).unwrap_or(1).max(1));
        entry
            .object
            .interval_set
            .push((access.const_offset as u32, interval_end));
        let escape = access.partition_key().escape_class != MemoryEscapeClass::NonEscaping;
        entry.object.escaped |= escape;
        inventory.store.evidences.push(FactEvidence {
            source: FactEvidenceSource::Partition,
            confidence: if escape { 96 } else { 160 },
            kind: FactEvidenceKind::ObjectRoot,
            subject: format!("{key}@0x{:x}", access.const_offset),
        });
    }

    let mut typed_fact_conflicts = 0usize;
    for (binding_name, facts) in &mut inventory.objects {
        facts.object.interval_set.sort_unstable();
        facts.object.interval_set.dedup();

        if let Some(type_hint) = facts.object.type_hint.as_ref() {
            inventory.store.evidences.push(FactEvidence {
                source: FactEvidenceSource::ExplicitType,
                confidence: 224,
                kind: FactEvidenceKind::TypedShape,
                subject: format!("{binding_name}:{type_hint}"),
            });
        }

        let inferred_size = inferred_aggregate_size(&facts.accesses).unwrap_or_default();
        let resolved_struct_name =
            candidate_struct_name(facts.object.type_hint.as_deref(), &structures)
                .map(|name| (Some(name), 0usize))
                .unwrap_or_else(|| {
                    infer_struct_name_from_offsets(
                        &facts.accesses,
                        inferred_size,
                        func.is_64bit,
                        &structures,
                    )
                });
        facts.resolved_struct_name = resolved_struct_name.0;
        typed_fact_conflicts += resolved_struct_name.1;

        let mut named_fields = false;
        let mut name_by_offset = BTreeMap::<u32, String>::new();
        if let Some(struct_name) = facts.resolved_struct_name.as_ref()
            && let Some(struct_def) = structures.get(struct_name)
        {
            let struct_size = if func.is_64bit {
                struct_def.size_64 as u32
            } else {
                struct_def.size_32 as u32
            };
            if struct_size == inferred_size {
                for field in &struct_def.fields {
                    let offset = if func.is_64bit {
                        field.offset_64 as u32
                    } else {
                        field.offset_32 as u32
                    };
                    name_by_offset.insert(offset, field.name.clone());
                }
            }
        }

        facts.shape.fields = facts
            .accesses
            .iter()
            .map(|(&offset, access)| {
                let name = name_by_offset
                    .get(&offset)
                    .cloned()
                    .unwrap_or_else(|| format!("field_{offset:x}"));
                named_fields |= name_by_offset.contains_key(&offset);
                StructField {
                    offset,
                    ty: access.ty.clone(),
                    name,
                }
            })
            .collect();
        facts.shape.confidence = if named_fields {
            224
        } else if facts.accesses.len() >= 2 {
            160
        } else {
            96
        };
        if facts.shape.fields.is_empty() && inferred_size > 0 {
            facts.shape.opaque_ranges.push((0, inferred_size));
        }

        inventory
            .store
            .object_facts
            .insert(binding_name.clone(), facts.object.clone());

        if facts.object.type_hint.is_some() || facts.resolved_struct_name.is_some() {
            let preferred_type = if inferred_size > 0 && should_infer_aggregate(&facts.accesses) {
                NirType::Ptr(Box::new(NirType::Aggregate {
                    size: inferred_size,
                    fields: facts.shape.fields.clone(),
                }))
            } else {
                tracked
                    .get(binding_name.as_str())
                    .map(|binding| binding.ty.clone())
                    .unwrap_or(NirType::Unknown)
            };
            let reason = facts
                .resolved_struct_name
                .as_ref()
                .map(|name| format!("struct:{name}"))
                .or_else(|| {
                    facts
                        .object
                        .type_hint
                        .as_ref()
                        .map(|hint| format!("surface:{hint}"))
                })
                .unwrap_or_else(|| "fact".to_string());
            inventory.store.surface_facts.insert(
                binding_name.clone(),
                SurfaceFact {
                    binding: binding_name.clone(),
                    preferred_name: binding_name.clone(),
                    preferred_type,
                    reason,
                },
            );
            inventory.store.evidences.push(FactEvidence {
                source: if facts.resolved_struct_name.is_some() {
                    FactEvidenceSource::StructuralInference
                } else {
                    FactEvidenceSource::ExplicitType
                },
                confidence: facts.shape.confidence,
                kind: FactEvidenceKind::SurfaceBinding,
                subject: binding_name.clone(),
            });
        }
    }

    if record_stats {
        add_typed_fact_evidences(inventory.store.evidences.len());
        add_typed_fact_conflicts(typed_fact_conflicts);
        add_object_root_fact_promotions(inventory.store.object_facts.len());
        add_surface_fact_promotions(inventory.store.surface_facts.len());
    }

    inventory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_fact_inventory_keeps_escaped_root_coarse() {
        let func = HirFunction {
            name: "escaped".to_string(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("param_1".to_string())),
                    offset: 4,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            })],
            ..Default::default()
        };
        let inventory = collect_typed_fact_inventory(&func, false);
        let facts = inventory.objects.get("param_1").expect("object facts");
        assert!(facts.object.escaped);
    }

    #[test]
    fn typed_fact_inventory_prefers_explicit_surface_type() {
        let func = HirFunction {
            name: "rect".to_string(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: Some("LPRECT".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![],
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
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
            ],
            ..Default::default()
        };
        let inventory = collect_typed_fact_inventory(&func, false);
        let facts = inventory.objects.get("param_1").expect("object facts");
        assert_eq!(facts.resolved_struct_name.as_deref(), Some("RECT"));
        assert!(inventory.store.surface_facts.contains_key("param_1"));
    }
}
