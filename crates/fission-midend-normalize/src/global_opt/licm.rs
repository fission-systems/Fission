use crate::analysis::liveness::LivenessTransfer;
/// Loop Invariant Code Motion (LICM) for HIR.
///
/// Identifies assignments at the start of `DoWhile` loops whose
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
///   For each DoWhile:
///     1. Collect loop_defs: how many times each Var is assigned in the body.
///     2. Scan the contiguous top-level prefix of the loop body:
///        For each Assign { lhs: Var(y), rhs: E }:
///          - If E contains no Load/Call (pure), AND
///          - all Var(v) in E satisfy v ∉ loop_defs, AND
///          - y is assigned exactly once in the loop
///          → mark as invariant.
///     3. Collect invariant statements into a "hoist" list; remove them from body.
///     4. Insert hoist list before the loop statement in the parent.
///   Return true if any hoisting occurred.
/// ```
///
/// ## Soundness
///
/// Only `Assign { lhs: Var(y), rhs: E }` in the contiguous prefix of a
/// `DoWhile` body are candidates.  Restricting the pass to `DoWhile` avoids
/// speculating an assignment when a `While`/`For` body executes zero times;
/// restricting it to the prefix avoids moving an assignment past a preceding
/// conditional or control-flow transfer.  Assignments inside nested
/// `if`/`while`/`for` are not hoisted.  Memory writes (`Deref`/`Index` lhs)
/// are never hoisted.
///
/// ## References
///
/// - LLVM `lib/Transforms/Scalar/LICM.cpp` (concept)
/// - Aho, Lam, Sethi, Ullman "Compilers" §9.5 (code motion)
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Apply LICM to all loops in `func`.  Returns `true` if any statement was
/// hoisted.
pub fn apply_licm_pass(func: &mut PreHirFunction) -> bool {
    hoist_in_stmts(&mut func.body)
}

/// Recursively process a statement list, applying LICM innermost-first.
///
/// Returns `true` if any hoisting occurred (so the caller can re-run cleanup).
fn hoist_in_stmts(stmts: &mut Vec<PreHirStmt>) -> bool {
    let live_after = HashSet::default();
    hoist_in_stmts_with_live_after(stmts, &live_after)
}

