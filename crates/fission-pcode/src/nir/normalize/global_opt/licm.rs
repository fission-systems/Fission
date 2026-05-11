/// Loop Invariant Code Motion (LICM) for HIR.
///
/// Identifies assignments inside `While`/`DoWhile`/`For` loops whose
/// right-hand side is **loop-invariant**: all variable operands are defined
/// outside the loop body, and the expression has no observable side effects
/// (no `Load` or `Call`).  Such assignments are hoisted to just before the
/// loop, reducing redundant computation and improving structural similarity
/// with Ghidra output (which also performs LICM).
///
/// ## Algorithm
///
/// ```text
/// apply_licm_pass(func):
///   Traverse body recursively (innermost loops first via post-order).
///   For each While/DoWhile/For:
///     1. Collect loop_defs: all Var names assigned anywhere in the loop body.
///     2. Scan the top-level statement list of the loop body:
///        For each Assign { lhs: Var(y), rhs: E }:
///          - If E contains no Load/Call (pure), AND
///          - all Var(v) in E satisfy v ∉ loop_defs, AND
///          - y ∉ loop_defs (the target itself isn't re-assigned later)
///          → mark as invariant.
///     3. Collect invariant statements into a "hoist" list; remove them from body.
///     4. Insert hoist list before the loop statement in the parent.
///   Return true if any hoisting occurred.
/// ```
///
/// ## Soundness
///
/// Only `Assign { lhs: Var(y), rhs: E }` at the top level of the loop body
/// are candidates.  Assignments inside nested `if`/`while`/`for` are not
/// hoisted (conservatively assumed to be conditional).  Memory writes (`Deref`
/// / `Index` lhs) are never hoisted.
///
/// ## References
///
/// - LLVM `lib/Transforms/Scalar/LICM.cpp` (concept)
/// - Aho, Lam, Sethi, Ullman "Compilers" §9.5 (code motion)
use super::super::*;
use std::collections::HashSet;

/// Apply LICM to all loops in `func`.  Returns `true` if any statement was
/// hoisted.
pub(crate) fn apply_licm_pass(func: &mut HirFunction) -> bool {
    hoist_in_stmts(&mut func.body)
}

/// Recursively process a statement list, applying LICM innermost-first.
///
/// Returns `true` if any hoisting occurred (so the caller can re-run cleanup).
fn hoist_in_stmts(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;

    // First, recurse into nested bodies (innermost-first / post-order).
    // We do this before extracting loop-level info from *this* level.
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                if hoist_in_stmts(body) {
                    changed = true;
                }
            }
            HirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    hoist_single(s);
                }
                if hoist_in_stmts(body) {
                    changed = true;
                }
                if let Some(s) = update {
                    hoist_single(s);
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if hoist_in_stmts(then_body) {
                    changed = true;
                }
                if hoist_in_stmts(else_body) {
                    changed = true;
                }
            }
            HirStmt::Block(body) => {
                if hoist_in_stmts(body) {
                    changed = true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    if hoist_in_stmts(&mut case.body) {
                        changed = true;
                    }
                }
                if hoist_in_stmts(default) {
                    changed = true;
                }
            }
            _ => {}
        }
    }

    // Now process *this* level: find loops and try to hoist.
    let mut i = 0;
    while i < stmts.len() {
        let hoisted = match &stmts[i] {
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {
                extract_invariants_from_loop(&mut stmts[i])
            }
            _ => vec![],
        };
        if !hoisted.is_empty() {
            // Insert hoisted statements before the loop.
            let insert_pos = i;
            for (offset, stmt) in hoisted.into_iter().enumerate() {
                stmts.insert(insert_pos + offset, stmt);
                i += 1;
            }
            changed = true;
        }
        i += 1;
    }

    changed
}

/// Dummy to satisfy compiler when visiting init/update of For in inner pass.
fn hoist_single(_stmt: &mut HirStmt) {}

