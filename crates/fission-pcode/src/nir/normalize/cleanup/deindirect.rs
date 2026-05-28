//! ActionDeindirect — Fission HIR equivalent of Ghidra's `ActionDeindirect`.
//!
//! Ghidra source: `coreaction.hh` L206–213, `coreaction.cc` L9964–10041.
//!
//! ## Algorithm
//!
//! Ghidra's `ActionDeindirect` attempts to convert indirect calls (CALLIND) into
//! direct calls (CALL) when the target address of the call points to a constant
//! address whose symbol name is known in the function call summaries.
//!
//! ## Fission HIR mapping
//!
//! In Fission, when the builder is unable to statically resolve an indirect call
//! at materialization time, it creates an opaque call to `__fission_callind_opaque`
//! with the target function pointer expression (`fn_ptr`) as the first argument,
//! followed by any recovered arguments:
//!
//! ```text
//! __fission_callind_opaque(fn_ptr, args...)
//! ```
//!
//! This pass recursively traverses the HIR function body to find such calls and
//! analyzes the `fn_ptr` expression:
//! - If `fn_ptr` evaluates to a constant address (directly or via variable initializers),
//!   or is a direct global symbol pointer, it matches that address/symbol against
//!   `callee_summaries` to find the resolved symbol name.
//! - If a match is found, the call is rewritten to a direct call to the target symbol:
//!   ```text
//!   resolved_symbol(args...)
//!   ```

use super::super::*;
use std::collections::HashMap;

/// Traverse and statically resolve indirect calls in a function.
///
/// Returns `true` if any indirect calls were successfully rewritten to direct calls.
pub(crate) fn apply_deindirect_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // 1. Gather all local variable initializers representing constants or global symbol addresses.
    // This allows resolving variables whose values are established at local definition sites.
    let mut const_initializers = HashMap::<String, HirExpr>::new();
    for local in &func.locals {
        if let Some(initializer) = &local.initializer {
            match initializer {
                HirExpr::Const(val, ty) => {
                    const_initializers.insert(local.name.clone(), HirExpr::Const(*val, ty.clone()));
                }
                HirExpr::Cast { expr, .. } => {
                    if let HirExpr::Const(val, ty) = expr.as_ref() {
                        const_initializers.insert(local.name.clone(), HirExpr::Const(*val, ty.clone()));
                    }
                }
                HirExpr::AddressOfGlobal(global_name) => {
                    const_initializers.insert(local.name.clone(), HirExpr::AddressOfGlobal(global_name.clone()));
                }
                _ => {}
            }
        }
    }

    // 2. Build address-to-symbol mapping from the callee summaries in the function context.
    let mut addr_to_symbol = HashMap::<u64, String>::new();
    for (symbol, summary) in &func.callee_summaries {
        if let Some(addr) = summary.target.address {
            addr_to_symbol.insert(addr, symbol.clone());
        }
    }

    // 3. Recursively traverse and rewrite calls in function statements.
    for stmt in &mut func.body {
        changed |= deindirect_in_stmt(stmt, &const_initializers, &addr_to_symbol);
    }

    changed
}

fn deindirect_in_stmt(
    stmt: &mut HirStmt,
    initializers: &HashMap<String, HirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= deindirect_in_lvalue(lhs, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(rhs, initializers, addr_to_symbol);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= deindirect_in_expr(va_list, initializers, addr_to_symbol);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= deindirect_in_expr(expr, initializers, addr_to_symbol);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. } => {
            for s in body.iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        HirStmt::For {
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
            for s in body.iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= deindirect_in_expr(cond, initializers, addr_to_symbol);
            for s in then_body.iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
            for s in else_body.iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= deindirect_in_expr(expr, initializers, addr_to_symbol);
            for case in cases.iter_mut() {
                for s in case.body.iter_mut() {
                    changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
                }
            }
            for s in default.iter_mut() {
                changed |= deindirect_in_stmt(s, initializers, addr_to_symbol);
            }
        }
        _ => {}
    }
    changed
}

