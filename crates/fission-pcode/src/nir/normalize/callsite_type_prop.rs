/// Call-site inter-procedural type propagation pass.
///
/// All type inference so far has been intra-procedural: it only sees the types
/// of expressions *within* the current function.  `call malloc(size)` still
/// returns `Ptr(Unknown)`, `memcpy(dst, src, n)` arguments stay `Unknown`.
///
/// This pass connects the existing `fission-signatures` Windows API database
/// to the Fission type inference pipeline:
///
/// 1. Walk every `HirStmt::Assign { rhs: Call { target, args } }` and
///    `HirStmt::Expr(Call { target, args })`.
/// 2. Look up `target` in `fission_signatures::win_api::WIN_API_DB`.
/// 3. For the return value: if there is a receiver binding (the lhs `Var` of
///    the Assign), update `NirBinding.ty` to the resolved return type.
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
use super::*;
use fission_signatures::win_api::WIN_API_DB;

/// Convert a Windows API type name string to a `NirType`, or `None` for
/// unconstrained types (void, variadic, …).
pub(super) fn win_type_name_to_nir(name: &str) -> Option<NirType> {
    // Strip leading/trailing whitespace and trailing `*` for pointer types.
    let name = name.trim();

    // Pointer types first.
    if name.ends_with('*') {
        let inner_name = name.trim_end_matches('*').trim();
        let inner = match inner_name {
            "VOID" | "void" | "" => NirType::Unknown,
            "CHAR" | "char" => NirType::Int { bits: 8, signed: true },
            "WCHAR" | "wchar_t" | "TCHAR" => NirType::Int { bits: 16, signed: false },
            "BYTE" | "UCHAR" => NirType::Int { bits: 8, signed: false },
            _ => NirType::Unknown,
        };
        return Some(NirType::Ptr(Box::new(inner)));
    }

    let nir = match name {
        // Void — no constraint.
        "void" | "VOID" => return None,
        // 32-bit unsigned integers.
        "DWORD" | "UINT" | "ULONG" | "UINT32" | "ULONG32" | "DWORD32" => {
            NirType::Int { bits: 32, signed: false }
        }
        // 32-bit signed integers.
        "INT" | "LONG" | "INT32" | "LONG32" => {
            NirType::Int { bits: 32, signed: true }
        }
        // BOOL is signed int32 in Windows ABI.
        "BOOL" => NirType::Int { bits: 32, signed: true },
        // 16-bit.
        "WORD" | "USHORT" | "UINT16" => NirType::Int { bits: 16, signed: false },
        "SHORT" | "INT16" => NirType::Int { bits: 16, signed: true },
        // 8-bit.
        "BYTE" | "UCHAR" | "UINT8" | "BOOLEAN" => NirType::Int { bits: 8, signed: false },
        "CHAR" | "INT8" => NirType::Int { bits: 8, signed: true },
        // 64-bit unsigned.
        "QWORD" | "UINT64" | "ULONG64" | "DWORD64" | "ULONGLONG" | "ULONG_PTR" | "SIZE_T"
        | "UINT_PTR" => NirType::Int { bits: 64, signed: false },
        // 64-bit signed.
        "LONGLONG" | "INT64" | "LONG64" | "LONG_PTR" | "SSIZE_T" | "INT_PTR" => {
            NirType::Int { bits: 64, signed: true }
        }
        // Generic pointer to void.
        "LPVOID" | "PVOID" | "HANDLE" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Typed string pointers.
        "LPSTR" | "LPCSTR" | "PSTR" | "PCSTR" => {
            NirType::Ptr(Box::new(NirType::Int { bits: 8, signed: false }))
        }
        "LPWSTR" | "LPCWSTR" | "PWSTR" | "PCWSTR" => {
            NirType::Ptr(Box::new(NirType::Int { bits: 16, signed: false }))
        }
        // Opaque Windows handle types — typed as Ptr to empty Aggregate.
        "HWND" | "HMODULE" | "HINSTANCE" | "HKEY" | "HFILE" | "HBITMAP"
        | "HBRUSH" | "HFONT" | "HPEN" | "HICON" | "HCURSOR" | "HMENU"
        | "HRGN" | "HDC" | "HGLOBAL" | "HLOCAL" | "HRSRC" | "HWINSTA"
        | "HDESK" | "HPALETTE" | "HENHMETAFILE" | "HMETAFILE"
        | "HCOLORSPACE" | "HCONV" | "HCONVLIST" | "HDDEDATA" | "HDDERESERVATION"
        | "HSZ" | "HHOOK" | "HMONITOR" | "HWINEVENTHOOK" | "HPOWERNOTIFY"
        | "SC_HANDLE" | "SERVICE_STATUS_HANDLE" => {
            NirType::Ptr(Box::new(NirType::Aggregate { size: 0, fields: vec![] }))
        }
        // NTSTATUS / HRESULT: signed 32-bit.
        "NTSTATUS" | "HRESULT" => NirType::Int { bits: 32, signed: true },
        // MSVC va_list (opaque; model as generic pointer).
        "va_list" => NirType::Ptr(Box::new(NirType::Unknown)),
        // Unknown / not yet mapped → no constraint.
        _ => return None,
    };
    Some(nir)
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
fn tighten_binding_ty(binding: &mut NirBinding, candidate: &NirType) -> bool {
    if binding.ty == *candidate { return false; }
    match (&binding.ty, candidate) {
        (NirType::Unknown, _) => { binding.ty = candidate.clone(); true }
        (NirType::Ptr(a), NirType::Ptr(b)) if **a == NirType::Unknown && **b != NirType::Unknown => {
            binding.ty = candidate.clone(); true
        }
        _ => false,
    }
}

/// Apply call-site type propagation to a function.
///
/// Collects all `Call` expressions, looks up each target in `WIN_API_DB`, and
/// updates argument/receiver bindings with the resolved types.
///
/// Returns `true` if any binding type was updated.
pub(super) fn apply_callsite_type_prop_pass(func: &mut HirFunction) -> bool {
    // Build a lookup map from binding name to index in func.locals / func.params.
    let mut changed = false;

    // Collect call sites: (receiver_name_opt, callee_name, arg_var_names)
    let mut callsites: Vec<(Option<String>, String, Vec<Option<String>>)> = Vec::new();
    collect_callsites_stmts(&func.body, &mut callsites);

    for (receiver, callee, arg_vars) in &callsites {
        let Some(sig) = WIN_API_DB.get(callee) else { continue; };

        // Resolve return type and update receiver binding.
        if let Some(ret_ty) = resolve_return_ty(&sig.return_type) {
            if let Some(recv_name) = receiver {
                if let Some(b) = binding_by_name_mut(&mut func.locals, recv_name)
                    .or_else(|| binding_by_name_mut(&mut func.params, recv_name))
                {
                    changed |= tighten_binding_ty(b, &ret_ty);
                }
            }
        }

        // Resolve each parameter type and update argument bindings.
        for (i, arg_var_opt) in arg_vars.iter().enumerate() {
            let Some(arg_var) = arg_var_opt else { continue; };
            let Some(param) = sig.params.get(i) else { break; };
            let Some(param_ty) = win_type_name_to_nir(&param.type_name) else { continue; };
            if let Some(b) = binding_by_name_mut(&mut func.locals, arg_var)
                .or_else(|| binding_by_name_mut(&mut func.params, arg_var))
            {
                changed |= tighten_binding_ty(b, &param_ty);
            }
        }
    }

    changed
}

fn binding_by_name_mut<'a>(bindings: &'a mut Vec<NirBinding>, name: &str) -> Option<&'a mut NirBinding> {
    bindings.iter_mut().find(|b| b.name == name)
}

