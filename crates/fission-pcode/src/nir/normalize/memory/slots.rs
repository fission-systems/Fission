use super::super::cleanup::expr_has_side_effects;
use super::super::pipeline::normalize_expr;
use super::super::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct MemorySlotKey {
    base_repr: String,
    offset: i64,
    access_size: u32,
    stride: Option<i64>,
}

#[derive(Debug, Clone)]
struct MemorySlotCandidate {
    key: MemorySlotKey,
    base: HirExpr,
    offset: i64,
    elem_ty: NirType,
    count: usize,
}

#[derive(Debug, Clone)]
struct MemorySlotPattern {
    key: MemorySlotKey,
    base: HirExpr,
    elem_ty: NirType,
    index: Option<HirExpr>,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<HirExpr>,
    const_offset: i64,
    scaled_index: Option<(HirExpr, i64)>,
}

#[derive(Debug, Clone)]
struct MemorySlotAlias {
    alias: String,
    elem_ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemorySlotFamilyKey {
    base_repr: String,
    family_offset: i64,
    access_size: u32,
    stride: i64,
}

pub(crate) fn normalize_binding_initializers(bindings: &mut [NirBinding]) {
    for binding in bindings {
        if let Some(initializer) = &mut binding.initializer {
            normalize_expr(initializer);
        }
    }
}

pub(crate) fn apply_memory_slot_surfacing(func: &mut HirFunction) -> bool {
    apply_memory_slot_surfacing_with_mode(func, false)
}

pub(crate) fn apply_memory_slot_surfacing_cheap(func: &mut HirFunction) -> bool {
    apply_memory_slot_surfacing_with_mode(func, true)
}

fn apply_memory_slot_surfacing_with_mode(func: &mut HirFunction, cheap_only: bool) -> bool {
    let mut candidates = HashMap::<MemorySlotKey, MemorySlotCandidate>::new();
    collect_memory_slot_candidates_from_stmts(&func.body, &mut candidates);
    let mut family_counts = HashMap::<MemorySlotFamilyKey, usize>::new();
    let mut family_lanes = HashMap::<MemorySlotFamilyKey, HashSet<i64>>::new();
    let mut family_base_offsets = HashMap::<MemorySlotFamilyKey, i64>::new();
    for candidate in candidates.values() {
        let family_key = memory_slot_family_key(&candidate.key);
        *family_counts.entry(family_key.clone()).or_insert(0) += candidate.count;
        family_lanes
            .entry(family_key.clone())
            .or_default()
            .insert(candidate.key.offset);
        family_base_offsets
            .entry(family_key)
            .and_modify(|offset| *offset = (*offset).min(candidate.key.offset))
            .or_insert(candidate.key.offset);
    }
    let mut aliases = HashMap::<MemorySlotKey, MemorySlotAlias>::new();
    let mut used_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    for candidate in candidates.values().filter(|candidate| {
        if cheap_only && !is_cheap_slot_candidate(candidate) {
            return false;
        }
        let family_key = memory_slot_family_key(&candidate.key);
        let family_total = family_counts.get(&family_key).copied().unwrap_or(0);
        let family_lane_count = family_lanes.get(&family_key).map(HashSet::len).unwrap_or(0);
        let exact_indexable = candidate.key.stride.is_none()
            || candidate.key.stride == Some(i64::from(candidate.key.access_size));
        (exact_indexable && candidate.count >= 2) || (family_total >= 2 && family_lane_count >= 2)
    }) {
        let family_base = family_base_offsets
            .get(&memory_slot_family_key(&candidate.key))
            .copied();
        let alias = next_slot_alias_name(&candidate.key, family_base, &mut used_names);
        aliases.insert(
            candidate.key.clone(),
            MemorySlotAlias {
                alias: alias.clone(),
                elem_ty: candidate.elem_ty.clone(),
            },
        );
        let derived_origin = derive_slot_alias_origin(func, &candidate.base);
        func.locals.push(NirBinding {
            name: alias,
            ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
            surface_type_name: None,
            origin: derived_origin,
            initializer: Some(HirExpr::Cast {
                ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
                expr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(candidate.base.clone()),
                    offset: candidate.offset,
                }),
            }),
        });
    }

    rewrite_memory_slot_stmts(&mut func.body, &aliases)
}

