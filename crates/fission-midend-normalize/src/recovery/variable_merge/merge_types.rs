use crate::prelude::*;

pub(super) fn unify_types_for_merge(t1: &NirType, t2: &NirType) -> Option<NirType> {
    if *t1 == NirType::Unknown {
        return Some(t2.clone());
    }
    if *t2 == NirType::Unknown {
        return Some(t1.clone());
    }
    match (t1, t2) {
        (
            NirType::Int {
                bits: b1,
                signed: s1,
            },
            NirType::Int {
                bits: b2,
                signed: s2,
            },
        ) => Some(NirType::Int {
            bits: (*b1).max(*b2),
            signed: *s1 || *s2,
        }),
        (NirType::Ptr(p1), NirType::Ptr(p2)) => {
            let inner = unify_types_for_merge(p1, p2)?;
            Some(NirType::Ptr(Box::new(inner)))
        }
        (t1, t2) if t1 == t2 => Some(t1.clone()),
        _ => None,
    }
}

pub(super) fn force_unify_types_for_merge(t1: &NirType, t2: &NirType) -> NirType {
    if let Some(unified) = unify_types_for_merge(t1, t2) {
        return unified;
    }
    let size1 = type_byte_size(t1).unwrap_or(4);
    let size2 = type_byte_size(t2).unwrap_or(4);
    if size1 >= size2 {
        t1.clone()
    } else {
        t2.clone()
    }
}

pub(super) fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}
