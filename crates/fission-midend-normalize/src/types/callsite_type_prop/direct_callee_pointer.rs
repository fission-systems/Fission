//! Direct-callee pointer contract propagation.
//!
//! This module applies admitted concrete pointer parameters from an isolated
//! callee to stable caller argument chains, while rejecting conflicts with
//! caller-side pointer evidence.

use super::*;

/// Keep a typed direct-call pointer contract at the call boundary when the
/// recovered actual is scalar-shaped. This leaves the caller binding and its
/// other uses untouched, avoiding pointer-scale changes to local arithmetic.
pub(super) fn cast_direct_callee_pointer_arguments(func: &mut PreHirFunction) -> bool {
    let binding_types = func
        .params
        .iter()
        .chain(&func.locals)
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect::<HashMap<_, _>>();
    let mut binding_surface_types = func
        .params
        .iter()
        .chain(&func.locals)
        .filter_map(|binding| {
            binding
                .surface_type_name
                .as_ref()
                .map(|surface| (binding.name.clone(), surface.clone()))
        })
        .collect::<HashMap<_, _>>();
    let mut copy_sources = HashMap::default();
    collect_copy_sources(&func.body, &mut copy_sources);
    for alias in copy_sources.keys() {
        let mut current = alias.as_str();
        let mut visited = HashSet::default();
        while visited.insert(current) {
            if let Some(surface) = binding_surface_types.get(current).cloned() {
                binding_surface_types.insert(alias.clone(), surface);
                break;
            }
            let Some(source) = copy_sources.get(current) else {
                break;
            };
            current = source;
        }
    }
    let summaries = &func.callee_summaries;
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    cast_pointer_arguments_in_stmts(
        &mut func.body,
        &binding_types,
        &binding_surface_types,
        summaries,
        pointer_bits,
    )
}

