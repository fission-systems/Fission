/// Call-site inter-procedural type propagation pass.
///
/// All type inference so far has been intra-procedural: it only sees the types
/// of expressions *within* the current function.  `call malloc(size)` still
/// returns `Ptr(Unknown)`, `memcpy(dst, src, n)` arguments stay `Unknown`.
///
/// This pass connects the existing `fission-signatures` Windows API database
/// to the Fission type inference pipeline:
///
/// 1. Walk every `DirStmt::Assign { rhs: Call { target, args } }` and
///    `DirStmt::Expr(Call { target, args })`.
/// 2. Look up `target` in the signatures API type provider.
/// 3. For the return value: if there is a receiver binding (the lhs `Var` of
///    the Assign), update `DirBinding.ty` to the resolved return type.
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
use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::util_dir::rename_vars_in_stmts;
use fission_midend_core::wave_stats::{
    add_call_prototype_exact_api_arity_pruned, add_call_prototype_signature_missing,
    add_call_prototype_unknown_target_kept, add_call_prototype_wrapper_resolved,
    add_call_signature_refinements, add_surface_fact_promotions, add_typed_fact_conflicts,
};
use fission_signatures::{ApiSignature, SIGNATURE_RESOURCES, symbol_for_win_api_database_lookup};

/// Convert a Windows API type name string to a `NirType`, or `None` for
/// unconstrained types (void, variadic, …).
pub fn win_type_name_to_nir(name: &str) -> Option<NirType> {
    // Strip leading/trailing whitespace and trailing `*` for pointer types.
    let name = name.trim();

    // Pointer types first.
    if name.ends_with('*') {
        let inner_name = name.trim_end_matches('*').trim();
        let inner = match inner_name {
            "VOID" | "void" | "" => NirType::Unknown,
            "CHAR" | "char" => NirType::Int {
                bits: 8,
                signed: true,
            },
            "WCHAR" | "wchar_t" | "TCHAR" => NirType::Int {
                bits: 16,
                signed: false,
            },
            "BYTE" | "UCHAR" => NirType::Int {
                bits: 8,
                signed: false,
            },
            _ => NirType::Unknown,
        };
        return Some(NirType::Ptr(Box::new(inner)));
    }

    let nir = match name {
        // Void — no constraint.
        "void" | "VOID" => return None,
        // 32-bit unsigned integers.
        "DWORD" | "UINT" | "ULONG" | "UINT32" | "ULONG32" | "DWORD32" => NirType::Int {
            bits: 32,
            signed: false,
        },
        // 32-bit signed integers.
        "INT" | "LONG" | "INT32" | "LONG32" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // BOOL is signed int32 in Windows ABI.
        "BOOL" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // 16-bit.
        "WORD" | "USHORT" | "UINT16" => NirType::Int {
            bits: 16,
            signed: false,
        },
        "SHORT" | "INT16" => NirType::Int {
            bits: 16,
            signed: true,
        },
        // 8-bit.
        "BYTE" | "UCHAR" | "UINT8" | "BOOLEAN" => NirType::Int {
            bits: 8,
            signed: false,
        },
        "CHAR" | "INT8" => NirType::Int {
            bits: 8,
            signed: true,
        },
        // 64-bit unsigned.
        "QWORD" | "UINT64" | "ULONG64" | "DWORD64" | "ULONGLONG" | "ULONG_PTR" | "SIZE_T"
        | "UINT_PTR" => NirType::Int {
            bits: 64,
            signed: false,
        },
        // 64-bit signed.
        "LONGLONG" | "INT64" | "LONG64" | "LONG_PTR" | "SSIZE_T" | "INT_PTR" => NirType::Int {
            bits: 64,
            signed: true,
        },
        // Generic pointer to void.
        "LPVOID" | "PVOID" | "HANDLE" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Typed string pointers.
        "LPSTR" | "LPCSTR" | "PSTR" | "PCSTR" => NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        })),
        "LPWSTR" | "LPCWSTR" | "PWSTR" | "PCWSTR" => NirType::Ptr(Box::new(NirType::Int {
            bits: 16,
            signed: false,
        })),
        // Opaque Windows handle types — typed as Ptr to empty Aggregate.
        "HWND"
        | "HMODULE"
        | "HINSTANCE"
        | "HKEY"
        | "HFILE"
        | "HBITMAP"
        | "HBRUSH"
        | "HFONT"
        | "HPEN"
        | "HICON"
        | "HCURSOR"
        | "HMENU"
        | "HRGN"
        | "HDC"
        | "HGLOBAL"
        | "HLOCAL"
        | "HRSRC"
        | "HWINSTA"
        | "HDESK"
        | "HPALETTE"
        | "HENHMETAFILE"
        | "HMETAFILE"
        | "HCOLORSPACE"
        | "HCONV"
        | "HCONVLIST"
        | "HDDEDATA"
        | "HDDERESERVATION"
        | "HSZ"
        | "HHOOK"
        | "HMONITOR"
        | "HWINEVENTHOOK"
        | "HPOWERNOTIFY"
        | "SC_HANDLE"
        | "SERVICE_STATUS_HANDLE" => NirType::Ptr(Box::new(NirType::Aggregate {
            size: 0,
            fields: vec![],
        })),
        // NTSTATUS / HRESULT: signed 32-bit.
        "NTSTATUS" | "HRESULT" => NirType::Int {
            bits: 32,
            signed: true,
        },
        // MSVC va_list (opaque; model as generic pointer).
        "va_list" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Unknown / not yet mapped → no constraint.
        _ => return None,
    };
    Some(nir)
}

