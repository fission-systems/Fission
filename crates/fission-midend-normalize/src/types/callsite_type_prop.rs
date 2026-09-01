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
use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::wave_stats::{
    add_call_prototype_exact_api_arity_pruned, add_call_prototype_signature_missing,
    add_call_prototype_unknown_target_kept, add_call_prototype_wrapper_resolved,
    add_call_signature_refinements, add_surface_fact_promotions, add_typed_fact_conflicts,
};
use fission_midend_prehir::util::rename_vars_in_stmts;
use fission_signatures::{
    ApiSignature, SIGNATURE_RESOURCES, canonical_variadic_runtime_symbol,
    is_known_variadic_runtime_symbol, printf_style_format_string_arg_index,
    symbol_for_win_api_database_lookup, type_name_is_informative,
};

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

fn rewrite_call_targets_stmts(
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
/// already wrapped in quotes and escaped -- to `PreHirExpr::AddressOfGlobal(
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
    func: &mut PreHirFunction,
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
        let Some(format_index) = admitted_printf_style_format_index(func, callee) else {
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

fn admitted_printf_style_format_index(func: &PreHirFunction, target: &str) -> Option<usize> {
    let format_index = printf_style_format_string_arg_index(target)?;
    let canonical = canonical_variadic_runtime_symbol(target);
    if matches!(
        canonical.as_str(),
        "error" | "error_at_line" | "printf_chk" | "fprintf_chk" | "sprintf_chk" | "snprintf_chk"
    ) {
        let summary = func.callee_summaries.get(target)?;
        if !summary.target.is_import_locked() {
            return None;
        }
    }
    Some(format_index)
}

#[derive(Clone, Default)]
struct FormatFlowState {
    translated_literals: HashMap<String, String>,
    copy_chains: HashMap<String, Vec<String>>,
}

impl FormatFlowState {
    fn clear(&mut self) {
        self.translated_literals.clear();
        self.copy_chains.clear();
    }

    fn copy_chain_for_expr(&self, expr: &PreHirExpr) -> Vec<String> {
        let Some(name) = plain_copy_var(expr) else {
            return Vec::new();
        };
        self.copy_chains
            .get(name)
            .cloned()
            .unwrap_or_else(|| vec![name.to_string()])
    }

    fn format_literal_for_expr(&self, expr: &PreHirExpr) -> Option<String> {
        match expr {
            PreHirExpr::AddressOfGlobal(name) => {
                quoted_string_literal_text(name).map(str::to_string)
            }
            PreHirExpr::Var(name) => self.translated_literals.get(name).cloned(),
            PreHirExpr::Cast { expr, .. } => self.format_literal_for_expr(expr),
            _ => None,
        }
    }
}

fn plain_copy_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr, .. } => plain_copy_var(expr),
        _ => None,
    }
}

fn imported_translation_message_index(func: &PreHirFunction, target: &str) -> Option<usize> {
    let message_index = match canonical_variadic_runtime_symbol(target).as_str() {
        "gettext" => 0,
        "dcgettext" => 1,
        _ => return None,
    };
    func.callee_summaries
        .get(target)
        .filter(|summary| summary.target.is_import_locked())?;
    Some(message_index)
}

fn translated_literal_from_expr(func: &PreHirFunction, expr: &PreHirExpr) -> Option<String> {
    let expr = match expr {
        PreHirExpr::Cast { expr, .. } => expr.as_ref(),
        _ => expr,
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        return None;
    };
    let message_index = imported_translation_message_index(func, target)?;
    args.get(message_index).and_then(|message| match message {
        PreHirExpr::AddressOfGlobal(name) => quoted_string_literal_text(name).map(str::to_string),
        PreHirExpr::Cast { expr, .. } => match expr.as_ref() {
            PreHirExpr::AddressOfGlobal(name) => {
                quoted_string_literal_text(name).map(str::to_string)
            }
            _ => None,
        },
        _ => None,
    })
}

type FormatCallEvidence = (usize, String, Vec<Vec<String>>);

