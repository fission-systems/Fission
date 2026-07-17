//! Orphan-goto repair free functions (Ghidra `ruleBlockGoto` analog).

use crate::cleanup::{finalize_structured_body, has_orphan_goto_labels, orphan_goto_labels};
use crate::helpers::block_label;
use crate::host::StructuringHost;
use crate::linear_types::LoweredTerminator;
use fission_midend_core::ir::{HirStmt, MlilPreviewError};

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
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    let label = block_label(host.block_target_key(block_idx));
    let mut stmts = vec![HirStmt::Label(label)];
    stmts.extend(host.lower_block_stmts(block_idx)?);
    match host.lower_block_terminator(block_idx)? {
        LoweredTerminator::Return(expr) => stmts.push(HirStmt::Return(expr)),
        LoweredTerminator::Goto(target) => {
            if host.next_block_address(block_idx) != Some(target) {
                stmts.push(HirStmt::Goto(block_label(target)));
            }
        }
        LoweredTerminator::Fallthrough(Some(target)) => {
            if let Some(target_idx) = host.find_block_index_by_address(target)
                && let Some(expr) =
                    host.lower_return_join_expr_for_predecessor(block_idx, target_idx)?
            {
                stmts.push(HirStmt::Return(Some(expr)));
            } else if host.next_block_address(block_idx) != Some(target) {
                stmts.push(HirStmt::Goto(block_label(target)));
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
                vec![HirStmt::Return(Some(expr))]
            } else {
                vec![HirStmt::Goto(block_label(true_target))]
            };
            let else_body = if let Some(false_target) = false_target {
                if let Some(false_idx) = host.find_block_index_by_address(false_target)
                    && let Some(expr) =
                        host.lower_return_join_expr_for_predecessor(block_idx, false_idx)?
                {
                    vec![HirStmt::Return(Some(expr))]
                } else {
                    vec![HirStmt::Goto(block_label(false_target))]
                }
            } else {
                Vec::new()
            };
            stmts.push(HirStmt::If {
                cond,
                then_body,
                else_body,
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

/// Keep structured SESE output and localize orphan goto targets by appending
/// missing block labels/bodies instead of rebuilding the whole function.
pub fn try_repair_orphan_gotos(
    host: &mut impl StructuringHost,
    body: Vec<HirStmt>,
) -> Option<Vec<HirStmt>> {
    if !has_orphan_goto_labels(&body) {
        return Some(body);
    }

    let mut body = body;
    for _ in 0..host.block_count().saturating_add(8) {
        let orphans = orphan_goto_labels(&body);
        if orphans.is_empty() {
            return Some(finalize_structured_body(body));
        }

        let mut repaired_any = false;
        for label in orphans {
            let block_idx = find_block_index_by_label(host, &label)?;
            let fragment = emit_orphan_target_block(host, block_idx).ok()?;
            body.extend(fragment);
            repaired_any = true;
        }

        if !repaired_any {
            return None;
        }
        body = finalize_structured_body(body);
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
