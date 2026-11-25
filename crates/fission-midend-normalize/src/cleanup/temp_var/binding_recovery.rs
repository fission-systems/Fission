use crate::HashSet;
use crate::prelude::*;

fn is_rescue_candidate_name(name: &str) -> bool {
    if name.starts_with("iVar")
        || name.starts_with("uVar")
        || name.starts_with("bVar")
        || name.starts_with("xVar")
    {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
    } else if name.starts_with("tmp_") {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if let Some(suffix) = name.strip_prefix("local_") {
        // Stack-home surface names (`local_0`, `local_4`, `local_1c`) from
        // materialize. Used but undeclared → compile_error in semantic harness.
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if matches!(name, "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_") {
        // Named EFLAGS bits (SLA 0x200 layout). Prefer dead-flag cleanup; if a
        // live use remains, declare as Bool so the C harness compiles.
        true
    } else if is_simd_register_name(name) {
        // RegisterNamer spells vector registers and their scalar lanes as
        // `xmmN`, `xmmN_<lane>`, etc. Builder paths can leave one of these
        // names in the body without a binding; keep declaration closure
        // aligned with the same stable hardware-name grammar.
        true
    } else if name.starts_with('r') || name.starts_with('e') {
        name != "reg" && name != "rsp" && name != "rbp" && name != "esp" && name != "ebp"
    } else {
        false
    }
}

fn is_simd_register_name(name: &str) -> bool {
    let Some(suffix) = ["xmm", "ymm", "zmm"]
        .iter()
        .find_map(|family| name.strip_prefix(family))
    else {
        return false;
    };
    let index_end = suffix
        .bytes()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(suffix.len());
    if index_end == 0 {
        return false;
    }
    let lane_suffix = &suffix[index_end..];
    if lane_suffix.is_empty() {
        return true;
    }
    lane_suffix.strip_prefix('_').is_some_and(|lane| {
        !lane.is_empty()
            && lane.split('_').all(|segment| {
                !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
    })
}

pub fn rescue_undeclared_bindings(func: &mut PreHirFunction) -> bool {
    use fission_midend_prehir::util::expr_type;

    let mut declared: HashSet<String> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|b| b.name.clone())
        .collect();

    // Collect every variable name that appears anywhere in the body.
    let mut body_names: HashSet<String> = HashSet::default();
    collect_all_body_names_stmts(&func.body, &mut body_names);

    // Find undeclared names and try to infer their type from the first
    // assignment RHS in the body. Sort before appending so rescuing multiple
    // lane aliases cannot make rendered declaration order depend on HashSet
    // iteration order.
    // A file-scope object is declared at file scope, not here. The register
    // test below admits any name starting with `r` or `e`, so `rl_point`
    // and `expanding_redir` became uninitialised locals and their reads
    // and writes stopped reaching the globals.
    let global_names = crate::pipeline::GLOBAL_SYMBOL_CONTEXT.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|context| {
                context
                    .names
                    .values()
                    .chain(context.storage_names.iter())
                    .cloned()
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default()
    });
    let mut undeclared_candidates = body_names
        .into_iter()
        .filter(|name| !declared.contains(name.as_str()))
        .filter(|name| !global_names.contains(name.as_str()))
        .filter(|name| is_rescue_candidate_name(name))
        .collect::<Vec<_>>();
    undeclared_candidates.sort_unstable();

    let mut changed = false;
    for name in undeclared_candidates {
        let inferred_ty = if matches!(
            name.as_str(),
            "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_"
        ) {
            NirType::Bool
        } else {
            infer_type_from_first_assign(&func.body, &name)
        };
        func.locals.push(PreHirBinding {
            name: name.clone(),
            ty: inferred_ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        declared.insert(name);
        changed = true;
    }
    changed
}

pub(super) fn collect_all_body_names_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
        PreHirExpr::Unary { expr, .. } | PreHirExpr::Cast { expr, .. } => {
            collect_all_body_names_expr(expr, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_all_body_names_expr(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        PreHirExpr::Call { target, args, .. } => {
            // target is a function name String, not PreHirExpr.
            for arg in args {
                collect_all_body_names_expr(arg, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_expr(then_expr, out);
            collect_all_body_names_expr(else_expr, out);
        }
        PreHirExpr::Load { ptr, .. } => {
            collect_all_body_names_expr(ptr, out);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        PreHirExpr::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            collect_all_body_names_expr(src, out);
        }
    }
}

fn collect_all_body_names_lvalue(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(name) => {
            out.insert(name.clone());
        }
        PreHirLValue::Deref { ptr, .. } => collect_all_body_names_expr(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
    }
}

fn collect_all_body_names_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_all_body_names_lvalue(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            collect_all_body_names_expr(va_list, out);
        }
        PreHirStmt::Return(Some(expr)) => collect_all_body_names_expr(expr, out),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
            collect_all_body_names_stmts(body, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_all_body_names_stmts(body, out);
            collect_all_body_names_expr(cond, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_all_body_names_stmt(init, out);
            }
            if let Some(cond) = cond {
                collect_all_body_names_expr(cond, out);
            }
            if let Some(update) = update {
                collect_all_body_names_stmt(update, out);
            }
            collect_all_body_names_stmts(body, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_stmts(then_body, out);
            collect_all_body_names_stmts(else_body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_all_body_names_expr(expr, out);
            for case in cases {
                collect_all_body_names_stmts(&case.body, out);
            }
            collect_all_body_names_stmts(default, out);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_all_body_names_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_all_body_names_stmt(stmt, out);
    }
}

/// Try to infer the type of a variable from its first assignment RHS in the body.
fn infer_type_from_first_assign(stmts: &[PreHirStmt], name: &str) -> NirType {
    use fission_midend_prehir::util::expr_type;
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return ty;
        }
    }
    NirType::Unknown
}

fn infer_type_from_stmt(stmt: &PreHirStmt, name: &str) -> Option<NirType> {
    use fission_midend_prehir::util::expr_type;
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == name => {
            let ty = expr_type(rhs);
            Some(if ty == NirType::Unknown {
                NirType::Int {
                    bits: 32,
                    signed: true,
                }
            } else {
                ty
            })
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
            infer_type_from_first_assign_stmts(body, name)
        }
        PreHirStmt::DoWhile { body, .. } => infer_type_from_first_assign_stmts(body, name),
        PreHirStmt::For { body, .. } => infer_type_from_first_assign_stmts(body, name),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => infer_type_from_first_assign_stmts(then_body, name)
            .or_else(|| infer_type_from_first_assign_stmts(else_body, name)),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                if let Some(ty) = infer_type_from_first_assign_stmts(&case.body, name) {
                    return Some(ty);
                }
            }
            infer_type_from_first_assign_stmts(default, name)
        }
        _ => None,
    }
}

fn infer_type_from_first_assign_stmts(stmts: &[PreHirStmt], name: &str) -> Option<NirType> {
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return Some(ty);
        }
    }
    None
}