fn collect_site_sensitive_format_evidence_expr(
    func: &PreHirFunction,
    expr: &PreHirExpr,
    state: &FormatFlowState,
    out: &mut Vec<FormatCallEvidence>,
) {
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            if let Some(format_index) = admitted_printf_style_format_index(func, target)
                && let Some(literal) = args
                    .get(format_index)
                    .and_then(|format| state.format_literal_for_expr(format))
            {
                out.push((
                    format_index,
                    literal,
                    args.iter()
                        .map(|arg| state.copy_chain_for_expr(arg))
                        .collect(),
                ));
            }
            for arg in args {
                collect_site_sensitive_format_evidence_expr(func, arg, state, out);
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_site_sensitive_format_evidence_expr(func, lhs, state, out);
            collect_site_sensitive_format_evidence_expr(func, rhs, state, out);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            collect_site_sensitive_format_evidence_expr(func, expr, state, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_site_sensitive_format_evidence_expr(func, base, state, out);
            collect_site_sensitive_format_evidence_expr(func, index, state, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_site_sensitive_format_evidence_expr(func, cond, state, out);
            collect_site_sensitive_format_evidence_expr(func, then_expr, state, out);
            collect_site_sensitive_format_evidence_expr(func, else_expr, state, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_site_sensitive_format_evidence_stmts(
    func: &PreHirFunction,
    stmts: &[PreHirStmt],
    state: &mut FormatFlowState,
    out: &mut Vec<FormatCallEvidence>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_site_sensitive_format_evidence_expr(func, rhs, state, out);
                let PreHirLValue::Var(target) = lhs else {
                    continue;
                };
                let translated_literal = translated_literal_from_expr(func, rhs).or_else(|| {
                    plain_copy_var(rhs)
                        .and_then(|source| state.translated_literals.get(source).cloned())
                });
                match translated_literal {
                    Some(literal) => {
                        state.translated_literals.insert(target.clone(), literal);
                    }
                    None => {
                        state.translated_literals.remove(target);
                    }
                }

                if let Some(source) = plain_copy_var(rhs) {
                    let mut chain = vec![target.clone()];
                    chain.extend(
                        state
                            .copy_chains
                            .get(source)
                            .cloned()
                            .unwrap_or_else(|| vec![source.to_string()]),
                    );
                    if chain.iter().collect::<HashSet<_>>().len() == chain.len() {
                        state.copy_chains.insert(target.clone(), chain);
                    } else {
                        state.copy_chains.remove(target);
                    }
                } else {
                    state
                        .copy_chains
                        .insert(target.clone(), vec![target.clone()]);
                }
            }
            PreHirStmt::Expr(expr) => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out)
            }
            PreHirStmt::Return(Some(expr)) => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out);
                state.clear();
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_site_sensitive_format_evidence_expr(func, va_list, state, out)
            }
            PreHirStmt::Block(body) => {
                collect_site_sensitive_format_evidence_stmts(func, body, state, out)
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_site_sensitive_format_evidence_expr(func, cond, state, out);
                let mut then_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, then_body, &mut then_state, out);
                let mut else_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, else_body, &mut else_state, out);
                state.clear();
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                collect_site_sensitive_format_evidence_expr(func, cond, state, out);
                let mut body_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(func, body, &mut body_state, out);
                state.clear();
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let mut loop_state = state.clone();
                if let Some(init) = init {
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        std::slice::from_ref(init.as_ref()),
                        &mut loop_state,
                        out,
                    );
                }
                if let Some(cond) = cond {
                    collect_site_sensitive_format_evidence_expr(func, cond, &loop_state, out);
                }
                collect_site_sensitive_format_evidence_stmts(func, body, &mut loop_state, out);
                if let Some(update) = update {
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        std::slice::from_ref(update.as_ref()),
                        &mut loop_state,
                        out,
                    );
                }
                state.clear();
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_site_sensitive_format_evidence_expr(func, expr, state, out);
                for case in cases {
                    let mut case_state = state.clone();
                    collect_site_sensitive_format_evidence_stmts(
                        func,
                        &case.body,
                        &mut case_state,
                        out,
                    );
                }
                let mut default_state = state.clone();
                collect_site_sensitive_format_evidence_stmts(
                    func,
                    default,
                    &mut default_state,
                    out,
                );
                state.clear();
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => state.clear(),
        }
    }
}