pub fn is_known_api_signature(name: &str) -> bool {
    api_signature_via_import_aliases(name).is_some()
}

pub fn api_signature(name: &str) -> Option<&'static ApiSignature> {
    SIGNATURE_RESOURCES.api_signature(name)
}

#[inline]
fn api_signature_via_import_aliases(name: &str) -> Option<&'static ApiSignature> {
    api_signature(name)
        .or_else(|| symbol_for_win_api_database_lookup(name).and_then(|flat| api_signature(flat)))
}

/// Return the NirType implied by the API signature's return type string.
/// Returns `None` when the return type is void or not mappable.
fn resolve_return_ty(ret_type_str: &str) -> Option<NirType> {
    win_type_name_to_nir(ret_type_str)
}

/// Attempt to tighten a binding's type using a new candidate.
/// Follows the same monotone strengthening logic as `use_type_infer`:
/// Unknown can be replaced by anything; a concrete type is only replaced if the
/// candidate is strictly more informative (pointer vs. integer, or known vs. unknown).
fn tighten_binding_ty(binding: &mut DirBinding, candidate: &NirType) -> bool {
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

fn resolve_call_target_symbol<'a>(
    target: &'a str,
    summaries: &'a indexmap::IndexMap<String, CallSummary>,
) -> &'a str {
    resolve_call_target_symbol_with_wrapper(target, summaries).0
}

fn resolve_call_target_symbol_with_wrapper<'a>(
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

