use super::PointerLayout;
use super::address_terms::typed_pointer_base;
use super::pointer_types::{pointee_ty, type_byte_size};
use crate::HashMap;
use crate::prelude::*;

pub(super) fn try_recover_field_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let mut current_ptr = ptr;
    if let PreHirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let (base_expr, offset) = match current_ptr {
        PreHirExpr::PtrOffset { base, offset } => (base.as_ref().clone(), *offset),
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            let PreHirExpr::Const(raw_offset, _) = rhs.as_ref() else {
                return None;
            };
            let element_offset = match op {
                PreHirBinaryOp::Add => *raw_offset,
                PreHirBinaryOp::Sub => raw_offset.checked_neg()?,
                _ => return None,
            };
            // A surviving Binary operation on a concrete `T *` is C pointer
            // arithmetic, so its constant is an element count. Field layouts
            // use byte offsets. Preserve the expression's own pointer type as
            // the unit witness even when the binding has just been promoted
            // to `Ptr(Aggregate)` by object-shape recovery.
            let offset = match ty {
                NirType::Ptr(pointee) => type_byte_size(pointee, pointer_layout)
                    .filter(|size| *size > 1)
                    .and_then(|size| i64::try_from(size).ok())
                    .and_then(|size| element_offset.checked_mul(size))
                    .unwrap_or(element_offset),
                _ => element_offset,
            };
            (lhs.as_ref().clone(), offset)
        }
        _ => return None,
    };

    let (typed_ptr_expr, ptr_ty, _from_byte_cast) = typed_pointer_base(&base_expr, binding_types)?;
    let elem_ty = pointee_ty(&ptr_ty)?;
    let NirType::Aggregate { size, fields } = elem_ty else {
        return None;
    };

    if offset < 0 {
        return None;
    }
    let field_offset = offset as u32;

    let access_size = type_byte_size(access_ty, pointer_layout)?;
    if access_size == 0 {
        return None;
    }

    if *size > 0 && (field_offset as u64).checked_add(access_size)? > *size as u64 {
        return None;
    }
    if *size == 0 && fields.is_empty() {
        return None;
    }

    let field = fields.iter().find(|f| f.offset == field_offset);
    let (field_name, final_ty) = match field {
        Some(f) => {
            if f.ty != NirType::Unknown {
                if type_byte_size(&f.ty, pointer_layout) != Some(access_size) {
                    return None;
                }
                (f.name.clone(), f.ty.clone())
            } else {
                (f.name.clone(), access_ty.clone())
            }
        }
        None => {
            if *size > 0 && field_offset >= *size {
                return None;
            }
            (format!("field_{field_offset:x}"), access_ty.clone())
        }
    };

    Some(PreHirExpr::FieldAccess {
        base: Box::new(typed_ptr_expr),
        field_name,
        offset: field_offset,
        ty: final_ty,
    })
}

/// Reinterpret a scalar `Index` as an aggregate member only when its original
/// scalar element stride lands exactly on a known member with the same storage
/// width. This handles pointers whose access was recovered before a later pass
/// refined the binding from `T *` to `Aggregate *`, without changing genuine
/// aggregate-array indexing, partial-width accesses, or unknown layout slots.
pub(super) fn try_recover_aggregate_scalar_index_field_access(
    base: &PreHirExpr,
    index: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    let PreHirExpr::Const(index, _) = index else {
        return None;
    };
    if !matches!(
        access_ty,
        NirType::Bool | NirType::Int { .. } | NirType::Float { .. } | NirType::Ptr(_)
    ) {
        return None;
    }
    let element_size = type_byte_size(access_ty, pointer_layout).filter(|size| *size > 0)?;
    let byte_offset = index.checked_mul(i64::try_from(element_size).ok()?)?;
    let field_offset = u32::try_from(byte_offset).ok()?;

    let (typed_base, pointer_ty, _) = typed_pointer_base(base, binding_types)?;
    let NirType::Aggregate { fields, .. } = pointee_ty(&pointer_ty)? else {
        return None;
    };
    let field = fields.iter().find(|field| field.offset == field_offset)?;
    if field.ty == NirType::Unknown
        || type_byte_size(&field.ty, pointer_layout) != Some(element_size)
    {
        return None;
    }

    Some(PreHirExpr::FieldAccess {
        base: Box::new(typed_base),
        field_name: field.name.clone(),
        offset: field_offset,
        ty: field.ty.clone(),
    })
}

fn is_direct_pointer_base(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => true,
        PreHirExpr::Cast {
            ty: NirType::Ptr(_),
            expr,
        } => is_direct_pointer_base(expr),
        _ => false,
    }
}

/// Recover a width-preserving access to the first member when the address is
/// the aggregate pointer itself. Unlike arbitrary offset recovery, this path
/// requires a known scalar member with exactly the access width; a narrow
/// view of a wider member remains a byte/scalar access.
pub(super) fn try_recover_direct_aggregate_scalar_field_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    if !is_direct_pointer_base(ptr)
        || !matches!(
            access_ty,
            NirType::Bool | NirType::Int { .. } | NirType::Float { .. } | NirType::Ptr(_)
        )
    {
        return None;
    }
    let access_size = type_byte_size(access_ty, pointer_layout).filter(|size| *size > 0)?;
    let (typed_base, pointer_ty, _) = typed_pointer_base(ptr, binding_types)?;
    let NirType::Aggregate { fields, .. } = pointee_ty(&pointer_ty)? else {
        return None;
    };
    let field = fields.iter().find(|field| field.offset == 0)?;
    if field.ty == NirType::Unknown
        || type_byte_size(&field.ty, pointer_layout) != Some(access_size)
    {
        return None;
    }

    Some(PreHirExpr::FieldAccess {
        base: Box::new(typed_base),
        field_name: field.name.clone(),
        offset: 0,
        ty: field.ty.clone(),
    })
}