fn apply_site_sensitive_translated_format_types(func: &mut PreHirFunction) -> bool {
    let mut evidence = Vec::new();
    collect_site_sensitive_format_evidence_stmts(
        func,
        &func.body,
        &mut FormatFlowState::default(),
        &mut evidence,
    );

    let mut changed = false;
    for (format_index, literal, arg_chains) in evidence {
        for (offset, ty) in parse_printf_format_specifier_types(&literal)
            .into_iter()
            .enumerate()
        {
            let Some(ty) = ty else { continue };
            let Some(chain) = arg_chains.get(format_index + 1 + offset) else {
                continue;
            };
            for name in chain {
                if let Some(binding) = binding_by_name_mut(&mut func.locals, name)
                    .or_else(|| binding_by_name_mut(&mut func.params, name))
                {
                    changed |= apply_variadic_printf_arg_ty(binding, &ty);
                }
            }
        }
    }
    changed
}

/// Like [`tighten_binding_ty`], but additionally allowed to override a
/// generic *unsigned*-int binding with a format-specifier scalar type.
///
/// By the time this pass runs on real compiled code, a call-argument
/// binding almost never still has `NirType::Unknown` -- `fission-pcode`'s
/// HIR builder always assigns *some* default int type at materialization
/// time based purely on the raw register/stack-slot width (`type_from_
/// size(size, false)`, used throughout the builder), and that default is
/// always unsigned. That default is not real type evidence, just "whatever
/// size the value happened to be passed in". A format specifier is strong,
/// authoritative evidence for scalar variadic arguments and may also refine
/// signedness/width in ways the ordinary monotone rule deliberately does not.
fn apply_variadic_printf_arg_ty(binding: &mut PreHirBinding, candidate: &NirType) -> bool {
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
    func: &mut PreHirFunction,
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
fn collect_copy_sources(stmts: &[PreHirStmt], out: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(target),
                rhs: PreHirExpr::Var(source),
            } => {
                out.insert(target.clone(), source.clone());
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_copy_sources(body, out);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_sources(then_body, out);
                collect_copy_sources(else_body, out);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_sources(&case.body, out);
                }
                collect_copy_sources(default, out);
            }
            _ => {}
        }
    }
}

