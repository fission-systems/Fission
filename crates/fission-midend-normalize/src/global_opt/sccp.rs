//! Sparse conditional constant propagation (SCCP) on structured HIR.
//!
//! Tracks a lattice of `Var → (i64, NirType)` along straight-line flow, merges at
//! `if`/`switch` joins, and conservatively drops variables written in loop bodies
//! from the map after `while`/`for`/`do-while`.  This complements
//! [`super::super::analysis::defuse::constant_folding_pass`] (single-statement fold) and VSA
//! [`fission_midend_prehir::vsa::jump_resolver`] (intervals, not a constant lattice).

use super::super::analysis::defuse::{eval_hir_expr_with_const_env, fold_expr_hir};
use super::super::cleanup::expr_has_side_effects;
use super::super::pipeline::is_large_hir_function;
use crate::prelude::*;
use crate::{HashMap, HashSet};
use std::collections::VecDeque;

type ConstEnv = HashMap<String, (i64, NirType)>;

pub fn apply_sccp_pass(func: &mut PreHirFunction) -> bool {
    let max_rounds = if is_large_hir_function(func) { 2 } else { 8 };
    let goto_targets = collect_goto_targets(&func.body);
    let mut all_xvars = HashSet::default();
    collect_xvars_in_stmts(&func.body, &mut all_xvars);
    let mut any = false;
    for _ in 0..max_rounds {
        let mut env = ConstEnv::default();
        let address_aliases = collect_function_address_aliases(func);
        if !sccp_transform_stmts(
            &mut func.body,
            &mut env,
            &goto_targets,
            &all_xvars,
            &address_aliases,
        ) {
            break;
        }
        any = true;
    }
    any
}

fn collect_goto_targets(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut targets = HashSet::default();
    for stmt in stmts {
        collect_goto_targets_stmt(stmt, &mut targets);
    }
    targets
}

fn collect_goto_targets_stmt(stmt: &PreHirStmt, targets: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Goto(label) => {
            targets.insert(label.clone());
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(s) = init.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
            for s in body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
            if let Some(s) = update.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
            for s in else_body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    collect_goto_targets_stmt(s, targets);
                }
            }
            for s in default.iter() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        _ => {}
    }
}

fn merge_env(a: &ConstEnv, b: &ConstEnv) -> ConstEnv {
    let keys: HashSet<_> = a.keys().chain(b.keys()).cloned().collect();
    let mut out = ConstEnv::default();
    for k in keys {
        match (a.get(&k), b.get(&k)) {
            (Some(ca), Some(cb)) if ca == cb => {
                out.insert(k, ca.clone());
            }
            _ => {}
        }
    }
    out
}

fn env_without_vars(
    env: &ConstEnv,
    vars: &HashSet<String>,
    address_aliases: &AddressAliases,
) -> ConstEnv {
    let mut out = env.clone();
    for var in vars {
        out.remove(var);
    }
    if vars.contains(UNKNOWN_FRAME_ADDRESS) {
        address_aliases.forget_frame_storage(&mut out);
    }
    out
}

fn stmt_list_has_side_effects(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(stmt_has_side_effects)
}

/// Whether `branch` defines a label some `Goto` elsewhere in the function
/// still targets. GCC loop-rotation emits `if (precheck) { while (1) { L:
/// ... } }` where the real entry is an unconditional `goto L` from a
/// different scope, landing straight past this guard -- once constant
/// propagation reduces `precheck` to a literal, discarding the branch that
/// contains `L` would erase it out from under that goto, even though
/// nothing in `branch` itself makes the discard obviously unsafe (no side
/// effects visible from here). `goto_targets` is the whole-function
/// referenced-label set collected once in `apply_sccp_pass`, so this sees
/// cross-scope references a purely local scan would miss.
fn branch_defines_goto_target(branch: &[PreHirStmt], goto_targets: &HashSet<String>) -> bool {
    super::super::cleanup::utils::collect_defined_labels(branch)
        .iter()
        .any(|label| goto_targets.contains(label))
}

fn stmt_has_side_effects(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        PreHirStmt::Block(body) => stmt_list_has_side_effects(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => stmt_list_has_side_effects(then_body) || stmt_list_has_side_effects(else_body),
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => stmt_list_has_side_effects(body),
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmt_list_has_side_effects(&c.body))
                || stmt_list_has_side_effects(default)
        }
        PreHirStmt::VaStart { .. } => true,
        _ => false,
    }
}

fn lvalue_has_side_effects(lhs: &PreHirLValue) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_has_side_effects(ptr),
        PreHirLValue::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_has_side_effects(base),
    }
}

fn loop_variant_vars(
    body: &[PreHirStmt],
    all_xvars: &HashSet<String>,
    address_aliases: &AddressAliases,
) -> HashSet<String> {
    let mut vars = HashSet::default();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
        collect_escaped_and_indirect_in_stmt(stmt, address_aliases, &mut vars);
    }
    for xvar in all_xvars {
        vars.insert(xvar.clone());
    }
    vars
}

