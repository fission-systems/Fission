use super::super::cleanup::expr_has_side_effects;
use super::super::pipeline::normalize_expr;
use crate::prelude::*;
use super::partition::{collect_partitioned_memory_accesses, type_byte_size};
use super::typed_facts::collect_typed_fact_inventory;
use fission_midend_core::wave_stats::add_surface_binding_promotions;
use crate::{HashMap, HashSet};

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
    base: DirExpr,
    offset: i64,
    elem_ty: NirType,
    count: usize,
    first_seen: usize,
}

#[derive(Debug, Clone)]
struct MemorySlotPattern {
    key: MemorySlotKey,
    base: DirExpr,
    elem_ty: NirType,
    index: Option<DirExpr>,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<DirExpr>,
    const_offset: i64,
    scaled_index: Option<(DirExpr, i64)>,
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

pub fn normalize_binding_initializers(bindings: &mut [DirBinding]) {
    for binding in bindings {
        if let Some(initializer) = &mut binding.initializer {
            normalize_expr(initializer);
        }
    }
}

pub fn apply_memory_slot_surfacing(func: &mut DirFunction) -> bool {
    apply_memory_slot_surfacing_with_mode(func, false)
}

pub fn apply_memory_slot_surfacing_cheap(func: &mut DirFunction) -> bool {
    apply_memory_slot_surfacing_with_mode(func, true)
}

fn apply_memory_slot_surfacing_with_mode(func: &mut DirFunction, cheap_only: bool) -> bool {
    let mut candidates = HashMap::<MemorySlotKey, MemorySlotCandidate>::default();
    collect_memory_slot_candidates(func, &mut candidates);
    let alias_defs = collect_single_var_aliases(&func.body);
    let mut ordered_candidates = candidates.values().collect::<Vec<_>>();
    ordered_candidates.sort_by(|lhs, rhs| {
        lhs.first_seen.cmp(&rhs.first_seen).then_with(|| {
            lhs.key
                .base_repr
                .cmp(&rhs.key.base_repr)
                .then_with(|| lhs.key.offset.cmp(&rhs.key.offset))
                .then_with(|| lhs.key.access_size.cmp(&rhs.key.access_size))
                .then_with(|| lhs.key.stride.cmp(&rhs.key.stride))
                .then_with(|| lhs.offset.cmp(&rhs.offset))
                .then_with(|| lhs.count.cmp(&rhs.count))
                .then_with(|| format_expr_key(&lhs.base).cmp(&format_expr_key(&rhs.base)))
                .then_with(|| format!("{:?}", lhs.elem_ty).cmp(&format!("{:?}", rhs.elem_ty)))
        })
    });
    let inventory = collect_typed_fact_inventory(func, false);
    let mut family_counts = HashMap::<MemorySlotFamilyKey, usize>::default();
    let mut family_lanes = HashMap::<MemorySlotFamilyKey, HashSet<i64>>::default();
    let mut family_base_offsets = HashMap::<MemorySlotFamilyKey, i64>::default();
    for candidate in &ordered_candidates {
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
    let mut aliases = HashMap::<MemorySlotKey, MemorySlotAlias>::default();
    let mut used_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    let mut promoted_bindings = Vec::new();
    for candidate in ordered_candidates {
        if cheap_only && !is_cheap_slot_candidate(candidate) {
            continue;
        }
        let family_key = memory_slot_family_key(&candidate.key);
        let family_total = family_counts.get(&family_key).copied().unwrap_or(0);
        let family_lane_count = family_lanes.get(&family_key).map(HashSet::len).unwrap_or(0);
        let exact_indexable = candidate.key.stride.is_none()
            || candidate.key.stride == Some(i64::from(candidate.key.access_size));
        if !((exact_indexable && candidate.count >= 2)
            || (family_total >= 2 && family_lane_count >= 2))
        {
            continue;
        }
        let display_base = resolve_slot_alias_base(func, &alias_defs, &candidate.base);
        if !is_surface_stable_slot_display_base(func, &inventory, &display_base, candidate.offset) {
            continue;
        }
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
        let derived_origin = derive_slot_alias_origin(func, &display_base);
        promoted_bindings.push(DirBinding {
            name: alias,
            ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
            surface_type_name: slot_surface_type_name(&display_base, func, &inventory),
            origin: derived_origin,
            initializer: Some(DirExpr::Cast {
                ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
                expr: Box::new(DirExpr::PtrOffset {
                    base: Box::new(display_base),
                    offset: candidate.offset,
                }),
            }),
        });
    }

    promoted_bindings.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    func.locals.extend(promoted_bindings);

    add_surface_binding_promotions(aliases.len());

    rewrite_memory_slot_stmts(&mut func.body, &aliases)
}

fn slot_surface_type_name(
    base: &DirExpr,
    func: &DirFunction,
    inventory: &super::typed_facts::TypedFactInventory,
) -> Option<String> {
    let DirExpr::Var(name) = base else {
        return None;
    };
    if let Some(object_facts) = inventory.objects.get(name)
        && let Some(struct_name) = object_facts.resolved_struct_name.as_ref()
    {
        return Some(struct_name.clone());
    }
    func.params
        .iter()
        .chain(func.locals.iter())
        .find(|binding| binding.name == *name)
        .and_then(|binding| binding.surface_type_name.clone())
}

fn collect_single_var_aliases(stmts: &[DirStmt]) -> HashMap<String, DirExpr> {
    let mut counts = HashMap::<String, usize>::default();
    let mut defs = HashMap::<String, DirExpr>::default();

    fn visit_stmt(
        stmt: &DirStmt,
        counts: &mut HashMap<String, usize>,
        defs: &mut HashMap<String, DirExpr>,
    ) {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if matches!(rhs, DirExpr::Var(_)) => {
                let entry = counts.entry(name.clone()).or_insert(0);
                *entry += 1;
                if *entry == 1 {
                    defs.insert(name.clone(), rhs.clone());
                } else {
                    defs.remove(name);
                }
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                for nested in body {
                    visit_stmt(nested, counts, defs);
                }
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for nested in then_body {
                    visit_stmt(nested, counts, defs);
                }
                for nested in else_body {
                    visit_stmt(nested, counts, defs);
                }
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_deref() {
                    visit_stmt(init, counts, defs);
                }
                if let Some(update) = update.as_deref() {
                    visit_stmt(update, counts, defs);
                }
                for nested in body {
                    visit_stmt(nested, counts, defs);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for nested in &case.body {
                        visit_stmt(nested, counts, defs);
                    }
                }
                for nested in default {
                    visit_stmt(nested, counts, defs);
                }
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue
            | DirStmt::Label(_)
            | DirStmt::Goto(_) => {}
        }
    }

    for stmt in stmts {
        visit_stmt(stmt, &mut counts, &mut defs);
    }
    defs
}

fn resolve_slot_alias_base(
    func: &DirFunction,
    alias_defs: &HashMap<String, DirExpr>,
    base: &DirExpr,
) -> DirExpr {
    fn resolve_var(
        func: &DirFunction,
        alias_defs: &HashMap<String, DirExpr>,
        name: &str,
        depth: usize,
    ) -> DirExpr {
        if depth >= 8 {
            return DirExpr::Var(name.to_string());
        }
        if let Some(DirExpr::Var(other)) = alias_defs.get(name)
            && other != name
        {
            return resolve_var(func, alias_defs, other, depth + 1);
        }
        let maybe_initializer = func
            .params
            .iter()
            .chain(func.locals.iter())
            .find(|binding| binding.name == name)
            .and_then(|binding| binding.initializer.as_ref());
        let Some(initializer) = maybe_initializer else {
            return DirExpr::Var(name.to_string());
        };
        match initializer {
            DirExpr::Var(other) if other != name => resolve_var(func, alias_defs, other, depth + 1),
            _ => DirExpr::Var(name.to_string()),
        }
    }

    match base {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            resolve_var(func, alias_defs, name, 0)
        }
        DirExpr::Cast { ty, expr } => DirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(resolve_slot_alias_base(func, alias_defs, expr)),
        },
        DirExpr::PtrOffset { base, offset } => DirExpr::PtrOffset {
            base: Box::new(resolve_slot_alias_base(func, alias_defs, base)),
            offset: *offset,
        },
        _ => base.clone(),
    }
}