/// Process a statement list with names used by the enclosing suffix.  A
/// definition that is observed after the loop is deliberately not moved out
/// of the loop: preserving that def-use boundary keeps loop-carried and
/// preheader temporaries available to later normalization passes.
fn hoist_in_stmts_with_live_after(
    stmts: &mut Vec<PreHirStmt>,
    inherited_live_after: &HashSet<String>,
) -> bool {
    let mut changed = false;

    // First, recurse into nested bodies (innermost-first / post-order).
    // We do this before extracting loop-level info from *this* level.
    for idx in 0..stmts.len() {
        let live_after = live_after_index(stmts, idx, inherited_live_after);
        match &mut stmts[idx] {
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &live_after,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    hoist_single(s);
                }
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &live_after,
                ) {
                    changed = true;
                }
                if let Some(s) = update {
                    hoist_single(s);
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    &live_after,
                ) {
                    changed = true;
                }
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    &live_after,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::Block(body) => {
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &live_after,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    if hoist_in_stmts_with_live_after(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        &live_after,
                    ) {
                        changed = true;
                    }
                }
                if hoist_in_stmts_with_live_after(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    &live_after,
                ) {
                    changed = true;
                }
            }
            _ => {}
        }
    }

    // Now process *this* level: find loops and try to hoist.
    let mut i = 0;
    while i < stmts.len() {
        let live_after = live_after_index(stmts, i, inherited_live_after);
        let hoisted = match &stmts[i] {
            PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {
                extract_invariants_from_loop(&mut stmts[i], &live_after)
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

fn live_after_index(
    stmts: &[PreHirStmt],
    index: usize,
    inherited_live_after: &HashSet<String>,
) -> HashSet<String> {
    let mut live_after = inherited_live_after.clone();
    live_after.extend(
        LivenessTransfer::for_stmts(&stmts[index + 1..])
            .uses_before_definition()
            .map(str::to_owned),
    );
    live_after
}

/// Dummy to satisfy compiler when visiting init/update of For in inner pass.
fn hoist_single(_stmt: &mut PreHirStmt) {}

/// Extract loop-invariant assignments from the top-level body of `loop_stmt`.
///
/// Returns the list of hoisted assignments (removed from the loop body).
fn extract_invariants_from_loop(
    loop_stmt: &mut PreHirStmt,
    live_after: &HashSet<String>,
) -> Vec<PreHirStmt> {
    let body = match loop_stmt {
        // A `While` or `For` may not execute its body.  Without a liveness
        // proof for the target, moving an assignment before either loop would
        // be an observable speculative write.  `DoWhile` is the one loop
        // shape whose body is guaranteed to run at least once.
        PreHirStmt::DoWhile { body, .. } => body,
        _ => return vec![],
    };

    // 1. Count definitions anywhere in the loop body.  The candidate itself
    // must not make its own target look redefined.
    let mut loop_defs: HashMap<String, usize> = HashMap::default();
    collect_all_defs(body, &mut loop_defs);

    // 2. Identify a contiguous prefix of invariant assignments.  A later
    // assignment may be semantically invariant too, but hoisting it would
    // require reasoning about the control flow before it.
    let mut invariant_indices = vec![];
    for (idx, stmt) in body.iter().enumerate() {
        if is_invariant_stmt(stmt, &loop_defs, live_after) {
            invariant_indices.push(idx);
        } else {
            break;
        }
    }

    if invariant_indices.is_empty() {
        return vec![];
    }

    // 3. Remove them from the body (in reverse order to preserve indices).
    let mut hoisted = Vec::with_capacity(invariant_indices.len());
    for &idx in invariant_indices.iter().rev() {
        hoisted.push(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).remove(idx));
    }
    hoisted.reverse(); // Restore original order.
    hoisted
}

/// Collect all Var names that are **assigned** (defined) anywhere in `stmts`,
/// including in nested blocks.  Memory writes (Deref/Index lhs) are also noted
/// so that loads from those locations are treated as non-invariant.
fn collect_all_defs(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        collect_defs_in_stmt(stmt, out);
    }
}

fn collect_defs_in_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => {
            if let PreHirLValue::Var(name) = lhs {
                *out.entry(name.clone()).or_default() += 1;
            }
            // Memory writes are tracked as a sentinel key to block Load hoisting.
            // We use a special name that can never be a real variable.
            // (We only hoist pure non-Load expressions anyway, so this is a no-op
            // but makes the invariant check explicit.)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_all_defs(then_body, out);
            collect_all_defs(else_body, out);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            collect_all_defs(body, out);
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_all_defs(&case.body, out);
            }
            collect_all_defs(default, out);
        }
        PreHirStmt::Block(body) => collect_all_defs(body, out),
        _ => {}
    }
}

/// Return `true` if `stmt` is an assignment that is safe to hoist out of a
/// loop whose definitions are `loop_defs`.
fn is_invariant_stmt(
    stmt: &PreHirStmt,
    loop_defs: &HashMap<String, usize>,
    live_after: &HashSet<String>,
) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(target),
        rhs,
    } = stmt
    else {
        return false; // Only Var-lhs assigns are hoistable.
    };
    // The candidate itself accounts for one definition.  A second definition
    // means the target is not stable across iterations or paths.
    if loop_defs.get(target.as_str()).copied().unwrap_or(0) != 1 {
        return false;
    }
    // A value observed after the loop is part of the loop's def-use contract.
    // Keep its definition in the body so later passes can still recognize
    // loop-carried/preheader relationships.
    if live_after.contains(target) {
        return false;
    }
    // The RHS must be pure (no Load, no Call) and loop-invariant.
    is_pure_and_invariant(rhs, loop_defs)
}

