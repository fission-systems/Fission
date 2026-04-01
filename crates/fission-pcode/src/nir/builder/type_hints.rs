use super::*;

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
) -> PreviewHintStats {
    let mut stats = apply_function_name_hints(func, context);

    let mut pointer_hints: HashMap<String, PreviewCallParamRule> = HashMap::new();
    collect_call_type_hints(&func.body, context, &mut pointer_hints);

    for (var_name, hint) in &pointer_hints {
        if let Some(binding) = find_binding_mut(func, var_name)
            && binding.surface_type_name.is_none()
            && binding_byte_size(&binding.ty) == Some(hint.pointer_size)
        {
            binding.surface_type_name = Some(hint.pointer_alias.clone());
            stats.heuristic_pointer_alias_hits += 1;
        }
    }

    let mut local_hints: HashMap<String, String> = HashMap::new();
    collect_local_surface_hints(&func.body, &pointer_hints, func, &mut local_hints);
    for (var_name, surface_type_name) in local_hints {
        if let Some(binding) = func
            .locals
            .iter_mut()
            .find(|binding| binding.name == var_name)
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name);
            stats.heuristic_local_surface_hits += 1;
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
        let Some(NirBindingOrigin::StackOffset(offset)) = binding.origin else {
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
                    && let Some(local_size) = binding_byte_size(&local_binding.ty)
                    && rule.pointee_sizes.contains(&local_size)
                {
                    local_hints
                        .entry(local_name.to_string())
                        .or_insert_with(|| rule.pointee_alias.clone());
                }
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_local_surface_hints(stmts, pointer_hints, func, local_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_local_surface_hints(&case.body, pointer_hints, func, local_hints);
                }
                collect_local_surface_hints(default, pointer_hints, func, local_hints);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_local_surface_hints(then_body, pointer_hints, func, local_hints);
                collect_local_surface_hints(else_body, pointer_hints, func, local_hints);
            }
            HirStmt::Expr(_)
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

fn rename_vars_in_stmts(body: &mut [HirStmt], renames: &[(String, String)]) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                rename_var_in_lvalue(lhs, renames);
                rename_var_in_expr(rhs, renames);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => rename_var_in_expr(expr, renames),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => rename_vars_in_stmts(stmts, renames),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rename_var_in_expr(expr, renames);
                for case in cases {
                    rename_vars_in_stmts(&mut case.body, renames);
                }
                rename_vars_in_stmts(default, renames);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(then_body, renames);
                rename_vars_in_stmts(else_body, renames);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn rename_var_in_lvalue(lvalue: &mut HirLValue, renames: &[(String, String)]) {
    match lvalue {
        HirLValue::Var(name) => rename_var_name(name, renames),
        HirLValue::Deref { ptr, .. } => rename_var_in_expr(ptr, renames),
        HirLValue::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
    }
}

fn rename_var_in_expr(expr: &mut HirExpr, renames: &[(String, String)]) {
    match expr {
        HirExpr::Var(name) => rename_var_name(name, renames),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => rename_var_in_expr(expr, renames),
        HirExpr::Binary { lhs, rhs, .. } => {
            rename_var_in_expr(lhs, renames);
            rename_var_in_expr(rhs, renames);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                rename_var_in_expr(arg, renames);
            }
        }
        HirExpr::PtrOffset { base, .. } => rename_var_in_expr(base, renames),
        HirExpr::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        HirExpr::Const(_, _) => {}
    }
}

fn rename_var_name(name: &mut String, renames: &[(String, String)]) {
    if let Some((_, replacement)) = renames.iter().find(|(from, _)| from == name) {
        *name = replacement.clone();
    }
}

fn binding_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}