fn derive_slot_alias_origin(func: &DirFunction, base: &DirExpr) -> Option<NirBindingOrigin> {
    match base {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => func
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
        DirExpr::Cast { expr, .. } => derive_slot_alias_origin(func, expr),
        DirExpr::PtrOffset { base, .. } => derive_slot_alias_origin(func, base),
        _ => None,
    }
}

fn is_surface_stable_slot_display_base(
    func: &DirFunction,
    inventory: &super::typed_facts::TypedFactInventory,
    base: &DirExpr,
    offset: i64,
) -> bool {
    if offset != 0 {
        return true;
    }
    match base {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            if is_cheap_slot_base(base) || slot_surface_type_name(base, func, inventory).is_some() {
                return true;
            }
            if let Some(binding) = func
                .params
                .iter()
                .chain(func.locals.iter())
                .find(|binding| binding.name == *name)
            {
                return !binding.is_temp_like();
            }
            !looks_like_synthetic_temp_name(name)
        }
        DirExpr::Cast { expr, .. } => {
            is_surface_stable_slot_display_base(func, inventory, expr, offset)
        }
        DirExpr::PtrOffset {
            base,
            offset: base_offset,
        } => {
            if *base_offset != 0 {
                return true;
            }
            is_surface_stable_slot_display_base(func, inventory, base, offset)
        }
        _ => true,
    }
}