fn collect_escaped_and_indirect_in_lvalue(
    lhs: &PreHirLValue,
    address_aliases: &AddressAliases,
    out: &mut HashSet<String>,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => {
            collect_call_escaped_vars_in_expr(ptr, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(ptr, address_aliases, out);
        }
        PreHirLValue::Index { base, index, .. } => {
            collect_call_escaped_vars_in_expr(base, address_aliases, out);
            collect_call_escaped_vars_in_expr(index, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(base, address_aliases, out);
            if let Some(root) = expr_root_var_sccp(base) {
                out.insert(root.to_string());
            }
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_call_escaped_vars_in_expr(base, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(base, address_aliases, out);
            if let Some(root) = expr_root_var_sccp(base) {
                out.insert(root.to_string());
            }
        }
    }
}

fn collect_escaped_and_indirect_in_stmt(
    stmt: &PreHirStmt,
    address_aliases: &AddressAliases,
    out: &mut HashSet<String>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_escaped_and_indirect_in_lvalue(lhs, address_aliases, out);
            collect_call_escaped_vars_in_expr(rhs, address_aliases, out);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            collect_call_escaped_vars_in_expr(va_list, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(va_list, address_aliases, out);
            if let PreHirExpr::Var(name) = va_list {
                out.insert(name.clone());
            }
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_call_escaped_vars_in_expr(expr, address_aliases, out);
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            collect_call_escaped_vars_in_expr(cond, address_aliases, out);
            for s in body.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_call_escaped_vars_in_expr(cond, address_aliases, out);
            for s in then_body.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
            for s in else_body.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
            if let Some(c) = cond {
                collect_call_escaped_vars_in_expr(c, address_aliases, out);
            }
            for s in body.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
            if let Some(s) = update {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_call_escaped_vars_in_expr(expr, address_aliases, out);
            for case in cases {
                for s in case.body.iter() {
                    collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
                }
            }
            for s in default.iter() {
                collect_escaped_and_indirect_in_stmt(s, address_aliases, out);
            }
        }
        _ => {}
    }
}

fn loop_variant_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                loop_variant_stmt(s, out);
            }
            for s in else_body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::For {
            init, body, update, ..
        } => {
            if let Some(s) = init {
                loop_variant_stmt(s, out);
            }
            for s in body.iter() {
                loop_variant_stmt(s, out);
            }
            if let Some(s) = update {
                loop_variant_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn sccp_subst_expr(expr: &mut PreHirExpr, env: &ConstEnv) -> bool {
    let mut changed = false;
    match expr {
        // `env` maps a variable NAME to the value currently held IN it
        // (the only insertion site is `Assign{lhs: Var(name), ..}` below).
        // `AddressOfLocal`/`AddressOfGlobal` name the same storage but ask
        // for its ADDRESS, an unrelated quantity -- substituting them here
        // turned `getline(&buf, &n, f)` into `getline(0, 0, f)` once `buf`'s
        // initializer (`buf = NULL;`) put "buf -> 0" in the env, because nothing
        // told this pass that passing `&buf` to a call also reads `buf`'s
        // address, not its value. See `kill_locals_with_address_taken_in_calls`
        // for the matching write-side half: a call receiving a local's address
        // may write through it, so the local's value goes unknown too.
        PreHirExpr::Var(name) => {
            if let Some((v, ty)) = env.get(name) {
                *expr = PreHirExpr::Const(*v, ty.clone());
                changed = true;
            }
        }
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {}
        PreHirExpr::Unary { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= sccp_subst_expr(lhs, env);
            changed |= sccp_subst_expr(rhs, env);
        }
        PreHirExpr::Cast { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        PreHirExpr::Load { ptr, .. } => changed |= sccp_subst_expr(ptr, env),
        PreHirExpr::PtrOffset { base, .. } => changed |= sccp_subst_expr(base, env),
        PreHirExpr::FieldAccess { base, .. } => changed |= sccp_subst_expr(base, env),
        PreHirExpr::Index { base, index, .. } => {
            changed |= sccp_subst_expr(base, env);
            changed |= sccp_subst_expr(index, env);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= sccp_subst_expr(a, env);
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => changed |= sccp_subst_expr(src, env),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= sccp_subst_expr(cond, env);
            changed |= sccp_subst_expr(then_expr, env);
            changed |= sccp_subst_expr(else_expr, env);
        }
        PreHirExpr::Const(_, _) => {}
    }
    changed
}

/// Alias target standing for "somewhere in this function's own frame, at an
/// offset nothing here records".
///
/// A value read before anything in the function defines it -- a live-in
/// register that is not a parameter, such as the stack pointer the builder
/// prints as `rsp` -- carries no provenance. It can be the address of the
/// frame, and a callee handed it can write any slot the frame holds; so can
/// a store through it. Parameters are excluded: they are the caller's values
/// and cannot point into a frame that did not exist when they were passed.
/// Not a C identifier, so it never collides with a variable name.
const UNKNOWN_FRAME_ADDRESS: &str = "<frame>";

/// Drop from `env` any local whose address is passed to a call anywhere in
/// `expr`: the callee may write through that pointer (`getline(&buf, &n,
/// f)`, `scanf("%d", &x)`, `fstat(fd, &st)`), so the local's last-known value
/// no longer holds past the call. This pass has no per-callee effect summary
/// to consult, so it kills unconditionally -- always sound (declining to
/// treat something as constant is never wrong), and cheap since a written
/// local was going to lose its constant status at the next real write anyway.
///
/// Must run on `expr` *after* `sccp_subst_expr`/`fold_expr_hir`: an
/// `AddressOfLocal` cannot itself have been folded into a `Const` (that is
/// exactly what `sccp_subst_expr` now refuses to do), so this always finds
/// the real argument shape, not a folded stand-in.
///
/// What each variable may point at, for deciding which constants a call or a
/// store through a pointer can overwrite.
#[derive(Default)]
struct AddressAliases {
    /// Local addresses (and [`UNKNOWN_FRAME_ADDRESS`]) each variable may hold.
    targets: HashMap<String, HashSet<String>>,
    /// Variables assigned somewhere in the body.
    defined: HashSet<String>,
    /// Parameters: they hold what the caller passed, which is never an
    /// address in this frame.
    entry_values: HashSet<String>,
    /// Variables no callee or store can reach: parameters and the builder's
    /// register temporaries. Every other variable may be frame storage.
    /// This reads binding origins as the builder sets them; SCCP runs before
    /// the normalize passes that mint temporaries over other variables.
    register_storage: HashSet<String>,
}

impl AddressAliases {
    fn get(&self, name: &str) -> Option<&HashSet<String>> {
        self.targets.get(name)
    }

    /// Whether `name` is read with no definition and is not a parameter, so
    /// its value -- possibly the frame's own address -- is unknown.
    fn is_unknown_live_in(&self, name: &str) -> bool {
        !self.defined.contains(name) && !self.entry_values.contains(name)
    }

    /// Drop every constant that frame storage holds: an address into the
    /// frame escaped at an offset that is not known, so any slot may change.
    fn forget_frame_storage(&self, env: &mut ConstEnv) {
        env.retain(|name, _| self.register_storage.contains(name));
    }
}

#[derive(Default)]
struct AddressAliasSummary {
    dependencies: HashSet<String>,
    local_addresses: HashSet<String>,
}

fn collect_address_alias_expr(
    expr: &PreHirExpr,
    dependencies: &mut HashSet<String>,
    local_addresses: &mut HashSet<String>,
) {
    let mut pending = vec![expr];
    while let Some(expr) = pending.pop() {
        match expr {
            PreHirExpr::AddressOfLocal(name) => {
                local_addresses.insert(name.clone());
            }
            PreHirExpr::Var(name) => {
                dependencies.insert(name.clone());
            }
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => pending.push(expr),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                pending.push(lhs);
                pending.push(rhs);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                pending.push(cond);
                pending.push(then_expr);
                pending.push(else_expr);
            }
            PreHirExpr::Index { base, index, .. } => {
                pending.push(base);
                pending.push(index);
            }
            PreHirExpr::Load { .. }
            | PreHirExpr::Call { .. }
            | PreHirExpr::AggregateCopy { .. }
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::Const(_, _) => {}
        }
    }
}

/// [`collect_address_aliases_with`] for `func`: its parameters, the bindings
/// that start with a value, and the register temporaries a callee cannot reach.
fn collect_function_address_aliases(func: &PreHirFunction) -> AddressAliases {
    let entry_values: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();
    // A binding that starts with a value is defined by that value: an
    // initializer such as `(T *)(sp + off)` is a frame address, so it goes
    // through the same summary as an assignment rather than counting as a
    // parameter.
    let initializers: Vec<(String, &PreHirExpr)> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .filter_map(|binding| {
            binding
                .initializer
                .as_ref()
                .map(|init| (binding.name.clone(), init))
        })
        .collect();
    let register_storage = func
        .params
        .iter()
        .map(|binding| binding.name.clone())
        .chain(
            func.locals
                .iter()
                .filter(|binding| binding.is_temp_like())
                .map(|binding| binding.name.clone()),
        )
        .collect();
    collect_address_aliases_with(&func.body, &initializers, entry_values, register_storage)
}

#[cfg(test)]
fn collect_address_aliases(stmts: &[PreHirStmt]) -> AddressAliases {
    collect_address_aliases_with(stmts, &[], HashSet::default(), HashSet::default())
}

/// Collect pointer-like variable assignments so an address passed through a
/// temporary is still recognized as an escaping local.  The builder commonly
/// materializes `p = &local; call(p)` rather than keeping the address
/// expression directly in the call argument.  SCCP must treat those two
/// forms identically: the callee can write through `p`, so the pre-call value
/// of `local` is not a valid constant afterwards.
///
/// This is deliberately a may-alias summary for the whole function.  Keeping
/// an old possible address after a later reassignment can only discard a
/// constant that would otherwise be propagated; dropping a possible address
/// could substitute a stale value after a call and change semantics.
fn collect_address_aliases_with(
    stmts: &[PreHirStmt],
    initializers: &[(String, &PreHirExpr)],
    entry_values: HashSet<String>,
    register_storage: HashSet<String>,
) -> AddressAliases {
    let mut definitions = HashMap::<String, AddressAliasSummary>::default();

    fn visit(stmts: &[PreHirStmt], definitions: &mut HashMap<String, AddressAliasSummary>) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs,
                } => {
                    let summary = definitions.entry(name.clone()).or_default();
                    collect_address_alias_expr(
                        rhs,
                        &mut summary.dependencies,
                        &mut summary.local_addresses,
                    );
                }
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => visit(body, definitions),
                PreHirStmt::For {
                    init, update, body, ..
                } => {
                    if let Some(init) = init {
                        visit(std::slice::from_ref(init.as_ref()), definitions);
                    }
                    visit(body, definitions);
                    if let Some(update) = update {
                        visit(std::slice::from_ref(update.as_ref()), definitions);
                    }
                }
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    visit(then_body, definitions);
                    visit(else_body, definitions);
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(&case.body, definitions);
                    }
                    visit(default, definitions);
                }
                _ => {}
            }
        }
    }

    visit(stmts, &mut definitions);
    for (name, init) in initializers {
        let summary = definitions.entry(name.clone()).or_default();
        collect_address_alias_expr(
            init,
            &mut summary.dependencies,
            &mut summary.local_addresses,
        );
    }

    let defined: HashSet<String> = definitions.keys().cloned().collect();
    // A definition that reads an unknown live-in (`r15 = rsp`) may hold the
    // frame's address; the fixed point below carries that to its users.
    for summary in definitions.values_mut() {
        if summary
            .dependencies
            .iter()
            .any(|dependency| !defined.contains(dependency) && !entry_values.contains(dependency))
        {
            summary
                .local_addresses
                .insert(UNKNOWN_FRAME_ADDRESS.to_string());
        }
    }

    // `aliases[name]` is the least fixed point of the local addresses in that
    // variable's definitions plus the aliases of every referenced variable.
    // The reverse dependency map propagates only when a set grows, including
    // through cycles, without recursively re-expanding shared definitions.
    let mut dependents = HashMap::<String, Vec<String>>::default();
    let mut aliases = HashMap::<String, HashSet<String>>::default();
    for (name, summary) in &definitions {
        aliases.insert(name.clone(), summary.local_addresses.clone());
        for dependency in &summary.dependencies {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(name.clone());
        }
    }

    let mut pending = VecDeque::new();
    let mut queued = HashSet::default();
    for (name, targets) in &aliases {
        if !targets.is_empty() && queued.insert(name.clone()) {
            pending.push_back(name.clone());
        }
    }

    while let Some(name) = pending.pop_front() {
        queued.remove(&name);
        let Some(inherited) = aliases.get(&name).cloned() else {
            continue;
        };
        let Some(users) = dependents.get(&name) else {
            continue;
        };
        for user in users {
            let Some(targets) = aliases.get_mut(user) else {
                continue;
            };
            let old_len = targets.len();
            targets.extend(inherited.iter().cloned());
            if targets.len() != old_len && queued.insert(user.clone()) {
                pending.push_back(user.clone());
            }
        }
    }

    aliases.retain(|_, targets| !targets.is_empty());
    AddressAliases {
        targets: aliases,
        defined,
        entry_values,
        register_storage,
    }
}

