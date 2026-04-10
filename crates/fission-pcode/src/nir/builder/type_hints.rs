use super::*;
use crate::nir::abstract_location::AbstractStackSlot;
use crate::nir::var_rename::rename_vars_in_stmts;
use tracing::trace_span;

pub(super) struct StackAliasCollector {
    alias_boundaries: Vec<(AbstractStackSlot, u64)>,
}

impl StackAliasCollector {
    pub(super) fn new(func: &HirFunction) -> Self {
        let mut boundaries = Vec::new();
        for local in &func.locals {
            if let Some(slot) = AbstractStackSlot::from_binding_origin(local.origin) {
                if let Some(size) = binding_byte_size(&local.ty) {
                    boundaries.push((slot, size as u64));
                }
            }
        }
        Self { alias_boundaries: boundaries }
    }

    fn might_alias(&self, offset: i64, size: u32) -> bool {
        let probe = AbstractStackSlot(offset);
        let sz = size as u64;
        self.alias_boundaries
            .iter()
            .any(|&(slot, slot_sz)| probe.intervals_overlap(sz, slot, slot_sz))
    }
}

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    let _hints = trace_span!("preview_type_hints", fn_name = %func.name).entered();
    let mut stats = apply_function_name_hints(func, context);
    let alias_collector = StackAliasCollector::new(func);

    let mut pointer_hints: HashMap<String, PreviewCallParamRule> = HashMap::new();
    collect_call_type_hints(&func.body, context, &mut pointer_hints);

    for (var_name, hint) in &pointer_hints {
        if let Some(binding) = find_binding_mut(func, var_name)
            && binding.surface_type_name.is_none()
        {
            let should_apply = match stack_origin_offset(binding.origin) {
                Some((offset, is_derived)) => {
                    is_derived && alias_collector.might_alias(offset, hint.pointer_size)
                }
                // Keep synthetic/test bodies and non-stack params eligible.
                None => true,
            };
            if should_apply {
                binding.surface_type_name = Some(hint.pointer_alias.clone());
                stats.pointer_alias_hits += 1;
            }
        }
    }

    let mut local_hints: HashMap<String, String> = HashMap::new();
    collect_local_surface_hints(&func.body, &pointer_hints, func, &alias_collector, &mut local_hints);

    for (var_name, surface_type_name) in local_hints {
        if let Some(binding) = func
            .locals
            .iter_mut()
            .find(|binding| binding.name == var_name)
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name);
            stats.local_surface_hits += 1;
        }
    }

    stats
}

fn apply_function_name_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    let mut stats = PreviewHintStats::default();
    let Some(hints) = &context.function_hints else {
        return stats;
    };

    let mut renames = Vec::new();
    let mut reserved_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    for binding in &mut func.params {
        let Some(NirBindingOrigin::ParamIndex(index)) = binding.origin else {
            continue;
        };
        let Some(new_name) = hints.param_names.get(index) else {
            continue;
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == binding.name {
            continue;
        }
        if reserved_names.contains(new_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(new_name.to_string());
        renames.push((binding.name.clone(), new_name.to_string()));
        binding.name = new_name.to_string();
        stats.explicit_param_name_hits += 1;
    }

    for binding in &mut func.locals {
        let Some(
            NirBindingOrigin::StackOffset(offset)
            | NirBindingOrigin::HomeSlot(offset)
            | NirBindingOrigin::OutgoingArgSlot(offset),
        ) = binding.origin
        else {
            continue;
        };
        let Some(new_name) = hints.stack_local_names.get(&offset) else {
            continue;
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == binding.name {
            continue;
        }
        if reserved_names.contains(new_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(new_name.to_string());
        renames.push((binding.name.clone(), new_name.to_string()));
        binding.name = new_name.to_string();
        stats.explicit_local_name_hits += 1;
    }

    if !renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &renames);
    }

    for binding in &mut func.params {
        let Some(NirBindingOrigin::ParamIndex(index)) = binding.origin else {
            continue;
        };
        let Some(type_name) = hints.param_type_names.get(&index) else {
            continue;
        };
        let type_name = type_name.trim();
        if !type_name.is_empty() {
            binding.surface_type_name = Some(type_name.to_string());
            stats.explicit_param_type_hits += 1;
        }
    }

    for binding in &mut func.locals {
        let Some((offset, is_derived)) = stack_origin_offset(binding.origin) else {
            continue;
        };
        let Some(type_name) = hints.stack_local_type_names.get(&offset) else {
            continue;
        };
        let type_name = type_name.trim();
        if !type_name.is_empty() {
            binding.surface_type_name = Some(type_name.to_string());
            stats.explicit_local_type_hits += 1;
            if is_derived {
                stats.derived_origin_type_hits += 1;
            }
        }
    }

    if let Some(return_type_name) = hints
        .return_type_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        func.surface_return_type_name = Some(return_type_name.to_string());
        stats.explicit_return_type_hit += 1;
    }

    stats
}

