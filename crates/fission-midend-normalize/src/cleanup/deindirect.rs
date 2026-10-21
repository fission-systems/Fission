//! ActionDeindirect — Fission HIR equivalent of Ghidra's `ActionDeindirect`.
//!
//! Ghidra source: `coreaction.hh` L206–213, `coreaction.cc` L9964–10041.
//!
//! Fission materializes unresolved indirect calls as
//! `__fission_callind_opaque(fn_ptr, args...)`. This pass rewrites one to a
//! direct call only when the target can be tied to a known callee summary.
//! Unique-space temporary names and data-symbol names are not addresses.

use std::collections::HashSet;

use crate::HashMap;
use crate::prelude::*;

/// Traverse and statically resolve indirect calls in a function.
///
/// Returns `true` if any indirect calls were successfully rewritten to direct calls.
pub fn apply_deindirect_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;

    // A declaration initializer is not a reaching definition once the local
    // is assigned elsewhere. Without statement-order SSA here, fail closed:
    // retain initializer facts only for locals that are never reassigned.
    let mut reassigned = HashSet::new();
    collect_assigned_vars(&func.body, &mut reassigned);

    let mut const_initializers = HashMap::<String, PreHirExpr>::default();
    for local in &func.locals {
        if reassigned.contains(&local.name) {
            continue;
        }
        if let Some(initializer) = &local.initializer {
            match initializer {
                PreHirExpr::Const(val, ty) => {
                    const_initializers
                        .insert(local.name.clone(), PreHirExpr::Const(*val, ty.clone()));
                }
                PreHirExpr::Cast { expr, .. } => {
                    if let PreHirExpr::Const(val, ty) = expr.as_ref() {
                        const_initializers
                            .insert(local.name.clone(), PreHirExpr::Const(*val, ty.clone()));
                    }
                }
                PreHirExpr::AddressOfGlobal(global_name) => {
                    const_initializers.insert(
                        local.name.clone(),
                        PreHirExpr::AddressOfGlobal(global_name.clone()),
                    );
                }
                _ => {}
            }
        }
    }

    // Known executable/function targets. An arbitrary numeric constant is not
    // enough evidence to synthesize `sub_<addr>`; it may name data or unmapped
    // memory. Callee summaries are the proof boundary this pass already owns.
    let mut addr_to_symbol = HashMap::<u64, String>::default();
    for (symbol, summary) in &func.callee_summaries {
        if let Some(addr) = summary.target.address {
            addr_to_symbol.insert(addr, symbol.clone());
        }
    }

    for stmt in &mut func.body {
        changed |= deindirect_in_stmt(stmt, &const_initializers, &addr_to_symbol);
    }

    changed
}

fn collect_assigned_vars(stmts: &[PreHirStmt], assigned: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => {
                if let PreHirLValue::Var(name) = lhs {
                    assigned.insert(name.clone());
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => collect_assigned_vars(body, assigned),
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_assigned_vars(std::slice::from_ref(init.as_ref()), assigned);
                }
                if let Some(update) = update {
                    collect_assigned_vars(std::slice::from_ref(update.as_ref()), assigned);
                }
                collect_assigned_vars(body, assigned);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assigned_vars(then_body, assigned);
                collect_assigned_vars(else_body, assigned);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_assigned_vars(&case.body, assigned);
                }
                collect_assigned_vars(default, assigned);
            }
            _ => {}
        }
    }
}

