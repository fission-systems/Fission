//! Call printing helpers: indirect-call targets, argument and parameter types,
//! Win32 enum annotations.

use super::*;

/// Callee symbol behind an opaque indirect call, when it is a named import.
pub(super) fn callind_target_symbol(fn_ptr: &HirExpr) -> Option<String> {
    match fn_ptr {
        HirExpr::Var(name) => Some(name.clone()),
        HirExpr::Cast { expr, .. } => callind_target_symbol(expr),
        _ => None,
    }
}

/// Infer the written argument type only when it agrees with the printer's
/// treatment of the operand. The HIR type alone can be a storage integer
/// even when a binding's surface declaration or pointer arithmetic prints a pointer.
pub(in crate::render) fn call_argument_type<'a>(
    func: &'a HirFunction,
    global_names: &'a HashMap<u64, String>,
    profile: PrintProfile,
) -> impl Fn(&HirExpr) -> Option<String> + 'a {
    let mut ctx = PrintCtx::build_with_profile(func, profile);
    ctx.global_names = Some(global_names);
    move |arg| {
        let ty = match arg {
            HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) if name.starts_with('"') => {
                return Some("const char *".into());
            }
            HirExpr::Const(value, ty) => {
                if let Some(name) = ctx.constant_name(*value) {
                    // A relocation or string can print here in place of the
                    // integer literal. Only string names also tell us its type.
                    return name.starts_with('"').then(|| "const char *".into());
                }
                ty.clone()
            }
            HirExpr::Var(name) | HirExpr::AddressOfLocal(name) => {
                let ty = *ctx.var_types.get(name.as_str())?;
                let declared = ctx.decl_type_text.get(name.as_str())?;
                if *declared != print_type(ty) {
                    // An alias or surface override can change the pointee and
                    // even pointer depth. They cannot be inferred from storage.
                    return None;
                }
                if matches!(arg, HirExpr::AddressOfLocal(_)) {
                    NirType::Ptr(Box::new(ty.clone()))
                } else {
                    ty.clone()
                }
            }
            _ => expr_type(arg),
        };
        if ctx.operand_prints_as_pointer(arg) && !matches!(ty, NirType::Ptr(_)) {
            return None;
        }
        c_parameter_type(&ty)
    }
}

pub(super) fn c_parameter_type(ty: &NirType) -> Option<String> {
    // Declarations stand on their own without the decompiler's shorthand
    // typedefs. Unknown or unrepresentable arguments leave the list open.
    let name = match ty {
        NirType::Unknown => return None,
        NirType::Bool => "_Bool",
        NirType::Int {
            bits: 8,
            signed: false,
        } => "unsigned char",
        NirType::Int {
            bits: 8,
            signed: true,
        } => "char",
        NirType::Int {
            bits: 16,
            signed: false,
        } => "unsigned short",
        NirType::Int {
            bits: 32,
            signed: false,
        } => "unsigned int",
        NirType::Int {
            bits: 16 | 32 | 64 | 128,
            ..
        }
        | NirType::Aggregate { .. }
        | NirType::Float { bits: 32 | 64 | 80 } => return Some(print_type(ty)),
        NirType::Ptr(inner) if **inner == NirType::Unknown => "void *",
        NirType::Ptr(inner) => return c_parameter_type(inner).map(|name| format!("{name} *")),
        NirType::Int { .. } | NirType::Float { .. } => return None,
    };
    Some(name.to_string())
}

/// Append the constant's Win32 name to a call argument, when win32metadata
/// says that parameter carries an enum group and the value resolves in it.
///
/// The name goes in a comment rather than replacing the literal.
/// `PAGE_EXECUTE_READWRITE` is not declared in the recompilation harness's
/// translation unit, so emitting it bare fails to build where `0x40` builds --
/// and recompilation is one of the three things this output is scored on.
/// A comment keeps both.
///
/// Only a literal argument is annotated. A variable holding the value would
/// need the definition chased to know what reaches the call, and guessing there
/// would attach a name to something that is not that constant.
pub(super) fn annotate_enum_argument(
    target: &str,
    index: usize,
    arg: &HirExpr,
    printed: String,
) -> String {
    let HirExpr::Const(value, _) = arg else {
        return printed;
    };
    // An imported call names its target `KERNEL32.dll!VirtualAlloc`; the
    // databases are keyed by the bare symbol.
    let target = fission_signatures::symbol_for_win_api_database_lookup(target).unwrap_or(target);
    let Some(signature) = fission_signatures::SIGNATURE_RESOURCES.api_signature(target) else {
        return printed;
    };
    let Some(param) = signature.params.get(index) else {
        return printed;
    };
    match fission_signatures::WIN_CONSTANTS_DB.resolve_parameter(target, &param.name, *value as u64)
    {
        Some(name) => format!("{printed} /* {name} */"),
        None => printed,
    }
}

pub(super) fn print_callable_target(
    target: &str,
    args: &[HirExpr],
    return_ty: &NirType,
    ctx: Option<&PrintCtx<'_>>,
) -> String {
    let Some(target_expr) = target
        .strip_prefix("((code *)")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return if ctx.is_none_or(|ctx| ctx.sanitize_symbols) || target.starts_with("__pcodeop_") {
            sanitize_c_identifier(target)
        } else {
            target.to_string()
        };
    };
    let ret_ty = if matches!(return_ty, NirType::Unknown) {
        ctx.map(|ctx| ctx.return_type).unwrap_or(return_ty)
    } else {
        return_ty
    };
    if matches!(ret_ty, NirType::Unknown) {
        return target.to_string();
    }
    let arg_types = args
        .iter()
        .map(|arg| callable_arg_type_name(arg, ctx))
        .collect::<Vec<_>>()
        .join(", ");
    format!("(({} (*)({arg_types})){target_expr})", print_type(ret_ty))
}

pub(super) fn callable_arg_type_name(arg: &HirExpr, ctx: Option<&PrintCtx<'_>>) -> String {
    if let Some(ctx) = ctx
        && let HirExpr::Var(name) = arg
        && let Some(ty) = ctx.var_types.get(name.as_str())
    {
        return print_type(ty);
    }
    match arg {
        HirExpr::Const(_, ty)
        | HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Select { ty, .. } => {
            if matches!(ty, NirType::Unknown) {
                "uint".to_string()
            } else {
                print_type(ty)
            }
        }
        HirExpr::Index { elem_ty, .. } => print_type(elem_ty),
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::PtrOffset { .. }
        | HirExpr::AggregateCopy { .. } => "uint".to_string(),
    }
}
