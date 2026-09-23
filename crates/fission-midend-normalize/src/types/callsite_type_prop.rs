/// Call-site inter-procedural type propagation pass.
///
/// All type inference so far has been intra-procedural: it only sees the types
/// of expressions *within* the current function.  `call malloc(size)` still
/// returns `Ptr(Unknown)`, `memcpy(dst, src, n)` arguments stay `Unknown`.
///
/// This pass connects the existing `fission-signatures` Windows API database
/// to the Fission type inference pipeline:
///
/// 1. Walk every `PreHirStmt::Assign { rhs: Call { target, args } }` and
///    `PreHirStmt::Expr(Call { target, args })`.
/// 2. Look up `target` in the signatures API type provider.
/// 3. For the return value: if there is a receiver binding (the lhs `Var` of
///    the Assign), update `PreHirBinding.ty` to the resolved return type.
/// 4. For each argument: if the argument is a `Var(x)` and the corresponding
///    parameter has a concrete type, update the binding for `x`.
/// 5. Indirect/unknown calls (target not in DB) are silently skipped.
/// 6. Variadic functions (e.g. `printf`): only the first parameter is typed.
///
/// Type resolution (`win_type_name_to_nir`) maps Windows type-name strings
/// (from `ApiSignature.return_type` / `ParamInfo.type_name`) to `NirType`:
///
/// | Win type string | NirType |
/// |-----------------|---------|
/// | DWORD / UINT / ULONG | Int { bits: 32, signed: false } |
/// | INT / BOOL / LONG | Int { bits: 32, signed: true } |
/// | WORD / USHORT | Int { bits: 16, signed: false } |
/// | SHORT | Int { bits: 16, signed: true } |
/// | BYTE / UCHAR | Int { bits: 8, signed: false } |
/// | CHAR | Int { bits: 8, signed: true } |
/// | QWORD / UINT64 / ULONG64 | Int { bits: 64, signed: false } |
/// | LONGLONG / INT64 | Int { bits: 64, signed: true } |
/// | SIZE_T / ULONG_PTR | Int { bits: 64, signed: false } |
/// | HANDLE / LPVOID / PVOID | Ptr(Unknown) |
/// | LPSTR / LPCSTR | Ptr(Int8 unsigned) |
/// | LPWSTR / LPCWSTR | Ptr(Int16 unsigned) |
/// | HWND / HMODULE / HKEY / … HANDLEs | Ptr(Aggregate{size:0}) |
/// | BOOL | Int { bits: 32, signed: true } |
/// | void / VOID | (no constraint) |
///
/// Constraints are injected using the same `merge_constraint` / fixed-point
/// loop from `use_type_infer.rs`, so existing type knowledge is never weakened.
mod call_arity;
mod call_target_surface;
mod direct_callee_pointer;
mod format_inference;

pub use super::api_signature::{api_signature, is_known_api_signature, win_type_name_to_nir};
use super::api_signature::{api_signature_via_import_aliases, resolve_return_ty};
use call_arity::{
    drop_unused_call_receivers, drop_void_call_receivers, prune_known_api_call_args_stmts,
    prune_self_call_args_stmts,
};
use call_target_surface::{
    apply_api_surface_type_transitively, apply_binding_surface_renames, build_call_target_rewrites,
    is_generic_binding_name, register_name_candidate, resolve_call_target_symbol,
    resolve_call_target_symbol_with_wrapper, rewrite_call_targets_stmts,
};
use direct_callee_pointer::{
    apply_direct_callee_pointer_transitively, cast_direct_callee_pointer_arguments,
};
use format_inference::{
    apply_site_sensitive_translated_format_types, apply_variadic_printf_format_string_arg_types,
    collect_copy_sources, parse_printf_format_specifier_types,
};

use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::wave_stats::{
    add_call_prototype_exact_api_arity_pruned, add_call_prototype_signature_missing,
    add_call_prototype_unknown_target_kept, add_call_prototype_wrapper_resolved,
    add_call_signature_refinements, add_surface_fact_promotions, add_typed_fact_conflicts,
};
use fission_midend_prehir::util::rename_vars_in_stmts;
use fission_signatures::{
    canonical_variadic_runtime_symbol, is_known_variadic_runtime_symbol,
    pointer_surface_type_name_is_specific, printf_style_format_string_arg_index,
    type_name_is_informative,
};

/// Attempt to tighten a binding's type using a new candidate.
/// Follows the same monotone strengthening logic as `use_type_infer`:
/// Unknown can be replaced by anything; a concrete type is only replaced if the
/// candidate is strictly more informative (pointer vs. integer, or known vs. unknown).
fn tighten_binding_ty(binding: &mut PreHirBinding, candidate: &NirType) -> bool {
    if binding.ty == *candidate {
        return false;
    }
    match (&binding.ty, candidate) {
        (NirType::Unknown, _) => {
            binding.ty = candidate.clone();
            true
        }
        (NirType::Ptr(a), NirType::Ptr(b))
            if **a == NirType::Unknown && **b != NirType::Unknown =>
        {
            binding.ty = candidate.clone();
            true
        }
        _ => false,
    }
}

/// Apply a vetted pointer contract to a binding whose current integer type is
/// only the machine-width representation of a pointer value.
pub(super) fn tighten_binding_ty_from_pointer_contract(
    binding: &mut PreHirBinding,
    candidate: &NirType,
    pointer_bits: u32,
) -> bool {
    if tighten_binding_ty(binding, candidate) {
        return true;
    }
    if binding.surface_type_name.is_some() || !matches!(candidate, NirType::Ptr(_)) {
        return false;
    }
    match binding.ty {
        NirType::Int { bits, .. } if bits == pointer_bits => {
            binding.ty = candidate.clone();
            true
        }
        _ => false,
    }
}