fn build_call_target_rewrites(
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

fn is_generic_binding_name(name: &str) -> bool {
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

fn is_renameable_generic_binding(binding: &DirBinding) -> bool {
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

fn register_name_candidate(
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

fn apply_binding_surface_renames(
    func: &mut DirFunction,
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

fn rewrite_call_targets_stmts(stmts: &mut [DirStmt], rewrites: &HashMap<String, String>) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                changed |= rewrite_call_targets_expr(rhs, rewrites);
            }
            DirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_call_targets_expr(va_list, rewrites)
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= rewrite_call_targets_stmts(body, rewrites);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_call_targets_expr(expr, rewrites);
                for case in cases {
                    changed |= rewrite_call_targets_stmts(&mut case.body, rewrites);
                }
                changed |= rewrite_call_targets_stmts(default, rewrites);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_call_targets_expr(cond, rewrites);
                changed |= rewrite_call_targets_stmts(then_body, rewrites);
                changed |= rewrite_call_targets_stmts(else_body, rewrites);
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

fn rewrite_call_targets_expr(expr: &mut DirExpr, rewrites: &HashMap<String, String>) -> bool {
    let mut changed = false;
    match expr {
        DirExpr::Call { target, args, .. } => {
            if let Some(replacement) = rewrites.get(target) {
                *target = replacement.clone();
                changed = true;
            }
            for arg in args {
                changed |= rewrite_call_targets_expr(arg, rewrites);
            }
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_call_targets_expr(lhs, rewrites);
            changed |= rewrite_call_targets_expr(rhs, rewrites);
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => {
            changed |= rewrite_call_targets_expr(expr, rewrites);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= rewrite_call_targets_expr(base, rewrites);
            changed |= rewrite_call_targets_expr(index, rewrites);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_call_targets_expr(cond, rewrites);
            changed |= rewrite_call_targets_expr(then_expr, rewrites);
            changed |= rewrite_call_targets_expr(else_expr, rewrites);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    changed
}

fn canonical_variadic_runtime_symbol(target: &str) -> String {
    let mut symbol = target
        .rsplit_once('!')
        .map(|(_, symbol)| symbol)
        .unwrap_or(target)
        .trim()
        .to_ascii_lowercase();

    loop {
        if let Some(stripped) = symbol.strip_prefix("__imp_") {
            symbol = stripped.to_string();
            continue;
        }
        if let Some(stripped) = symbol.strip_prefix("__mingw_") {
            symbol = stripped.to_string();
            continue;
        }
        if let Some(stripped) = symbol.strip_prefix('_') {
            symbol = stripped.to_string();
            continue;
        }
        break;
    }
    symbol
}

fn is_known_variadic_runtime_symbol(target: &str) -> bool {
    matches!(
        canonical_variadic_runtime_symbol(target).as_str(),
        "printf"
            | "fprintf"
            | "sprintf"
            | "snprintf"
            | "scanf"
            | "fscanf"
            | "sscanf"
            | "wprintf"
            | "fwprintf"
            | "swprintf"
            | "snwprintf"
            | "wscanf"
            | "fwscanf"
            | "swscanf"
            | "sprintf_s"
            | "snprintf_s"
            | "fprintf_s"
            | "printf_s"
            | "scanf_s"
            | "fscanf_s"
            | "sscanf_s"
            | "swprintf_s"
            | "snwprintf_s"
            | "fwprintf_s"
            | "wprintf_s"
            | "wscanf_s"
            | "fwscanf_s"
            | "swscanf_s"
            | "wsprintf"
            | "wsprintfw"
    )
}

/// Ghidra `FormatStringAnalyzer` scorecard item: types printf-family
/// variadic arguments from their own format string's `%`-conversion
/// specifiers, e.g. `printf("%d %s", x, y)` types `x` as `int` and `y` as
/// `char*` -- previously only the fixed leading parameter (if any) was
/// ever typed for a variadic call, per [`is_known_variadic_runtime_symbol`]'s
/// role elsewhere in this file (arity pruning only).
///
/// The format string's *text* is trivially available here already: `lower_
/// varnode_inner` (`fission-pcode/src/midend/builder/expr/lower_expr.rs`)
/// resolves a constant matching `options.global_names` -- which
/// `NirRenderOptions::from_loaded_binary` (`fission-midend-core/src/ir/
/// options.rs`) pre-populates with every extracted `.rdata` string,
/// already wrapped in quotes and escaped -- to `DirExpr::AddressOfGlobal(
/// "\"...\"")`. [`arg_var_name`] (used by [`collect_callsites_stmts`]
/// already, for the unrelated existing per-parameter typing above) already
/// captures `AddressOfGlobal` names verbatim, so the quoted format-string
/// text is already sitting in `arg_vars` by the time this runs -- no new
/// binary access or HIR traversal needed.
///
/// Deliberately scoped to the unambiguous ANSI narrow-string printf family
/// (`printf`/`fprintf`/`sprintf`/`snprintf`/their `_s` secure-CRT
/// variants). Two families are intentionally excluded, not overlooked:
/// scanf-family functions take *pointers* to write into (a different
/// typing rule -- `%d` there means `int*`, not `int`), and the wide-
/// character `wprintf`/`swprintf` family flips `%s`'s meaning (narrow
/// `char*`, not `wchar_t*`, per the ANSI convention -- a correctness trap
/// not worth the risk without a dedicated fixture to validate against).
fn apply_variadic_printf_format_string_arg_types(
    func: &mut DirFunction,
    callsites: &[(Option<String>, String, Vec<Option<String>>)],
) -> bool {
    // The call-site argument variable is often just a same-block temp
    // holding a plain copy of the real source binding (`argN = param_2;
    // printf(fmt, argN)`) -- a shape later copy-propagation would
    // normally collapse, but doing so isn't guaranteed to happen *before*
    // this pass's own type refinement would otherwise need to survive
    // (confirmed via a real fixture: the temp's type refined correctly on
    // every fixed-point iteration, but never reached the real `char *`
    // parameter it was copied from in the final output). Walking the
    // copy chain back to the true source and refining every hop directly
    // sidesteps that pipeline-ordering fragility instead of depending on
    // it.
    let mut copy_sources = HashMap::default();
    collect_copy_sources(&func.body, &mut copy_sources);

    let mut changed = false;
    for (_, callee, arg_vars) in callsites {
        let Some(format_index) = printf_family_format_string_arg_index(callee) else {
            continue;
        };
        let Some(literal) = arg_vars
            .get(format_index)
            .and_then(|arg| arg.as_deref())
            .and_then(quoted_string_literal_text)
        else {
            continue;
        };
        for (offset, ty) in parse_printf_format_specifier_types(literal)
            .into_iter()
            .enumerate()
        {
            let Some(ty) = ty else { continue };
            let arg_index = format_index + 1 + offset;
            let Some(Some(arg_var)) = arg_vars.get(arg_index) else {
                continue;
            };
            changed |= apply_variadic_printf_arg_ty_transitively(func, &copy_sources, arg_var, &ty);
        }
    }
    changed
}

/// Like [`tighten_binding_ty`], but additionally allowed to override a
/// generic *unsigned*-int binding, not just an `Unknown` one.
///
/// By the time this pass runs on real compiled code, a call-argument
/// binding almost never still has `NirType::Unknown` -- `fission-pcode`'s
/// HIR builder always assigns *some* default int type at materialization
/// time based purely on the raw register/stack-slot width (`type_from_
/// size(size, false)`, used throughout the builder), and that default is
/// always unsigned. That default is not real type evidence, just "whatever
/// size the value happened to be passed in" -- unlike `tighten_binding_ty`'s
/// caller above (the WinAPI-signature-driven typing, which has this exact
/// same real-world limitation, confirmed via a real fixture: `GetWindowRect`'s
/// own `HWND`/`RECT*` parameters stayed `ulonglong` end-to-end), a format
/// specifier IS strong, authoritative evidence (the actual variadic-call
/// API contract, not a guess) -- specific enough to be worth overriding
/// that generic default for. `tighten_binding_ty` itself stays untouched
/// (its conservatism is appropriate for its many other, less-certain
/// callers); this local override is scoped to just this one, narrow,
/// high-confidence case.
fn apply_variadic_printf_arg_ty(binding: &mut DirBinding, candidate: &NirType) -> bool {
    if tighten_binding_ty(binding, candidate) {
        return true;
    }
    if matches!(binding.ty, NirType::Int { signed: false, .. }) && binding.ty != *candidate {
        binding.ty = candidate.clone();
        return true;
    }
    false
}

/// Applies [`apply_variadic_printf_arg_ty`] to `arg_var`'s own binding,
/// then walks `copy_sources` backward (bounded by a visited-set, same
/// cycle-safety pattern used throughout this crate) applying it to every
/// transitive copy-source too, so a refinement on a call-site temp
/// reaches the real originating parameter/local it was copied from.
fn apply_variadic_printf_arg_ty_transitively(
    func: &mut DirFunction,
    copy_sources: &HashMap<String, String>,
    arg_var: &str,
    ty: &NirType,
) -> bool {
    let mut changed = false;
    let mut current = arg_var.to_string();
    let mut visited = HashSet::default();
    while visited.insert(current.clone()) {
        if let Some(b) = binding_by_name_mut(&mut func.locals, &current)
            .or_else(|| binding_by_name_mut(&mut func.params, &current))
        {
            changed |= apply_variadic_printf_arg_ty(b, ty);
        }
        match copy_sources.get(&current) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    changed
}

/// Single-hop `target = source` (bare `Var`-to-`Var`) copy map, used by
/// [`apply_variadic_printf_arg_ty_transitively`] to trace a call-site
/// argument temp back to its real originating binding.
fn collect_copy_sources(stmts: &[DirStmt], out: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(target),
                rhs: DirExpr::Var(source),
            } => {
                out.insert(target.clone(), source.clone());
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_copy_sources(body, out);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_sources(then_body, out);
                collect_copy_sources(else_body, out);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_copy_sources(std::slice::from_ref(i), out);
                }
                if let Some(u) = update {
                    collect_copy_sources(std::slice::from_ref(u), out);
                }
                collect_copy_sources(body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_sources(&case.body, out);
                }
                collect_copy_sources(default, out);
            }
            _ => {}
        }
    }
}

/// 0-based argument index of the format-string parameter for a known
/// printf-family symbol, or `None` if `target` isn't one of the (narrow-
/// string-only, see [`apply_variadic_printf_format_string_arg_types`]'s
/// doc comment) symbols this recognizes.
fn printf_family_format_string_arg_index(target: &str) -> Option<usize> {
    match canonical_variadic_runtime_symbol(target).as_str() {
        "printf" | "printf_s" => Some(0),
        "fprintf" | "fprintf_s" | "sprintf" => Some(1),
        "sprintf_s" | "snprintf" | "snprintf_s" => Some(2),
        _ => None,
    }
}

/// Strips the surrounding quotes `NirRenderOptions::from_loaded_binary`
/// wraps every extracted string constant in, or `None` if `name` isn't
/// one (e.g. an ordinary symbol/global name, or a non-constant argument
/// [`arg_var_name`] captured by variable name instead).
fn quoted_string_literal_text(name: &str) -> Option<&str> {
    name.strip_prefix('"')?.strip_suffix('"')
}

/// Scans a printf-style format string for `%`-conversion specifiers,
/// returning one entry per specifier (in order) with the `NirType` it
/// implies for that variadic argument, or `None` for a specifier this
/// doesn't have a confident type for (unrecognized conversion character --
/// leaves that argument's type alone rather than guessing). `%%` (a
/// literal percent) and a `*` dynamic width/precision (which itself
/// consumes an extra leading `int` argument, per the C standard: "the
/// argument supplying [a `*`] width/precision... shall appear before the
/// argument (if any) to be converted") are both handled to keep the
/// specifier-to-argument-position alignment correct.
fn parse_printf_format_specifier_types(text: &str) -> Vec<Option<NirType>> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        if chars.peek().copied() == Some('%') {
            chars.next();
            continue;
        }
        while matches!(chars.peek().copied(), Some('-' | '+' | ' ' | '#' | '0')) {
            chars.next();
        }
        if chars.peek().copied() == Some('*') {
            chars.next();
            result.push(Some(NirType::Int {
                bits: 32,
                signed: true,
            }));
        } else {
            while matches!(chars.peek().copied(), Some(c) if c.is_ascii_digit()) {
                chars.next();
            }
        }
        if chars.peek().copied() == Some('.') {
            chars.next();
            if chars.peek().copied() == Some('*') {
                chars.next();
                result.push(Some(NirType::Int {
                    bits: 32,
                    signed: true,
                }));
            } else {
                while matches!(chars.peek().copied(), Some(c) if c.is_ascii_digit()) {
                    chars.next();
                }
            }
        }
        // Length modifiers: `hh`/`h` (narrower, doesn't affect promoted
        // vararg width so ignored), `l`/`ll` (widens to 64-bit at `ll`;
        // a lone `l` also means wide-char for `%s`/`%c`), `L`/`z`/`j`/`t`
        // (ignored, doesn't change the promoted vararg width this cares
        // about), MSVC `I32`/`I64`.
        let mut long_count = 0u8;
        loop {
            match chars.peek().copied() {
                Some('h') => {
                    chars.next();
                    if chars.peek().copied() == Some('h') {
                        chars.next();
                    }
                }
                Some('l') => {
                    chars.next();
                    long_count += 1;
                    if chars.peek().copied() == Some('l') {
                        chars.next();
                        long_count += 1;
                    }
                }
                Some('L' | 'z' | 'j' | 't') => {
                    chars.next();
                }
                Some('I') => {
                    chars.next();
                    if chars.peek().copied() == Some('6') {
                        chars.next();
                        if chars.peek().copied() == Some('4') {
                            chars.next();
                            long_count = 2;
                        }
                    } else if chars.peek().copied() == Some('3') {
                        chars.next();
                        if chars.peek().copied() == Some('2') {
                            chars.next();
                        }
                    }
                }
                _ => break,
            }
        }
        let Some(conv) = chars.next() else {
            break;
        };
        let is_wide = long_count >= 1;
        let is_64 = long_count >= 2;
        let ty = match conv {
            'd' | 'i' => Some(NirType::Int {
                bits: if is_64 { 64 } else { 32 },
                signed: true,
            }),
            'u' | 'x' | 'X' | 'o' => Some(NirType::Int {
                bits: if is_64 { 64 } else { 32 },
                signed: false,
            }),
            'c' => Some(NirType::Int {
                bits: 32,
                signed: true,
            }),
            's' => Some(NirType::Ptr(Box::new(NirType::Int {
                bits: if is_wide { 16 } else { 8 },
                signed: false,
            }))),
            'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'a' | 'A' => Some(NirType::Float { bits: 64 }),
            'p' => Some(NirType::Ptr(Box::new(NirType::Unknown))),
            'n' => Some(NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: true,
            }))),
            _ => None,
        };
        result.push(ty);
    }
    result
}