/// Return `true` if `expr` contains no `Load`/`Call`/`AggregateCopy` and all
/// `Var` operands are not in `loop_defs`.
fn is_pure_and_invariant(expr: &PreHirExpr, loop_defs: &HashMap<String, usize>) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => !loop_defs.contains_key(name.as_str()),
        PreHirExpr::Cast { expr: inner, .. } => is_pure_and_invariant(inner, loop_defs),
        PreHirExpr::Unary { expr: inner, .. } => is_pure_and_invariant(inner, loop_defs),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            is_pure_and_invariant(lhs, loop_defs) && is_pure_and_invariant(rhs, loop_defs)
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            is_pure_and_invariant(base, loop_defs)
        }
        // Loads, calls, aggregate copies are never considered pure/invariant.
        PreHirExpr::Load { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::AggregateCopy { .. }
        | PreHirExpr::Select { .. } => false,
        PreHirExpr::Index { base, index, .. } => {
            // Array index expression can be invariant if both parts are.
            // We are conservative: only hoist if both are pure & invariant.
            is_pure_and_invariant(base, loop_defs) && is_pure_and_invariant(index, loop_defs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn do_while(body: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::DoWhile {
            body: Rc::new(body),
            cond: PreHirExpr::Var("loop_cond".to_owned()),
        }
    }

    #[test]
    fn hoists_a_pure_assignment_from_a_do_while_prefix() {
        let mut func = PreHirFunction {
            body: vec![do_while(vec![assign(
                "invariant",
                PreHirExpr::Const(7, u32_ty()),
            )])],
            ..Default::default()
        };

        assert!(apply_licm_pass(&mut func));
        assert!(matches!(
            &func.body[..],
            [PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Const(7, _),
            }, PreHirStmt::DoWhile { body, .. }] if name == "invariant" && body.is_empty()
        ));
    }

    #[test]
    fn does_not_hoist_when_the_target_has_another_definition() {
        let mut func = PreHirFunction {
            body: vec![do_while(vec![
                assign("value", PreHirExpr::Const(1, u32_ty())),
                assign("value", PreHirExpr::Const(2, u32_ty())),
            ])],
            ..Default::default()
        };

        assert!(!apply_licm_pass(&mut func));
        assert!(matches!(&func.body[0], PreHirStmt::DoWhile { body, .. } if body.len() == 2));
    }

    #[test]
    fn does_not_speculate_into_a_maybe_zero_iteration_loop() {
        let mut func = PreHirFunction {
            body: vec![PreHirStmt::While {
                cond: PreHirExpr::Var("loop_cond".to_owned()),
                body: Rc::new(vec![assign("value", PreHirExpr::Const(1, u32_ty()))]),
            }],
            ..Default::default()
        };

        assert!(!apply_licm_pass(&mut func));
        assert!(matches!(&func.body[0], PreHirStmt::While { body, .. } if body.len() == 1));
    }

    #[test]
    fn keeps_a_value_observed_after_the_loop_in_the_loop_body() {
        let mut func = PreHirFunction {
            body: vec![
                do_while(vec![assign("value", PreHirExpr::Const(1, u32_ty()))]),
                PreHirStmt::Return(Some(PreHirExpr::Var("value".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(!apply_licm_pass(&mut func));
        assert!(matches!(&func.body[0], PreHirStmt::DoWhile { body, .. } if body.len() == 1));
    }

    #[test]
    fn only_hoists_the_unconditional_prefix() {
        let mut func = PreHirFunction {
            body: vec![do_while(vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("guard".to_owned()),
                    then_body: Rc::new(vec![]),
                    else_body: Rc::new(vec![]),
                },
                assign("value", PreHirExpr::Const(1, u32_ty())),
            ])],
            ..Default::default()
        };

        assert!(!apply_licm_pass(&mut func));
        assert!(matches!(&func.body[0], PreHirStmt::DoWhile { body, .. } if body.len() == 2));
    }
}
