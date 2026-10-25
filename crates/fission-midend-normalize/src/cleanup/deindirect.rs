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

use crate::HashMap;
use crate::prelude::*;
use std::collections::HashSet;

/// Traverse and statically resolve indirect calls in a function.
///
/// Returns `true` if any indirect calls were successfully rewritten to direct calls.
pub fn apply_deindirect_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;

    // 1. Gather all variables that are assigned or mutated anywhere in the function body.
    // If a variable is reassigned in the body, its declaration initializer cannot be trusted
    // as an invariant value across indirect call sites.
    let mut modified_vars = HashSet::new();
    collect_assigned_vars(&func.body, &mut modified_vars);

    // 2. Gather all local variable initializers representing constants or global symbol addresses
    // for variables that are NEVER reassigned in the body.
    let mut const_initializers = HashMap::<String, PreHirExpr>::default();
    for local in &func.locals {
        if modified_vars.contains(&local.name) {
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

    // 3. Build address-to-symbol mapping from the callee summaries in the function context.
    let mut addr_to_symbol = HashMap::<u64, String>::default();
    for (symbol, summary) in &func.callee_summaries {
        if let Some(addr) = summary.target.address {
            addr_to_symbol.insert(addr, symbol.clone());
        }
    }

    // 4. Recursively traverse and rewrite calls in function statements.
    for stmt in &mut func.body {
        changed |= deindirect_in_stmt(stmt, &const_initializers, &addr_to_symbol);
    }

    changed
}

fn collect_assigned_vars(stmts: &[PreHirStmt], modified: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => match lhs {
                PreHirLValue::Var(name) => {
                    modified.insert(name.clone());
                }
                PreHirLValue::FieldAccess { base, .. } => {
                    if let PreHirExpr::Var(name) = base.as_ref() {
                        modified.insert(name.clone());
                    }
                }
                _ => {}
            },
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_assigned_vars(body, modified);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(s) = init {
                    collect_assigned_vars(std::slice::from_ref(s.as_ref()), modified);
                }
                if let Some(s) = update {
                    collect_assigned_vars(std::slice::from_ref(s.as_ref()), modified);
                }
                collect_assigned_vars(body, modified);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assigned_vars(then_body, modified);
                collect_assigned_vars(else_body, modified);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_assigned_vars(&case.body, modified);
                }
                collect_assigned_vars(default, modified);
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

    // 1. Process sub-expressions first to normalize targets nested in complexes.
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

    // 2. Try to resolve __fission_callind_opaque in the current expression.
    if let PreHirExpr::Call {
        target, args, ty, ..
    } = expr
    {
        if target == "__fission_callind_opaque" && !args.is_empty() {
            let fn_ptr = &args[0];
            if let Some(resolved_symbol) = resolve_call_target(fn_ptr, initializers, addr_to_symbol)
            {
                // Found a static direct target symbol. Re-write the call!
                let remaining_args = args[1..].to_vec();
                *expr = PreHirExpr::Call {
                    origin: None,
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
    fn_ptr: &PreHirExpr,
    initializers: &HashMap<String, PreHirExpr>,
    addr_to_symbol: &HashMap<u64, String>,
) -> Option<String> {
    match fn_ptr {
        // Direct constant address: must match a verified callee symbol in addr_to_symbol.
        PreHirExpr::Const(addr, _) => {
            let target_addr = *addr as u64;
            if let Some(symbol) = addr_to_symbol.get(&target_addr) {
                return Some(symbol.clone());
            }
        }
        // Pointer cast const.
        PreHirExpr::Cast { expr: inner, .. } => {
            return resolve_call_target(inner, initializers, addr_to_symbol);
        }
        // Direct global symbol.
        PreHirExpr::AddressOfGlobal(symbol_name) => {
            return Some(symbol_name.clone());
        }
        // Load through a known IAT slot address: *(IAT_addr).
        // This is the pattern emitted by x86-64 Sleigh for `CALL qword ptr [IAT_addr]`
        // where the IAT slot address is a statically known constant.
        PreHirExpr::Load { ptr, .. } => {
            if let PreHirExpr::Const(iat_addr, _) = ptr.as_ref() {
                let slot_addr = *iat_addr as u64;
                if let Some(symbol) = addr_to_symbol.get(&slot_addr) {
                    return Some(symbol.clone());
                }
            }
        }
        // Local variable reference, trace to its definition initializer if unmodified.
        // Note: tmp_ and DAT_ names are never parsed as code addresses; temporary varnode
        // offsets belong to the unique address space, and DAT_ variables reside in data sections.
        PreHirExpr::Var(var_name) => {
            if let Some(init_expr) = initializers.get(var_name) {
                if let PreHirExpr::Var(next_var) = init_expr {
                    if next_var == var_name {
                        return None;
                    }
                }
                return resolve_call_target(init_expr, initializers, addr_to_symbol);
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_callee(name: &str, addr: u64) -> (String, CallSummary) {
        (
            name.to_string(),
            CallSummary {
                target: CallTargetRef {
                    address: Some(addr),
                    symbol: name.to_string(),
                    provenance: CallTargetProvenance::Reference,
                    edge_kind: CallEdgeKind::Reference,
                    confidence: 128,
                },
                prototype: PrototypeSummary {
                    variadic_fixed_arity: None,
                    min_arity: 0,
                    max_arity: 0,
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
                    regions: Vec::new(),
                    wrapper_class: WrapperClass::None,
                    wrapper_of: None,
                    confidence: 160,
                },
            },
        )
    }

    #[test]
    fn deindirect_ignores_tmp_varnodes_and_dat_variables() {
        let mut summaries = IndexMap::new();
        let (name, summary) = make_callee("sub_100", 0x100);
        summaries.insert(name, summary);

        // Indirect call to tmp_100 (unique space temporary varnode)
        let mut func = PreHirFunction {
            name: "test_tmp".to_string(),
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "__fission_callind_opaque".to_string(),
                args: vec![PreHirExpr::Var("tmp_100".to_string())],
                ty: NirType::Unknown,
            })],
            callee_summaries: summaries.clone(),
            ..Default::default()
        };
        assert!(
            !apply_deindirect_pass(&mut func),
            "tmp_100 must not be treated as a code address"
        );

        // Indirect call to DAT_00405000 (data section variable)
        let mut func_dat = PreHirFunction {
            name: "test_dat".to_string(),
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "__fission_callind_opaque".to_string(),
                args: vec![PreHirExpr::Var("DAT_00405000".to_string())],
                ty: NirType::Unknown,
            })],
            callee_summaries: summaries,
            ..Default::default()
        };
        assert!(
            !apply_deindirect_pass(&mut func_dat),
            "DAT_ variables must not be invoked directly as code"
        );
    }

    #[test]
    fn deindirect_ignores_reassigned_local_variable() {
        let mut summaries = IndexMap::new();
        let (name, summary) = make_callee("func_A", 0x401000);
        summaries.insert(name, summary);

        // fn_ptr is declared with initializer func_A (0x401000), but reassigned in body
        let mut func = PreHirFunction {
            name: "test_reassign".to_string(),
            locals: vec![PreHirBinding {
                name: "fn_ptr".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: Some(PreHirExpr::Const(
                    0x401000,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
            }],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("fn_ptr".to_string()),
                    rhs: PreHirExpr::Var("runtime_ptr".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "__fission_callind_opaque".to_string(),
                    args: vec![PreHirExpr::Var("fn_ptr".to_string())],
                    ty: NirType::Unknown,
                }),
            ],
            callee_summaries: summaries,
            ..Default::default()
        };

        assert!(
            !apply_deindirect_pass(&mut func),
            "reassigned local variable must not be resolved using stale initializer"
        );
    }

    #[test]
    fn deindirect_does_not_synthesize_unverified_sub_address() {
        // Constant 0x500000 is NOT in callee_summaries; it must not synthesize sub_500000
        let mut func = PreHirFunction {
            name: "test_unverified".to_string(),
            body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "__fission_callind_opaque".to_string(),
                args: vec![PreHirExpr::Const(
                    0x500000,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )],
                ty: NirType::Unknown,
            })],
            ..Default::default()
        };

        assert!(
            !apply_deindirect_pass(&mut func),
            "unverified constant address must not be synthesized into sub_<addr>"
        );
    }
}