/// Carry an exact API parameter declaration back through stable plain-copy
/// aliases.  The call argument itself keeps the historical behavior of
/// receiving the surface declaration even when its PreHIR name is reused;
/// propagation beyond that name requires the same single-definition and
/// non-self-referential proof as operation-edge type flow.
fn apply_api_surface_type_transitively(
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
    let generic_void_pointer = matches!(compact_surface.as_str(), "VOID*" | "LPVOID" | "PVOID");
    while visited.insert(current.clone()) {
        if let Some(binding) = binding_by_name_mut(&mut func.locals, &current)
            .or_else(|| binding_by_name_mut(&mut func.params, &current))
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name.to_string());
            changed = true;
        }
        // `free(void *)` proves the call accepts this value, not that the
        // source declaration itself was `void *`: any object pointer may be
        // converted for that call. Keep the surface on the immediate argument
        // but do not erase a more specific source pointee across its copy.
        if generic_void_pointer {
            break;
        }
        if !super::type_flow::binding_is_safe_for_backward_refine(
            &current,
            definition_counts,
            self_referential,
        ) {
            break;
        }
        match copy_sources.get(&current) {
            Some(source)
                if super::type_flow::binding_is_safe_for_backward_refine(
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

fn tighten_binding_from_direct_callee_pointer(
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
fn apply_direct_callee_pointer_transitively(
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
        if !super::type_flow::binding_is_safe_for_backward_refine(
            &current,
            definition_counts,
            self_referential,
        ) {
            break;
        }
        match copy_sources.get(&current) {
            Some(source)
                if super::type_flow::binding_is_safe_for_backward_refine(
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
            changed |= tighten_binding_from_direct_callee_pointer(binding, param_ty, pointer_bits);
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
        // vararg width so ignored), `l`/`ll` (`ll` is always 64-bit, while
        // a lone `l` has ABI-dependent integer width and also means wide-char
        // for `%s`/`%c`), `L`/`z`/`j`/`t`
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
        let integer_bits = match long_count {
            0 => Some(32),
            1 => None,
            _ => Some(64),
        };
        let ty = match conv {
            'd' | 'i' => integer_bits.map(|bits| NirType::Int { bits, signed: true }),
            'u' | 'x' | 'X' | 'o' => integer_bits.map(|bits| NirType::Int {
                bits,
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
pub fn apply_callsite_type_prop_pass(func: &mut PreHirFunction) -> bool {
    // Build a lookup map from binding name to index in func.locals / func.params.
    let mut changed = false;
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
            let informative = type_name_is_informative(&param.type_name);
            if let Some(b) = binding_by_name_mut(&mut func.locals, arg_var)
                .or_else(|| binding_by_name_mut(&mut func.params, arg_var))
            {
                let tightened = informative
                    && win_type_name_to_nir(&param.type_name)
                        .map(|param_ty| tighten_binding_ty(b, &param_ty))
                        .unwrap_or(false);
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
    let callee_summaries = func.callee_summaries.clone();
    let void_receivers = drop_void_call_receivers(&mut func.body, &callee_summaries);
    if void_receivers > 0 {
        add_call_signature_refinements(void_receivers);
        changed = true;
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

/// Whether this target leaves nothing for a receiver to read.
///
/// Two independent sources, either of which is decisive:
///
/// 1. The API type library's signature says the return type is `void`.
///    `resolve_return_ty` maps `void` to `None`, the same answer it gives for
///    a type it could not resolve, so the two are indistinguishable there --
///    this asks the signature string directly.
/// 2. Ghidra's no-return lists name it. A function that never returns cannot
///    have left a result behind either, and that covers the ones missing from
///    the signature library (`__stack_chk_fail`, `setutent`). Asked across
///    every executable format rather than the binary's own, which this pass
///    does not carry: the names on those lists are libc/OS primitives whose
///    no-return-ness does not vary by container.
fn api_target_returns_void(target: &str) -> bool {
    if api_signature_via_import_aliases(target)
        .is_some_and(|sig| matches!(sig.return_type.trim(), "void" | "VOID"))
    {
        return true;
    }
    let index = fission_core::core::ghidra_no_return::ghidra_no_return_index();
    [
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_ELF,
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_PE,
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_MACHO,
    ]
    .iter()
    .any(|format| index.is_no_return(format, None, None, target))
}

/// Drop the receiver from a call whose target is known to return nothing.
///
/// A call clobbers the ABI's result register, so a later read of it that
/// liveness cannot rule out makes the call site materialize a receiver --
/// giving `rax = free(ptr);` and `rax = (uchar *)(__stack_chk_fail());`.
/// Neither is valid C (gcc: "void value not ignored as it ought to be"), and
/// neither is true: the callee left nothing there to read.
///
/// Only the assignment goes; the call itself stays as an expression
/// statement, and the receiver stays declared, so a later use of it reads an
/// uninitialized local -- which is exactly as defined as reading the
/// register the callee never wrote.
fn drop_void_call_receivers(
    stmts: &mut Vec<PreHirStmt>,
    summaries: &indexmap::IndexMap<String, fission_midend_core::CallSummary>,
) -> usize {
    let mut dropped = 0usize;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Assign { rhs, .. } => {
                if let PreHirExpr::Call { target, .. } = rhs
                    && (api_target_returns_void(target)
                        || summaries
                            .get(target)
                            .is_some_and(|summary| summary.prototype.returns_void))
                {
                    *stmt = PreHirStmt::Expr(rhs.clone());
                    dropped += 1;
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    summaries,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    summaries,
                );
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    summaries,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    dropped += drop_void_call_receivers(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        summaries,
                    );
                }
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    summaries,
                );
            }
            _ => {}
        }
    }
    dropped
}

fn prune_known_api_call_args_stmts(
    stmts: &mut [PreHirStmt],
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                pruned += prune_known_api_call_args_expr(rhs, summaries);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                pruned += prune_known_api_call_args_expr(va_list, summaries);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    summaries,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_known_api_call_args_expr(expr, summaries);
                for case in cases {
                    pruned += prune_known_api_call_args_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        summaries,
                    );
                }
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    summaries,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_known_api_call_args_expr(cond, summaries);
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    summaries,
                );
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    summaries,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_known_api_call_args_expr(
    expr: &mut PreHirExpr,
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    match expr {
        PreHirExpr::Call { target, args, .. } => {
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
        PreHirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_known_api_call_args_expr(lhs, summaries);
            pruned += prune_known_api_call_args_expr(rhs, summaries);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_known_api_call_args_expr(expr, summaries);
        }
        PreHirExpr::Index { base, index, .. } => {
            pruned += prune_known_api_call_args_expr(base, summaries);
            pruned += prune_known_api_call_args_expr(index, summaries);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_known_api_call_args_expr(cond, summaries);
            pruned += prune_known_api_call_args_expr(then_expr, summaries);
            pruned += prune_known_api_call_args_expr(else_expr, summaries);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    pruned
}

fn prune_self_call_args_stmts(stmts: &mut [PreHirStmt], func_name: &str, arity: usize) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                pruned += prune_self_call_args_expr(rhs, func_name, arity);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                pruned += prune_self_call_args_expr(va_list, func_name, arity);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_self_call_args_expr(expr, func_name, arity);
                for case in cases {
                    pruned += prune_self_call_args_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        func_name,
                        arity,
                    );
                }
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_self_call_args_expr(cond, func_name, arity);
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    func_name,
                    arity,
                );
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_self_call_args_expr(expr: &mut PreHirExpr, func_name: &str, arity: usize) -> usize {
    let mut pruned = 0usize;
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            for arg in args.iter_mut() {
                pruned += prune_self_call_args_expr(arg, func_name, arity);
            }
            if target == func_name && args.len() > arity {
                let removed = args.len() - arity;
                args.truncate(arity);
                pruned += removed;
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_self_call_args_expr(lhs, func_name, arity);
            pruned += prune_self_call_args_expr(rhs, func_name, arity);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_self_call_args_expr(expr, func_name, arity);
        }
        PreHirExpr::Index { base, index, .. } => {
            pruned += prune_self_call_args_expr(base, func_name, arity);
            pruned += prune_self_call_args_expr(index, func_name, arity);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_self_call_args_expr(cond, func_name, arity);
            pruned += prune_self_call_args_expr(then_expr, func_name, arity);
            pruned += prune_self_call_args_expr(else_expr, func_name, arity);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    pruned
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
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: Some(1),
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![param_ty],
                param_surface_type_names: vec![surface.map(str::to_string)],
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
                min_arity: fixed_arity,
                max_arity: fixed_arity,
                locked_exact_arity: None,
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown; fixed_arity],
                param_surface_type_names: vec![None; fixed_arity],
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
    fn callsite_type_prop_promotes_import_param_name_and_surface_type() {
        let mut func = PreHirFunction {
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
                        min_arity: 4,
                        max_arity: 4,
                        locked_exact_arity: Some(4),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown; 4],
                        param_surface_type_names: vec![None; 4],
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
            name: "fib".to_string(),
            int_param_offsets: Vec::new(),
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
                        min_arity: 0,
                        max_arity: 0,
                        locked_exact_arity: Some(0),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
                        min_arity: 1,
                        max_arity: 1,
                        locked_exact_arity: Some(1),
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![NirType::Unknown],
                        param_surface_type_names: vec![None],
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
            name: "caller".to_string(),
            int_param_offsets: Vec::new(),
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
                        min_arity: 0,
                        max_arity: 2,
                        locked_exact_arity: None,
                        returns_void: false,
                        return_lattice: NirType::Unknown,
                        param_lattices: vec![],
                        param_surface_type_names: vec![],
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