fn deindirect_in_stmt(
    stmt: &mut PreHirStmt,
    initializers: &HashMap<String, PreHirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= deindirect_in_lvalue(lhs, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(rhs, initializers, addr_to_symbol);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            changed |= deindirect_in_expr(va_list, initializers, addr_to_symbol);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= deindirect_in_expr(expr, initializers, addr_to_symbol);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        PreHirStmt::For {
            init,
            update,
            cond,
            body,
        } => {
            if let Some(s) = init {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
            if let Some(s) = update {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
            if let Some(c) = cond {
                changed |= deindirect_in_expr(c, initializers, addr_to_symbol);
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= deindirect_in_expr(cond, initializers, addr_to_symbol);
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body).iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body).iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= deindirect_in_expr(expr, initializers, addr_to_symbol);
            for case in cases.iter_mut() {
                for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body).iter_mut() {
                    changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
                }
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default).iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        _ => {}
    }
    changed
}

fn deindirect_in_lvalue(
    lhs: &mut PreHirLValue,
    initializers: &HashMap<String, PreHirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => deindirect_in_expr(ptr, initializers, addr_to_symbol),
        PreHirLValue::Index { base, index, .. } => {
            let mut changed = deindirect_in_expr(base, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(index, initializers, addr_to_symbol);
            changed
        }
        PreHirLValue::FieldAccess { base, .. } => {
            deindirect_in_expr(base, initializers, addr_to_symbol)
        }
    }
}

fn deindirect_in_expr(
    expr: &mut PreHirExpr,
    initializers: &HashMap<String, PreHirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    let mut changed = false;

    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            changed |= deindirect_in_expr(inner, initializers, addr_to_symbol);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= deindirect_in_expr(lhs, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(rhs, initializers, addr_to_symbol);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= deindirect_in_expr(cond, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(then_expr, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(else_expr, initializers, addr_to_symbol);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            changed |= deindirect_in_expr(base, initializers, addr_to_symbol);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= deindirect_in_expr(base, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(index, initializers, addr_to_symbol);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= deindirect_in_expr(arg, initializers, addr_to_symbol);
            }
        }
        PreHirExpr::Load { ptr, .. } => {
            changed |= deindirect_in_expr(ptr, initializers, addr_to_symbol);
        }
        _ => {}
    }

    if let PreHirExpr::Call { target, args, ty } = expr
        && target == "__fission_callind_opaque"
        && !args.is_empty()
    {
        let fn_ptr = &args[0];
        if let Some(resolved_symbol) = resolve_call_target(fn_ptr, initializers, addr_to_symbol) {
            let remaining_args = args[1..].to_vec();
            *expr = PreHirExpr::Call {
                target: resolved_symbol,
                args: remaining_args,
                ty: ty.clone(),
            };
            changed = true;
        }
    }

    changed
}

fn resolve_call_target(
    fn_ptr: &PreHirExpr,
    initializers: &HashMap<String, PreHirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> Option<String> {
    match fn_ptr {
        // A numeric constant becomes a direct call only if the analysis context
        // already knows that exact address as a callee/function target.
        PreHirExpr::Const(addr, _) => addr_to_symbol.get(&(*addr as u64)).cloned(),
        PreHirExpr::Cast { expr: inner, .. } => {
            resolve_call_target(inner, initializers, addr_to_symbol)
        }
        // `&symbol` is sufficient only when the symbol is also present among
        // known callees. This keeps `&DAT_xxx` and other data globals indirect.
        PreHirExpr::AddressOfGlobal(symbol_name) => addr_to_symbol
            .values()
            .any(|known| known == symbol_name)
            .then(|| symbol_name.clone()),
        // Load through a known IAT slot address: *(IAT_addr).
        PreHirExpr::Load { ptr, .. } => {
            if let PreHirExpr::Const(iat_addr, _) = ptr.as_ref() {
                return addr_to_symbol.get(&(*iat_addr as u64)).cloned();
            }
            None
        }
        PreHirExpr::Var(var_name) => {
            let init_expr = initializers.get(var_name)?;
            if matches!(init_expr, PreHirExpr::Var(next) if next == var_name) {
                return None;
            }
            resolve_call_target(init_expr, initializers, addr_to_symbol)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unknown_ty() -> NirType {
        NirType::Unknown
    }

    #[test]
    fn temporary_and_data_names_are_not_lexical_code_addresses() {
        let initializers = HashMap::default();
        let known = HashMap::default();
        assert_eq!(
            resolve_call_target(&PreHirExpr::Var("tmp_100".into()), &initializers, &known),
            None
        );
        assert_eq!(
            resolve_call_target(
                &PreHirExpr::Var("DAT_00405000".into()),
                &initializers,
                &known
            ),
            None
        );
    }

    #[test]
    fn constants_and_globals_require_known_callee_evidence() {
        let initializers = HashMap::default();
        let mut known = HashMap::default();
        known.insert(0x401000, "target".to_string());

        assert_eq!(
            resolve_call_target(
                &PreHirExpr::Const(0x401000, unknown_ty()),
                &initializers,
                &known
            ),
            Some("target".to_string())
        );
        assert_eq!(
            resolve_call_target(
                &PreHirExpr::Const(0x405000, unknown_ty()),
                &initializers,
                &known
            ),
            None
        );
        assert_eq!(
            resolve_call_target(
                &PreHirExpr::AddressOfGlobal("target".to_string()),
                &initializers,
                &known
            ),
            Some("target".to_string())
        );
        assert_eq!(
            resolve_call_target(
                &PreHirExpr::AddressOfGlobal("DAT_00405000".to_string()),
                &initializers,
                &known
            ),
            None
        );
    }

    #[test]
    fn assigned_locals_are_removed_from_initializer_facts() {
        let mut assigned = HashSet::new();
        collect_assigned_vars(
            &[PreHirStmt::Assign {
                lhs: PreHirLValue::Var("fn_ptr".to_string()),
                rhs: PreHirExpr::Const(0x402000, unknown_ty()),
            }],
            &mut assigned,
        );
        assert!(assigned.contains("fn_ptr"));
    }
}
