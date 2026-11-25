//! HIR presentation pass — readability-only tree polish before HIR print.
//!
//! Contract: [`docs/adr/0011-hir-presentation-contract.md`] and `render/AGENTS.md`.
//!
//! - Clone-only: NIR print uses the pre-presentation tree.
//! - Preserve evaluation count/order for calls and loads (no double-eval inlines).
//! - Structural invariants only — no function/address/binary special cases.
//! - Semantic recovery stays in normalize/structuring.
//! - Post-pass structural firewall ([`invariants`]): on violation, restore pre-polish tree.

mod alias_propagation;
mod cleanup;
mod condition_normalization;
mod counts;
mod goto_recovery;
mod inline;
mod invariants;
mod loop_folds;
mod naming;
mod purity;
mod seed_folding;
mod stmt_folds;
mod switch_recovery;
#[cfg(test)]
mod tests;
mod var_rewrite;

use super::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, NirBinding,
    NirBindingOrigin, NirType,
};
use alias_propagation::propagate_pure_var_aliases;
use cleanup::{
    drop_unused_presentation_locals, eliminate_pure_dead_assigns, simplify_presentation_casts,
};
use condition_normalization::{
    body_is_effectively_empty, canonicalize_conditions_in_expr,
    canonicalize_presentation_conditions, expr_result_type, fold_redundant_select_return_join,
    single_return_expr, single_var_assign,
};
use counts::*;
use fission_midend_core::ir::HirSwitchCase;
use inline::*;
use invariants::check_hir_presentation_invariants;
use loop_folds::*;
use purity::*;
use seed_folding::{fold_seed_transform, fold_self_update_after_seed};
use std::collections::{HashMap, HashSet};
use stmt_folds::*;
use var_rewrite::*;

/// Apply HIR-facing presentation polish in place.
///
/// On structural invariant failure (use-without-def, call/load inflation, empty
/// if shells), restores the pre-polish tree so broken presentation never ships.
pub(crate) fn apply_hir_presentation(func: &mut HirFunction) {
    apply_hir_presentation_with_globals(func, &HashSet::new());
}

/// As `apply_hir_presentation`, told which names denote file-scope globals.
///
/// Presentation's dead-assignment pass reasons about values: a name nothing
/// reads back holds nothing worth printing. That is true of a local and false
/// of a global -- writing one is the observable effect, whether or not this
/// function reads it again -- so without the set it silently deleted stores
/// like `max_flush_loops = rax;` from the emitted C.
pub(crate) fn apply_hir_presentation_with_globals(
    func: &mut HirFunction,
    globals: &HashSet<String>,
) {
    let before = func.clone();
    apply_hir_presentation_passes(func, globals);
    if let Err(_violations) = check_hir_presentation_invariants(&before, func) {
        // Prefer mechanical NIR-shaped tree over observationally broken HIR polish.
        *func = before;
        #[cfg(debug_assertions)]
        {
            // Surface in debug builds without aborting release decomp paths.
            eprintln!(
                "hir presentation invariants failed; restored pre-polish tree: {_violations:?}"
            );
        }
    }
}