/// Apply call-site type propagation to a function.
///
/// Collects all `Call` expressions, looks up each target in the API type provider, and
/// updates argument/receiver bindings with the resolved types.
///
/// Returns `true` if any binding type was updated.
pub fn apply_callsite_type_prop_pass(func: &mut DirFunction) -> bool {
    // Build a lookup map from binding name to index in func.locals / func.params.
    let mut changed = false;
    let mut rename_candidates = HashMap::<String, String>::default();
    let mut rename_conflicts = HashSet::<String>::default();
    let mut wrapper_resolved_count = 0usize;
    let mut signature_missing_count = 0usize;
    let mut unknown_target_kept_count = 0usize;

    // Collect call sites: (receiver_name_opt, callee_name, arg_var_names)
    let mut callsites: Vec<(Option<String>, String, Vec<Option<String>>)> = Vec::new();
    collect_callsites_stmts(&func.body, &mut callsites);
    changed |= apply_variadic_printf_format_string_arg_types(func, &callsites);
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
            .or_else(|| func.callee_summaries.get(resolved_callee));
        let Some(sig) = api_signature_via_import_aliases(resolved_callee)
            .or_else(|| api_signature_via_import_aliases(callee))
        else {
            if summary.is_some() {
                signature_missing_count += 1;
            } else {
                unknown_target_kept_count += 1;
            }
            if let Some(summary) = summary {
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
                    if let Some(b) = binding_by_name_mut(&mut func.locals, arg_var)
                        .or_else(|| binding_by_name_mut(&mut func.params, arg_var))
                    {
                        let tightened = tighten_binding_ty(b, param_ty);
                        changed |= tightened;
                        refined_here |= tightened;
                    }
                }
                if refined_here {
                    add_call_signature_refinements(1);
                }
            }
            continue;
        };
        let mut refined_here = false;

        // Resolve return type and update receiver binding.
        if let Some(ret_ty) = resolve_return_ty(&sig.return_type) {
            if let Some(recv_name) = receiver {
                if let Some(b) = binding_by_name_mut(&mut func.locals, recv_name)
                    .or_else(|| binding_by_name_mut(&mut func.params, recv_name))
                {
                    let tightened = tighten_binding_ty(b, &ret_ty);
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
            if let Some(b) = binding_by_name_mut(&mut func.locals, arg_var)
                .or_else(|| binding_by_name_mut(&mut func.params, arg_var))
            {
                let tightened = win_type_name_to_nir(&param.type_name)
                    .map(|param_ty| tighten_binding_ty(b, &param_ty))
                    .unwrap_or(false);
                let surface_tightened =
                    b.surface_type_name.is_none() && !param.type_name.trim().is_empty();
                if surface_tightened {
                    b.surface_type_name = Some(param.type_name.trim().to_string());
                }
                changed |= tightened || surface_tightened;
                refined_here |= tightened || surface_tightened;
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
    let self_pruned_count =
        prune_self_call_args_stmts(&mut func.body, &func.name, func.params.len());
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

    changed
}

fn exact_arity_for_target(
    target: &str,
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> Option<usize> {
    let resolved_target = resolve_call_target_symbol(target, summaries);
    if is_known_variadic_runtime_symbol(target) || is_known_variadic_runtime_symbol(resolved_target)
    {
        return None;
    }
    if resolved_target != target {
        return api_signature_via_import_aliases(resolved_target)
            .map(|sig| sig.params.len())
            .or_else(|| {
                summaries
                    .get(resolved_target)
                    .and_then(|summary| summary.prototype.locked_exact_arity)
            })
            .or_else(|| api_signature_via_import_aliases(target).map(|sig| sig.params.len()));
    }
    summaries
        .get(target)
        .and_then(|summary| summary.prototype.locked_exact_arity)
        .or_else(|| api_signature_via_import_aliases(resolved_target).map(|sig| sig.params.len()))
        .or_else(|| api_signature_via_import_aliases(target).map(|sig| sig.params.len()))
}

fn prune_known_api_call_args_stmts(
    stmts: &mut [DirStmt],
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                pruned += prune_known_api_call_args_expr(rhs, summaries);
            }
            DirStmt::VaStart { va_list, .. } => {
                pruned += prune_known_api_call_args_expr(va_list, summaries);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                pruned += prune_known_api_call_args_stmts(body, summaries);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_known_api_call_args_expr(expr, summaries);
                for case in cases {
                    pruned += prune_known_api_call_args_stmts(&mut case.body, summaries);
                }
                pruned += prune_known_api_call_args_stmts(default, summaries);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_known_api_call_args_expr(cond, summaries);
                pruned += prune_known_api_call_args_stmts(then_body, summaries);
                pruned += prune_known_api_call_args_stmts(else_body, summaries);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_known_api_call_args_expr(
    expr: &mut DirExpr,
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    match expr {
        DirExpr::Call { target, args, .. } => {
            for arg in args.iter_mut() {
                pruned += prune_known_api_call_args_expr(arg, summaries);
            }
            if let Some(exact_arity) = exact_arity_for_target(target, summaries)
                && args.len() > exact_arity
            {
                let removed = args.len() - exact_arity;
                args.truncate(exact_arity);
                pruned += removed;
            }
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_known_api_call_args_expr(lhs, summaries);
            pruned += prune_known_api_call_args_expr(rhs, summaries);
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_known_api_call_args_expr(expr, summaries);
        }
        DirExpr::Index { base, index, .. } => {
            pruned += prune_known_api_call_args_expr(base, summaries);
            pruned += prune_known_api_call_args_expr(index, summaries);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_known_api_call_args_expr(cond, summaries);
            pruned += prune_known_api_call_args_expr(then_expr, summaries);
            pruned += prune_known_api_call_args_expr(else_expr, summaries);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    pruned
}

fn prune_self_call_args_stmts(stmts: &mut [DirStmt], func_name: &str, arity: usize) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                pruned += prune_self_call_args_expr(rhs, func_name, arity);
            }
            DirStmt::VaStart { va_list, .. } => {
                pruned += prune_self_call_args_expr(va_list, func_name, arity);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                pruned += prune_self_call_args_stmts(body, func_name, arity);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_self_call_args_expr(expr, func_name, arity);
                for case in cases {
                    pruned += prune_self_call_args_stmts(&mut case.body, func_name, arity);
                }
                pruned += prune_self_call_args_stmts(default, func_name, arity);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_self_call_args_expr(cond, func_name, arity);
                pruned += prune_self_call_args_stmts(then_body, func_name, arity);
                pruned += prune_self_call_args_stmts(else_body, func_name, arity);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_self_call_args_expr(expr: &mut DirExpr, func_name: &str, arity: usize) -> usize {
    let mut pruned = 0usize;
    match expr {
        DirExpr::Call { target, args, .. } => {
            for arg in args.iter_mut() {
                pruned += prune_self_call_args_expr(arg, func_name, arity);
            }
            if target == func_name && args.len() > arity {
                let removed = args.len() - arity;
                args.truncate(arity);
                pruned += removed;
            }
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_self_call_args_expr(lhs, func_name, arity);
            pruned += prune_self_call_args_expr(rhs, func_name, arity);
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_self_call_args_expr(expr, func_name, arity);
        }
        DirExpr::Index { base, index, .. } => {
            pruned += prune_self_call_args_expr(base, func_name, arity);
            pruned += prune_self_call_args_expr(index, func_name, arity);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_self_call_args_expr(cond, func_name, arity);
            pruned += prune_self_call_args_expr(then_expr, func_name, arity);
            pruned += prune_self_call_args_expr(else_expr, func_name, arity);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    pruned
}

fn binding_by_name_mut<'a>(
    bindings: &'a mut Vec<DirBinding>,
    name: &str,
) -> Option<&'a mut DirBinding> {
    bindings.iter_mut().find(|b| b.name == name)
}

/// Extract the plain variable name from a Call argument expression (if it's
/// `Var(x)` or `Cast(_, Var(x))`).  Returns `None` for complex expressions.
fn arg_var_name(expr: &DirExpr) -> Option<String> {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => Some(name.clone()),
        DirExpr::Cast { expr: inner, .. } => arg_var_name(inner),
        _ => None,
    }
}

fn collect_callsites_stmts(
    stmts: &[DirStmt],
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    for stmt in stmts {
        collect_callsites_stmt(stmt, out);
    }
}

fn collect_callsites_stmt(
    stmt: &DirStmt,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            if let DirExpr::Call { target, args, .. } = rhs {
                let recv = match lhs {
                    DirLValue::Var(name) => Some(name.clone()),
                    _ => None,
                };
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((recv, target.clone(), arg_vars));
            }
            // Also recurse in case call appears inside a more complex rhs.
            collect_callsites_expr(rhs, out);
        }
        DirStmt::Expr(expr) => {
            if let DirExpr::Call { target, args, .. } = expr {
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((None, target.clone(), arg_vars));
            }
        }
        DirStmt::Return(Some(expr)) => collect_callsites_expr(expr, out),
        DirStmt::Block(body) => collect_callsites_stmts(body, out),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(then_body, out);
            collect_callsites_stmts(else_body, out);
        }
        DirStmt::While { cond, body } | DirStmt::DoWhile { body, cond } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(body, out);
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
    expr: &DirExpr,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match expr {
        DirExpr::Call { target, args, .. } => {
            let arg_vars = args.iter().map(arg_var_name).collect();
            out.push((None, target.clone(), arg_vars));
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_callsites_expr(lhs, out);
            collect_callsites_expr(rhs, out);
        }
        DirExpr::Cast { expr: inner, .. } | DirExpr::Unary { expr: inner, .. } => {
            collect_callsites_expr(inner, out);
        }
        DirExpr::Load { ptr, .. } => collect_callsites_expr(ptr, out),
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
            collect_callsites_expr(base, out)
        }
        DirExpr::Index { base, index, .. } => {
            collect_callsites_expr(base, out);
            collect_callsites_expr(index, out);
        }
        DirExpr::AggregateCopy { src, .. } => collect_callsites_expr(src, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::wave_stats::{reset_normalize_wave_stats, take_normalize_wave_stats};
    // prelude via parent
    use fission_core::CallingConvention;

    fn unknown_binding(name: &str, origin: Option<NirBindingOrigin>) -> DirBinding {
        DirBinding {
            name: name.to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    #[test]
    fn callsite_type_prop_promotes_import_param_name_and_surface_type() {
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![unknown_binding(
                "param_1",
                Some(NirBindingOrigin::ParamIndex(0)),
            )],
            locals: vec![unknown_binding(
                "local_2",
                Some(NirBindingOrigin::DerivedFromStackOffset(-0x20)),
            )],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "GetWindowRect".to_string(),
                args: vec![
                    DirExpr::Var("param_1".to_string()),
                    DirExpr::Var("local_2".to_string()),
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
    }

    #[test]
    fn callsite_type_prop_rewrites_target_through_wrapper_summary() {
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
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
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
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
            DirStmt::Expr(DirExpr::Call { target, .. }) => {
                assert_eq!(target, "MessageBoxA");
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_extra_args_only_for_exact_api_signature() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Expr(DirExpr::Call {
                    target: "MessageBoxA".to_string(),
                    args: vec![
                        DirExpr::Const(0, NirType::Unknown),
                        DirExpr::Const(1, NirType::Unknown),
                        DirExpr::Const(2, NirType::Unknown),
                        DirExpr::Const(3, NirType::Unknown),
                        DirExpr::Const(4, NirType::Unknown),
                        DirExpr::Const(5, NirType::Unknown),
                    ],
                    ty: NirType::Unknown,
                }),
                DirStmt::Expr(DirExpr::Call {
                    target: "unresolved_target".to_string(),
                    args: vec![
                        DirExpr::Const(0, NirType::Unknown),
                        DirExpr::Const(1, NirType::Unknown),
                        DirExpr::Const(2, NirType::Unknown),
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
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 4),
            other => panic!("unexpected first stmt: {other:?}"),
        }
        match &func.body[1] {
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 3),
            other => panic!("unexpected second stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_keeps_extra_args_for_known_variadic_runtime_symbol() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "printf".to_string(),
                args: vec![
                    DirExpr::Const(0, NirType::Unknown),
                    DirExpr::Const(1, NirType::Unknown),
                    DirExpr::Const(2, NirType::Unknown),
                    DirExpr::Const(3, NirType::Unknown),
                    DirExpr::Const(4, NirType::Unknown),
                    DirExpr::Const(5, NirType::Unknown),
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
                        min_arity: 4,
                        max_arity: 4,
                        locked_exact_arity: Some(4),
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown; 4],
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
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 6),
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

        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![
                DirBinding {
                    name: "param_1".to_string(),
                    ty: generic_uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(0)),
                    initializer: None,
                },
                DirBinding {
                    name: "param_2".to_string(),
                    ty: generic_uint(64),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(1)),
                    initializer: None,
                },
            ],
            locals: vec![
                DirBinding {
                    name: "arg_tmp".to_string(),
                    ty: generic_uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                DirBinding {
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
                DirStmt::Assign {
                    lhs: DirLValue::Var("arg_tmp".to_string()),
                    rhs: DirExpr::Var("param_1".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("arg_tmp2".to_string()),
                    rhs: DirExpr::Var("param_2".to_string()),
                },
                DirStmt::Expr(DirExpr::Call {
                    target: "printf".to_string(),
                    args: vec![
                        DirExpr::AddressOfGlobal("\"value=%d name=%s\\n\"".to_string()),
                        DirExpr::Var("arg_tmp".to_string()),
                        DirExpr::Var("arg_tmp2".to_string()),
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
    fn callsite_type_prop_prunes_self_recursive_args_to_function_arity() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "fib".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![DirBinding {
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
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "fib".to_string(),
                args: vec![
                    DirExpr::Const(1, NirType::Unknown),
                    DirExpr::Const(2, NirType::Unknown),
                    DirExpr::Const(3, NirType::Unknown),
                    DirExpr::Const(4, NirType::Unknown),
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
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 1),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_wrapper_args_after_resolving_import_summary() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "wrapper_message_box".to_string(),
                args: vec![
                    DirExpr::Const(0, NirType::Unknown),
                    DirExpr::Const(1, NirType::Unknown),
                    DirExpr::Const(2, NirType::Unknown),
                    DirExpr::Const(3, NirType::Unknown),
                    DirExpr::Const(4, NirType::Unknown),
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
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
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
            DirStmt::Expr(DirExpr::Call { target, args, .. }) => {
                assert_eq!(target, "MessageBoxA");
                assert_eq!(args.len(), 4);
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_prunes_locked_internal_callee_arity() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "recursive_fib".to_string(),
                args: vec![
                    DirExpr::Const(0, NirType::Unknown),
                    DirExpr::Const(1, NirType::Unknown),
                    DirExpr::Const(2, NirType::Unknown),
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
                        min_arity: 1,
                        max_arity: 1,
                        locked_exact_arity: Some(1),
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown],
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
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 1),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn callsite_type_prop_keeps_args_when_summary_signature_missing() {
        reset_normalize_wave_stats();
        let mut func = DirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Expr(DirExpr::Call {
                target: "known_without_signature".to_string(),
                args: vec![
                    DirExpr::Const(0, NirType::Unknown),
                    DirExpr::Const(1, NirType::Unknown),
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
                        min_arity: 0,
                        max_arity: 2,
                        locked_exact_arity: None,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
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
            DirStmt::Expr(DirExpr::Call { args, .. }) => assert_eq!(args.len(), 2),
            other => panic!("unexpected stmt: {other:?}"),
        }
    }
}