/// Apply call-site type propagation to a function.
///
/// Collects all `Call` expressions, looks up each target in the API type provider, and
/// updates argument/receiver bindings with the resolved types.
///
/// Returns `true` if any binding type was updated.
pub fn apply_callsite_type_prop_pass(func: &mut PreHirFunction) -> bool {
    // Build a lookup map from binding name to index in func.locals / func.params.
    let mut changed = false;
    let pointer_bits = if func.is_64bit { 64 } else { 32 };
    let callee_summaries = func.callee_summaries.clone();
    let void_receivers = drop_void_call_receivers(&mut func.body, &callee_summaries);
    let unused_receivers = drop_unused_call_receivers(func);
    let dropped_receivers = void_receivers + unused_receivers;
    if dropped_receivers > 0 {
        add_call_signature_refinements(dropped_receivers);
        changed = true;
    }
    let mut rename_candidates = HashMap::<String, String>::default();
    let mut rename_conflicts = HashSet::<String>::default();
    let mut wrapper_resolved_count = 0usize;
    let mut signature_missing_count = 0usize;
    let mut unknown_target_kept_count = 0usize;
    let mut definition_counts = HashMap::default();
    let mut self_referential = HashSet::default();
    super::type_flow::collect_definition_counts(&func.body, &mut definition_counts);
    super::type_flow::collect_self_referential_bindings(&func.body, &mut self_referential);
    let mut copy_sources = HashMap::default();
    collect_copy_sources(&func.body, &mut copy_sources);
    let mut pointer_copy_sources = HashMap::default();
    collect_pointer_copy_sources(&func.body, pointer_bits, &mut pointer_copy_sources);
    changed |= apply_api_pointer_return_types_in_stmts(
        &mut func.body,
        &func.callee_summaries,
        pointer_bits,
    );

    // Collect call sites: (receiver_name_opt, callee_name, arg_var_names)
    let mut callsites: Vec<(Option<String>, String, Vec<Option<String>>)> = Vec::new();
    collect_callsites_stmts(&func.body, &mut callsites);
    changed |= apply_variadic_printf_format_string_arg_types(func, &callsites);
    changed |= apply_site_sensitive_translated_format_types(func);
    let call_target_rewrites = build_call_target_rewrites(&func.callee_summaries);

    for (receiver, callee, arg_vars) in &callsites {
        let (resolved_callee, resolved_through_wrapper) =
            resolve_call_target_symbol_with_wrapper(callee, &func.callee_summaries);
        if resolved_through_wrapper {
            wrapper_resolved_count += 1;
        }
        let summary = func
            .callee_summaries
            .get(callee)
            .or_else(|| func.callee_summaries.get(resolved_callee))
            .cloned();
        let Some(sig) = api_signature_via_import_aliases(resolved_callee)
            .or_else(|| api_signature_via_import_aliases(callee))
        else {
            if summary.is_some() {
                signature_missing_count += 1;
            } else {
                unknown_target_kept_count += 1;
            }
            if let Some(summary) = summary.as_ref() {
                let mut refined_here = false;
                if let Some(recv_name) = receiver
                    && summary.prototype.return_lattice != NirType::Unknown
                    && let Some(b) = binding_by_name_mut(&mut func.locals, recv_name)
                        .or_else(|| binding_by_name_mut(&mut func.params, recv_name))
                {
                    let tightened = tighten_binding_ty(b, &summary.prototype.return_lattice);
                    changed |= tightened;
                    refined_here |= tightened;
                }
                for (i, arg_var_opt) in arg_vars.iter().enumerate() {
                    let Some(arg_var) = arg_var_opt else {
                        continue;
                    };
                    let Some(param_ty) = summary.prototype.param_lattices.get(i) else {
                        break;
                    };
                    if *param_ty == NirType::Unknown {
                        continue;
                    }
                    let surface_type_name = summary
                        .prototype
                        .param_surface_type_names
                        .get(i)
                        .and_then(Option::as_deref);
                    let tightened = apply_direct_callee_pointer_transitively(
                        func,
                        &copy_sources,
                        &definition_counts,
                        &self_referential,
                        arg_var,
                        param_ty,
                        surface_type_name,
                    );
                    changed |= tightened;
                    refined_here |= tightened;
                }
                if refined_here {
                    add_call_signature_refinements(1);
                }
            }
            continue;
        };
        let mut refined_here = false;

        // A type string of `int` records that nothing was recovered -- the GDT
        // extractor writes it for any type ID it could not resolve, so `int` is
        // either a recovered `int` or a lost `FILE *`. Applying it replaces
        // inference with a confident wrong answer.
        //
        // The test is per type string, not per signature: `difftime` is stored
        // `difftime|double|_Time1:int,_Time2:int`, and judging the entry whole
        // keeps that `double` and takes both placeholders with it. Parameter
        // names are unaffected, so the rename path below stays live.
        //
        // Resolve return type and update receiver binding.
        if let Some(ret_ty) = resolve_return_ty(&sig.return_type)
            .filter(|_| type_name_is_informative(&sig.return_type))
        {
            if let Some(recv_name) = receiver {
                if matches!(ret_ty, NirType::Ptr(_)) {
                    let stable_local_result = definition_counts.get(recv_name).copied() == Some(1)
                        && !self_referential.contains(recv_name)
                        && !func.params.iter().any(|param| param.name == *recv_name);
                    if stable_local_result {
                        let (tightened, can_apply_surface) = if let Some(binding) =
                            binding_by_name_mut(&mut func.locals, recv_name)
                        {
                            let tightened = if binding.surface_type_name.is_none() {
                                tighten_binding_ty_from_pointer_contract(
                                    binding,
                                    &ret_ty,
                                    pointer_bits,
                                )
                            } else {
                                false
                            };
                            (
                                tightened,
                                binding.surface_type_name.is_none()
                                    && matches!(binding.ty, NirType::Ptr(_)),
                            )
                        } else {
                            (false, false)
                        };
                        changed |= tightened;
                        refined_here |= tightened;
                        if can_apply_surface
                            && pointer_surface_type_name_is_specific(&sig.return_type)
                        {
                            let surfaced = apply_api_surface_type_transitively(
                                func,
                                &copy_sources,
                                &definition_counts,
                                &self_referential,
                                recv_name,
                                sig.return_type.trim(),
                            );
                            changed |= surfaced;
                            refined_here |= surfaced;
                        }
                    }
                } else if let Some(binding) = binding_by_name_mut(&mut func.locals, recv_name)
                    .or_else(|| binding_by_name_mut(&mut func.params, recv_name))
                {
                    let tightened = tighten_binding_ty(binding, &ret_ty);
                    changed |= tightened;
                    refined_here |= tightened;
                }
            }
        }

        // Resolve each parameter type and update argument bindings.
        for (i, arg_var_opt) in arg_vars.iter().enumerate() {
            let Some(arg_var) = arg_var_opt else {
                continue;
            };
            let Some(param) = sig.params.get(i) else {
                break;
            };
            let informative = type_name_is_informative(&param.type_name);
            let param_ty = informative
                .then(|| win_type_name_to_nir(&param.type_name))
                .flatten();
            if matches!(param_ty.as_ref(), Some(NirType::Ptr(_))) {
                changed |= promote_pointer_copy_carrier_from_typed_source(
                    func,
                    &pointer_copy_sources,
                    &definition_counts,
                    &self_referential,
                    arg_var,
                    pointer_bits,
                );
            }
            if let Some(b) = binding_by_name_mut(&mut func.locals, arg_var)
                .or_else(|| binding_by_name_mut(&mut func.params, arg_var))
            {
                let tightened = param_ty
                    .as_ref()
                    .is_some_and(|param_ty| tighten_binding_ty(b, param_ty));
                changed |= tightened;
                refined_here |= tightened;
                if !matches!(b.origin, Some(NirBindingOrigin::ParamIndex(_)))
                    && is_generic_binding_name(arg_var)
                {
                    register_name_candidate(
                        &mut rename_candidates,
                        &mut rename_conflicts,
                        arg_var,
                        &param.name,
                    );
                }
            }
            if informative {
                let surface_tightened = apply_api_surface_type_transitively(
                    func,
                    &copy_sources,
                    &definition_counts,
                    &self_referential,
                    arg_var,
                    param.type_name.trim(),
                );
                changed |= surface_tightened;
                refined_here |= surface_tightened;
            }
        }
        if refined_here {
            add_call_signature_refinements(1);
        }
    }

    let rename_count = apply_binding_surface_renames(func, rename_candidates, &rename_conflicts);
    if rename_count > 0 {
        add_surface_fact_promotions(rename_count);
        changed = true;
    }
    if !rename_conflicts.is_empty() {
        add_typed_fact_conflicts(rename_conflicts.len());
    }
    let pruned_count = prune_known_api_call_args_stmts(&mut func.body, &func.callee_summaries);
    if pruned_count > 0 {
        add_call_signature_refinements(pruned_count);
        add_call_prototype_exact_api_arity_pruned(pruned_count);
        changed = true;
    }
    let self_pruned_count = if func.variadic_fixed_arity.is_none() {
        prune_self_call_args_stmts(&mut func.body, &func.name, func.params.len())
    } else {
        0
    };
    if self_pruned_count > 0 {
        add_call_signature_refinements(self_pruned_count);
        changed = true;
    }
    add_call_prototype_wrapper_resolved(wrapper_resolved_count);
    add_call_prototype_signature_missing(signature_missing_count);
    add_call_prototype_unknown_target_kept(unknown_target_kept_count);
    if !call_target_rewrites.is_empty()
        && rewrite_call_targets_stmts(&mut func.body, &call_target_rewrites)
    {
        changed = true;
    }
    changed |= cast_direct_callee_pointer_arguments(func);
    changed |= strip_pointer_width_integer_copy_casts_in_stmts(
        &mut func.body,
        &func.locals,
        &func.params,
        pointer_bits,
    );

    changed
}

fn binding_by_name_mut<'a>(
    bindings: &'a mut Vec<PreHirBinding>,
    name: &str,
) -> Option<&'a mut PreHirBinding> {
    bindings.iter_mut().find(|b| b.name == name)
}

