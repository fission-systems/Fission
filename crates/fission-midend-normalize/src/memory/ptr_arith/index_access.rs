use super::PointerLayout;
use super::address_terms::{try_extract_index_scale, typed_pointer_base};
use super::pointer_types::{pointee_ty, type_byte_size};
use crate::HashMap;
use crate::prelude::*;

pub(super) fn try_recover_index_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let mut current_ptr = ptr;
    if let PreHirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = current_ptr
    else {
        return None;
    };
    let ptr_ty = match lhs.as_ref() {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => binding_types.get(name.as_str()).and_then(|t| {
            if matches!(t, NirType::Ptr(_)) {
                Some(t)
            } else {
                None
            }
        })?,
        _ => return None,
    };
    let mut elem_ty = pointee_ty(ptr_ty)
        .cloned()
        .unwrap_or_else(|| access_ty.clone());
    // Prefer the access type when it is float of the same width as a default
    // int pointee. FLOAT_* stores/loads lower with float access_ty, but the
    // base may still be bound as `uint*` from size-default typing; do not let
    // that erase float matrix evidence before use-type-infer runs.
    if let (
        NirType::Float { bits: access_bits },
        NirType::Int {
            bits: elem_bits, ..
        },
    ) = (access_ty, &elem_ty)
    {
        if access_bits == elem_bits {
            elem_ty = access_ty.clone();
        }
    }
    let (idx_expr, stride, from_shift) = try_extract_index_scale(rhs.as_ref())?;

    // On-the-fly refinement for stride/index matching
    if matches!(elem_ty, NirType::Unknown | NirType::Int { bits: 8, .. }) && stride > 0 {
        elem_ty = match stride {
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

    let access_size = type_byte_size(access_ty, pointer_layout)
        .or_else(|| type_byte_size(&elem_ty, pointer_layout))?;
    if stride > 0 && stride as u64 == access_size {
        Some(PreHirExpr::Index {
            base: lhs.clone(),
            index: Box::new(idx_expr),
            elem_ty: access_ty.clone(),
        })
    } else if from_shift {
        // A shifted stride that is not the access size is a field of a wider
        // element; only the multiplied form has been measured for that shape.
        None
    } else if stride > 0
        && access_size > 0
        && (stride as u64) > access_size
        && (stride as u64) % access_size == 0
    {
        // Array-of-struct / wide-stride: load of the first field of element i
        // (`*(T*)(base + i * stride)` with sizeof(T) < stride).
        let elem_ty = NirType::Aggregate {
            size: stride as u32,
            fields: vec![fission_midend_core::ir::StructField {
                offset: 0,
                ty: access_ty.clone(),
                name: "field_0".to_string(),
            }],
        };
        Some(PreHirExpr::FieldAccess {
            base: Box::new(PreHirExpr::Index {
                base: lhs.clone(),
                index: Box::new(idx_expr),
                elem_ty,
            }),
            field_name: "field_0".to_string(),
            offset: 0,
            ty: access_ty.clone(),
        })
    } else {
        None
    }
}

fn pointer_index_base_for_access(
    expr: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<(Box<PreHirExpr>, i64)> {
    let (_base, ptr_ty, _from_byte_cast) = typed_pointer_base(expr, binding_types)?;
    let elem_ty = pointee_ty(&ptr_ty)?;
    if matches!(elem_ty, NirType::Aggregate { .. } | NirType::Unknown) {
        return None;
    }
    let access_size = i64::try_from(type_byte_size(access_ty, pointer_layout)?).ok()?;
    if access_size == 0 {
        return None;
    }
    let elem_size = i64::try_from(type_byte_size(elem_ty, pointer_layout)?).ok()?;
    if elem_size != access_size {
        return None;
    }
    Some((Box::new(expr.clone()), access_size))
}

pub(super) fn try_recover_const_index_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let mut current_ptr = ptr;
    if let PreHirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let PreHirExpr::Binary { op, lhs, rhs, .. } = current_ptr else {
        return None;
    };
    let PreHirExpr::Const(raw_index, index_ty) = rhs.as_ref() else {
        return None;
    };
    let raw_offset = match op {
        PreHirBinaryOp::Add => *raw_index,
        PreHirBinaryOp::Sub => raw_index.checked_neg()?,
        _ => return None,
    };
    let (base, elem_size) =
        pointer_index_base_for_access(lhs, access_ty, binding_types, pointer_layout)?;
    if raw_offset % elem_size != 0 {
        return None;
    }
    let index = raw_offset / elem_size;
    Some(PreHirExpr::Index {
        base,
        index: Box::new(PreHirExpr::Const(index, index_ty.clone())),
        elem_ty: access_ty.clone(),
    })
}