fn kill_locals_with_address_taken_in_calls(
    expr: &PreHirExpr,
    env: &mut ConstEnv,
    address_aliases: &AddressAliases,
) {
    if let PreHirExpr::Call { args, .. } = expr {
        for arg in args {
            collect_address_taken_locals(arg, env, address_aliases);
        }
    }
    match expr {
        PreHirExpr::Call { args, .. } => {
            for a in args {
                kill_locals_with_address_taken_in_calls(a, env, address_aliases);
            }
        }
        PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            kill_locals_with_address_taken_in_calls(inner, env, address_aliases);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            kill_locals_with_address_taken_in_calls(lhs, env, address_aliases);
            kill_locals_with_address_taken_in_calls(rhs, env, address_aliases);
        }
        PreHirExpr::Index { base, index, .. } => {
            kill_locals_with_address_taken_in_calls(base, env, address_aliases);
            kill_locals_with_address_taken_in_calls(index, env, address_aliases);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            kill_locals_with_address_taken_in_calls(cond, env, address_aliases);
            kill_locals_with_address_taken_in_calls(then_expr, env, address_aliases);
            kill_locals_with_address_taken_in_calls(else_expr, env, address_aliases);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_addr_or_alias_targets_in_expr(
    expr: &PreHirExpr,
    address_aliases: &AddressAliases,
    out: &mut HashSet<String>,
) {
    match expr {
        PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Var(name) => {
            if let Some(aliases) = address_aliases.get(name) {
                for alias in aliases {
                    out.insert(alias.clone());
                }
            } else if address_aliases.is_unknown_live_in(name) {
                out.insert(UNKNOWN_FRAME_ADDRESS.to_string());
            }
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_addr_or_alias_targets_in_expr(inner, address_aliases, out)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_addr_or_alias_targets_in_expr(lhs, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(rhs, address_aliases, out);
        }
        PreHirExpr::Select {
            then_expr,
            else_expr,
            ..
        } => {
            collect_addr_or_alias_targets_in_expr(then_expr, address_aliases, out);
            collect_addr_or_alias_targets_in_expr(else_expr, address_aliases, out);
        }
        PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::Const(_, _)
        | PreHirExpr::Load { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_call_escaped_vars_in_expr(
    expr: &PreHirExpr,
    address_aliases: &AddressAliases,
    out: &mut HashSet<String>,
) {
    if let PreHirExpr::Call { args, .. } = expr {
        for arg in args {
            collect_addr_or_alias_targets_in_expr(arg, address_aliases, out);
        }
    }
    match expr {
        PreHirExpr::Call { args, .. } => {
            for a in args {
                collect_call_escaped_vars_in_expr(a, address_aliases, out);
            }
        }
        PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            collect_call_escaped_vars_in_expr(inner, address_aliases, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_call_escaped_vars_in_expr(lhs, address_aliases, out);
            collect_call_escaped_vars_in_expr(rhs, address_aliases, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_call_escaped_vars_in_expr(base, address_aliases, out);
            collect_call_escaped_vars_in_expr(index, address_aliases, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_call_escaped_vars_in_expr(cond, address_aliases, out);
            collect_call_escaped_vars_in_expr(then_expr, address_aliases, out);
            collect_call_escaped_vars_in_expr(else_expr, address_aliases, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn expr_root_var_sccp(expr: &PreHirExpr) -> Option<&str> {
    let mut cur = expr;
    loop {
        match cur {
            PreHirExpr::Var(name) => return Some(name.as_str()),
            PreHirExpr::FieldAccess { base, .. }
            | PreHirExpr::Index { base, .. }
            | PreHirExpr::Cast { expr: base, .. } => cur = base,
            _ => return None,
        }
    }
}

fn kill_locals_in_lvalue(lhs: &PreHirLValue, env: &mut ConstEnv, address_aliases: &AddressAliases) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => {
            kill_locals_with_address_taken_in_calls(ptr, env, address_aliases);
            collect_address_taken_locals(ptr, env, address_aliases);
        }
        PreHirLValue::Index { base, index, .. } => {
            kill_locals_with_address_taken_in_calls(base, env, address_aliases);
            kill_locals_with_address_taken_in_calls(index, env, address_aliases);
            collect_address_taken_locals(base, env, address_aliases);
            if let Some(root) = expr_root_var_sccp(base) {
                env.remove(root);
            }
        }
        PreHirLValue::FieldAccess { base, .. } => {
            kill_locals_with_address_taken_in_calls(base, env, address_aliases);
            collect_address_taken_locals(base, env, address_aliases);
            if let Some(root) = expr_root_var_sccp(base) {
                env.remove(root);
            }
        }
    }
}

/// Names of every local whose address appears anywhere in `expr` (a call
/// argument, or a sub-address like `&local + k` / `(char *)&local` nested
/// inside one) -- the removal side of [`kill_locals_with_address_taken_in_calls`].
fn collect_address_taken_locals(
    expr: &PreHirExpr,
    env: &mut ConstEnv,
    address_aliases: &AddressAliases,
) {
    let mut killed = HashSet::default();
    collect_addr_or_alias_targets_in_expr(expr, address_aliases, &mut killed);
    if killed.contains(UNKNOWN_FRAME_ADDRESS) {
        address_aliases.forget_frame_storage(env);
    }
    for name in killed {
        env.remove(&name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    #[test]
    fn sccp_address_alias_worklist_resolves_deep_variable_chains() {
        const DEPTH: usize = 4_096;
        let mut body = Vec::with_capacity(DEPTH + 1);
        body.push(PreHirStmt::Assign {
            lhs: PreHirLValue::Var("alias_0".to_string()),
            rhs: PreHirExpr::AddressOfLocal("escaped".to_string()),
        });
        for index in 1..=DEPTH {
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(format!("alias_{index}")),
                rhs: var(&format!("alias_{}", index - 1)),
            });
        }

        let aliases = collect_address_aliases(&body);
        assert!(
            aliases
                .get(&format!("alias_{DEPTH}"))
                .is_some_and(|targets| targets.contains("escaped")),
            "the final variable must retain the local address at the start of the chain"
        );
    }

    #[test]
    fn sccp_address_alias_worklist_closes_cycles_and_unions_definitions() {
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(var("q")),
                    rhs: Box::new(PreHirExpr::AddressOfLocal("first".to_string())),
                    ty: int(64),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("q".to_string()),
                rhs: var("p"),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("q".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(PreHirExpr::AddressOfLocal("second".to_string())),
                },
            },
        ];

        let aliases = collect_address_aliases(&body);
        for name in ["p", "q"] {
            let targets = aliases
                .get(name)
                .expect("cyclic aliases should be retained");
            assert!(targets.contains("first"));
            assert!(targets.contains("second"));
        }
    }

    #[test]
    fn sccp_keeps_backedge_label_values_nonconstant() {
        let mut func = PreHirFunction {
            name: "test_sccp_unstructured_backedge".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::Label("loop".to_string()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("x")),
                        rhs: Box::new(PreHirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(var("x")),
                        ty: int(32),
                    },
                    then_body: vec![PreHirStmt::Goto("loop".to_string())].into(),
                    else_body: vec![].into(),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::If { cond, .. } = &func.body[3] else {
            panic!("expected loop branch to remain an if");
        };
        let PreHirExpr::Binary { rhs, .. } = cond else {
            panic!("expected branch condition to remain binary");
        };
        assert_eq!(rhs.as_ref(), &var("x"));
    }

    #[test]
    fn sccp_does_not_fold_address_of_local_to_its_known_value() {
        // `char *buf = NULL; getline(&buf, &n, f);` -- `buf`'s value (0) must
        // never be substituted into the `&buf` ARGUMENT itself. Reinstating
        // the old shared match arm turns this into `getline(0, &n, f)`.
        let mut func = PreHirFunction {
            name: "test_sccp_address_of_local".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("buf".to_string()),
                    rhs: PreHirExpr::Const(0, int(64)),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "getline".to_string(),
                    args: vec![PreHirExpr::AddressOfLocal("buf".to_string())],
                    ty: int(64),
                }),
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Expr(PreHirExpr::Call { args, .. }) = &func.body[1] else {
            panic!("expected the call statement to remain a call");
        };
        assert_eq!(
            args[0],
            PreHirExpr::AddressOfLocal("buf".to_string()),
            "an address-of-local argument must never be replaced by the \
             local's last-known value: `&buf` is not `buf`"
        );
    }

    #[test]
    fn sccp_kills_a_locals_known_value_when_its_address_escapes_to_a_call() {
        // Even once the argument itself is protected, a later plain READ of
        // the same local (`x = buf;`, e.g. from `mov rdi, [rbp-0x10]` after
        // the call reloads its value) must not inherit the local's PRE-call
        // constant: `getline` may have written through `&buf`.
        let mut func = PreHirFunction {
            name: "test_sccp_call_kills_escaped_local".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("buf".to_string()),
                    rhs: PreHirExpr::Const(0, int(64)),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "getline".to_string(),
                    args: vec![PreHirExpr::AddressOfLocal("buf".to_string())],
                    ty: int(64),
                }),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: var("buf"),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
            panic!("expected the trailing read to remain an assignment");
        };
        assert_eq!(
            rhs,
            &var("buf"),
            "a local whose address was passed to a call must not keep its \
             pre-call constant value: the call may have written through it"
        );
    }

    #[test]
    fn sccp_kills_a_local_when_its_address_reaches_a_call_through_a_pointer_alias() {
        // The p-code materializer commonly separates the address computation
        // from the call argument (`p = &buf; call(p)`).  Treating only a
        // direct `call(&buf)` as an escape lets SCCP reuse the pre-call
        // constant even though the callee receives the same address.
        let mut func = PreHirFunction {
            name: "test_sccp_pointer_alias_escape".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("buf".to_string()),
                    rhs: PreHirExpr::Const(0, int(64)),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("p".to_string()),
                    rhs: PreHirExpr::AddressOfLocal("buf".to_string()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "write_through_pointer".to_string(),
                    args: vec![var("p")],
                    ty: int(64),
                }),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: var("buf"),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Assign { rhs, .. } = &func.body[3] else {
            panic!("expected the trailing read to remain an assignment");
        };
        assert_eq!(
            rhs,
            &var("buf"),
            "an address passed through a pointer alias must invalidate the \
             pointee's pre-call constant"
        );
    }

    #[test]
    fn sccp_does_not_prune_if_when_discarded_branch_has_side_effects() {
        let mut func = PreHirFunction {
            name: "test".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![PreHirStmt::If {
                cond: PreHirExpr::Const(1, int(32)),
                then_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "add".to_string(),
                    args: vec![PreHirExpr::Const(1, int(32)), PreHirExpr::Const(2, int(32))],
                    ty: int(32),
                })]
                .into(),
                else_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "max".to_string(),
                    args: vec![PreHirExpr::Const(3, int(32)), PreHirExpr::Const(4, int(32))],
                    ty: int(32),
                })]
                .into(),
            }],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &func.body[0]
        else {
            panic!("side-effectful branch should not be pruned");
        };
        assert_eq!(then_body.len(), 1);
        assert_eq!(else_body.len(), 1);
    }

    #[test]
    fn sccp_loop_variant_vars_includes_nested_loops() {
        let nested_body = vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(100, int(32)),
            }]
            .into(),
        }];
        let mut out = HashSet::default();
        for s in &nested_body {
            loop_variant_stmt(s, &mut out);
        }
        assert!(out.contains("x"), "x must be recognized as loop-variant");
    }

    #[test]
    fn sccp_does_not_prune_switch_case_with_goto_target() {
        let mut func = PreHirFunction {
            name: "test_switch_goto".to_string(),
            body: vec![
                PreHirStmt::Goto("label_case_2".to_string()),
                PreHirStmt::Switch {
                    expr: PreHirExpr::Const(1, int(32)),
                    cases: vec![
                        PreHirSwitchCase {
                            values: vec![1],
                            body: vec![PreHirStmt::Return(None)].into(),
                        },
                        PreHirSwitchCase {
                            values: vec![2],
                            body: vec![
                                PreHirStmt::Label("label_case_2".to_string()),
                                PreHirStmt::Return(None),
                            ]
                            .into(),
                        },
                    ],
                    default: vec![].into(),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);
        // Case 2 must NOT be deleted because it defines a label targeted by a goto!
        let PreHirStmt::Switch { cases, .. } = &func.body[1] else {
            panic!("switch must not be pruned to single block when unreached case has goto target");
        };
        assert_eq!(cases.len(), 2);
    }

    #[test]
    fn if_cond_call_taking_local_addr_kills_const_in_branches() {
        let mut func = PreHirFunction {
            name: "test_if_cond_escape".to_string(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Ne,
                        lhs: Box::new(PreHirExpr::Call {
                            origin: None,
                            target: "init".to_string(),
                            args: vec![PreHirExpr::AddressOfLocal("x".to_string())],
                            ty: int(32),
                        }),
                        rhs: Box::new(PreHirExpr::Const(0, int(32))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("y".to_string()),
                        rhs: var("x"),
                    }]
                    .into(),
                    else_body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("z".to_string()),
                        rhs: var("x"),
                    }]
                    .into(),
                },
                PreHirStmt::Return(Some(var("x"))),
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &func.body[1]
        else {
            panic!("expected If");
        };
        assert!(matches!(
            &then_body[0],
            PreHirStmt::Assign { rhs: PreHirExpr::Var(n), .. } if n == "x"
        ));
        assert!(matches!(
            &else_body[0],
            PreHirStmt::Assign { rhs: PreHirExpr::Var(n), .. } if n == "x"
        ));
        assert!(matches!(
            &func.body[2],
            PreHirStmt::Return(Some(PreHirExpr::Var(n))) if n == "x"
        ));
    }

    #[test]
    fn while_cond_call_taking_local_addr_kills_const_in_body_and_after() {
        let mut func = PreHirFunction {
            name: "test_while_cond_escape".to_string(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::While {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Ne,
                        lhs: Box::new(PreHirExpr::Call {
                            origin: None,
                            target: "next".to_string(),
                            args: vec![PreHirExpr::AddressOfLocal("x".to_string())],
                            ty: int(32),
                        }),
                        rhs: Box::new(PreHirExpr::Const(0, int(32))),
                        ty: NirType::Bool,
                    },
                    body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("y".to_string()),
                        rhs: var("x"),
                    }]
                    .into(),
                },
                PreHirStmt::Return(Some(var("x"))),
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::While { body, .. } = &func.body[1] else {
            panic!("expected While");
        };
        assert!(matches!(
            &body[0],
            PreHirStmt::Assign { rhs: PreHirExpr::Var(n), .. } if n == "x"
        ));
        assert!(matches!(
            &func.body[2],
            PreHirStmt::Return(Some(PreHirExpr::Var(n))) if n == "x"
        ));
    }

    #[test]
    fn switch_expr_and_assign_lhs_call_kill_escaped_const() {
        let mut func = PreHirFunction {
            name: "test_switch_and_lhs_escape".to_string(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::Switch {
                    expr: PreHirExpr::Call {
                        origin: None,
                        target: "pick".to_string(),
                        args: vec![PreHirExpr::AddressOfLocal("x".to_string())],
                        ty: int(32),
                    },
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: vec![PreHirStmt::Assign {
                            lhs: PreHirLValue::Var("a".to_string()),
                            rhs: var("x"),
                        }]
                        .into(),
                    }],
                    default: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("b".to_string()),
                        rhs: var("x"),
                    }]
                    .into(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".to_string()),
                    rhs: PreHirExpr::Const(7, int(32)),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Call {
                            origin: None,
                            target: "slot".to_string(),
                            args: vec![PreHirExpr::AddressOfLocal("y".to_string())],
                            ty: NirType::Ptr(Box::new(int(32))),
                        }),
                        ty: int(32),
                    },
                    rhs: PreHirExpr::Const(9, int(32)),
                },
                PreHirStmt::Return(Some(var("y"))),
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Switch { cases, default, .. } = &func.body[1] else {
            panic!("expected Switch");
        };
        assert!(matches!(
            &cases[0].body[0],
            PreHirStmt::Assign { rhs: PreHirExpr::Var(n), .. } if n == "x"
        ));
        assert!(matches!(
            &default[0],
            PreHirStmt::Assign { rhs: PreHirExpr::Var(n), .. } if n == "x"
        ));
        assert!(matches!(
            func.body.last(),
            Some(PreHirStmt::Return(Some(PreHirExpr::Var(n)))) if n == "y"
        ));
    }

    /// `frame_ptr = sp; slot = -1; call(frame_ptr); if (0 <= (int)slot) call2();`
    /// -- the shape of a `struct tm` built on the stack and handed to
    /// `mktime` through the stack pointer, with a field preset before the
    /// call and tested after it. `sp` is read with no definition and is not
    /// a parameter: it is the frame's own address, so the callee may rewrite
    /// `slot`, and neither the slot's constant nor the branch it decides may
    /// survive the call.
    fn frame_escape_body(call_arg: PreHirExpr) -> Vec<PreHirStmt> {
        vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("frame_ptr".to_string()),
                rhs: var("sp"),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("slot".to_string()),
                rhs: PreHirExpr::Const(0xffff_ffff, int(32)),
            },
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "fills_struct".to_string(),
                args: vec![call_arg],
                ty: int(64),
            }),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("field".to_string()),
                rhs: var("slot"),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLe,
                    lhs: Box::new(PreHirExpr::Const(0, signed_int(32))),
                    rhs: Box::new(PreHirExpr::Cast {
                        ty: signed_int(32),
                        expr: Box::new(var("field")),
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "on_success".to_string(),
                    args: vec![],
                    ty: int(64),
                })]
                .into(),
                else_body: vec![].into(),
            },
        ]
    }

    fn signed_int(bits: u32) -> NirType {
        NirType::Int { bits, signed: true }
    }

    fn assert_slot_survives_call(func: &PreHirFunction) {
        let PreHirStmt::Assign { rhs, .. } = &func.body[3] else {
            panic!("expected the post-call read to remain an assignment");
        };
        assert_eq!(
            rhs,
            &var("slot"),
            "a call given the frame's address may rewrite every slot; the \
             pre-call constant must not reach the read after it:\n{:#?}",
            func.body
        );
        let PreHirStmt::If { cond, .. } = &func.body[4] else {
            panic!("expected the branch to remain an if:\n{:#?}", func.body);
        };
        assert!(
            format!("{cond:?}").contains("Var(\"field\")"),
            "the branch on the rewritten slot must not be decided from the \
             pre-call value:\n{:#?}",
            func.body
        );
    }

    #[test]
    fn sccp_kills_frame_slots_when_the_stack_pointer_reaches_a_call_through_a_register() {
        let mut func = PreHirFunction {
            name: "test_sccp_frame_escape_via_register".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: frame_escape_body(var("frame_ptr")),
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        assert_slot_survives_call(&func);
    }

    #[test]
    fn sccp_kills_frame_slots_when_the_stack_pointer_is_a_call_argument() {
        let mut func = PreHirFunction {
            name: "test_sccp_frame_escape_direct".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: frame_escape_body(var("sp")),
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        assert_slot_survives_call(&func);
    }

    #[test]
    fn sccp_keeps_frame_constants_across_a_call_given_only_a_parameter() {
        // A parameter is the caller's value: it cannot point into a frame
        // that did not exist when it was passed, so handing it to a callee
        // leaves this frame's constants alone.
        let mut func = PreHirFunction {
            name: "test_sccp_param_does_not_escape_frame".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![PreHirBinding {
                name: "param_1".to_string(),
                ty: int(64),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            return_type: int(32),
            body: frame_escape_body(var("param_1")),
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Assign { rhs, .. } = &func.body[3] else {
            panic!("expected the post-call read to remain an assignment");
        };
        assert!(
            matches!(rhs, PreHirExpr::Const(0xffff_ffff, _)),
            "a parameter argument does not reach the frame:\n{:#?}",
            func.body
        );
    }

    #[test]
    fn sccp_kills_frame_slots_when_a_loop_body_hands_out_the_frame() {
        // `slot = -1; while (c) { fills_struct(sp); } field = slot;` -- the
        // loop path forgets loop-variant names through `env_without_vars`.
        let mut func = PreHirFunction {
            name: "test_sccp_frame_escape_in_loop".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".to_string()),
                    rhs: PreHirExpr::Const(0xffff_ffff, int(32)),
                },
                PreHirStmt::While {
                    cond: var("keep_going"),
                    body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                        origin: None,
                        target: "fills_struct".to_string(),
                        args: vec![var("sp")],
                        ty: int(64),
                    })]
                    .into(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("field".to_string()),
                    rhs: var("slot"),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
            panic!("expected the read after the loop to remain an assignment");
        };
        assert_eq!(rhs, &var("slot"), "{:#?}", func.body);
    }

    #[test]
    fn sccp_kills_frame_slots_when_an_initialized_binding_holds_the_frame() {
        // A binding whose initializer reads the stack pointer is a frame
        // address like an assigned one, not an entry value like a parameter.
        let mut body = frame_escape_body(var("frame_alias"));
        body.remove(0);
        let mut func = PreHirFunction {
            name: "test_sccp_frame_escape_via_initializer".to_string(),
            int_param_offsets: Vec::new(),
            locals: vec![PreHirBinding {
                name: "frame_alias".to_string(),
                ty: int(64),
                surface_type_name: None,
                origin: None,
                initializer: Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(var("sp")),
                    rhs: Box::new(PreHirExpr::Const(16, int(64))),
                    ty: int(64),
                }),
            }],
            return_type: int(32),
            body,
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
            panic!("expected the post-call read to remain an assignment");
        };
        assert_eq!(rhs, &var("slot"), "{:#?}", func.body);
    }

    #[test]
    fn sccp_keeps_register_temporaries_when_the_frame_escapes() {
        // Only storage a callee can reach is forgotten: a register temporary
        // keeps its constant across the call that receives the frame.
        let mut body = frame_escape_body(var("frame_ptr"));
        body.insert(
            0,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("reg".to_string()),
                rhs: PreHirExpr::Const(7, int(64)),
            },
        );
        body.push(PreHirStmt::Return(Some(var("reg"))));
        let mut func = PreHirFunction {
            name: "test_sccp_frame_escape_keeps_temps".to_string(),
            int_param_offsets: Vec::new(),
            locals: vec![PreHirBinding {
                name: "reg".to_string(),
                ty: int(64),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: int(64),
            body,
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        assert!(
            matches!(
                func.body.last(),
                Some(PreHirStmt::Return(Some(PreHirExpr::Const(7, _))))
            ),
            "a register temporary is not frame storage:\n{:#?}",
            func.body
        );
    }
}

fn eval_truth(expr: &PreHirExpr, env: &ConstEnv) -> Option<bool> {
    let (v, _) = eval_hir_expr_with_const_env(expr, env)?;
    Some(v != 0)
}

/// Ghidra ActionConditionalConst analog: given a branch condition, derive
/// constant bindings that are known to hold in the then/else branches.
///
/// Returns `(then_bindings, else_bindings)` where each binding is `(name, value, ty)`.
///
/// Handles:
/// - `x == K`  → then: x=K
/// - `x != K`  → else: x=K
/// - `!(x == K)` → else: x=K (same as x != K)
/// - `cond1 && cond2` → then: union of both
fn derive_branch_constants(
    cond: &PreHirExpr,
) -> (Vec<(String, i64, NirType)>, Vec<(String, i64, NirType)>) {
    let mut then_bindings: Vec<(String, i64, NirType)> = Vec::new();
    let mut else_bindings: Vec<(String, i64, NirType)> = Vec::new();
    extract_branch_constants(cond, false, &mut then_bindings, &mut else_bindings);
    (then_bindings, else_bindings)
}

fn extract_branch_constants(
    cond: &PreHirExpr,
    negated: bool,
    then_bindings: &mut Vec<(String, i64, NirType)>,
    else_bindings: &mut Vec<(String, i64, NirType)>,
) {
    match cond {
        // NOT: flip then/else roles.
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: inner,
            ..
        } => {
            extract_branch_constants(inner, !negated, then_bindings, else_bindings);
        }
        // x == K or K == x  → then: x=K ; x != K or K != x → else: x=K
        PreHirExpr::Binary {
            op: op @ (PreHirBinaryOp::Eq | PreHirBinaryOp::Ne),
            lhs,
            rhs,
            ..
        } => {
            let (var_name, const_val, ty) = match (lhs.as_ref(), rhs.as_ref()) {
                (PreHirExpr::Var(name), PreHirExpr::Const(k, ty)) => (name.clone(), *k, ty.clone()),
                (PreHirExpr::Const(k, ty), PreHirExpr::Var(name)) => (name.clone(), *k, ty.clone()),
                _ => return,
            };
            // For `==`: const holds in then-branch (unless negated → else-branch).
            // For `!=`: const holds in else-branch.
            let const_in_then = matches!(op, PreHirBinaryOp::Eq) ^ negated;
            if const_in_then {
                then_bindings.push((var_name, const_val, ty));
            } else {
                else_bindings.push((var_name, const_val, ty));
            }
        }
        // cond_a && cond_b → then: both hold; else: nothing (either could be false).
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if !negated => {
            extract_branch_constants(lhs, false, then_bindings, else_bindings);
            extract_branch_constants(rhs, false, then_bindings, else_bindings);
        }
        _ => {}
    }
}

fn sccp_transform_stmts(
    stmts: &mut Vec<PreHirStmt>,
    env: &mut ConstEnv,
    goto_targets: &HashSet<String>,
    all_xvars: &HashSet<String>,
    address_aliases: &AddressAliases,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        changed |= sccp_stmt(&mut stmts[i], env, goto_targets, all_xvars, address_aliases);
        i += 1;
    }
    changed
}