/// Extract the plain variable name from a Call argument expression (if it's
/// `Var(x)` or `Cast(_, Var(x))`).  Returns `None` for complex expressions.
fn arg_var_name(expr: &PreHirExpr) -> Option<String> {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => Some(name.clone()),
        PreHirExpr::Cast { expr: inner, .. } => arg_var_name(inner),
        _ => None,
    }
}

fn strip_pointer_width_integer_copy_casts_in_stmts(
    stmts: &mut [PreHirStmt],
    locals: &[PreHirBinding],
    params: &[PreHirBinding],
    pointer_bits: u32,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(destination),
                rhs,
            } => {
                let destination_is_pointer = binding_type(locals, params, destination)
                    .is_some_and(|ty| matches!(ty, NirType::Ptr(_)));
                if destination_is_pointer
                    && let Some(source) = pointer_width_integer_copy_source(rhs, pointer_bits)
                    && binding_type(locals, params, source)
                        .is_some_and(|ty| matches!(ty, NirType::Ptr(_)))
                {
                    *rhs = PreHirExpr::Var(source.to_string());
                    changed = true;
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    locals,
                    params,
                    pointer_bits,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    locals,
                    params,
                    pointer_bits,
                );
                changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    locals,
                    params,
                    pointer_bits,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                        std::slice::from_mut(init.as_mut()),
                        locals,
                        params,
                        pointer_bits,
                    );
                }
                if let Some(update) = update {
                    changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                        std::slice::from_mut(update.as_mut()),
                        locals,
                        params,
                        pointer_bits,
                    );
                }
                changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    locals,
                    params,
                    pointer_bits,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        locals,
                        params,
                        pointer_bits,
                    );
                }
                changed |= strip_pointer_width_integer_copy_casts_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    locals,
                    params,
                    pointer_bits,
                );
            }
            _ => {}
        }
    }
    changed
}

fn binding_type<'a>(
    locals: &'a [PreHirBinding],
    params: &'a [PreHirBinding],
    name: &str,
) -> Option<&'a NirType> {
    locals
        .iter()
        .chain(params.iter())
        .find(|binding| binding.name == name)
        .map(|binding| &binding.ty)
}

fn pointer_width_integer_copy_source(expr: &PreHirExpr, pointer_bits: u32) -> Option<&str> {
    let mut current = expr;
    let mut saw_integer_cast = false;
    while let PreHirExpr::Cast { ty, expr } = current {
        if !matches!(ty, NirType::Int { bits, .. } if *bits == pointer_bits) {
            return None;
        }
        saw_integer_cast = true;
        current = expr;
    }
    match current {
        PreHirExpr::Var(name) if saw_integer_cast => Some(name),
        _ => None,
    }
}

fn pointer_copy_source(expr: &PreHirExpr, pointer_bits: u32) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        _ => pointer_width_integer_copy_source(expr, pointer_bits),
    }
}

fn collect_pointer_copy_sources(
    stmts: &[PreHirStmt],
    pointer_bits: u32,
    out: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(destination),
                rhs,
            } => {
                if let Some(source) = pointer_copy_source(rhs, pointer_bits) {
                    out.insert(destination.clone(), source.to_string());
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_pointer_copy_sources(body, pointer_bits, out);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_pointer_copy_sources(then_body, pointer_bits, out);
                collect_pointer_copy_sources(else_body, pointer_bits, out);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_pointer_copy_sources(
                        std::slice::from_ref(init.as_ref()),
                        pointer_bits,
                        out,
                    );
                }
                if let Some(update) = update {
                    collect_pointer_copy_sources(
                        std::slice::from_ref(update.as_ref()),
                        pointer_bits,
                        out,
                    );
                }
                collect_pointer_copy_sources(body, pointer_bits, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_pointer_copy_sources(&case.body, pointer_bits, out);
                }
                collect_pointer_copy_sources(default, pointer_bits, out);
            }
            _ => {}
        }
    }
}

/// Refine a scalar copy carrier only when its unique-definition chain ends at
/// an already typed pointer. The API parameter alone is not enough: integer
/// stack locals passed to pointer-taking APIs remain integer-typed unless a
/// bit-preserving copy from pointer evidence proves their value role.
fn promote_pointer_copy_carrier_from_typed_source(
    func: &mut PreHirFunction,
    copy_sources: &HashMap<String, String>,
    definition_counts: &HashMap<String, usize>,
    self_referential: &HashSet<String>,
    arg_var: &str,
    pointer_bits: u32,
) -> bool {
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    let mut carriers = Vec::new();
    let pointer_type = loop {
        if !visited.insert(current.clone()) {
            return false;
        }
        let Some(binding) = func
            .locals
            .iter()
            .chain(func.params.iter())
            .find(|binding| binding.name == current)
        else {
            return false;
        };
        if let NirType::Ptr(_) = &binding.ty {
            break binding.ty.clone();
        }
        if func.params.iter().any(|param| param.name == current)
            || binding.surface_type_name.is_some()
            || !matches!(binding.ty, NirType::Int { bits, .. } if bits == pointer_bits)
            || !super::type_flow::binding_is_safe_for_backward_refine(
                &current,
                definition_counts,
                self_referential,
            )
        {
            return false;
        }
        carriers.push(current.clone());
        let Some(source) = copy_sources.get(&current) else {
            return false;
        };
        current = source.clone();
    };

    if carriers.is_empty() {
        return false;
    }
    for name in carriers {
        let Some(binding) = binding_by_name_mut(&mut func.locals, &name) else {
            return false;
        };
        binding.ty = pointer_type.clone();
    }
    true
}

fn apply_api_pointer_return_types_in_stmts(
    stmts: &mut [PreHirStmt],
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_api_pointer_return_types_in_stmt(stmt, summaries, pointer_bits);
    }
    changed
}

fn apply_api_pointer_return_types_in_rc_stmts(
    stmts: &mut std::rc::Rc<Vec<PreHirStmt>>,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    apply_api_pointer_return_types_in_stmts(
        std::rc::Rc::make_mut(stmts).as_mut_slice(),
        summaries,
        pointer_bits,
    )
}