/// Extract the plain variable name from a Call argument expression (if it's
/// `Var(x)` or `Cast(_, Var(x))`).  Returns `None` for complex expressions.
fn arg_var_name(expr: &HirExpr) -> Option<String> {
    match expr {
        HirExpr::Var(name) => Some(name.clone()),
        HirExpr::Cast { expr: inner, .. } => arg_var_name(inner),
        _ => None,
    }
}

fn collect_callsites_stmts(
    stmts: &[HirStmt],
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    for stmt in stmts {
        collect_callsites_stmt(stmt, out);
    }
}

fn collect_callsites_stmt(
    stmt: &HirStmt,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            if let HirExpr::Call { target, args, .. } = rhs {
                let recv = match lhs {
                    HirLValue::Var(name) => Some(name.clone()),
                    _ => None,
                };
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((recv, target.clone(), arg_vars));
            }
            // Also recurse in case call appears inside a more complex rhs.
            collect_callsites_expr(rhs, out);
        }
        HirStmt::Expr(expr) => {
            if let HirExpr::Call { target, args, .. } = expr {
                let arg_vars = args.iter().map(arg_var_name).collect();
                out.push((None, target.clone(), arg_vars));
            }
        }
        HirStmt::Return(Some(expr)) => collect_callsites_expr(expr, out),
        HirStmt::Block(body) => collect_callsites_stmts(body, out),
        HirStmt::If { cond, then_body, else_body } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(then_body, out);
            collect_callsites_stmts(else_body, out);
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_callsites_expr(cond, out);
            collect_callsites_stmts(body, out);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init { collect_callsites_stmt(i, out); }
            if let Some(c) = cond { collect_callsites_expr(c, out); }
            if let Some(u) = update { collect_callsites_stmt(u, out); }
            collect_callsites_stmts(body, out);
        }
        HirStmt::Switch { expr, cases, default } => {
            collect_callsites_expr(expr, out);
            for case in cases { collect_callsites_stmts(&case.body, out); }
            collect_callsites_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_callsites_expr(
    expr: &HirExpr,
    out: &mut Vec<(Option<String>, String, Vec<Option<String>>)>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            let arg_vars = args.iter().map(arg_var_name).collect();
            out.push((None, target.clone(), arg_vars));
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_callsites_expr(lhs, out);
            collect_callsites_expr(rhs, out);
        }
        HirExpr::Cast { expr: inner, .. } | HirExpr::Unary { expr: inner, .. } => {
            collect_callsites_expr(inner, out);
        }
        HirExpr::Load { ptr, .. } => collect_callsites_expr(ptr, out),
        HirExpr::PtrOffset { base, .. } => collect_callsites_expr(base, out),
        HirExpr::Index { base, index, .. } => {
            collect_callsites_expr(base, out);
            collect_callsites_expr(index, out);
        }
        HirExpr::AggregateCopy { src, .. } => collect_callsites_expr(src, out),
        _ => {}
    }
}