/// Extract loop-invariant assignments from the top-level body of `loop_stmt`.
///
/// Returns the list of hoisted assignments (removed from the loop body).
fn extract_invariants_from_loop(loop_stmt: &mut HirStmt) -> Vec<HirStmt> {
    let body = match loop_stmt {
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => body,
        HirStmt::For { body, .. } => body,
        _ => return vec![],
    };

    // 1. Collect all variable names defined anywhere in the loop body.
    let mut loop_defs: HashSet<String> = HashSet::new();
    collect_all_defs(body, &mut loop_defs);

    // 2. Identify invariant top-level assignments.
    let mut invariant_indices = vec![];
    for (idx, stmt) in body.iter().enumerate() {
        if is_invariant_stmt(stmt, &loop_defs) {
            invariant_indices.push(idx);
        }
    }

    if invariant_indices.is_empty() {
        return vec![];
    }

    // 3. Remove them from the body (in reverse order to preserve indices).
    let mut hoisted = Vec::with_capacity(invariant_indices.len());
    for &idx in invariant_indices.iter().rev() {
        hoisted.push(body.remove(idx));
    }
    hoisted.reverse(); // Restore original order.
    hoisted
}

/// Collect all Var names that are **assigned** (defined) anywhere in `stmts`,
/// including in nested blocks.  Memory writes (Deref/Index lhs) are also noted
/// so that loads from those locations are treated as non-invariant.
fn collect_all_defs(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_defs_in_stmt(stmt, out);
    }
}

fn collect_defs_in_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, .. } => {
            if let HirLValue::Var(name) = lhs {
                out.insert(name.clone());
            }
            // Memory writes are tracked as a sentinel key to block Load hoisting.
            // We use a special name that can never be a real variable.
            // (We only hoist pure non-Load expressions anyway, so this is a no-op
            // but makes the invariant check explicit.)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_all_defs(then_body, out);
            collect_all_defs(else_body, out);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_all_defs(body, out);
        }
        HirStmt::For {
            init, body, update, ..
        } => {
            if let Some(s) = init {
                collect_defs_in_stmt(s, out);
            }
            collect_all_defs(body, out);
            if let Some(s) = update {
                collect_defs_in_stmt(s, out);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_all_defs(&case.body, out);
            }
            collect_all_defs(default, out);
        }
        HirStmt::Block(body) => collect_all_defs(body, out),
        _ => {}
    }
}

/// Return `true` if `stmt` is an assignment that is safe to hoist out of a
/// loop whose definitions are `loop_defs`.
fn is_invariant_stmt(stmt: &HirStmt, loop_defs: &HashSet<String>) -> bool {
    let HirStmt::Assign {
        lhs: HirLValue::Var(target),
        rhs,
    } = stmt
    else {
        return false; // Only Var-lhs assigns are hoistable.
    };
    // The target must not be re-defined elsewhere in the loop.
    if loop_defs.contains(target.as_str()) {
        return false;
    }
    // The RHS must be pure (no Load, no Call) and loop-invariant.
    is_pure_and_invariant(rhs, loop_defs)
}

/// Return `true` if `expr` contains no `Load`/`Call`/`AggregateCopy` and all
/// `Var` operands are not in `loop_defs`.
fn is_pure_and_invariant(expr: &HirExpr, loop_defs: &HashSet<String>) -> bool {
    match expr {
        HirExpr::Const(_, _) => true,
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => !loop_defs.contains(name.as_str()),
        HirExpr::Cast { expr: inner, .. } => is_pure_and_invariant(inner, loop_defs),
        HirExpr::Unary { expr: inner, .. } => is_pure_and_invariant(inner, loop_defs),
        HirExpr::Binary { lhs, rhs, .. } => {
            is_pure_and_invariant(lhs, loop_defs) && is_pure_and_invariant(rhs, loop_defs)
        }
        HirExpr::PtrOffset { base, .. } => is_pure_and_invariant(base, loop_defs),
        // Loads, calls, aggregate copies are never considered pure/invariant.
        HirExpr::Load { .. } | HirExpr::Call { .. } | HirExpr::AggregateCopy { .. } => false,
        HirExpr::Index { base, index, .. } => {
            // Array index expression can be invariant if both parts are.
            // We are conservative: only hoist if both are pure & invariant.
            is_pure_and_invariant(base, loop_defs) && is_pure_and_invariant(index, loop_defs)
        }
    }
}
