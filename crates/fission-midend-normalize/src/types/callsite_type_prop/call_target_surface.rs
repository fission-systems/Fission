//! Callee identity and API surface normalization.
//!
//! This module resolves wrapper/canonical call targets, applies stable API
//! parameter names, rewrites call expressions, and carries informative API
//! surface declarations through safe copy chains.

use super::*;

pub(super) fn resolve_call_target_symbol<'a>(
    target: &'a str,
    summaries: &'a indexmap::IndexMap<String, CallSummary>,
) -> &'a str {
    resolve_call_target_symbol_with_wrapper(target, summaries).0
}

pub(super) fn resolve_call_target_symbol_with_wrapper<'a>(
    target: &'a str,
    summaries: &'a indexmap::IndexMap<String, CallSummary>,
) -> (&'a str, bool) {
    summaries
        .get(target)
        .map(|summary| {
            if let Some(wrapped) = summary.effect_summary.wrapper_of.as_ref() {
                let symbol = wrapped.symbol.as_str();
                (symbol, symbol != target)
            } else {
                (summary.target.symbol.as_str(), false)
            }
        })
        .unwrap_or((target, false))
}

pub(super) fn build_call_target_rewrites(
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> HashMap<String, String> {
    summaries
        .iter()
        .filter_map(|(target, summary)| {
            let canonical = summary
                .effect_summary
                .wrapper_of
                .as_ref()
                .map(|wrapped| wrapped.symbol.as_str())
                .unwrap_or_else(|| summary.target.symbol.as_str());
            (canonical != target).then(|| (target.clone(), canonical.to_string()))
        })
        .collect()
}

pub(super) fn is_generic_binding_name(name: &str) -> bool {
    matches!(
        name,
        _
            if name.starts_with("param_")
                || name.starts_with("local_")
                || name.starts_with("home_")
                || name.starts_with("arg_out_")
                || name.starts_with("ret_scaffold_")
                || name.starts_with("xVar")
    )
}

fn is_renameable_generic_binding(binding: &PreHirBinding) -> bool {
    is_generic_binding_name(&binding.name)
        && !matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_)))
}

fn sanitize_binding_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let lowered = out.to_ascii_lowercase();
    if lowered.starts_with("arg") && lowered[3..].chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(out)
}

pub(super) fn register_name_candidate(
    candidates: &mut HashMap<String, String>,
    conflicts: &mut HashSet<String>,
    binding_name: &str,
    candidate_name: &str,
) {
    let Some(candidate_name) = sanitize_binding_name(candidate_name) else {
        return;
    };
    if let Some(existing) = candidates.get(binding_name) {
        if existing != &candidate_name {
            conflicts.insert(binding_name.to_string());
        }
        return;
    }
    candidates.insert(binding_name.to_string(), candidate_name);
}

pub(super) fn apply_binding_surface_renames(
    func: &mut PreHirFunction,
    rename_candidates: HashMap<String, String>,
    conflicts: &HashSet<String>,
) -> usize {
    if rename_candidates.is_empty() {
        return 0;
    }

    let mut reserved_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();
    let mut renames = Vec::new();

    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if !is_renameable_generic_binding(binding) || conflicts.contains(&binding.name) {
            continue;
        }
        let Some(candidate_name) = rename_candidates.get(&binding.name) else {
            continue;
        };
        if candidate_name == &binding.name {
            continue;
        }
        if reserved_names.contains(candidate_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(candidate_name.clone());
        renames.push((binding.name.clone(), candidate_name.clone()));
        binding.name = candidate_name.clone();
    }

    if renames.is_empty() {
        return 0;
    }
    rename_vars_in_stmts(&mut func.body, &renames);
    renames.len()
}

