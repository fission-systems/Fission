//! Orphan-goto repair free functions (Ghidra `ruleBlockGoto` analog).

use crate::cleanup::{finalize_structured_body, has_orphan_goto_labels, orphan_goto_labels};
use crate::helpers::block_label;
use crate::host::StructuringHost;
use crate::linear_types::LoweredTerminator;
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::PreHirStmt;

/// Resolve a block index from a structured `block_<addr>` label.
pub fn find_block_index_by_label(host: &impl StructuringHost, label: &str) -> Option<usize> {
    for idx in 0..host.block_count() {
        if block_label(host.block_target_key(idx)) == label {
            return Some(idx);
        }
    }
    None
}

/// Emit a localized fragment for an orphan goto target block.
pub fn emit_orphan_target_block(
    host: &mut impl StructuringHost,
    block_idx: usize,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    let label = block_label(host.block_target_key(block_idx));
    let mut stmts = vec![PreHirStmt::Label(label)];
    stmts.extend(host.lower_block_stmts(block_idx)?);
    match host.lower_block_terminator(block_idx)? {
        LoweredTerminator::Return(expr) => stmts.push(PreHirStmt::Return(expr)),
        LoweredTerminator::Goto(target) => {
            stmts.push(PreHirStmt::Goto(block_label(target)));
        }
        LoweredTerminator::Fallthrough(Some(target)) => {
            if let Some(target_idx) = host.find_block_index_by_address(target)
                && let Some(expr) =
                    host.lower_return_join_expr_for_predecessor(block_idx, target_idx)?
            {
                stmts.push(PreHirStmt::Return(Some(expr)));
            } else {
                // Previously suppressed when `target` was the next block in
                // raw address order (`host.next_block_address(block_idx) ==
                // Some(target)`), on the assumption that address-adjacent
                // code would "naturally" appear right after this fragment
                // in the emitted output. That's only true if `target`'s
                // block hasn't been emitted anywhere else yet -- but an
                // orphan repair fragment is, by construction, being
                // appended standalone after code that may have already
                // rendered `target` (e.g. a loop header emitted once at the
                // top of the function, before the loop was recognized as a
                // loop). Suppressing the goto there drops the only
                // remaining link to that code, leaving this fragment with
                // no terminator at all. Always emit it; a redundant goto to
                // truly-adjacent code is harmless, a missing one isn't.
                stmts.push(PreHirStmt::Goto(block_label(target)));
            }
        }
        LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } => {
            let then_body = if let Some(true_idx) = host.find_block_index_by_address(true_target)
                && let Some(expr) =
                    host.lower_return_join_expr_for_predecessor(block_idx, true_idx)?
            {
                vec![PreHirStmt::Return(Some(expr))]
            } else {
                vec![PreHirStmt::Goto(block_label(true_target))]
            };
            let else_body = if let Some(false_target) = false_target {
                if let Some(false_idx) = host.find_block_index_by_address(false_target)
                    && let Some(expr) =
                        host.lower_return_join_expr_for_predecessor(block_idx, false_idx)?
                {
                    vec![PreHirStmt::Return(Some(expr))]
                } else {
                    vec![PreHirStmt::Goto(block_label(false_target))]
                }
            } else {
                Vec::new()
            };
            stmts.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(then_body),
                else_body: std::rc::Rc::new(else_body),
            });
        }
        LoweredTerminator::Fallthrough(None) => {}
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            stmts.push(host.emit_unsupported_control_surface(evidence, target_expr));
        }
        LoweredTerminator::Switch { .. } => {
            return Err(MlilPreviewError::UnsupportedCfgRegionShape);
        }
    }
    Ok(stmts)
}

/// Recursively search `stmts` for the first position where a contiguous run
/// matching `needle` already occurs, and insert `Label(label)` right before
/// it in place. Returns `true` on success (search stops at the first hit,
/// matching this module's existing first-match idiom).
///
/// `needle` must be non-empty; an empty needle would match everywhere and
/// isn't a meaningful "does this content already exist" question.
fn try_insert_label_before_existing_occurrence(
    stmts: &mut Vec<PreHirStmt>,
    needle: &[PreHirStmt],
    label: &str,
) -> bool {
    debug_assert!(!needle.is_empty());
    if stmts.len() >= needle.len() {
        for i in 0..=stmts.len() - needle.len() {
            if stmts[i..i + needle.len()] == *needle {
                stmts.insert(i, PreHirStmt::Label(label.to_string()));
                return true;
            }
        }
    }
    for stmt in stmts.iter_mut() {
        let found = match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => try_insert_label_before_existing_occurrence(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                needle,
                label,
            ),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                try_insert_label_before_existing_occurrence(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    needle,
                    label,
                ) || try_insert_label_before_existing_occurrence(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    needle,
                    label,
                )
            }
            PreHirStmt::Switch { cases, default, .. } => {
                let mut found = false;
                for case in cases.iter_mut() {
                    if try_insert_label_before_existing_occurrence(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        needle,
                        label,
                    ) {
                        found = true;
                        break;
                    }
                }
                found
                    || try_insert_label_before_existing_occurrence(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                        needle,
                        label,
                    )
            }
            _ => false,
        };
        if found {
            return true;
        }
    }
    false
}

/// Keep structured SESE output and localize orphan goto targets by appending
/// missing block labels/bodies instead of rebuilding the whole function.
///
/// Before re-materializing a target block from scratch, first checks whether
/// its body is *already* present somewhere in `body` sans label: some
/// structuring rules absorb a block into a larger linear sequence's own
/// statements without giving it an individually addressable label, and if a
/// goto from a different, unrelated branch of the function also needs to
/// reach that same block, the naive repair (always re-lower a fresh copy
/// from p-code and append it) duplicated real side-effecting code -- calls,
/// stores -- that should execute exactly once per run, not once per
/// reference. Inserting the missing label at the existing occurrence instead
/// keeps both the goto and the original single-copy semantics intact.
pub fn try_repair_orphan_gotos(
    host: &mut impl StructuringHost,
    body: Vec<PreHirStmt>,
) -> Option<Vec<PreHirStmt>> {
    if !has_orphan_goto_labels(&body) {
        return Some(body);
    }

    let protected = host.lsda_landing_pad_labels();
    let mut body = body;
    for _ in 0..host.block_count().saturating_add(8) {
        let orphans = orphan_goto_labels(&body);
        if orphans.is_empty() {
            return Some(finalize_structured_body(&protected, body));
        }

        let mut repaired_any = false;
        for label in orphans {
            let block_idx = find_block_index_by_label(host, &label)?;
            let fragment = emit_orphan_target_block(host, block_idx).ok()?;
            // `fragment` is `[Label, ...body_stmts, terminator]`. The body
            // stmts are a pure function of the block's own p-code (unlike
            // the terminator, which can vary by predecessor via
            // `lower_return_join_expr_for_predecessor`), so they're the
            // reliable part to match against already-emitted content.
            let core: &[PreHirStmt] = if fragment.len() > 2 {
                &fragment[1..fragment.len() - 1]
            } else {
                &[]
            };
            if !core.is_empty()
                && try_insert_label_before_existing_occurrence(&mut body, core, &label)
            {
                repaired_any = true;
                continue;
            }
            body.extend(fragment);
            repaired_any = true;
        }

        if !repaired_any {
            return None;
        }
        body = finalize_structured_body(&protected, body);
        if !has_orphan_goto_labels(&body) {
            return Some(body);
        }
    }

    if has_orphan_goto_labels(&body) {
        None
    } else {
        Some(body)
    }
}
