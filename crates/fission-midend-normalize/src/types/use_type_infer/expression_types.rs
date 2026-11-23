use crate::HashMap;
use crate::prelude::*;

pub(super) fn expr_int_bits(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<u32> {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => known_binding_types.get(name).and_then(nir_type_bits),
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. }
        | PreHirExpr::Index { elem_ty: ty, .. }
        | PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::FieldAccess { ty, .. } => nir_type_bits(ty),
        PreHirExpr::Binary { ty, .. } => nir_type_bits(ty),
        PreHirExpr::PtrOffset { .. } | PreHirExpr::AggregateCopy { .. } => None,
    }
}

pub(super) fn indirect_call_target_binding_name(target: &str) -> Option<&str> {
    if is_binding_name(target) {
        return Some(target);
    }
    target
        .strip_prefix("((code *)")
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|name| is_binding_name(name))
}

fn is_binding_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Extract the bit-width of an integer/bool NirType, if applicable.
pub(super) fn nir_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Bool => None,
        _ => None,
    }
}

pub(super) fn collect_known_binding_types(func: &PreHirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::default();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if binding.ty != NirType::Unknown {
            known.insert(binding.name.clone(), binding.ty.clone());
        }
    }
    known
}

pub(super) fn return_expr_type(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => known_binding_types.get(name).cloned(),
        other => {
            let ty = expr_type(other);
            (ty != NirType::Unknown).then_some(ty)
        }
    }
}