fn cast_pointer_arguments_in_stmts(
    stmts: &mut [PreHirStmt],
    binding_types: &HashMap<String, NirType>,
    binding_surface_types: &HashMap<String, String>,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                changed |= cast_pointer_arguments_in_lvalue(
                    lhs,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
                changed |= cast_pointer_arguments_in_expr(
                    rhs,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                changed |= cast_pointer_arguments_in_expr(
                    expr,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::VaStart { va_list, .. } => {
                changed |= cast_pointer_arguments_in_expr(
                    va_list,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= cast_pointer_arguments_in_stmts(
                    std::rc::Rc::make_mut(body).as_mut_slice(),
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= cast_pointer_arguments_in_expr(
                    cond,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
                changed |= cast_pointer_arguments_in_stmts(
                    std::rc::Rc::make_mut(then_body).as_mut_slice(),
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
                changed |= cast_pointer_arguments_in_stmts(
                    std::rc::Rc::make_mut(else_body).as_mut_slice(),
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    changed |= cast_pointer_arguments_in_stmts(
                        std::slice::from_mut(init.as_mut()),
                        binding_types,
                        binding_surface_types,
                        summaries,
                        pointer_bits,
                    );
                }
                if let Some(cond) = cond {
                    changed |= cast_pointer_arguments_in_expr(
                        cond,
                        binding_types,
                        binding_surface_types,
                        summaries,
                        pointer_bits,
                    );
                }
                if let Some(update) = update {
                    changed |= cast_pointer_arguments_in_stmts(
                        std::slice::from_mut(update.as_mut()),
                        binding_types,
                        binding_surface_types,
                        summaries,
                        pointer_bits,
                    );
                }
                changed |= cast_pointer_arguments_in_stmts(
                    std::rc::Rc::make_mut(body).as_mut_slice(),
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= cast_pointer_arguments_in_expr(
                    expr,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
                for case in cases {
                    changed |= cast_pointer_arguments_in_stmts(
                        std::rc::Rc::make_mut(&mut case.body).as_mut_slice(),
                        binding_types,
                        binding_surface_types,
                        summaries,
                        pointer_bits,
                    );
                }
                changed |= cast_pointer_arguments_in_stmts(
                    std::rc::Rc::make_mut(default).as_mut_slice(),
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    changed
}

fn cast_pointer_arguments_in_lvalue(
    lvalue: &mut PreHirLValue,
    binding_types: &HashMap<String, NirType>,
    binding_surface_types: &HashMap<String, String>,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    match lvalue {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => cast_pointer_arguments_in_expr(
            ptr,
            binding_types,
            binding_surface_types,
            summaries,
            pointer_bits,
        ),
        PreHirLValue::Index { base, index, .. } => {
            cast_pointer_arguments_in_expr(
                base,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            ) | cast_pointer_arguments_in_expr(
                index,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            )
        }
        PreHirLValue::FieldAccess { base, .. } => cast_pointer_arguments_in_expr(
            base,
            binding_types,
            binding_surface_types,
            summaries,
            pointer_bits,
        ),
    }
}

fn cast_pointer_arguments_in_expr(
    expr: &mut PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    binding_surface_types: &HashMap<String, String>,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    match expr {
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => cast_pointer_arguments_in_expr(
            expr,
            binding_types,
            binding_surface_types,
            summaries,
            pointer_bits,
        ),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            cast_pointer_arguments_in_expr(
                lhs,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            ) | cast_pointer_arguments_in_expr(
                rhs,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            )
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            cast_pointer_arguments_in_expr(
                cond,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            ) | cast_pointer_arguments_in_expr(
                then_expr,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            ) | cast_pointer_arguments_in_expr(
                else_expr,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            )
        }
        PreHirExpr::Index { base, index, .. } => {
            cast_pointer_arguments_in_expr(
                base,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            ) | cast_pointer_arguments_in_expr(
                index,
                binding_types,
                binding_surface_types,
                summaries,
                pointer_bits,
            )
        }
        PreHirExpr::Call { target, args, .. } => {
            let mut changed = false;
            for arg in args.iter_mut() {
                changed |= cast_pointer_arguments_in_expr(
                    arg,
                    binding_types,
                    binding_surface_types,
                    summaries,
                    pointer_bits,
                );
            }

            let resolved = resolve_call_target_symbol_with_wrapper(target, summaries).0;
            let Some(summary) = summaries.get(target).or_else(|| summaries.get(resolved)) else {
                return changed;
            };
            if matches!(
                summary.target.provenance,
                CallTargetProvenance::Import | CallTargetProvenance::Intrinsic
            ) {
                return changed;
            }

            for (index, arg) in args.iter_mut().enumerate() {
                if !matches!(
                    summary.prototype.param_lattices.get(index),
                    Some(NirType::Ptr(_))
                ) {
                    continue;
                }
                let Some(surface) = summary
                    .prototype
                    .param_surface_type_names
                    .get(index)
                    .and_then(Option::as_deref)
                    .map(str::trim)
                else {
                    continue;
                };
                if !surface.contains('*')
                    || surface.contains("(*")
                    || !fission_signatures::pointer_surface_type_name_is_specific(surface)
                {
                    continue;
                }

                let actual_ty = prehir_expr_type(arg, binding_types);
                let explicit_pointer_contract = summary
                    .prototype
                    .param_pointer_contracts
                    .get(index)
                    .copied()
                    .unwrap_or(false);
                if matches!(actual_ty, NirType::Ptr(_))
                    || !explicit_pointer_contract
                        && matches!(arg, PreHirExpr::Var(name) if binding_surface_types.contains_key(name))
                    || !matches!(actual_ty, NirType::Unknown)
                        && !matches!(actual_ty, NirType::Int { bits, .. } if bits == pointer_bits)
                {
                    continue;
                }

                *arg = PreHirExpr::Cast {
                    ty: NirType::Ptr(Box::new(NirType::Unknown)),
                    expr: Box::new(arg.clone()),
                };
                changed = true;
            }
            changed
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn prehir_expr_type(expr: &PreHirExpr, binding_types: &HashMap<String, NirType>) -> NirType {
    match expr {
        PreHirExpr::Var(name) => binding_types.get(name).cloned().unwrap_or(NirType::Unknown),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
            NirType::Ptr(Box::new(NirType::Unknown))
        }
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. }
        | PreHirExpr::FieldAccess { ty, .. }
        | PreHirExpr::Cast { ty, .. } => ty.clone(),
        PreHirExpr::PtrOffset { base, .. } => prehir_expr_type(base, binding_types),
        PreHirExpr::Index { elem_ty, .. } => elem_ty.clone(),
        PreHirExpr::AggregateCopy { .. } => NirType::Unknown,
    }
}

fn binding_accepts_direct_callee_pointer(
    binding: &PreHirBinding,
    candidate: &NirType,
    pointer_bits: u32,
) -> bool {
    match (&binding.ty, candidate) {
        (NirType::Unknown, NirType::Ptr(_)) => true,
        (existing, candidate) if existing == candidate => true,
        (NirType::Ptr(existing), NirType::Ptr(candidate)) => {
            **candidate == NirType::Unknown || **existing == NirType::Unknown
        }
        (NirType::Int { bits, .. }, NirType::Ptr(_)) => {
            *bits == pointer_bits && binding.surface_type_name.is_none()
        }
        _ => false,
    }
}

fn expr_root_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr, .. } => expr_root_var(expr),
        _ => None,
    }
}

fn expr_uses_pointer_base(expr: &PreHirExpr, names: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Load { ptr, .. } => {
            expr_root_var(ptr).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(ptr, names)
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
                || expr_uses_pointer_base(index, names)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_pointer_base(lhs, names) || expr_uses_pointer_base(rhs, names)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_uses_pointer_base(expr, names)
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_uses_pointer_base(src, names),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_pointer_base(cond, names)
                || expr_uses_pointer_base(then_expr, names)
                || expr_uses_pointer_base(else_expr, names)
        }
        PreHirExpr::Call { args, .. } => args
            .iter()
            .any(|argument| expr_uses_pointer_base(argument, names)),
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn lvalue_uses_pointer_base(lvalue: &PreHirLValue, names: &HashSet<String>) -> bool {
    match lvalue {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => {
            expr_root_var(ptr).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(ptr, names)
        }
        PreHirLValue::Index { base, index, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
                || expr_uses_pointer_base(index, names)
        }
        PreHirLValue::FieldAccess { base, .. } => {
            expr_root_var(base).is_some_and(|name| names.contains(name))
                || expr_uses_pointer_base(base, names)
        }
    }
}

fn stmts_use_pointer_base(stmts: &[PreHirStmt], names: &HashSet<String>) -> bool {
    stmts.iter().any(|stmt| match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_uses_pointer_base(lhs, names) || expr_uses_pointer_base(rhs, names)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            expr_uses_pointer_base(expr, names)
        }
        PreHirStmt::VaStart { va_list, .. } => expr_uses_pointer_base(va_list, names),
        PreHirStmt::Block(body) => stmts_use_pointer_base(body, names),
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            expr_uses_pointer_base(cond, names) || stmts_use_pointer_base(body, names)
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_pointer_base(cond, names)
                || stmts_use_pointer_base(then_body, names)
                || stmts_use_pointer_base(else_body, names)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmts_use_pointer_base(std::slice::from_ref(stmt), names))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_uses_pointer_base(expr, names))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmts_use_pointer_base(std::slice::from_ref(stmt), names))
                || stmts_use_pointer_base(body, names)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_pointer_base(expr, names)
                || cases
                    .iter()
                    .any(|case| stmts_use_pointer_base(&case.body, names))
                || stmts_use_pointer_base(default, names)
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    })
}

/// Apply an isolated direct callee's admitted pointer parameter contract to
/// the matching caller argument. The callee fact producer has already rejected
/// generic `void*` and pointer types without a concrete pointee or informative
/// surface declaration. Backward transit uses the same stable-copy proof as
/// API prototype propagation.
pub(super) fn apply_direct_callee_pointer_transitively(
    func: &mut PreHirFunction,
    copy_sources: &HashMap<String, String>,
    definition_counts: &HashMap<String, usize>,
    self_referential: &HashSet<String>,
    arg_var: &str,
    param_ty: &NirType,
    surface_type_name: Option<&str>,
) -> bool {
    let concrete_pointee = matches!(param_ty, NirType::Ptr(inner) if **inner != NirType::Unknown);
    let informative_surface =
        surface_type_name.is_some_and(fission_signatures::pointer_surface_type_name_is_specific);
    if !matches!(param_ty, NirType::Ptr(_)) || (!concrete_pointee && !informative_surface) {
        return false;
    }
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    let mut chain = Vec::new();
    while visited.insert(current.clone()) {
        chain.push(current.clone());
        if !super::super::type_flow::binding_is_safe_for_backward_refine(
            &current,
            definition_counts,
            self_referential,
        ) {
            break;
        }
        match copy_sources.get(&current) {
            Some(source)
                if super::super::type_flow::binding_is_safe_for_backward_refine(
                    source,
                    definition_counts,
                    self_referential,
                ) =>
            {
                current = source.clone();
            }
            None | Some(_) => break,
        }
    }

    if !chain
        .iter()
        .any(|name| func.params.iter().any(|param| param.name == *name))
    {
        return false;
    }

    let has_surface_conflict = chain.iter().any(|name| {
        func.locals
            .iter()
            .chain(func.params.iter())
            .find(|binding| binding.name == *name)
            .and_then(|binding| binding.surface_type_name.as_deref())
            .is_some_and(|existing| surface_type_name != Some(existing))
    });
    if has_surface_conflict {
        return false;
    }

    let chain_names = chain.iter().cloned().collect::<HashSet<_>>();
    if surface_type_name.is_none() && stmts_use_pointer_base(&func.body, &chain_names) {
        // A concrete pointee observed only through the callee must not be
        // stacked on top of the caller's independent dereference/index
        // evidence. The caller-side solver owns pointer depth in that case;
        // this interprocedural rule is for otherwise scalar forwarding chains.
        return false;
    }

    let has_type_conflict = chain.iter().any(|name| {
        func.locals
            .iter()
            .chain(func.params.iter())
            .find(|binding| binding.name == *name)
            .is_some_and(|binding| {
                !binding_accepts_direct_callee_pointer(binding, param_ty, pointer_bits)
            })
    });
    if has_type_conflict {
        return false;
    }

    let mut changed = false;
    for name in &chain {
        if let Some(binding) = binding_by_name_mut(&mut func.locals, name)
            .or_else(|| binding_by_name_mut(&mut func.params, name))
        {
            changed |= tighten_binding_ty_from_pointer_contract(binding, param_ty, pointer_bits);
            if binding.surface_type_name.is_none()
                && let Some(surface) = surface_type_name
            {
                binding.surface_type_name = Some(surface.to_string());
                changed = true;
            }
        }
    }
    if changed && std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[DIRECT-CALLEE-TYPE-DIAG] function={} arg={} candidate={:?} surface={:?} chain={:?}",
            func.name, arg_var, param_ty, surface_type_name, chain
        );
    }
    changed
}
