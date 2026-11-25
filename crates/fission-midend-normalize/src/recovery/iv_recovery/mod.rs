//! Loop induction variable (IV) recovery + break/continue accuracy pass.
//!
//! ## Part A — SCEV-lite: Linear Recurrence → `for` upgrade
//!
//! After `apply_for_loop_folding` has already converted simple `while` loops
//! to `for` loops based on *syntactic* patterns (init/update adjacent in the
//! AST), some loops are still `While { cond, body }` because the initial
//! `for_loop_folding` could not identify a clear preceding assignment.
//!
//! This pass looks deeper using a data-flow criterion:
//!
//! 1. Identify all variables that appear in the loop condition (`cond_vars`).
//! 2. For each cond_var, check if:
//!    (a) There is exactly one assignment to it *before* the loop in the same
//!        flat statement list (the "init" assignment).
//!    (b) The loop body contains exactly one assignment of the form
//!        `v = v ± k` where `k` does not mention any loop-variant variable
//!        (the "update").
//! 3. If both are found and there is no `Continue` in the loop (updating
//!    semantics are preserved), convert the `While` to `For`.
//!
//! This is a conservative subset of full SCEV: only *linear* recurrences with
//! loop-invariant steps, no irreducible or multi-update cases.  The algorithm
//! is entirely syntax-driven on the HIR and has no binary-specific thresholds.
//! ## Part B — Break/Continue recovery
//!
//! Inside every loop body (While/DoWhile/For), scan for:
//!
//! ```text
//! If { cond, then_body: [Goto(label)], else_body: [] }
//! ```
//!
//! where:
//! - `label` is a label that appears *after* the loop (exit) → replace with
//!   `If { cond, then_body: [Break], else_body: [] }` and remove the now-dead
//!   label if it has no other predecessors.
//! - `label` is a label that appears immediately before the loop (loop head)
//!   → replace with `If { cond, then_body: [Continue], else_body: [] }`.
//!
//! The detection is structural and label-based; no CFG reachability analysis
//! is required.  The replacement is only performed when the label has exactly
//! one incoming `Goto` (the one being replaced), so the label can also be
//! removed afterwards.
//!
//! ## Part C — Tail label loops → break-guarded `for (;;)`
//!
//! Structuring can leave a reducible tail loop as:
//!
//! ```text
//! L:
//!   body
//!   if (continue_cond) goto L
//! ```
//!
//! when interleaved labels inside the body prevent direct loop emission.  When
//! `L` has exactly one incoming goto and every remaining body goto is local to
//! the loop body, this pass lowers the shape to:
//!
//! ```text
//! for (;;) {
//!   body
//!   if (!continue_cond) break;
//! }
//! ```
//!
//! This preserves do-while entry semantics without requiring a separate
//! preheader proof, while still removing the outer label/goto recurrence.

mod break_continue;
mod counted_loops;
mod indexed_cursor_rewrite;
mod labels;
mod loop_expressions;
mod loop_latch;
mod merged_induction;
mod pointer_cursor_loops;
mod recurrences;
mod row_stride_fill;
mod tail_label_loops;
mod variable_uses;

#[cfg(test)]
mod tests;

use crate::HashMap;
use crate::prelude::*;
use break_continue::apply_break_continue_in_stmts;
use counted_loops::apply_scev_upgrade_in_stmts;
use labels::count_goto_targets;
use tail_label_loops::apply_tail_label_loop_recovery_in_stmts;

/// Apply IV-to-For upgrade (SCEV-lite) across the entire function body.
/// Returns `true` if any transformation was made.
pub fn apply_iv_recovery_pass(func: &mut PreHirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::default();
    count_goto_targets(&func.body, &mut goto_counts);
    let mut active_guards = Vec::new();
    apply_tail_label_loop_recovery_in_stmts(&mut func.body, &goto_counts)
        | apply_scev_upgrade_in_stmts(&mut func.body, &mut func.locals, &mut active_guards)
}

/// Apply break/continue recovery across the entire function body.
/// Returns `true` if any transformation was made.
pub fn apply_break_continue_pass(func: &mut PreHirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::default();
    count_goto_targets(&func.body, &mut goto_counts);
    apply_break_continue_in_stmts(&mut func.body, &goto_counts)
}