fn deindirect_in_lvalue(
    lhs: &mut HirLValue,
    initializers: &HashMap<String, HirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => deindirect_in_expr(ptr, initializers, addr_to_symbol),
        HirLValue::Index { base, index, .. } => {
            let mut changed = deindirect_in_expr(base, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(index, initializers, addr_to_symbol);
            changed
        }
        HirLValue::FieldAccess { base, .. } => deindirect_in_expr(base, initializers, addr_to_symbol),
    }
}

fn deindirect_in_expr(
    expr: &mut HirExpr,
    initializers: &HashMap<String, HirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> bool {
    let mut changed = false;

    // 1. Process sub-expressions first to normalize targets nested in complexes.
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= deindirect_in_expr(inner, initializers, addr_to_symbol);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= deindirect_in_expr(lhs, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(rhs, initializers, addr_to_symbol);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            changed |= deindirect_in_expr(cond, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(then_expr, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(else_expr, initializers, addr_to_symbol);
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= deindirect_in_expr(base, initializers, addr_to_symbol);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= deindirect_in_expr(base, initializers, addr_to_symbol);
            changed |= deindirect_in_expr(index, initializers, addr_to_symbol);
        }
        HirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= deindirect_in_expr(arg, initializers, addr_to_symbol);
            }
        }
        HirExpr::Load { ptr, .. } => {
            changed |= deindirect_in_expr(ptr, initializers, addr_to_symbol);
        }
        _ => {}
    }

    // 2. Try to resolve __fission_callind_opaque in the current expression.
    if let HirExpr::Call { target, args, ty } = expr {
        if target == "__fission_callind_opaque" && !args.is_empty() {
            let fn_ptr = &args[0];
            if let Some(resolved_symbol) = resolve_call_target(fn_ptr, initializers, addr_to_symbol) {
                // Found a static direct target symbol. Re-write the call!
                let remaining_args = args[1..].to_vec();
                *expr = HirExpr::Call {
                    target: resolved_symbol,
                    args: remaining_args,
                    ty: ty.clone(),
                };
                changed = true;
            }
        }
    }

    changed
}

fn resolve_call_target(
    fn_ptr: &HirExpr,
    initializers: &HashMap<String, HirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> Option<String> {
    match fn_ptr {
        // Direct constant address.
        HirExpr::Const(addr, _) => {
            let target_addr = *addr as u64;
            if let Some(symbol) = addr_to_symbol.get(&target_addr) {
                return Some(symbol.clone());
            }
            if target_addr > 0 {
                return Some(format!("sub_{:x}", target_addr));
            }
        }
        // Pointer cast const.
        HirExpr::Cast { expr: inner, .. } => {
            return resolve_call_target(inner, initializers, addr_to_symbol);
        }
        // Direct global symbol.
        HirExpr::AddressOfGlobal(symbol_name) => {
            return Some(symbol_name.clone());
        }
        // Load through a known IAT slot address: *(IAT_addr).
        // This is the pattern emitted by x86-64 Sleigh for `CALL qword ptr [IAT_addr]`
        // where the IAT slot address is a statically known constant.
        HirExpr::Load { ptr, .. } => {
            if let HirExpr::Const(iat_addr, _) = ptr.as_ref() {
                let slot_addr = *iat_addr as u64;
                if let Some(symbol) = addr_to_symbol.get(&slot_addr) {
                    return Some(symbol.clone());
                }
            }
        }
        // Local variable reference, trace to its definition initializer.
        HirExpr::Var(var_name) => {
            if let Some(init_expr) = initializers.get(var_name) {
                if let HirExpr::Var(next_var) = init_expr {
                    if next_var == var_name {
                        return None;
                    }
                }
                return resolve_call_target(init_expr, initializers, addr_to_symbol);
            }
            // Parse temporary register variables if they carry inline address hints in their names.
            if let Some(addr) = parse_address_from_name(var_name) {
                if let Some(symbol) = addr_to_symbol.get(&addr) {
                    return Some(symbol.clone());
                }
                return Some(format!("sub_{:x}", addr));
            }
        }
        _ => {}
    }
    None
}

fn parse_address_from_name(name: &str) -> Option<u64> {
    let raw = name
        .strip_prefix("tmp_")
        .or_else(|| name.strip_prefix("DAT_"))?;
    u64::from_str_radix(raw.trim_start_matches("0x"), 16).ok()
}
