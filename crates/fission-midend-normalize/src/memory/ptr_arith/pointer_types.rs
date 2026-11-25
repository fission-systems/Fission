use super::super::typed_facts;
use super::PointerLayout;
use crate::HashMap;
use crate::prelude::*;

/// Build a map from variable name → NirType for all locals and params.
pub(super) fn build_binding_type_map(func: &PreHirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| b.ty != NirType::Unknown)
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect()
}

/// Return the pointee type if `ty` is `Ptr(pointee)`.
pub(super) fn pointee_ty(ty: &NirType) -> Option<&NirType> {
    match ty {
        NirType::Ptr(inner) => Some(inner),
        _ => None,
    }
}

/// Return the byte size of a type, if known.
pub(super) fn type_byte_size(ty: &NirType, pointer_layout: PointerLayout) -> Option<u64> {
    match ty {
        NirType::Int { bits, .. } | NirType::Float { bits } => Some(u64::from(*bits / 8)),
        NirType::Bool => Some(1),
        NirType::Ptr(_) => Some(pointer_layout.bytes),
        _ => None,
    }
}

fn collect_pointer_assignment_types(
    stmts: &[PreHirStmt],
    out: &mut HashMap<String, Option<NirType>>,
) {
    for stmt in stmts {
        collect_pointer_assignment_types_stmt(stmt, out);
    }
}

fn record_pointer_assignment(out: &mut HashMap<String, Option<NirType>>, name: &str, ty: NirType) {
    if !matches!(ty, NirType::Ptr(_)) {
        out.insert(name.to_string(), None);
        return;
    }
    match out.get_mut(name) {
        Some(slot) => {
            if slot.as_ref().is_some_and(|existing| existing != &ty) {
                *slot = None;
            }
        }
        None => {
            out.insert(name.to_string(), Some(ty));
        }
    }
}

fn collect_pointer_assignment_types_stmt(
    stmt: &PreHirStmt,
    out: &mut HashMap<String, Option<NirType>>,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => record_pointer_assignment(out, name, expr_type(rhs)),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            collect_pointer_assignment_types(body, out);
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_pointer_assignment_types(then_body, out);
            collect_pointer_assignment_types(else_body, out);
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_pointer_assignment_types_stmt(init, out);
            }
            if let Some(update) = update {
                collect_pointer_assignment_types_stmt(update, out);
            }
            collect_pointer_assignment_types(body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_pointer_assignment_types(&case.body, out);
            }
            collect_pointer_assignment_types(default, out);
        }
        _ => {}
    }
}

pub(super) fn propagate_pointer_assignment_types(func: &mut PreHirFunction) -> bool {
    let mut inferred = HashMap::default();
    collect_pointer_assignment_types(&func.body, &mut inferred);
    if inferred.is_empty() {
        return false;
    }
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    let mut changed = false;
    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        let Some(Some(ty)) = inferred.get(binding.name.as_str()) else {
            continue;
        };
        let compatible = binding.surface_type_name.is_none()
            && match &binding.ty {
                NirType::Unknown => true,
                NirType::Int { bits, .. } => *bits == pointer_bits,
                _ => false,
            };
        if compatible {
            binding.ty = ty.clone();
            changed = true;
        }
    }
    changed
}

/// Infer a pointee type for a variable from its access patterns and/or dynamic stride.
pub(super) fn infer_pointee_type_from_patterns(
    name: &str,
    binding_ty: &NirType,
    inventory: &typed_facts::TypedFactInventory,
    has_dynamic_stride: Option<i64>,
) -> NirType {
    if let NirType::Ptr(inner) = binding_ty {
        if !matches!(
            inner.as_ref(),
            NirType::Unknown | NirType::Int { bits: 8, .. }
        ) {
            return inner.as_ref().clone();
        }
    }

    if let Some(stride) = has_dynamic_stride {
        if stride > 0 {
            return match stride {
                1 => NirType::Int {
                    bits: 8,
                    signed: false,
                },
                2 => NirType::Int {
                    bits: 16,
                    signed: false,
                },
                4 => NirType::Int {
                    bits: 32,
                    signed: false,
                },
                8 => NirType::Int {
                    bits: 64,
                    signed: false,
                },
                _ => NirType::Int {
                    bits: (stride * 8) as u32,
                    signed: false,
                },
            };
        }
    }

    if let Some(obj_facts) = inventory.objects.get(name) {
        let offsets: Vec<u32> = obj_facts.accesses.keys().copied().collect();
        if offsets.is_empty() {
            return NirType::Unknown;
        }

        for stride in [8, 4, 2] {
            let all_multiples = offsets.iter().all(|&o| o % stride == 0);
            if all_multiples && offsets.len() >= 2 {
                let mut indices: Vec<u32> = offsets.iter().map(|&o| o / stride).collect();
                indices.sort_unstable();
                let is_continuous = indices.windows(2).all(|w| w[1] - w[0] <= 2);
                if is_continuous {
                    return match stride {
                        2 => NirType::Int {
                            bits: 16,
                            signed: false,
                        },
                        4 => NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        8 => NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        _ => NirType::Unknown,
                    };
                }
            }
        }

        if offsets.len() >= 2 {
            let inferred_size =
                typed_facts::inferred_aggregate_size(&obj_facts.accesses).unwrap_or(0);
            if inferred_size > 0 {
                return NirType::Aggregate {
                    size: inferred_size,
                    fields: obj_facts.shape.fields.clone(),
                };
            }
        }
    }

    NirType::Unknown
}