fn apply_hir_presentation_passes(func: &mut HirFunction, globals: &HashSet<String>) {
    for _ in 0..16 {
        let mut changed = false;
        changed |= flatten_redundant_blocks(&mut func.body);
        changed |= propagate_pure_var_aliases(func, globals);
        changed |= fold_self_update_after_seed(&mut func.body);
        changed |= fold_seed_transform(&mut func.body);
        // Shared `goto L; ... L: return e` → direct returns (enables if-else recovery).
        changed |= goto_recovery::expand_goto_shared_returns(&mut func.body);
        changed |= collapse_trivial_assign_returns(&mut func.body, globals);
        // O0-style `if (c) goto L; body; L:` → structured if/else for readability.
        let mut global_goto_refs = HashMap::new();
        goto_recovery::collect_goto_ref_counts(&func.body, &mut global_goto_refs);
        changed |= goto_recovery::recover_if_else_from_gotos(&mut func.body, &global_goto_refs);
        // O0 while: `goto Lcond; Lbody: …; Lcond: if (c) goto Lbody;`
        changed |= goto_recovery::recover_while_from_gotos(&mut func.body);
        // Structuring often emits `while (1) { if (!c) break; body }` → `while (c)`.
        changed |= fold_while_true_break_guard(&mut func.body);
        // `i = 0; while (i < n) { …; i = i + 1; }` → `for (i = 0; i < n; i = i + 1)`.
        changed |= fold_seed_while_to_for(&mut func.body);
        // `if (x==c1) {..} else if (x==c2) {..} else ..` → `switch(x) {..}`
        // (GCC/Clang "switch lowering": a small or sparse-case switch
        // compiled to an if-chain instead of a jump table). Runs before the
        // pure-value/return-to-select folds below, which want the same
        // 2-arm `if`/`else` shape a 3+-arm chain also matches one level at
        // a time -- letting them go first would eat the chain into nested
        // ternaries before this pass ever sees it as a chain.
        changed |= switch_recovery::recover_switch_from_lowered_if_chain(&mut func.body);
        // `if (c) { x = a; } else { x = b; }` → `x = c ? a : b` (pure values only).
        changed |= fold_if_else_pure_same_var_assign(&mut func.body);
        // `if (c) { return a; } else { return b; }` is deliberately NOT folded to
        // `return c ? a : b`. The two forms are the same program, but the `if`
        // is the structure the code was written in, and the fold nests into
        // `a ? b ? x : y : z` chains that read worse than the statements they
        // replace. Measured on DecBench with the fold removed: O0 distance
        // +0.87% -> +0.06% against the old mechanical print, O2 neutral.
        // `return x==10 ? 2 : x>10 ? DFLT : ...` → `switch (x) {..}` -- the
        // *other* real "switch lowering" shape (a binary-search-style
        // decision tree, already collapsed into one nested Select by the
        // fold just above), companion to the linear if-chain recovery
        // earlier in this loop. Must run after that fold settles the tree
        // shape, not before.
        changed |= switch_recovery::recover_switch_from_select_decision_tree(&mut func.body);
        // `if (c) { return a; } return b;` → `return c ? a : b` (pure values).
        let mut fallthrough_goto_targets = HashSet::new();
        goto_recovery::collect_goto_targets(&func.body, &mut fallthrough_goto_targets);
        changed |= fold_if_return_fallthrough_return(&mut func.body, &fallthrough_goto_targets);
        // `x = seed; if (c) { x = a; }` → `x = c ? a : seed` (pure; empty else).
        changed |= fold_seed_if_overwrite_assign(&mut func.body);
        // Drop empty `else {}` arms after other folds.
        changed |= strip_empty_else_arms(&mut func.body);
        // `if (c) {} else { body }` → `if (!c) { body }`.
        changed |= fold_empty_then_invert_else(&mut func.body);
        // Prefer `x op k` over `k op x` in comparisons (and peel `!(eq/ne)` etc.).
        changed |= canonicalize_presentation_conditions(func);
        // `x = c ? a : b; return ~c ? x : a` (null-check join) → `return x` / fold.
        changed |= fold_redundant_select_return_join(&mut func.body, globals);
        let mut goto_targets = HashSet::new();
        goto_recovery::collect_goto_targets(&func.body, &mut goto_targets);
        changed |=
            goto_recovery::prune_unreachable_after_total_return(&mut func.body, &goto_targets);
        changed |= inline_single_use_pure_assigns(func, globals);
        changed |= eliminate_pure_dead_assigns(func, globals);
        changed |= goto_recovery::remove_unreferenced_labels(&mut func.body);
        if !changed {
            break;
        }
    }
    // Once, after the structural fixed-point settles: loop-counter detection
    // needs the final While/For/DoWhile shape, and running once avoids
    // transient intermediate shapes producing conflicting guesses.
    naming::apply_semantic_naming(func);
    simplify_presentation_casts(func);
    drop_unused_presentation_locals(func);
}