fn derive_slot_alias_origin(func: &HirFunction, base: &HirExpr) -> Option<NirBindingOrigin> {
    match base {
        HirExpr::Var(name) => func
            .params
            .iter()
            .chain(func.locals.iter())
            .find(|binding| binding.name == *name)
            .and_then(|binding| match binding.origin {
                Some(NirBindingOrigin::StackOffset(offset))
                | Some(NirBindingOrigin::DerivedFromStackOffset(offset)) => {
                    Some(NirBindingOrigin::DerivedFromStackOffset(offset))
                }
                _ => None,
            }),
        HirExpr::Cast { expr, .. } => derive_slot_alias_origin(func, expr),
        HirExpr::PtrOffset { base, .. } => derive_slot_alias_origin(func, base),
        _ => None,
    }
}

fn is_cheap_slot_candidate(candidate: &MemorySlotCandidate) -> bool {
    is_cheap_slot_base(&candidate.base)
        && candidate
            .key
            .stride
            .is_none_or(|stride| stride == i64::from(candidate.key.access_size))
}

fn is_cheap_slot_base(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(name) => {
            matches!(
                name.as_str(),
                "esp" | "ebp" | "rsp" | "rbp" | "eax" | "ecx" | "edx" | "ebx" | "esi" | "edi"
            ) || name.starts_with("param_")
                || name.starts_with("local_")
        }
        HirExpr::Cast { expr, .. } => is_cheap_slot_base(expr),
        _ => false,
    }
}

fn memory_slot_family_key(key: &MemorySlotKey) -> MemorySlotFamilyKey {
    let (family_offset, _) = slot_family_layout(key);
    MemorySlotFamilyKey {
        base_repr: key.base_repr.clone(),
        family_offset,
        access_size: key.access_size,
        stride: key.stride.unwrap_or(i64::from(key.access_size)),
    }
}