fn looks_like_synthetic_temp_name(name: &str) -> bool {
    name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

fn is_cheap_slot_candidate(candidate: &MemorySlotCandidate) -> bool {
    is_cheap_slot_base(&candidate.base)
        && candidate
            .key
            .stride
            .is_none_or(|stride| stride == i64::from(candidate.key.access_size))
}

fn is_cheap_slot_base(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            matches!(
                name.as_str(),
                "esp" | "ebp" | "rsp" | "rbp" | "eax" | "ecx" | "edx" | "ebx" | "esi" | "edi"
            ) || name.starts_with("param_")
                || name.starts_with("local_")
        }
        DirExpr::Cast { expr, .. } => is_cheap_slot_base(expr),
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

fn collect_memory_slot_candidates(
    func: &DirFunction,
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    for (first_seen, access) in collect_partitioned_memory_accesses(&func.body)
        .into_iter()
        .enumerate()
    {
        let access_size = match type_byte_size(&access.access_ty) {
            Some(size) => size,
            None => continue,
        };
        let key = MemorySlotKey {
            base_repr: access.base_repr.clone(),
            offset: access.const_offset,
            access_size,
            stride: access.stride,
        };
        candidates
            .entry(key.clone())
            .and_modify(|candidate| candidate.count += 1)
            .or_insert_with(|| MemorySlotCandidate {
                key: key.clone(),
                base: access.base.clone(),
                offset: access.const_offset,
                elem_ty: access.access_ty.clone(),
                count: 1,
                first_seen,
            });
    }
}

fn rewrite_memory_slot_stmts(
    stmts: &mut [DirStmt],
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_memory_slot_lvalue(lhs, aliases);
                changed |= rewrite_memory_slot_expr(rhs, aliases);
            }
            DirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_memory_slot_expr(va_list, aliases);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                changed |= rewrite_memory_slot_expr(expr, aliases);
            }
            DirStmt::Block(stmts)
            | DirStmt::While { body: stmts, .. }
            | DirStmt::DoWhile { body: stmts, .. }
            | DirStmt::For { body: stmts, .. } => {
                changed |= rewrite_memory_slot_stmts(stmts, aliases);
            }
            DirStmt::Switch {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_memory_slot_expr(cond, aliases);
                changed |= rewrite_memory_slot_stmts(then_body, aliases);
                changed |= rewrite_memory_slot_stmts(else_body, aliases);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
    changed
}