fn sccp_stmt(
    stmt: &mut PreHirStmt,
    env: &mut ConstEnv,
    goto_targets: &HashSet<String>,
    all_xvars: &HashSet<String>,
    address_aliases: &AddressAliases,
) -> bool {
    let mut changed = false;
    loop {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Var(name) = lhs {
                    changed |= sccp_subst_expr(rhs, env);
                    changed |= fold_expr_hir(rhs);
                    if let Some((v, ty)) = eval_hir_expr_with_const_env(rhs, env) {
                        if !matches!(rhs, PreHirExpr::Const(cv, _) if *cv == v) {
                            *rhs = PreHirExpr::Const(v, ty.clone());
                            changed = true;
                        }
                        env.insert(name.clone(), (v, ty));
                    } else {
                        env.remove(name);
                    }
                    kill_locals_with_address_taken_in_calls(rhs, env, address_aliases);
                } else {
                    changed |= sccp_subst_expr(rhs, env);
                    changed |= fold_expr_hir(rhs);
                    kill_locals_in_lvalue(lhs, env, address_aliases);
                    kill_locals_with_address_taken_in_calls(rhs, env, address_aliases);
                }
                break;
            }
            PreHirStmt::VaStart { va_list, .. } => {
                if !matches!(va_list, PreHirExpr::Var(_)) {
                    changed |= sccp_subst_expr(va_list, env);
                    changed |= fold_expr_hir(va_list);
                }
                kill_locals_with_address_taken_in_calls(va_list, env, address_aliases);
                collect_address_taken_locals(va_list, env, address_aliases);
                if let PreHirExpr::Var(name) = va_list {
                    env.remove(name);
                }
                break;
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                changed |= sccp_subst_expr(expr, env);
                changed |= fold_expr_hir(expr);
                kill_locals_with_address_taken_in_calls(expr, env, address_aliases);
                break;
            }
            PreHirStmt::Block(stmts) => {
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                    env,
                    goto_targets,
                    all_xvars,
                    address_aliases,
                );
                break;
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(cond, &pre);
                changed |= fold_expr_hir(cond);
                let cond_truth = if expr_has_side_effects(cond) {
                    None
                } else {
                    eval_truth(cond, &pre)
                };
                match cond_truth {
                    Some(true)
                        if !stmt_list_has_side_effects(else_body)
                            && !branch_defines_goto_target(else_body, goto_targets) =>
                    {
                        *stmt = PreHirStmt::Block(std::mem::take(then_body));
                        changed = true;
                        continue;
                    }
                    Some(false)
                        if !stmt_list_has_side_effects(then_body)
                            && !branch_defines_goto_target(then_body, goto_targets) =>
                    {
                        *stmt = PreHirStmt::Block(std::mem::take(else_body));
                        changed = true;
                        continue;
                    }
                    Some(_) | None => {
                        let mut post_cond = pre.clone();
                        kill_locals_with_address_taken_in_calls(
                            cond,
                            &mut post_cond,
                            address_aliases,
                        );
                        // Ghidra ActionConditionalConst: derive constants from the branch condition.
                        // Pattern: `if (x == K)` → inside then-branch, x=K
                        // Pattern: `if (x != K)` → inside else-branch, x=K
                        let (then_extra, else_extra) = derive_branch_constants(cond);
                        let mut e1 = post_cond.clone();
                        let mut e2 = post_cond;
                        for (name, val, ty) in then_extra {
                            e1.insert(name, (val, ty));
                        }
                        for (name, val, ty) in else_extra {
                            e2.insert(name, (val, ty));
                        }
                        kill_locals_with_address_taken_in_calls(cond, &mut e1, address_aliases);
                        kill_locals_with_address_taken_in_calls(cond, &mut e2, address_aliases);
                        changed |= sccp_transform_stmts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                            &mut e1,
                            goto_targets,
                            all_xvars,
                            address_aliases,
                        );
                        changed |= sccp_transform_stmts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                            &mut e2,
                            goto_targets,
                            all_xvars,
                            address_aliases,
                        );
                        *env = merge_env(&e1, &e2);
                    }
                }
                break;
            }
            PreHirStmt::While { cond, body } => {
                let pre = env.clone();
                let mut modified = loop_variant_vars(body, all_xvars, address_aliases);
                collect_call_escaped_vars_in_expr(cond, address_aliases, &mut modified);
                let loop_entry = env_without_vars(&pre, &modified, address_aliases);
                changed |= sccp_subst_expr(cond, &loop_entry);
                changed |= fold_expr_hir(cond);
                let mut inner = loop_entry;
                kill_locals_with_address_taken_in_calls(cond, &mut inner, address_aliases);
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                    address_aliases,
                );
                *env = env_without_vars(&pre, &modified, address_aliases);
                break;
            }
            PreHirStmt::DoWhile { body, cond } => {
                let pre = env.clone();
                let mut modified = loop_variant_vars(body, all_xvars, address_aliases);
                collect_call_escaped_vars_in_expr(cond, address_aliases, &mut modified);
                let mut inner = env_without_vars(&pre, &modified, address_aliases);
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                    address_aliases,
                );
                let cond_env = env_without_vars(&inner, &modified, address_aliases);
                changed |= sccp_subst_expr(cond, &cond_env);
                changed |= fold_expr_hir(cond);
                *env = env_without_vars(&pre, &modified, address_aliases);
                break;
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init.as_mut() {
                    changed |= sccp_stmt(i, env, goto_targets, all_xvars, address_aliases);
                }
                let loop_entry = env.clone();
                let mut modified = loop_variant_vars(body, all_xvars, address_aliases);
                if let Some(u) = update {
                    loop_variant_stmt(u, &mut modified);
                    collect_escaped_and_indirect_in_stmt(u, address_aliases, &mut modified);
                }
                if let Some(c) = cond.as_ref() {
                    collect_call_escaped_vars_in_expr(c, address_aliases, &mut modified);
                }
                let loop_body_entry = env_without_vars(&loop_entry, &modified, address_aliases);
                if let Some(c) = cond.as_mut() {
                    changed |= sccp_subst_expr(c, &loop_body_entry);
                    changed |= fold_expr_hir(c);
                }
                let mut inner = loop_body_entry;
                if let Some(c) = cond.as_ref() {
                    kill_locals_with_address_taken_in_calls(c, &mut inner, address_aliases);
                }
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                    address_aliases,
                );
                *env = env_without_vars(&loop_entry, &modified, address_aliases);
                if let Some(u) = update.as_mut() {
                    let mut update_env = env_without_vars(&inner, &modified, address_aliases);
                    changed |=
                        sccp_stmt(u, &mut update_env, goto_targets, all_xvars, address_aliases);
                }
                break;
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(expr, &pre);
                changed |= fold_expr_hir(expr);
                if !expr_has_side_effects(expr)
                    && let Some((v, _)) = eval_hir_expr_with_const_env(expr, &pre)
                {
                    let mut taken: Option<Vec<PreHirStmt>> = None;
                    for case in cases.iter_mut() {
                        if case.values.iter().any(|x| *x == v) {
                            taken = Some(std::mem::take(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                                &mut case.body,
                            )));
                            break;
                        }
                    }
                    let discarded_have_side_effects = cases
                        .iter()
                        .filter(|case| !case.values.iter().any(|x| *x == v))
                        .any(|case| {
                            stmt_list_has_side_effects(&case.body)
                                || branch_defines_goto_target(&case.body, goto_targets)
                        })
                        || stmt_list_has_side_effects(default)
                        || branch_defines_goto_target(default, goto_targets);
                    if !discarded_have_side_effects {
                        let blk = taken.unwrap_or_else(|| {
                            std::mem::take(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default))
                        });
                        *stmt = PreHirStmt::Block(blk.into());
                        changed = true;
                        continue;
                    }
                }
                let mut post_expr = pre;
                kill_locals_with_address_taken_in_calls(expr, &mut post_expr, address_aliases);
                let mut acc: Option<ConstEnv> = None;
                for case in cases.iter_mut() {
                    let mut e = post_expr.clone();
                    changed |= sccp_transform_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        &mut e,
                        goto_targets,
                        all_xvars,
                        address_aliases,
                    );
                    acc = Some(match acc {
                        None => e,
                        Some(a) => merge_env(&a, &e),
                    });
                }
                let mut ed = post_expr.clone();
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    &mut ed,
                    goto_targets,
                    all_xvars,
                    address_aliases,
                );
                *env = merge_env(acc.as_ref().unwrap_or(&post_expr), &ed);
                break;
            }
            PreHirStmt::Label(label) => {
                if goto_targets.contains(label) {
                    env.clear();
                }
                break;
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue
            | PreHirStmt::Goto(_) => {
                env.clear();
                break;
            }
        }
    }
    changed
}