fn apply_api_pointer_return_types_in_stmt(
    stmt: &mut PreHirStmt,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            apply_api_pointer_return_types_in_lvalue(lhs, summaries, pointer_bits)
                | apply_api_pointer_return_types_in_expr(rhs, summaries, pointer_bits)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            apply_api_pointer_return_types_in_expr(expr, summaries, pointer_bits)
        }
        PreHirStmt::VaStart { va_list, .. } => {
            apply_api_pointer_return_types_in_expr(va_list, summaries, pointer_bits)
        }
        PreHirStmt::Block(body) => {
            apply_api_pointer_return_types_in_rc_stmts(body, summaries, pointer_bits)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            let mut changed = apply_api_pointer_return_types_in_expr(expr, summaries, pointer_bits);
            for case in cases {
                changed |= apply_api_pointer_return_types_in_rc_stmts(
                    &mut case.body,
                    summaries,
                    pointer_bits,
                );
            }
            changed | apply_api_pointer_return_types_in_rc_stmts(default, summaries, pointer_bits)
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let cond_changed =
                apply_api_pointer_return_types_in_expr(cond, summaries, pointer_bits);
            let then_changed =
                apply_api_pointer_return_types_in_rc_stmts(then_body, summaries, pointer_bits);
            let else_changed =
                apply_api_pointer_return_types_in_rc_stmts(else_body, summaries, pointer_bits);
            cond_changed | then_changed | else_changed
        }
        PreHirStmt::While { cond, body } => {
            let cond_changed =
                apply_api_pointer_return_types_in_expr(cond, summaries, pointer_bits);
            let body_changed =
                apply_api_pointer_return_types_in_rc_stmts(body, summaries, pointer_bits);
            cond_changed | body_changed
        }
        PreHirStmt::DoWhile { body, cond } => {
            let body_changed =
                apply_api_pointer_return_types_in_rc_stmts(body, summaries, pointer_bits);
            let cond_changed =
                apply_api_pointer_return_types_in_expr(cond, summaries, pointer_bits);
            body_changed | cond_changed
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let init_changed = init.as_deref_mut().is_some_and(|init| {
                apply_api_pointer_return_types_in_stmt(init, summaries, pointer_bits)
            });
            let cond_changed = cond.as_mut().is_some_and(|cond| {
                apply_api_pointer_return_types_in_expr(cond, summaries, pointer_bits)
            });
            let update_changed = update.as_deref_mut().is_some_and(|update| {
                apply_api_pointer_return_types_in_stmt(update, summaries, pointer_bits)
            });
            let body_changed =
                apply_api_pointer_return_types_in_rc_stmts(body, summaries, pointer_bits);
            init_changed | cond_changed | update_changed | body_changed
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn apply_api_pointer_return_types_in_lvalue(
    lvalue: &mut PreHirLValue,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    match lvalue {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => {
            apply_api_pointer_return_types_in_expr(ptr, summaries, pointer_bits)
        }
        PreHirLValue::Index { base, index, .. } => {
            apply_api_pointer_return_types_in_expr(base, summaries, pointer_bits)
                | apply_api_pointer_return_types_in_expr(index, summaries, pointer_bits)
        }
        PreHirLValue::FieldAccess { base, .. } => {
            apply_api_pointer_return_types_in_expr(base, summaries, pointer_bits)
        }
    }
}

fn apply_api_pointer_return_types_in_expr(
    expr: &mut PreHirExpr,
    summaries: &indexmap::IndexMap<String, CallSummary>,
    pointer_bits: u32,
) -> bool {
    match expr {
        PreHirExpr::Call { target, args, ty } => {
            let resolved = resolve_call_target_symbol_with_wrapper(target, summaries).0;
            let signature = api_signature_via_import_aliases(resolved)
                .or_else(|| api_signature_via_import_aliases(target));
            let mut changed = false;
            if let Some(signature) = signature
                && type_name_is_informative(&signature.return_type)
                && let Some(candidate @ NirType::Ptr(_)) = resolve_return_ty(&signature.return_type)
            {
                let replaces_machine_word =
                    matches!(ty, NirType::Int { bits, .. } if *bits == pointer_bits);
                let replaces_unknown = *ty == NirType::Unknown;
                let refines_unknown_pointee = matches!(
                    (&*ty, &candidate),
                    (NirType::Ptr(existing), NirType::Ptr(next))
                        if **existing == NirType::Unknown && **next != NirType::Unknown
                );
                if replaces_machine_word || replaces_unknown || refines_unknown_pointee {
                    *ty = candidate;
                    changed = true;
                }
            }
            for arg in args {
                changed |= apply_api_pointer_return_types_in_expr(arg, summaries, pointer_bits);
            }
            changed
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            apply_api_pointer_return_types_in_expr(expr, summaries, pointer_bits)
        }
        PreHirExpr::Binary { lhs, rhs, .. }
        | PreHirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            apply_api_pointer_return_types_in_expr(lhs, summaries, pointer_bits)
                | apply_api_pointer_return_types_in_expr(rhs, summaries, pointer_bits)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let cond_changed =
                apply_api_pointer_return_types_in_expr(cond, summaries, pointer_bits);
            let then_changed =
                apply_api_pointer_return_types_in_expr(then_expr, summaries, pointer_bits);
            let else_changed =
                apply_api_pointer_return_types_in_expr(else_expr, summaries, pointer_bits);
            cond_changed | then_changed | else_changed
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn collect_callsites_stmts(
    stmts: &[PreHirStmt],
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    for stmt in stmts {
        collect_callsites_stmt(stmt, out);
    }
}

fn collect_callsites_stmt(
    stmt: &PreHirStmt,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            if let PreHirExpr::Call { target, args, .. } = rhs {
                let recv = match lhs {
                    PreHirLValue::Var(name) => Some(name.clone()),
                    _ => None,
                };
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((recv, target.clone(), arg_vars));
            }
            // Also recurse in case call appears inside a more complex rhs.
            collect_callsites_expr(rhs, out);
        }
        PreHirStmt::Expr(expr) => {
            if let PreHirExpr::Call { target, args, .. } = expr {
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((None, target.clone(), arg_vars));
            }
        }
        PreHirStmt::Return(Some(expr)) => collect_callsites_expr(expr, out),
        PreHirStmt::Block(body) => collect_callsites_stmts(body, out),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(then_body, out);
            collect_callsites_stmts(else_body, out);
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_callsites_stmt(i, out);
            }
            if let Some(c) = cond {
                collect_callsites_expr(c, out);
            }
            if let Some(u) = update {
                collect_callsites_stmt(u, out);
            }
            collect_callsites_stmts(body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_callsites_expr(expr, out);
            for case in cases {
                collect_callsites_stmts(&case.body, out);
            }
            collect_callsites_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_callsites_expr(
    expr: &PreHirExpr,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            let arg_vars = args.iter().map(arg_var_name).collect();
            out.push((None, target.clone(), arg_vars));
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_callsites_expr(lhs, out);
            collect_callsites_expr(rhs, out);
        }
        PreHirExpr::Cast { expr: inner, .. } | PreHirExpr::Unary { expr: inner, .. } => {
            collect_callsites_expr(inner, out);
        }
        PreHirExpr::Load { ptr, .. } => collect_callsites_expr(ptr, out),
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            collect_callsites_expr(base, out)
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_callsites_expr(base, out);
            collect_callsites_expr(index, out);
        }
        PreHirExpr::AggregateCopy { src, .. } => collect_callsites_expr(src, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::wave_stats::{reset_normalize_wave_stats, take_normalize_wave_stats};
    // prelude via parent
    use fission_core::CallingConvention;

    fn unknown_binding(name: &str, origin: Option<NirBindingOrigin>) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    fn unsigned_binding(name: &str, bits: u32, origin: Option<NirBindingOrigin>) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: NirType::Int {
                bits,
                signed: false,
            },
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    fn direct_pointer_summary(
        target: &str,
        param_ty: NirType,
        surface: Option<&str>,
    ) -> CallSummary {
        CallSummary {
            target: CallTargetRef {
                address: Some(0x2000),
                symbol: target.to_string(),
                provenance: CallTargetProvenance::Direct,
                edge_kind: CallEdgeKind::Direct,
                confidence: 160,
            },
            prototype: PrototypeSummary {
                variadic_fixed_arity: None,
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: Some(1),
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![param_ty],
                param_surface_type_names: vec![surface.map(str::to_string)],
                param_pointer_contracts: vec![false],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: Some(true),
                writes_memory: None,
                escapes_args: None,
                regions: vec![MemoryEffectRegion::Aggregate],
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 160,
            },
        }
    }

    fn imported_variadic_summary(target: &str, fixed_arity: usize) -> CallSummary {
        CallSummary {
            target: CallTargetRef {
                address: Some(0x3000),
                symbol: target.to_string(),
                provenance: CallTargetProvenance::Import,
                edge_kind: CallEdgeKind::Import,
                confidence: 255,
            },
            prototype: PrototypeSummary {
                variadic_fixed_arity: None,
                min_arity: fixed_arity,
                max_arity: fixed_arity,
                locked_exact_arity: None,
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown; fixed_arity],
                param_surface_type_names: vec![None; fixed_arity],
                param_pointer_contracts: vec![false; fixed_arity],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: Some(true),
                writes_memory: None,
                escapes_args: None,
                regions: vec![],
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 224,
            },
        }
    }

    fn translated_error_fixture() -> PreHirFunction {
        PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![
                unsigned_binding("param_name", 64, Some(NirBindingOrigin::ParamIndex(0))),
                unsigned_binding("param_count", 64, Some(NirBindingOrigin::ParamIndex(1))),
            ],
            locals: vec![
                unsigned_binding("format_result", 64, Some(NirBindingOrigin::Temp)),
                unsigned_binding("name_alias", 64, Some(NirBindingOrigin::Temp)),
            ],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("format_result".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "gettext".to_string(),
                        args: vec![PreHirExpr::AddressOfGlobal(
                            "\"name=%s count=%u\"".to_string(),
                        )],
                        ty: NirType::Unknown,
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("name_alias".to_string()),
                    rhs: PreHirExpr::Var("param_name".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "error".to_string(),
                    args: vec![
                        PreHirExpr::Const(0, NirType::Unknown),
                        PreHirExpr::Const(0, NirType::Unknown),
                        PreHirExpr::Var("format_result".to_string()),
                        PreHirExpr::Var("name_alias".to_string()),
                        PreHirExpr::Var("param_count".to_string()),
                    ],
                    ty: NirType::Unknown,
                }),
            ],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([
                (
                    "gettext".to_string(),
                    imported_variadic_summary("gettext", 1),
                ),
                ("error".to_string(), imported_variadic_summary("error", 3)),
            ]),
        }
    }

    fn caller_with_direct_pointer_summary(
        caller_bits: u32,
        param_surface: Option<&str>,
        summary_ty: NirType,
        summary_surface: Option<&str>,
    ) -> PreHirFunction {
        let mut param = unsigned_binding(
            "param_1",
            caller_bits,
            Some(NirBindingOrigin::ParamIndex(0)),
        );
        param.surface_type_name = param_surface.map(str::to_string);
        PreHirFunction {
            name: "caller".to_string(),
            params: vec![param],
            locals: vec![unsigned_binding(
                "alias",
                caller_bits,
                Some(NirBindingOrigin::Temp),
            )],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("alias".to_string()),
                    rhs: PreHirExpr::Var("param_1".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "sub_2000".to_string(),
                    args: vec![PreHirExpr::Var("alias".to_string())],
                    ty: NirType::Unknown,
                }),
            ],
            is_64bit: caller_bits == 64,
            callee_summaries: indexmap::IndexMap::from([(
                "sub_2000".to_string(),
                direct_pointer_summary("sub_2000", summary_ty, summary_surface),
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn direct_callee_concrete_pointer_reaches_stable_caller_source() {
        let char_ptr = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let mut func = caller_with_direct_pointer_summary(64, None, char_ptr.clone(), None);

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(func.locals[0].ty, char_ptr);
        assert_eq!(func.params[0].ty, func.locals[0].ty);
    }

    #[test]
    fn typed_object_pointer_call_contract_casts_scalar_actual_at_the_call() {
        let mut scalar = unsigned_binding("scalar", 64, Some(NirBindingOrigin::Temp));
        scalar.ty = NirType::Int {
            bits: 64,
            signed: true,
        };
        scalar.surface_type_name = Some("long long".to_string());
        let mut typed_target_summary = direct_pointer_summary(
            "typed_target",
            NirType::Ptr(Box::new(NirType::Unknown)),
            Some("char **"),
        );
        typed_target_summary.prototype.param_pointer_contracts[0] = true;
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            locals: vec![scalar],
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "typed_target".to_string(),
                    args: vec![PreHirExpr::Var("scalar".to_string())],
                    ty: NirType::Unknown,
                }),
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "typed_target".to_string(),
                    args: vec![PreHirExpr::AddressOfGlobal("buffer".to_string())],
                    ty: NirType::Unknown,
                }),
            ],
            callee_summaries: indexmap::IndexMap::from([(
                "typed_target".to_string(),
                typed_target_summary,
            )]),
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let PreHirStmt::Expr(PreHirExpr::Call { args, .. }) = &func.body[0] else {
            panic!("first statement remains the typed call");
        };
        assert_eq!(
            args[0],
            PreHirExpr::Cast {
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                expr: Box::new(PreHirExpr::Var("scalar".to_string())),
            }
        );
        let PreHirStmt::Expr(PreHirExpr::Call { args, .. }) = &func.body[1] else {
            panic!("second statement remains the typed call");
        };
        assert_eq!(args[0], PreHirExpr::AddressOfGlobal("buffer".to_string()));
        assert!(
            !apply_callsite_type_prop_pass(&mut func),
            "call-boundary casts are idempotent"
        );
    }

    #[test]
    fn direct_callee_surface_pointer_preserves_file_declaration() {
        let mut func = caller_with_direct_pointer_summary(
            64,
            None,
            NirType::Ptr(Box::new(NirType::Unknown)),
            Some("FILE*"),
        );

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("FILE*"));
        assert_eq!(func.params[0].surface_type_name.as_deref(), Some("FILE*"));
    }

    #[test]
    fn direct_callee_generic_void_pointer_is_not_a_source_declaration() {
        let mut func = caller_with_direct_pointer_summary(
            64,
            None,
            NirType::Ptr(Box::new(NirType::Unknown)),
            None,
        );

        assert!(!apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(func.params[0].ty, NirType::Int { bits: 64, .. }));
    }

    #[test]
    fn direct_callee_width_only_pointer_surface_is_not_a_source_declaration() {
        let mut func = caller_with_direct_pointer_summary(
            64,
            None,
            NirType::Ptr(Box::new(NirType::Unknown)),
            Some("longlong **"),
        );

        assert!(!apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(func.params[0].ty, NirType::Int { bits: 64, .. }));
        assert!(func.params[0].surface_type_name.is_none());
    }

    #[test]
    fn direct_callee_concrete_pointer_does_not_add_to_caller_pointer_depth() {
        let char_ptr = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let mut func = caller_with_direct_pointer_summary(64, None, char_ptr, None);
        func.body.push(PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Var("param_1".to_string())),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }));

        assert!(!apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(func.params[0].ty, NirType::Int { bits: 64, .. }));
    }

    #[test]
    fn direct_callee_pointer_does_not_retype_temporary_only_chains() {
        let char_ptr = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let mut func = caller_with_direct_pointer_summary(64, None, char_ptr, None);
        func.params.clear();
        func.body.remove(0);

        assert!(!apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(func.locals[0].ty, NirType::Int { bits: 64, .. }));
    }

    #[test]
    fn direct_callee_pointer_does_not_override_surface_or_wrong_width() {
        let char_ptr = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let mut surfaced =
            caller_with_direct_pointer_summary(64, Some("size_t"), char_ptr.clone(), Some("char*"));
        assert!(!apply_callsite_type_prop_pass(&mut surfaced));
        assert_eq!(
            surfaced.params[0].surface_type_name.as_deref(),
            Some("size_t")
        );

        let mut wrong_width = caller_with_direct_pointer_summary(64, None, char_ptr, Some("char*"));
        wrong_width.is_64bit = false;
        assert!(!apply_callsite_type_prop_pass(&mut wrong_width));
        assert!(matches!(
            wrong_width.params[0].ty,
            NirType::Int { bits: 64, .. }
        ));
    }

    #[test]
    fn api_pointer_return_types_stable_call_and_copy_carriers() {
        let uint64 = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "reader_wrapper".to_string(),
            params: vec![unknown_binding(
                "path",
                Some(NirBindingOrigin::ParamIndex(0)),
            )],
            locals: vec![
                unsigned_binding("rax", 64, Some(NirBindingOrigin::Temp)),
                unsigned_binding("saved", 64, Some(NirBindingOrigin::Temp)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("rax".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "fopen".to_string(),
                        args: vec![
                            PreHirExpr::Var("path".to_string()),
                            PreHirExpr::Const(
                                0,
                                NirType::Ptr(Box::new(NirType::Int {
                                    bits: 8,
                                    signed: true,
                                })),
                            ),
                        ],
                        ty: uint64.clone(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("saved".to_string()),
                    rhs: PreHirExpr::Var("rax".to_string()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("saved".to_string()))),
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let file_ptr = NirType::Ptr(Box::new(NirType::Unknown));
        assert_eq!(func.locals[0].ty, file_ptr);
        assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("FILE*"));
        let PreHirStmt::Assign {
            rhs: PreHirExpr::Call { ty, .. },
            ..
        } = &func.body[0]
        else {
            panic!("expected the fopen call assignment");
        };
        assert_eq!(ty, &func.locals[0].ty);

        assert!(super::super::type_flow::apply_type_flow_pass(&mut func));
        assert_eq!(func.locals[1].ty, func.locals[0].ty);
    }

    #[test]
    fn api_pointer_return_survives_a_dead_reused_register_result() {
        let uint32 = NirType::Int {
            bits: 32,
            signed: false,
        };
        let char_ptr = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: true,
        }));
        let mut func = PreHirFunction {
            name: "reader_wrapper".to_string(),
            params: vec![PreHirBinding {
                name: "path".to_string(),
                ty: char_ptr.clone(),
                surface_type_name: Some("const char*".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![
                unsigned_binding("eax", 32, Some(NirBindingOrigin::Temp)),
                unsigned_binding("saved", 32, Some(NirBindingOrigin::Temp)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("eax".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "fopen".to_string(),
                        args: vec![
                            PreHirExpr::Var("path".to_string()),
                            PreHirExpr::Const(0, char_ptr),
                        ],
                        ty: uint32.clone(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("saved".to_string()),
                    rhs: PreHirExpr::Var("eax".to_string()),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("eax".to_string()),
                    then_body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("eax".to_string()),
                        rhs: PreHirExpr::Call {
                            target: "setvbuf".to_string(),
                            args: Vec::new(),
                            ty: NirType::Int {
                                bits: 32,
                                signed: true,
                            },
                        },
                    }]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("saved".to_string()))),
            ],
            is_64bit: false,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let PreHirStmt::If { then_body, .. } = &func.body[2] else {
            panic!("expected conditional setvbuf call");
        };
        assert!(
            matches!(then_body.as_slice(), [PreHirStmt::Expr(PreHirExpr::Call { target, .. })] if target == "setvbuf")
        );

        let file_ptr = NirType::Ptr(Box::new(NirType::Unknown));
        assert_eq!(func.locals[0].ty, file_ptr);
        assert!(super::super::type_flow::apply_type_flow_pass(&mut func));
        assert_eq!(func.locals[1].ty, func.locals[0].ty);
    }

    #[test]
    fn pointer_width_round_trip_casts_are_removed_only_for_pointer_copies() {
        let file_ptr = NirType::Ptr(Box::new(NirType::Unknown));
        let mut func = PreHirFunction {
            locals: vec![
                PreHirBinding {
                    name: "source".to_string(),
                    ty: file_ptr.clone(),
                    surface_type_name: Some("FILE*".to_string()),
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                PreHirBinding {
                    name: "round_trip".to_string(),
                    ty: file_ptr.clone(),
                    surface_type_name: Some("FILE*".to_string()),
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                PreHirBinding {
                    name: "narrowed".to_string(),
                    ty: file_ptr,
                    surface_type_name: Some("FILE*".to_string()),
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("round_trip".to_string()),
                    rhs: PreHirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                        expr: Box::new(PreHirExpr::Cast {
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                            expr: Box::new(PreHirExpr::Var("source".to_string())),
                        }),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("narrowed".to_string()),
                    rhs: PreHirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        expr: Box::new(PreHirExpr::Cast {
                            ty: NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                            expr: Box::new(PreHirExpr::Var("source".to_string())),
                        }),
                    },
                },
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(
            func.body[0],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Var(ref source),
                ..
            } if source == "source"
        ));
        assert!(matches!(
            func.body[1],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Cast { .. },
                ..
            }
        ));
    }

    #[test]
    fn api_pointer_parameter_promotes_carrier_and_elides_pointer_round_trip() {
        let file_ptr = NirType::Ptr(Box::new(NirType::Unknown));
        let mut source = unknown_binding("source", Some(NirBindingOrigin::Temp));
        source.ty = file_ptr.clone();
        let mut destination =
            unsigned_binding("file_slot", 64, Some(NirBindingOrigin::StackOffset(-8)));
        destination.surface_type_name = None;
        let mut func = PreHirFunction {
            locals: vec![source, destination],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("file_slot".to_string()),
                    rhs: PreHirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                        expr: Box::new(PreHirExpr::Cast {
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                            expr: Box::new(PreHirExpr::Var("source".to_string())),
                        }),
                    },
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "setvbuf".to_string(),
                    args: vec![
                        PreHirExpr::Var("file_slot".to_string()),
                        PreHirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        ),
                        PreHirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 32,
                                signed: true,
                            },
                        ),
                        PreHirExpr::Const(
                            4096,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        ),
                    ],
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                }),
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert!(matches!(func.locals[1].ty, NirType::Ptr(_)));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Var(source),
                ..
            } if source == "source"
        ));
    }

    #[test]
    fn callsite_type_prop_promotes_import_param_name_and_surface_type() {
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![unknown_binding(
                "param_1",
                Some(NirBindingOrigin::ParamIndex(0)),
            )],
            locals: vec![unsigned_binding(
                "local_2",
                64,
                Some(NirBindingOrigin::DerivedFromStackOffset(-0x20)),
            )],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "GetWindowRect".to_string(),
                args: vec![
                    PreHirExpr::Var("param_1".to_string()),
                    PreHirExpr::Var("local_2".to_string()),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(func.locals[0].name, "lpRect");
        assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("RECT*"));
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 64,
                signed: false,
            }
        );
    }

    #[test]
    fn callsite_type_prop_keeps_existing_surface_type_locked() {
        let mut locked = unsigned_binding(
            "local_2",
            32,
            Some(NirBindingOrigin::DerivedFromStackOffset(-0x20)),
        );
        locked.surface_type_name = Some("uintptr_t".to_string());
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![unknown_binding(
                "param_1",
                Some(NirBindingOrigin::ParamIndex(0)),
            )],
            locals: vec![locked],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "GetWindowRect".to_string(),
                args: vec![
                    PreHirExpr::Var("param_1".to_string()),
                    PreHirExpr::Var("local_2".to_string()),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 32,
                signed: false,
            }
        );
        assert_eq!(
            func.locals[0].surface_type_name.as_deref(),
            Some("uintptr_t")
        );
    }

    #[test]
    fn callsite_type_prop_carries_surface_type_to_stable_copy_source() {
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            params: vec![unsigned_binding(
                "source",
                64,
                Some(NirBindingOrigin::ParamIndex(1)),
            )],
            locals: vec![
                unknown_binding("window", Some(NirBindingOrigin::ParamIndex(0))),
                unsigned_binding("alias", 64, Some(NirBindingOrigin::Temp)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("alias".to_string()),
                    rhs: PreHirExpr::Var("source".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "GetWindowRect".to_string(),
                    args: vec![
                        PreHirExpr::Var("window".to_string()),
                        PreHirExpr::Var("alias".to_string()),
                    ],
                    ty: NirType::Unknown,
                }),
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(func.params[0].surface_type_name.as_deref(), Some("RECT*"));
        assert_eq!(func.locals[1].surface_type_name.as_deref(), Some("RECT*"));
        assert!(matches!(func.params[0].ty, NirType::Int { bits: 64, .. }));
    }

    #[test]
    fn callsite_type_prop_keeps_generic_void_pointer_at_immediate_argument() {
        let specific_pointer = NirType::Ptr(Box::new(NirType::Int {
            bits: 64,
            signed: false,
        }));
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            params: vec![PreHirBinding {
                name: "source".to_string(),
                ty: specific_pointer.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![PreHirBinding {
                name: "alias".to_string(),
                ty: specific_pointer,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("alias".to_string()),
                    rhs: PreHirExpr::Var("source".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "free".to_string(),
                    args: vec![PreHirExpr::Var("alias".to_string())],
                    ty: NirType::Unknown,
                }),
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert!(func.locals[0].surface_type_name.is_some());
        assert!(func.params[0].surface_type_name.is_none());
    }

    #[test]
    fn callsite_type_prop_does_not_reach_multi_definition_copy_source() {
        let uint64 = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            locals: vec![
                unknown_binding("window", Some(NirBindingOrigin::ParamIndex(0))),
                unsigned_binding("source", 64, Some(NirBindingOrigin::Temp)),
                unsigned_binding("alias", 64, Some(NirBindingOrigin::Temp)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("source".to_string()),
                    rhs: PreHirExpr::Const(1, uint64.clone()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("source".to_string()),
                    rhs: PreHirExpr::Const(2, uint64),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("alias".to_string()),
                    rhs: PreHirExpr::Var("source".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "GetWindowRect".to_string(),
                    args: vec![
                        PreHirExpr::Var("window".to_string()),
                        PreHirExpr::Var("alias".to_string()),
                    ],
                    ty: NirType::Unknown,
                }),
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert!(func.locals[2].surface_type_name.is_some());
        assert!(func.locals[1].surface_type_name.is_none());
    }

    #[test]
    fn callsite_type_prop_does_not_retype_reused_call_receiver_as_pointer() {
        let uint64 = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            params: vec![unknown_binding(
                "text",
                Some(NirBindingOrigin::ParamIndex(0)),
            )],
            locals: vec![unsigned_binding("rax", 64, Some(NirBindingOrigin::Temp))],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("rax".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "unknown_scalar".to_string(),
                        args: vec![],
                        ty: uint64.clone(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("rax".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "strchr".to_string(),
                        args: vec![
                            PreHirExpr::Var("text".to_string()),
                            PreHirExpr::Const(
                                0,
                                NirType::Int {
                                    bits: 32,
                                    signed: true,
                                },
                            ),
                        ],
                        ty: NirType::Ptr(Box::new(NirType::Int {
                            bits: 8,
                            signed: true,
                        })),
                    },
                },
            ],
            is_64bit: true,
            ..Default::default()
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        assert_eq!(func.locals[0].ty, uint64);
        assert!(func.locals[0].surface_type_name.is_none());
    }

    #[test]
    fn callsite_type_prop_rewrites_target_through_wrapper_summary() {
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "wrapper_foo".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([(
                "wrapper_foo".to_string(),
                CallSummary {
                    target: CallTargetRef {
                        address: None,
                        symbol: "wrapper_foo".to_string(),
                        provenance: CallTargetProvenance::Reference,
                        edge_kind: CallEdgeKind::Reference,
                        confidence: 128,
                    },
                    prototype: PrototypeSummary {
                        variadic_fixed_arity: None,
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
                        param_pointer_contracts: vec![],
                        soundness: SummarySoundness::Optimistic,
                    },
                    effect_summary: CallEffectSummary {
                        reads_memory: None,
                        writes_memory: None,
                        escapes_args: Some(false),
                        regions: vec![],
                        wrapper_class: WrapperClass::TailForwarder,
                        wrapper_of: Some(CallTargetRef {
                            address: None,
                            symbol: "MessageBoxA".to_string(),
                            provenance: CallTargetProvenance::Import,
                            edge_kind: CallEdgeKind::Import,
                            confidence: 224,
                        }),
                        confidence: 160,
                    },
                },
            )]),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { target, .. }) => {
                assert_eq!(target, "MessageBoxA");
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_extra_args_only_for_exact_api_signature() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "MessageBoxA".to_string(),
                    args: vec![
                        PreHirExpr::Const(0, NirType::Unknown),
                        PreHirExpr::Const(1, NirType::Unknown),
                        PreHirExpr::Const(2, NirType::Unknown),
                        PreHirExpr::Const(3, NirType::Unknown),
                        PreHirExpr::Const(4, NirType::Unknown),
                        PreHirExpr::Const(5, NirType::Unknown),
                    ],
                    ty: NirType::Unknown,
                }),
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "unresolved_target".to_string(),
                    args: vec![
                        PreHirExpr::Const(0, NirType::Unknown),
                        PreHirExpr::Const(1, NirType::Unknown),
                        PreHirExpr::Const(2, NirType::Unknown),
                    ],
                    ty: NirType::Unknown,
                }),
            ],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 2);
        assert_eq!(stats.call_prototype_unknown_target_kept_count, 1);
        assert_eq!(stats.call_prototype_signature_missing_count, 0);
        assert_eq!(stats.call_prototype_wrapper_resolved_count, 0);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 4),
            other => panic!("unexpected first stmt: {other:?}"),
        }
        match &func.body[1] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 3),
            other => panic!("unexpected second stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_keeps_extra_args_for_known_variadic_runtime_symbol() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "printf".to_string(),
                args: vec![
                    PreHirExpr::Const(0, NirType::Unknown),
                    PreHirExpr::Const(1, NirType::Unknown),
                    PreHirExpr::Const(2, NirType::Unknown),
                    PreHirExpr::Const(3, NirType::Unknown),
                    PreHirExpr::Const(4, NirType::Unknown),
                    PreHirExpr::Const(5, NirType::Unknown),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([(
                "printf".to_string(),
                CallSummary {
                    target: CallTargetRef {
                        address: Some(0x140007000),
                        symbol: "printf".to_string(),
                        provenance: CallTargetProvenance::Direct,
                        edge_kind: CallEdgeKind::Direct,
                        confidence: 160,
                    },
                    prototype: PrototypeSummary {
                        variadic_fixed_arity: None,
                        min_arity: 4,
                        max_arity: 4,
                        locked_exact_arity: Some(4),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown; 4],
                        param_surface_type_names: vec![None; 4],
                        param_pointer_contracts: vec![false; 4],
                        soundness: SummarySoundness::Optimistic,
                    },
                    effect_summary: CallEffectSummary {
                        reads_memory: Some(true),
                        writes_memory: Some(true),
                        escapes_args: None,
                        regions: vec![],
                        wrapper_class: WrapperClass::None,
                        wrapper_of: None,
                        confidence: 160,
                    },
                },
            )]),
        };

        assert!(!apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 0);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 6),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    /// Ghidra `FormatStringAnalyzer` scorecard item. `arg_tmp`/`arg_tmp2`
    /// model the shape a real call-argument temp actually has by the time
    /// this pass runs on compiled code: a plain copy of the real
    /// parameter, already carrying the generic *unsigned*-int default
    /// `fission-pcode`'s HIR builder assigns purely from raw register
    /// width (confirmed via a real fixture, not `NirType::Unknown` the
    /// way `unknown_binding`-based tests elsewhere in this file assume --
    /// that idealized starting condition doesn't occur in practice for
    /// call-argument bindings, which is exactly why `tighten_binding_ty`
    /// alone wasn't enough and `apply_variadic_printf_arg_ty` exists).
    /// Checks both that the format-derived type reaches the immediate
    /// call-site temp *and* transitively reaches the real parameter it
    /// was copied from (`apply_variadic_printf_arg_ty_transitively`) --
    /// confirmed via the same real fixture that this transitive step is
    /// not optional: without it, the refinement computed correctly but
    /// never survived to the parameter shown in the final signature.
    #[test]
    fn callsite_type_prop_types_printf_variadic_args_from_format_specifiers() {
        fn generic_uint(bits: u32) -> NirType {
            NirType::Int {
                bits,
                signed: false,
            }
        }

        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![
                PreHirBinding {
                    name: "param_1".to_string(),
                    ty: generic_uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(0)),
                    initializer: None,
                },
                PreHirBinding {
                    name: "param_2".to_string(),
                    ty: generic_uint(64),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(1)),
                    initializer: None,
                },
            ],
            locals: vec![
                PreHirBinding {
                    name: "arg_tmp".to_string(),
                    ty: generic_uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                PreHirBinding {
                    name: "arg_tmp2".to_string(),
                    ty: generic_uint(64),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("arg_tmp".to_string()),
                    rhs: PreHirExpr::Var("param_1".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("arg_tmp2".to_string()),
                    rhs: PreHirExpr::Var("param_2".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "printf".to_string(),
                    args: vec![
                        PreHirExpr::AddressOfGlobal("\"value=%d name=%s\\n\"".to_string()),
                        PreHirExpr::Var("arg_tmp".to_string()),
                        PreHirExpr::Var("arg_tmp2".to_string()),
                    ],
                    ty: NirType::Unknown,
                }),
            ],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));

        let want_int = NirType::Int {
            bits: 32,
            signed: true,
        };
        let want_str = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        assert_eq!(
            func.params[0].ty, want_int,
            "param_1 should type as %d's int"
        );
        assert_eq!(
            func.params[1].ty, want_str,
            "param_2 should type as %s's char* (transitively, through the arg_tmp2 copy)"
        );
        assert_eq!(func.locals[0].ty, want_int);
        assert_eq!(func.locals[1].ty, want_str);
    }

    #[test]
    fn printf_format_parser_does_not_guess_abi_dependent_long_width() {
        let types = parse_printf_format_specifier_types("%u %lu %llu %s");
        assert_eq!(
            types,
            vec![
                Some(NirType::Int {
                    bits: 32,
                    signed: false,
                }),
                None,
                Some(NirType::Int {
                    bits: 64,
                    signed: false,
                }),
                Some(NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                }))),
            ]
        );
    }

    #[test]
    fn translated_format_literal_types_imported_error_arguments_site_sensitively() {
        let mut func = translated_error_fixture();

        assert!(apply_site_sensitive_translated_format_types(&mut func));
        let want_str = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let want_uint = NirType::Int {
            bits: 32,
            signed: false,
        };
        assert_eq!(func.params[0].ty, want_str);
        assert_eq!(func.locals[1].ty, want_str);
        assert_eq!(func.params[1].ty, want_uint);
    }

    #[test]
    fn translated_format_literal_does_not_cross_overwrite_or_internal_name() {
        let mut overwritten = translated_error_fixture();
        overwritten.body.insert(
            1,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("format_result".to_string()),
                rhs: PreHirExpr::Const(0, NirType::Unknown),
            },
        );
        assert!(!apply_site_sensitive_translated_format_types(
            &mut overwritten
        ));

        let mut internal = translated_error_fixture();
        let summary = internal
            .callee_summaries
            .get_mut("error")
            .expect("error summary");
        summary.target.provenance = CallTargetProvenance::Direct;
        summary.target.edge_kind = CallEdgeKind::Direct;
        assert!(!apply_site_sensitive_translated_format_types(&mut internal));
    }

    #[test]
    fn callsite_type_prop_prunes_self_recursive_args_to_function_arity() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "fib".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "fib".to_string(),
                args: vec![
                    PreHirExpr::Const(1, NirType::Unknown),
                    PreHirExpr::Const(2, NirType::Unknown),
                    PreHirExpr::Const(3, NirType::Unknown),
                    PreHirExpr::Const(4, NirType::Unknown),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 0);
        assert_eq!(stats.call_signature_refined_count, 3);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 1),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_wrapper_args_after_resolving_import_summary() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "wrapper_message_box".to_string(),
                args: vec![
                    PreHirExpr::Const(0, NirType::Unknown),
                    PreHirExpr::Const(1, NirType::Unknown),
                    PreHirExpr::Const(2, NirType::Unknown),
                    PreHirExpr::Const(3, NirType::Unknown),
                    PreHirExpr::Const(4, NirType::Unknown),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([(
                "wrapper_message_box".to_string(),
                CallSummary {
                    target: CallTargetRef {
                        address: None,
                        symbol: "wrapper_message_box".to_string(),
                        provenance: CallTargetProvenance::Reference,
                        edge_kind: CallEdgeKind::Reference,
                        confidence: 128,
                    },
                    prototype: PrototypeSummary {
                        variadic_fixed_arity: None,
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
                        param_pointer_contracts: vec![],
                        soundness: SummarySoundness::Optimistic,
                    },
                    effect_summary: CallEffectSummary {
                        reads_memory: None,
                        writes_memory: None,
                        escapes_args: Some(false),
                        regions: vec![],
                        wrapper_class: WrapperClass::TailForwarder,
                        wrapper_of: Some(CallTargetRef {
                            address: None,
                            symbol: "MessageBoxA".to_string(),
                            provenance: CallTargetProvenance::Import,
                            edge_kind: CallEdgeKind::Import,
                            confidence: 224,
                        }),
                        confidence: 160,
                    },
                },
            )]),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 1);
        assert_eq!(stats.call_prototype_wrapper_resolved_count, 1);
        assert_eq!(stats.call_prototype_signature_missing_count, 0);
        assert_eq!(stats.call_prototype_unknown_target_kept_count, 0);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { target, args, .. }) => {
                assert_eq!(target, "MessageBoxA");
                assert_eq!(args.len(), 4);
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_locked_internal_callee_arity() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "recursive_fib".to_string(),
                args: vec![
                    PreHirExpr::Const(0, NirType::Unknown),
                    PreHirExpr::Const(1, NirType::Unknown),
                    PreHirExpr::Const(2, NirType::Unknown),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([(
                "recursive_fib".to_string(),
                CallSummary {
                    target: CallTargetRef {
                        address: Some(0x100000),
                        symbol: "recursive_fib".to_string(),
                        provenance: CallTargetProvenance::Direct,
                        edge_kind: CallEdgeKind::Direct,
                        confidence: 224,
                    },
                    prototype: PrototypeSummary {
                        variadic_fixed_arity: None,
                        min_arity: 1,
                        max_arity: 1,
                        locked_exact_arity: Some(1),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown],
                        param_surface_type_names: vec![None],
                        param_pointer_contracts: vec![false],
                        soundness: SummarySoundness::Optimistic,
                    },
                    effect_summary: CallEffectSummary {
                        reads_memory: None,
                        writes_memory: None,
                        escapes_args: None,
                        regions: vec![],
                        wrapper_class: WrapperClass::None,
                        wrapper_of: None,
                        confidence: 160,
                    },
                },
            )]),
        };

        assert!(apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 2);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 1),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_keeps_args_when_summary_signature_missing() {
        reset_normalize_wave_stats();
        let mut func = PreHirFunction {
            variadic_fixed_arity: None,
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                target: "known_without_signature".to_string(),
                args: vec![
                    PreHirExpr::Const(0, NirType::Unknown),
                    PreHirExpr::Const(1, NirType::Unknown),
                ],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: indexmap::IndexMap::from([(
                "known_without_signature".to_string(),
                CallSummary {
                    target: CallTargetRef {
                        address: None,
                        symbol: "known_without_signature".to_string(),
                        provenance: CallTargetProvenance::Reference,
                        edge_kind: CallEdgeKind::Reference,
                        confidence: 128,
                    },
                    prototype: PrototypeSummary {
                        variadic_fixed_arity: None,
                        min_arity: 0,
                        max_arity: 2,
                        locked_exact_arity: None,
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
                        param_pointer_contracts: vec![],
                        soundness: SummarySoundness::Optimistic,
                    },
                    effect_summary: CallEffectSummary {
                        reads_memory: None,
                        writes_memory: None,
                        escapes_args: None,
                        regions: vec![],
                        wrapper_class: WrapperClass::None,
                        wrapper_of: None,
                        confidence: 0,
                    },
                },
            )]),
        };

        assert!(!apply_callsite_type_prop_pass(&mut func));
        let stats = take_normalize_wave_stats();
        assert_eq!(stats.call_prototype_exact_api_arity_pruned_count, 0);
        assert_eq!(stats.call_prototype_wrapper_resolved_count, 0);
        assert_eq!(stats.call_prototype_signature_missing_count, 1);
        assert_eq!(stats.call_prototype_unknown_target_kept_count, 0);
        match &func.body[0] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => assert_eq!(args.len(), 2),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }
}