fn stack_origin_offset(origin: Option<NirBindingOrigin>) -> Option<(i64, bool)> {
    match origin {
        Some(NirBindingOrigin::StackOffset(offset)) => Some((offset, false)),
        Some(NirBindingOrigin::HomeSlot(offset))
        | Some(NirBindingOrigin::OutgoingArgSlot(offset)) => Some((offset, false)),
        Some(NirBindingOrigin::DerivedFromStackOffset(offset)) => Some((offset, true)),
        _ => None,
    }
}

fn collect_call_type_hints(
    body: &[HirStmt],
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) => {
                collect_call_hints_from_expr(rhs, context, pointer_hints);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_call_hints_from_expr(va_list, context, pointer_hints);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_call_type_hints(stmts, context, pointer_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_call_type_hints(&case.body, context, pointer_hints);
                }
                collect_call_type_hints(default, context, pointer_hints);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_call_hints_from_expr(cond, context, pointer_hints);
                collect_call_type_hints(then_body, context, pointer_hints);
                collect_call_type_hints(else_body, context, pointer_hints);
            }
            HirStmt::Return(Some(expr)) => {
                collect_call_hints_from_expr(expr, context, pointer_hints);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_call_hints_from_expr(
    expr: &HirExpr,
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            for rule in &context.call_param_rules {
                if rule.callee_name != *target {
                    continue;
                }
                let Some(var_name) = args
                    .get(rule.arg_index)
                    .and_then(peel_surface_var_name_from_expr)
                else {
                    continue;
                };
                pointer_hints
                    .entry(var_name.to_string())
                    .or_insert_with(|| rule.clone());
            }
            for arg in args {
                collect_call_hints_from_expr(arg, context, pointer_hints);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_call_hints_from_expr(expr, context, pointer_hints);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_call_hints_from_expr(lhs, context, pointer_hints);
            collect_call_hints_from_expr(rhs, context, pointer_hints);
        }
        HirExpr::Index { base, index, .. } => {
            collect_call_hints_from_expr(base, context, pointer_hints);
            collect_call_hints_from_expr(index, context, pointer_hints);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    alias_collector: &StackAliasCollector,
    local_hints: &mut HashMap<String, String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref {
                    ptr,
                    ty: NirType::Aggregate { .. } | NirType::Unknown | NirType::Ptr(_),
                } = lhs
                    && let Some(param_name) = peel_surface_var_name_from_expr(ptr)
                    && let Some(local_name) = peel_local_surface_name(rhs)
                    && let Some(rule) = pointer_hints.get(param_name)
                    && let Some(local_binding) = func
                        .locals
                        .iter()
                        .find(|binding| binding.name == local_name)
                {
                    let should_apply = match stack_origin_offset(local_binding.origin) {
                        Some((offset, _)) => rule
                            .pointee_sizes
                            .iter()
                            .any(|&size| alias_collector.might_alias(offset, size)),
                        // Synthetic/test locals may not carry stack-origin metadata.
                        None => binding_byte_size(&local_binding.ty)
                            .map(|size| rule.pointee_sizes.iter().any(|&expected| expected == size))
                            .unwrap_or(false),
                    };
                    if should_apply {
                        local_hints
                            .entry(local_name.to_string())
                            .or_insert_with(|| rule.pointee_alias.clone());
                    }
                }
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_local_surface_hints(stmts, pointer_hints, func, alias_collector, local_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_local_surface_hints(&case.body, pointer_hints, func, alias_collector, local_hints);
                }
                collect_local_surface_hints(default, pointer_hints, func, alias_collector, local_hints);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_local_surface_hints(then_body, pointer_hints, func, alias_collector, local_hints);
                collect_local_surface_hints(else_body, pointer_hints, func, alias_collector, local_hints);
            }
            HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn peel_surface_var_name_from_expr(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => peel_surface_var_name_from_expr(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => {
            peel_surface_var_name_from_expr(base)
        }
        HirExpr::Index { base, index, .. } if matches!(index.as_ref(), HirExpr::Const(0, _)) => {
            peel_surface_var_name_from_expr(base)
        }
        _ => None,
    }
}

fn peel_local_surface_name(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name),
        HirExpr::Cast { expr, .. } | HirExpr::AggregateCopy { src: expr, .. } => {
            peel_local_surface_name(expr)
        }
        _ => None,
    }
}

fn find_binding_mut<'a>(func: &'a mut HirFunction, name: &str) -> Option<&'a mut NirBinding> {
    if let Some(param) = func.params.iter_mut().find(|binding| binding.name == name) {
        return Some(param);
    }
    func.locals.iter_mut().find(|binding| binding.name == name)
}

fn binding_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}