pub(super) fn rewrite_call_targets_stmts(
    stmts: &mut [PreHirStmt],
    rewrites: &HashMap<String, String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                changed |= rewrite_call_targets_expr(rhs, rewrites);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_call_targets_expr(va_list, rewrites)
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= rewrite_call_targets_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    rewrites,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_call_targets_expr(expr, rewrites);
                for case in cases {
                    changed |= rewrite_call_targets_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        rewrites,
                    );
                }
                changed |= rewrite_call_targets_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    rewrites,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_call_targets_expr(cond, rewrites);
                changed |= rewrite_call_targets_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    rewrites,
                );
                changed |= rewrite_call_targets_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    rewrites,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    changed
}

#[cfg(test)]
mod pointer_contract_tests {
    use super::*;

    #[test]
    fn generic_pointer_parameter_does_not_lock_a_specific_copy_or_source() {
        for surface in [
            "void *",
            "const void*",
            "LPVOID",
            "PVOID",
            "LPCVOID",
            "PCVOID",
        ] {
            let pointer = NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: true,
            }));
            let mut func = PreHirFunction {
                locals: ["source", "saved"]
                    .into_iter()
                    .map(|name| PreHirBinding {
                        name: name.into(),
                        ty: pointer.clone(),
                        surface_type_name: None,
                        origin: Some(NirBindingOrigin::Temp),
                        initializer: None,
                    })
                    .collect(),
                ..Default::default()
            };
            let copies = HashMap::from_iter([("saved".into(), "source".into())]);
            let definitions = HashMap::from_iter([("source".into(), 1), ("saved".into(), 1)]);
            assert!(!apply_api_surface_type_transitively(
                &mut func,
                &copies,
                &definitions,
                &HashSet::default(),
                "saved",
                surface,
            ));
            assert!(
                func.locals.iter().all(|binding| {
                    binding.ty == pointer && binding.surface_type_name.is_none()
                }),
                "{surface}"
            );
            assert!(apply_api_surface_type_transitively(
                &mut func,
                &copies,
                &definitions,
                &HashSet::default(),
                "saved",
                "char*",
            ));
            assert!(
                func.locals
                    .iter()
                    .all(|binding| { binding.surface_type_name.as_deref() == Some("char*") })
            );
        }
    }
}

fn rewrite_call_targets_expr(expr: &mut PreHirExpr, rewrites: &HashMap<String, String>) -> bool {
    let mut changed = false;
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            if let Some(replacement) = rewrites.get(target) {
                *target = replacement.clone();
                changed = true;
            }
            for arg in args {
                changed |= rewrite_call_targets_expr(arg, rewrites);
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_call_targets_expr(lhs, rewrites);
            changed |= rewrite_call_targets_expr(rhs, rewrites);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            changed |= rewrite_call_targets_expr(expr, rewrites);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= rewrite_call_targets_expr(base, rewrites);
            changed |= rewrite_call_targets_expr(index, rewrites);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_call_targets_expr(cond, rewrites);
            changed |= rewrite_call_targets_expr(then_expr, rewrites);
            changed |= rewrite_call_targets_expr(else_expr, rewrites);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    changed
}

/// Carry an exact API parameter declaration back through stable plain-copy
/// aliases. Generic object-pointer parameters accept existing pointer types
/// without declaring those variables as void pointers. Propagation of specific
/// declarations requires the same single-definition and non-self-referential
/// proof as operation-edge type flow.
pub(super) fn apply_api_surface_type_transitively(
    func: &mut PreHirFunction,
    copy_sources: &HashMap<String, String>,
    definition_counts: &HashMap<String, usize>,
    self_referential: &HashSet<String>,
    arg_var: &str,
    surface_type_name: &str,
) -> bool {
    let mut changed = false;
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    let compact_surface = surface_type_name
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect::<String>();
    let generic_void_pointer = matches!(
        compact_surface.as_str(),
        "VOID*" | "CONSTVOID*" | "LPVOID" | "PVOID" | "LPCVOID" | "PCVOID"
    );
    if generic_void_pointer {
        return false;
    }
    while visited.insert(current.clone()) {
        if let Some(binding) = binding_by_name_mut(&mut func.locals, &current)
            .or_else(|| binding_by_name_mut(&mut func.params, &current))
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name.to_string());
            changed = true;
        }
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
            None => break,
            Some(_) => break,
        }
    }
    changed
}