fn rewrite_memory_slot_lvalue(
    lhs: &mut DirLValue,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    match lhs {
        DirLValue::Var(_) => false,
        DirLValue::Deref { ptr, ty } => {
            let changed = rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                *lhs = if let Some(index) = pattern.index {
                    DirLValue::Index {
                        base: Box::new(DirExpr::Var(alias.alias.clone())),
                        index: Box::new(index),
                        elem_ty: alias.elem_ty.clone(),
                    }
                } else {
                    DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var(alias.alias.clone())),
                        ty: alias.elem_ty.clone(),
                    }
                };
                return true;
            }
            changed
        }
        DirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
            changed
        }
        DirLValue::FieldAccess { base, .. } => rewrite_memory_slot_expr(base, aliases),
    }
}

fn rewrite_memory_slot_expr(
    expr: &mut DirExpr,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    match expr {
        DirExpr::Load { ptr, ty } => {
            changed |= rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                *expr = if let Some(index) = pattern.index {
                    DirExpr::Index {
                        base: Box::new(DirExpr::Var(alias.alias.clone())),
                        index: Box::new(index),
                        elem_ty: ty.clone(),
                    }
                } else {
                    DirExpr::Load {
                        ptr: Box::new(DirExpr::Var(alias.alias.clone())),
                        ty: ty.clone(),
                    }
                };
                return true;
            }
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            changed |= rewrite_memory_slot_expr(expr, aliases);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_memory_slot_expr(lhs, aliases);
            changed |= rewrite_memory_slot_expr(rhs, aliases);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_memory_slot_expr(arg, aliases);
            }
        }
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_memory_slot_expr(cond, aliases);
            changed |= rewrite_memory_slot_expr(then_expr, aliases);
            changed |= rewrite_memory_slot_expr(else_expr, aliases);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    changed
}

fn parse_memory_slot_pattern(ptr: &DirExpr, elem_ty: &NirType) -> Option<MemorySlotPattern> {
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
        base_repr: format_expr_key(&base),
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

fn collect_address_parts(expr: &DirExpr, parts: &mut AddressParts, sign: i64) -> Option<()> {
    match expr {
        DirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        DirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign),
        DirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign)
        }
        DirExpr::FieldAccess { base, offset, .. } => {
            parts.const_offset += sign * i64::from(*offset);
            collect_address_parts(base, parts, sign)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, sign)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, -sign)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(value, _) = lhs.as_ref() {
                return add_scaled_index_expr(parts, rhs, sign * *value);
            }
            if let DirExpr::Const(value, _) = rhs.as_ref() {
                return add_scaled_index_expr(parts, lhs, sign * *value);
            }
            add_base_expr(parts, expr.clone(), sign)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            let DirExpr::Const(shift, _) = rhs.as_ref() else {
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

fn add_scaled_index_expr(parts: &mut AddressParts, expr: &DirExpr, stride: i64) -> Option<()> {
    if let DirExpr::Const(value, _) = expr {
        parts.const_offset += stride * *value;
        return Some(());
    }
    if let Some((index, bias)) = extract_index_bias(expr) {
        parts.const_offset += stride * bias;
        return add_scaled_index(parts, index, stride);
    }
    add_scaled_index(parts, expr.clone(), stride)
}

fn extract_index_bias(expr: &DirExpr) -> Option<(DirExpr, i64)> {
    match expr {
        DirExpr::Cast { expr, .. } => extract_index_bias(expr),
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(value, _) = lhs.as_ref() {
                let (index, bias) = extract_index_bias(rhs)?;
                return Some((index, bias + *value));
            }
            if let DirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias + *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(value, _) = rhs.as_ref() {
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

fn add_base_expr(parts: &mut AddressParts, expr: DirExpr, sign: i64) -> Option<()> {
    if sign != 1 || matches!(expr, DirExpr::Const(_, _)) {
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

fn add_scaled_index(parts: &mut AddressParts, expr: DirExpr, stride: i64) -> Option<()> {
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