fn collect_memory_slot_candidates_from_stmts(
    stmts: &[HirStmt],
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref { ptr, ty } = lhs {
                    collect_memory_slot_candidate_from_ptr(ptr, ty, candidates);
                }
                collect_memory_slot_candidates_from_expr(rhs, candidates);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_memory_slot_candidates_from_expr(va_list, candidates);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_memory_slot_candidates_from_expr(expr, candidates);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_memory_slot_candidates_from_stmts(stmts, candidates);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_memory_slot_candidates_from_expr(expr, candidates);
                for case in cases {
                    collect_memory_slot_candidates_from_stmts(&case.body, candidates);
                }
                collect_memory_slot_candidates_from_stmts(default, candidates);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_memory_slot_candidates_from_expr(cond, candidates);
                collect_memory_slot_candidates_from_stmts(then_body, candidates);
                collect_memory_slot_candidates_from_stmts(else_body, candidates);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_memory_slot_candidates_from_expr(
    expr: &HirExpr,
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            collect_memory_slot_candidate_from_ptr(ptr, ty, candidates);
            collect_memory_slot_candidates_from_expr(ptr, candidates);
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_memory_slot_candidates_from_expr(expr, candidates);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_memory_slot_candidates_from_expr(lhs, candidates);
            collect_memory_slot_candidates_from_expr(rhs, candidates);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_memory_slot_candidates_from_expr(arg, candidates);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            collect_memory_slot_candidates_from_expr(base, candidates)
        }
        HirExpr::Index { base, index, .. } => {
            collect_memory_slot_candidates_from_expr(base, candidates);
            collect_memory_slot_candidates_from_expr(index, candidates);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

fn collect_memory_slot_candidate_from_ptr(
    ptr: &HirExpr,
    elem_ty: &NirType,
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    let Some(pattern) = parse_memory_slot_pattern(ptr, elem_ty) else {
        return;
    };
    candidates
        .entry(pattern.key.clone())
        .and_modify(|candidate| candidate.count += 1)
        .or_insert_with(|| MemorySlotCandidate {
            key: pattern.key.clone(),
            base: pattern.base.clone(),
            offset: pattern.key.offset,
            elem_ty: pattern.elem_ty.clone(),
            count: 1,
        });
}

fn rewrite_memory_slot_stmts(
    stmts: &mut [HirStmt],
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_memory_slot_lvalue(lhs, aliases);
                changed |= rewrite_memory_slot_expr(rhs, aliases);
            }
            HirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_memory_slot_expr(va_list, aliases);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                changed |= rewrite_memory_slot_expr(expr, aliases);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                changed |= rewrite_memory_slot_stmts(stmts, aliases);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_memory_slot_expr(expr, aliases);
                for case in cases {
                    changed |= rewrite_memory_slot_stmts(&mut case.body, aliases);
                }
                changed |= rewrite_memory_slot_stmts(default, aliases);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_memory_slot_expr(cond, aliases);
                changed |= rewrite_memory_slot_stmts(then_body, aliases);
                changed |= rewrite_memory_slot_stmts(else_body, aliases);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
}

fn rewrite_memory_slot_lvalue(
    lhs: &mut HirLValue,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, ty } => {
            let changed = rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                let index = pattern.index.unwrap_or_else(zero_index_expr);
                *lhs = HirLValue::Index {
                    base: Box::new(HirExpr::Var(alias.alias.clone())),
                    index: Box::new(index),
                    elem_ty: alias.elem_ty.clone(),
                };
                return true;
            }
            changed
        }
        HirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
            changed
        }
    }
}

fn rewrite_memory_slot_expr(
    expr: &mut HirExpr,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Load { ptr, ty } => {
            changed |= rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                let index = pattern.index.unwrap_or_else(zero_index_expr);
                *expr = HirExpr::Index {
                    base: Box::new(HirExpr::Var(alias.alias.clone())),
                    index: Box::new(index),
                    elem_ty: ty.clone(),
                };
                return true;
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            changed |= rewrite_memory_slot_expr(expr, aliases);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_memory_slot_expr(lhs, aliases);
            changed |= rewrite_memory_slot_expr(rhs, aliases);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_memory_slot_expr(arg, aliases);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn parse_memory_slot_pattern(ptr: &HirExpr, elem_ty: &NirType) -> Option<MemorySlotPattern> {
    let access_size = type_byte_size(elem_ty)?;
    let elem_size = i64::from(access_size);
    let mut parts = AddressParts::default();
    collect_address_parts(ptr, &mut parts, 1)?;
    let base = parts.base?;
    if expr_has_side_effects(&base) {
        return None;
    }
    let stride = parts.scaled_index.as_ref().map(|(_, stride)| *stride);
    let index = match parts.scaled_index {
        Some((index, stride)) if stride == elem_size => Some(index),
        Some((index, stride)) if stride > elem_size && stride % elem_size == 0 => Some(index),
        Some(_) => return None,
        None => None,
    };
    let key = MemorySlotKey {
        base_repr: print_expr(&base),
        offset: parts.const_offset,
        access_size,
        stride,
    };
    Some(MemorySlotPattern {
        key,
        base,
        elem_ty: elem_ty.clone(),
        index,
    })
}

fn collect_address_parts(expr: &HirExpr, parts: &mut AddressParts, sign: i64) -> Option<()> {
    match expr {
        HirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        HirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign),
        HirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, -sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                return add_scaled_index_expr(parts, rhs, sign * *value);
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                return add_scaled_index_expr(parts, lhs, sign * *value);
            }
            add_base_expr(parts, expr.clone(), sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(shift, _) = rhs.as_ref() else {
                return add_base_expr(parts, expr.clone(), sign);
            };
            if *shift < 0 || *shift > 30 {
                return add_base_expr(parts, expr.clone(), sign);
            }
            add_scaled_index_expr(parts, lhs, sign * (1_i64 << shift))
        }
        _ => add_base_expr(parts, expr.clone(), sign),
    }
}