fn collect_xvars_in_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_xvars_in_stmt(stmt, out);
    }
}

fn collect_xvars_in_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_xvars_in_lvalue(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            collect_xvars_in_expr(va_list, out);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_xvars_in_expr(expr, out);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            collect_xvars_in_stmts(body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                collect_xvars_in_stmt(s, out);
            }
            if let Some(e) = cond {
                collect_xvars_in_expr(e, out);
            }
            if let Some(s) = update {
                collect_xvars_in_stmt(s, out);
            }
            collect_xvars_in_stmts(body, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_stmts(then_body, out);
            collect_xvars_in_stmts(else_body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_xvars_in_expr(expr, out);
            for case in cases {
                collect_xvars_in_stmts(&case.body, out);
            }
            collect_xvars_in_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_xvars_in_lvalue(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        PreHirLValue::Deref { ptr, .. } => {
            collect_xvars_in_expr(ptr, out);
        }
        PreHirLValue::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
    }
}

fn collect_xvars_in_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_xvars_in_expr(inner, out);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            collect_xvars_in_expr(src, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_xvars_in_expr(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_xvars_in_expr(arg, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_expr(then_expr, out);
            collect_xvars_in_expr(else_expr, out);
        }
        PreHirExpr::Const(_, _) => {}
    }
}