fn add_scaled_index_expr(parts: &mut AddressParts, expr: &HirExpr, stride: i64) -> Option<()> {
    if let HirExpr::Const(value, _) = expr {
        parts.const_offset += stride * *value;
        return Some(());
    }
    if let Some((index, bias)) = extract_index_bias(expr) {
        parts.const_offset += stride * bias;
        return add_scaled_index(parts, index, stride);
    }
    add_scaled_index(parts, expr.clone(), stride)
}

fn extract_index_bias(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    match expr {
        HirExpr::Cast { expr, .. } => extract_index_bias(expr),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                let (index, bias) = extract_index_bias(rhs)?;
                return Some((index, bias + *value));
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias + *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias - *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        _ if !expr_has_side_effects(expr) => Some((expr.clone(), 0)),
        _ => None,
    }
}

fn add_base_expr(parts: &mut AddressParts, expr: HirExpr, sign: i64) -> Option<()> {
    if sign != 1 || matches!(expr, HirExpr::Const(_, _)) {
        return None;
    }
    match &parts.base {
        Some(existing) if existing != &expr => None,
        Some(_) => Some(()),
        None => {
            parts.base = Some(expr);
            Some(())
        }
    }
}

fn add_scaled_index(parts: &mut AddressParts, expr: HirExpr, stride: i64) -> Option<()> {
    if stride <= 0 || expr_has_side_effects(&expr) {
        return None;
    }
    match &parts.scaled_index {
        Some((existing, existing_stride)) if existing != &expr || *existing_stride != stride => {
            None
        }
        Some(_) => Some(()),
        None => {
            parts.scaled_index = Some((expr, stride));
            Some(())
        }
    }
}

fn next_slot_alias_name(
    key: &MemorySlotKey,
    family_base: Option<i64>,
    used_names: &mut HashSet<String>,
) -> String {
    let (family_offset, lane) = slot_family_name_layout(key, family_base);
    let base = if family_offset >= 0 {
        format!("slot_{family_offset:x}")
    } else {
        format!("slot_neg_{:x}", family_offset.unsigned_abs())
    };
    let base = if lane > 0 {
        format!("{base}_lane{lane}")
    } else {
        base
    };
    if used_names.insert(base.clone()) {
        return base;
    }
    let sized = format!("{base}_{}", key.access_size);
    if used_names.insert(sized.clone()) {
        return sized;
    }
    let mut idx = 1usize;
    loop {
        let candidate = format!("{sized}_{idx}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        idx += 1;
    }
}

fn slot_family_name_layout(key: &MemorySlotKey, family_base: Option<i64>) -> (i64, i64) {
    if let Some(family_base) = family_base
        && key.offset >= family_base
    {
        let lane_bytes = key.offset - family_base;
        if lane_bytes % i64::from(key.access_size) == 0 {
            return (family_base, lane_bytes / i64::from(key.access_size));
        }
    }
    slot_family_layout(key)
}

fn slot_family_layout(key: &MemorySlotKey) -> (i64, i64) {
    let Some(stride) = key.stride else {
        return (key.offset, 0);
    };
    if stride <= i64::from(key.access_size) {
        return (key.offset, 0);
    }
    let lane_bytes = key.offset.rem_euclid(stride);
    if lane_bytes % i64::from(key.access_size) != 0 {
        return (key.offset, 0);
    }
    let family_offset = key.offset - lane_bytes;
    let lane = lane_bytes / i64::from(key.access_size);
    (family_offset, lane)
}

fn zero_index_expr() -> HirExpr {
    HirExpr::Const(
        0,
        NirType::Int {
            bits: 64,
            signed: false,
        },
    )
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
