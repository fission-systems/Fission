//! Loop structuring free functions (`try_lower_while`, for, do-while, infloop, subgraph).
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`.

use crate::HashSet;
use crate::conditionals::{
    is_trivial_structuring_stmt, try_lower_if, try_lower_if_else, try_lower_short_circuit_if,
};
use crate::graph::capture_structuring_failure;
use crate::helpers::{block_label, recovered_switch_case_values};
use crate::host::StructuringHost;
use crate::linear_types::{
    IfLoweringBudget, LinearExit, LoweredTerminator, STRUCTURING_TOTAL_WORK_BUDGET,
    structuring_diag_enabled,
};
use crate::switch::try_lower_switch;
use fission_midend_core::ir::{MlilPreviewError, NirType};
use fission_midend_prehir::util::negate_expr;
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt, PreHirSwitchCase};

// ---------------------------------------------------------------------------
// Loop-context-aware break/continue rewriting
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LoopControlRewriteStats {
    break_rewrites: usize,
    continue_rewrites: usize,
    skipped_nested_scope_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScopeFrame {
    Loop {
        continue_labels: std::collections::HashSet<String>,
        break_labels: std::collections::HashSet<String>,
    },
    Switch {
        break_labels: std::collections::HashSet<String>,
    },
}

fn rewrite_loop_control_gotos_with_stack(
    stmts: &mut [PreHirStmt],
    stack: &mut Vec<ScopeFrame>,
    stats: &mut LoopControlRewriteStats,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Goto(label) => {
                let target_label = label.clone();
                // 1. try matching continue: scan top-to-bottom for the innermost Loop frame
                let mut continue_matched = false;
                for frame in stack.iter().rev() {
                    if let ScopeFrame::Loop {
                        continue_labels, ..
                    } = frame
                    {
                        if continue_labels.contains(&target_label) {
                            *stmt = PreHirStmt::Continue;
                            stats.continue_rewrites += 1;
                            continue_matched = true;
                        }
                        break;
                    }
                }
                if continue_matched {
                    continue;
                }

                // 2. try matching break: only check the innermost frame (top of stack)
                if let Some(innermost) = stack.last() {
                    let break_matched = match innermost {
                        ScopeFrame::Loop { break_labels, .. } => {
                            break_labels.contains(&target_label)
                        }
                        ScopeFrame::Switch { break_labels } => break_labels.contains(&target_label),
                    };
                    if break_matched {
                        *stmt = PreHirStmt::Break;
                        stats.break_rewrites += 1;
                        continue;
                    }
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    stack,
                    stats,
                );
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    stack,
                    stats,
                );
            }
            PreHirStmt::Block(body) => {
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    stack,
                    stats,
                );
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Loop {
                    continue_labels: std::collections::HashSet::default(),
                    break_labels: std::collections::HashSet::default(),
                });
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    stack,
                    stats,
                );
                stack.pop();
            }
            PreHirStmt::For { body, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Loop {
                    continue_labels: std::collections::HashSet::default(),
                    break_labels: std::collections::HashSet::default(),
                });
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    stack,
                    stats,
                );
                stack.pop();
            }
            PreHirStmt::Switch { cases, default, .. } => {
                stats.skipped_nested_scope_count += 1;
                stack.push(ScopeFrame::Switch {
                    break_labels: std::collections::HashSet::default(),
                });
                for case in cases {
                    rewrite_loop_control_gotos_with_stack(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        stack,
                        stats,
                    );
                }
                rewrite_loop_control_gotos_with_stack(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    stack,
                    stats,
                );
                stack.pop();
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Label(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn rewrite_loop_control_gotos_in_stmts(
    stmts: &mut [PreHirStmt],
    continue_label: Option<&str>,
    break_label: Option<&str>,
    stats: &mut LoopControlRewriteStats,
) {
    let mut continue_labels = std::collections::HashSet::default();
    if let Some(cl) = continue_label {
        continue_labels.insert(cl.to_string());
    }
    let mut break_labels = std::collections::HashSet::default();
    if let Some(bl) = break_label {
        break_labels.insert(bl.to_string());
    }

    let mut stack = vec![ScopeFrame::Loop {
        continue_labels,
        break_labels,
    }];
    rewrite_loop_control_gotos_with_stack(stmts, &mut stack, stats);
}

fn rewrite_loop_control_gotos_multi(
    stmts: &mut [PreHirStmt],
    continue_labels: &std::collections::HashSet<String>,
    break_labels: &std::collections::HashSet<String>,
    stats: &mut LoopControlRewriteStats,
) {
    let mut stack = vec![ScopeFrame::Loop {
        continue_labels: continue_labels.clone(),
        break_labels: break_labels.clone(),
    }];
    rewrite_loop_control_gotos_with_stack(stmts, &mut stack, stats);
}

fn collect_defined_labels(stmts: &[PreHirStmt], labels: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(label) => {
                labels.insert(label.clone());
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defined_labels(then_body, labels);
                collect_defined_labels(else_body, labels);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                collect_defined_labels(body, labels);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defined_labels(&case.body, labels);
                }
                collect_defined_labels(default, labels);
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn has_goto_to_undefined_label(stmts: &[PreHirStmt]) -> bool {
    let mut labels = HashSet::default();
    collect_defined_labels(stmts, &mut labels);
    stmts_have_goto_to_undefined_label(stmts, &labels)
}

fn stmts_have_goto_to_undefined_label(stmts: &[PreHirStmt], labels: &HashSet<String>) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_has_goto_to_undefined_label(stmt, labels))
}

fn stmt_has_goto_to_undefined_label(stmt: &PreHirStmt, labels: &HashSet<String>) -> bool {
    match stmt {
        PreHirStmt::Goto(label) => !labels.contains(label),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmts_have_goto_to_undefined_label(then_body, labels)
                || stmts_have_goto_to_undefined_label(else_body, labels)
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => stmts_have_goto_to_undefined_label(body, labels),
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmts_have_goto_to_undefined_label(&case.body, labels))
                || stmts_have_goto_to_undefined_label(default, labels)
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn try_lower_infloop_with_break(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let block_addr = host.block_start_address(idx);
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };

    let candidate = if true_target == block_addr {
        false_target.map(|addr| (negate_expr(cond), addr))
    } else if false_target == Some(block_addr) {
        Some((cond, true_target))
    } else {
        None
    };
    let Some((break_cond, break_addr)) = candidate else {
        return Ok(None);
    };

    let Some(exit_idx) = host.find_block_index_by_address(break_addr) else {
        return Ok(None);
    };
    if exit_idx == idx {
        return Ok(None);
    }

    let mut body = host.lower_block_stmts(idx)?;
    body.push(PreHirStmt::If {
        cond: break_cond,
        then_body: std::rc::Rc::new(vec![PreHirStmt::Break]),
        else_body: std::rc::Rc::new(Vec::new()),
    });
    host.bump_loop_control_explicit_reducer();

    Ok(Some((
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: std::rc::Rc::new(body),
        },
        exit_idx,
    )))
}

pub fn try_lower_infloop(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    if host.successors().get(idx).map(|s| s.len()).unwrap_or(0) != 1 {
        return Ok(None);
    }
    let block_addr = host.block_start_address(idx);
    let terminator = host.lower_block_terminator(idx)?;
    let loops_to_self = matches!(
        terminator,
        LoweredTerminator::Goto(target) if target == block_addr
    ) || matches!(
        terminator,
        LoweredTerminator::Fallthrough(Some(target)) if target == block_addr
    );
    if !loops_to_self {
        return Ok(None);
    }

    let body = host.lower_block_stmts(idx)?;
    let mut body = body;
    let continue_label = block_label(block_addr);
    let mut stats = LoopControlRewriteStats::default();
    rewrite_loop_control_gotos_in_stmts(&mut body, Some(&continue_label), None, &mut stats);
    host.track_loop_control_rewrite_stats(
        stats.break_rewrites,
        stats.continue_rewrites,
        stats.skipped_nested_scope_count,
    );
    Ok(Some((
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: std::rc::Rc::new(body),
        },
        idx + 1,
    )))
}

pub fn try_lower_dowhile(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let Some((mut body, cond, cond_idx, skip_to)) = lower_do_while_region(host, idx)? else {
        return Ok(None);
    };
    let continue_label = block_label(host.block_target_key(cond_idx));
    let break_label = block_label(host.block_target_key(skip_to));
    let mut stats = LoopControlRewriteStats::default();
    rewrite_loop_control_gotos_in_stmts(
        &mut body,
        Some(&continue_label),
        Some(&break_label),
        &mut stats,
    );
    host.track_loop_control_rewrite_stats(
        stats.break_rewrites,
        stats.continue_rewrites,
        stats.skipped_nested_scope_count,
    );
    Ok(Some((
        PreHirStmt::DoWhile {
            body: std::rc::Rc::new(body),
            cond,
        },
        skip_to,
    )))
}

/// Deterministic replacement for the old shared 5000ms wall-clock "total
/// structuring" pre-check: counts checkpoint calls (shared with
/// `IfLoweringBudget::checkpoint`) instead of measuring elapsed time, so
/// how far a structuring attempt gets no longer depends on machine speed.
fn structuring_total_work_budget_exceeded(host: &impl StructuringHost) -> bool {
    let counter = host.structuring_total_work_counter();
    let total = counter.get() + 1;
    counter.set(total);
    total > STRUCTURING_TOTAL_WORK_BUDGET
}

// ---------------------------------------------------------------------------
// Condition-prefix folding: fold a trivial pure temp-assignment chain that
// only feeds the branch condition directly into the condition expression,
// so a head that materializes its comparison through an intermediate
// variable (the common -O0 "flag temp" pattern) still gets the clean
// `while(cond)` shape instead of the conservative `while(1){prefix; if
// (!cond) break; ...}` guard. Ghidra avoids needing this because its
// pcode-level copy-propagation already eliminates such temps before
// structuring ever runs; Fission's structuring stage sees the raw prefix.
// ---------------------------------------------------------------------------

fn count_var_refs(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(n) => usize::from(n == name),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => 0,
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            count_var_refs(expr, name)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_refs(lhs, name) + count_var_refs(rhs, name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_refs(cond, name)
                + count_var_refs(then_expr, name)
                + count_var_refs(else_expr, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(|a| count_var_refs(a, name)).sum(),
        PreHirExpr::Load { ptr, .. } => count_var_refs(ptr, name),
        PreHirExpr::PtrOffset { base, .. } => count_var_refs(base, name),
        PreHirExpr::Index { base, index, .. } => {
            count_var_refs(base, name) + count_var_refs(index, name)
        }
        PreHirExpr::FieldAccess { base, .. } => count_var_refs(base, name),
        PreHirExpr::AggregateCopy { src, .. } => count_var_refs(src, name),
    }
}

fn substitute_var(expr: &PreHirExpr, name: &str, replacement: &PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Var(n) if n == name => replacement.clone(),
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => expr.clone(),
        PreHirExpr::Cast { ty, expr: inner } => PreHirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(substitute_var(inner, name, replacement)),
        },
        PreHirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => PreHirExpr::Unary {
            op: *op,
            expr: Box::new(substitute_var(inner, name, replacement)),
            ty: ty.clone(),
        },
        PreHirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: *op,
            lhs: Box::new(substitute_var(lhs, name, replacement)),
            rhs: Box::new(substitute_var(rhs, name, replacement)),
            ty: ty.clone(),
        },
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => PreHirExpr::Select {
            cond: Box::new(substitute_var(cond, name, replacement)),
            then_expr: Box::new(substitute_var(then_expr, name, replacement)),
            else_expr: Box::new(substitute_var(else_expr, name, replacement)),
            ty: ty.clone(),
        },
        PreHirExpr::Call { target, args, ty } => PreHirExpr::Call {
            target: target.clone(),
            args: args
                .iter()
                .map(|a| substitute_var(a, name, replacement))
                .collect(),
            ty: ty.clone(),
        },
        PreHirExpr::Load { ptr, ty } => PreHirExpr::Load {
            ptr: Box::new(substitute_var(ptr, name, replacement)),
            ty: ty.clone(),
        },
        PreHirExpr::PtrOffset { base, offset } => PreHirExpr::PtrOffset {
            base: Box::new(substitute_var(base, name, replacement)),
            offset: *offset,
        },
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => PreHirExpr::Index {
            base: Box::new(substitute_var(base, name, replacement)),
            index: Box::new(substitute_var(index, name, replacement)),
            elem_ty: elem_ty.clone(),
        },
        PreHirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => PreHirExpr::FieldAccess {
            base: Box::new(substitute_var(base, name, replacement)),
            field_name: field_name.clone(),
            offset: *offset,
            ty: ty.clone(),
        },
        PreHirExpr::AggregateCopy { src, size } => PreHirExpr::AggregateCopy {
            src: Box::new(substitute_var(src, name, replacement)),
            size: *size,
        },
    }
}

fn lvalue_references_var(lv: &PreHirLValue, name: &str) -> bool {
    match lv {
        PreHirLValue::Var(n) => n == name,
        PreHirLValue::Deref { ptr, .. } => count_var_refs(ptr, name) > 0,
        PreHirLValue::Index { base, index, .. } => {
            count_var_refs(base, name) > 0 || count_var_refs(index, name) > 0
        }
        PreHirLValue::FieldAccess { base, .. } => count_var_refs(base, name) > 0,
    }
}

fn stmt_references_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_references_var(lhs, name) || count_var_refs(rhs, name) > 0
        }
        PreHirStmt::Expr(expr) => count_var_refs(expr, name) > 0,
        PreHirStmt::VaStart { va_list, .. } => count_var_refs(va_list, name) > 0,
        PreHirStmt::Block(body) => stmts_reference_var(body, name),
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_refs(expr, name) > 0
                || cases.iter().any(|c| stmts_reference_var(&c.body, name))
                || stmts_reference_var(default, name)
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_refs(cond, name) > 0
                || stmts_reference_var(then_body, name)
                || stmts_reference_var(else_body, name)
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            count_var_refs(cond, name) > 0 || stmts_reference_var(body, name)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|s| stmt_references_var(s, name))
                || cond.as_ref().is_some_and(|c| count_var_refs(c, name) > 0)
                || update
                    .as_deref()
                    .is_some_and(|s| stmt_references_var(s, name))
                || stmts_reference_var(body, name)
        }
        PreHirStmt::Label(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {
            false
        }
        PreHirStmt::Return(expr) => expr.as_ref().is_some_and(|e| count_var_refs(e, name) > 0),
    }
}

fn stmts_reference_var(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts.iter().any(|s| stmt_references_var(s, name))
}

/// True only when the *first* body statement (in program order) that
/// touches `name` at all is a plain top-level `name = rhs` overwrite whose
/// own rhs doesn't itself read `name` -- i.e. every possible execution path
/// through `body` clobbers the variable before any read can observe the
/// value a dropped cond-prefix statement would have left behind. Any other
/// shape (first touch inside an `If`/`Switch`/nested loop, a self-referential
/// assign, or a plain read) is conservatively unsafe.
fn body_overwrites_before_any_read(body: &[PreHirStmt], name: &str) -> bool {
    for stmt in body {
        if !stmt_references_var(stmt, name) {
            continue;
        }
        return match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(n),
                rhs,
            } if n == name => count_var_refs(rhs, name) == 0,
            _ => false,
        };
    }
    false
}

/// Fold `cond_prefix` (already proven all-trivial/side-effect-free by the
/// caller) directly into `cond`, processing statements innermost-use-first
/// (reverse program order) so chained temps (`a = f(x); b = g(a); if (b)`)
/// resolve correctly. Bails (returns `None`, leaving the existing
/// conservative shape untouched) unless every prefix variable is either (a)
/// used exactly once in the condition built so far, and gets substituted
/// in, or (b) unused (0 references) in both the condition-so-far and
/// `body`, and gets dropped as dead code -- the common case for a raw
/// x86 CMP's unused flag byproducts (PF/OF/etc. computed alongside the ZF
/// the branch actually reads). `body` must never reference a prefix
/// variable that survives folding/dropping: folding moves the computation
/// into a per-iteration re-evaluated `while` condition, so any other read
/// of the same variable inside the loop body would otherwise go stale.
fn try_fold_cond_prefix(
    cond_prefix: &[PreHirStmt],
    cond: &PreHirExpr,
    body: &[PreHirStmt],
) -> Option<PreHirExpr> {
    if cond_prefix.is_empty() {
        return None;
    }
    let diag = structuring_diag_enabled();
    let mut folded = cond.clone();
    for stmt in cond_prefix.iter().rev() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = stmt
        else {
            if diag {
                eprintln!(
                    "[DIAG] try_fold_cond_prefix bail: reason=not_var_assign stmt={:?}",
                    stmt
                );
            }
            return None;
        };
        if stmts_reference_var(body, name) && !body_overwrites_before_any_read(body, name) {
            if diag {
                eprintln!(
                    "[DIAG] try_fold_cond_prefix bail: reason=used_in_body var={}",
                    name
                );
            }
            return None;
        }
        match count_var_refs(&folded, name) {
            0 => {
                // Dead byproduct (e.g. an unused CMP flag) -- drop it, but
                // only if its own rhs doesn't reach outside this prefix
                // chain (it can't: `rhs` was already required trivial by
                // the caller, and any prefix var it reads gets the same
                // 0/1-use check on its own turn below).
            }
            1 => folded = substitute_var(&folded, name, rhs),
            n => {
                if diag {
                    eprintln!(
                        "[DIAG] try_fold_cond_prefix bail: reason=multi_use var={} count={}",
                        name, n
                    );
                }
                return None;
            }
        }
    }
    Some(folded)
}

pub fn try_lower_while(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    try_lower_while_impl(host, idx, false)
}

/// Lower an already graph-admitted rotated natural loop. Unlike the ordinary
/// speculative reducer, its body is recursively structured inside the proven
/// loop membership set before the loop node is committed.
pub fn try_lower_rotated_while(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    try_lower_while_impl(host, idx, true)
}

fn try_lower_while_impl(
    host: &mut impl StructuringHost,
    idx: usize,
    structure_body_members: bool,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    if structuring_total_work_budget_exceeded(host) {
        return Ok(None);
    }

    let diag = structuring_diag_enabled();
    let block_addr = host.block_start_address(idx);
    let mut budget = IfLoweringBudget::new(
        host.options(),
        idx,
        block_addr,
        "try_lower_while",
        host.structuring_total_work_counter(),
    );
    if diag {
        eprintln!(
            "[DIAG] try_lower_while start: idx={} block=0x{:x} x86_guard={}",
            idx, block_addr, budget.enabled
        );
    }

    let result = (|| {
        if budget.checkpoint("terminator_pre") {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = host.lower_block_terminator(idx)?
        else {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=non_conditional_head",
                    idx, block_addr
                );
            }
            return Ok(None);
        };
        if budget.checkpoint("terminator_post") {
            return Ok(None);
        }

        if budget.checkpoint("cond_prefix_pre") {
            return Ok(None);
        }
        let cond_prefix = host.lower_block_stmts(idx)?;
        if budget.checkpoint("cond_prefix_post") {
            return Ok(None);
        }
        if !cond_prefix.iter().all(is_trivial_structuring_stmt) {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=nontrivial_condition_prefix stmt_count={}",
                    idx,
                    block_addr,
                    cond_prefix.len()
                );
            }
            return Ok(None);
        }

        let loop_body = host.get_loop_body(idx);

        // While loops should always have an exit target
        let Some(exit_idx) = loop_body.and_then(|lb| lb.exit_idx) else {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=missing_loop_exit loop_body={:?}",
                    idx, block_addr, loop_body
                );
            }
            return Ok(None);
        };

        let exit_addr = host.block_target_key(exit_idx);

        let (cond, body_addr) = if true_target == exit_addr {
            let Some(body_addr) = false_target else {
                return Ok(None);
            };
            (negate_expr(cond), body_addr)
        } else if false_target == Some(exit_addr) {
            let body_addr = true_target;
            (cond, body_addr)
        } else {
            // If neither branch goes to the computed exit edge, this is not a strictly formed while loop tail
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=exit_target_mismatch true=0x{:x} false={:?} exit=0x{:x}",
                    idx, block_addr, true_target, false_target, exit_addr
                );
            }
            return Ok(None);
        };

        let body_idx = host
            .find_block_index_by_address(body_addr)
            .ok_or(MlilPreviewError::UnsupportedCfgRegionShape)?;

        if budget.checkpoint("body_pre") {
            return Ok(None);
        }
        let Some((body, loop_join_idx)) =
            host.lower_linear_body_with_budget(body_idx, LinearExit::Join(idx), Some(&mut budget))?
        else {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=linear_body_rejected body_idx={}",
                    idx, block_addr, body_idx
                );
            }
            return Ok(None);
        };
        if budget.checkpoint("body_post") {
            return Ok(None);
        }
        if loop_join_idx != idx {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while reject: idx={} block=0x{:x} reason=linear_body_wrong_join actual={} expected={}",
                    idx, block_addr, loop_join_idx, idx
                );
            }
            return Ok(None);
        }
        let continue_label = block_label(host.block_target_key(idx));
        let break_label = block_label(host.block_target_key(exit_idx));
        let mut body = std::rc::Rc::unwrap_or_clone(body);
        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some(&continue_label),
            Some(&break_label),
            &mut stats,
        );
        host.track_loop_control_rewrite_stats(
            stats.break_rewrites,
            stats.continue_rewrites,
            stats.skipped_nested_scope_count,
        );
        if cond_prefix.is_empty() {
            return Ok(Some((
                PreHirStmt::While {
                    cond,
                    body: std::rc::Rc::new(body),
                },
                exit_idx,
            )));
        }
        if let Some(folded_cond) = try_fold_cond_prefix(&cond_prefix, &cond, &body) {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_while cond_prefix_folded: idx={} block=0x{:x} prefix_stmts={}",
                    idx,
                    block_addr,
                    cond_prefix.len()
                );
            }
            return Ok(Some((
                PreHirStmt::While {
                    cond: folded_cond,
                    body: std::rc::Rc::new(body),
                },
                exit_idx,
            )));
        }

        let mut guarded_body = cond_prefix;
        guarded_body.push(PreHirStmt::If {
            cond: negate_expr(cond),
            then_body: std::rc::Rc::new(vec![PreHirStmt::Break]),
            else_body: std::rc::Rc::new(Vec::new()),
        });
        guarded_body.extend(body);
        Ok(Some((
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: std::rc::Rc::new(guarded_body),
            },
            exit_idx,
        )))
    })();

    // Fast path succeeded: return it directly.
    if result.is_ok() && result.as_ref().unwrap().is_some() {
        if diag {
            eprintln!(
                "[DIAG] try_lower_while done (fast path): idx={} block=0x{:x} elapsed={:.3}s",
                idx,
                block_addr,
                budget.start.elapsed().as_secs_f64(),
            );
        }
        return result;
    }

    // ------------------------------------------------------------------
    // Subgraph fallback: use the full body-set lowering when the linear
    // chain traversal failed (body has internal branching / multi-exit).
    // ------------------------------------------------------------------
    let subgraph_result = (|| -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
        // Re-derive the loop shape from LoopBody (must be valid while-loop).
        let Some(loop_body) = host.get_loop_body(idx) else {
            return Ok(None);
        };
        let Some(exit_idx) = loop_body.exit_idx else {
            return Ok(None);
        };

        let exit_addr = host.block_target_key(exit_idx);

        // Head must still have a CBranch with one arm pointing to exit.
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = host.lower_block_terminator(idx)?
        else {
            return Ok(None);
        };
        let cond_prefix = host.lower_block_stmts(idx)?;
        if !cond_prefix.iter().all(is_trivial_structuring_stmt) {
            return Ok(None);
        }

        let (cond, body_addr) = if true_target == exit_addr {
            let Some(body_addr) = false_target else {
                return Ok(None);
            };
            (negate_expr(cond), body_addr)
        } else if false_target == Some(exit_addr) {
            (cond, true_target)
        } else {
            return Ok(None);
        };

        let Some(body_start_idx) = host.find_block_index_by_address(body_addr) else {
            return Ok(None);
        };

        // Build body_set: all loop body blocks except the head.
        let body_set: HashSet<usize> = {
            let Some(lb) = host.get_loop_body(idx) else {
                return Ok(None);
            };
            lb.body.iter().copied().filter(|&b| b != idx).collect()
        };

        if body_set.is_empty() {
            return Ok(None);
        }

        let Some(lowered_body) = (if structure_body_members {
            lower_loop_body_via_sese(host, &body_set, body_start_idx, exit_idx, idx)?
        } else {
            lower_loop_body_subgraph(host, &body_set, body_start_idx, Some(exit_idx), idx)?
        }) else {
            return Ok(None);
        };

        host.bump_loop_while_subgraph_lowered();

        let body = if cond_prefix.is_empty() {
            lowered_body
        } else {
            let mut guarded = cond_prefix;
            guarded.push(PreHirStmt::If {
                cond: negate_expr(cond.clone()),
                then_body: std::rc::Rc::new(vec![PreHirStmt::Break]),
                else_body: std::rc::Rc::new(Vec::new()),
            });
            guarded.extend(lowered_body);
            return Ok(Some((
                PreHirStmt::While {
                    cond: PreHirExpr::Const(1, NirType::Bool),
                    body: std::rc::Rc::new(guarded),
                },
                exit_idx,
            )));
        };

        Ok(Some((
            PreHirStmt::While {
                cond,
                body: std::rc::Rc::new(body),
            },
            exit_idx,
        )))
    })();

    if diag {
        eprintln!(
            "[DIAG] try_lower_while done: idx={} block=0x{:x} elapsed={:.3}s success={} budget_tripped={} subgraph={}",
            idx,
            block_addr,
            budget.start.elapsed().as_secs_f64(),
            matches!(subgraph_result, Ok(Some(_))),
            budget.tripped,
            matches!(subgraph_result, Ok(Some(_))),
        );
    }
    subgraph_result
}

/// Whether `block_idx` is nothing but a bare `return <expr>;` -- i.e. a
/// do-while's embedded early-return guard target. Uses the same (cached,
/// idempotent-per-call) lowering hooks the reconstruction pass below already
/// relies on, so re-invoking it during reconstruction is safe.
fn trivial_return_expr(
    host: &mut impl StructuringHost,
    block_idx: usize,
) -> Result<Option<Option<PreHirExpr>>, MlilPreviewError> {
    let stmts = host.lower_block_stmts(block_idx)?;
    if !stmts.is_empty() {
        return Ok(None);
    }
    match host.lower_block_terminator(block_idx)? {
        LoweredTerminator::Return(expr) => Ok(Some(expr)),
        _ => Ok(None),
    }
}

pub fn lower_do_while_region(
    host: &mut impl StructuringHost,
    start_idx: usize,
) -> Result<Option<(Vec<PreHirStmt>, PreHirExpr, usize, usize)>, MlilPreviewError> {
    let diag = structuring_diag_enabled();
    // Confirmed loop head (real back-edge) -- required before we allow any
    // speculative lowering of blocks reachable from `start_idx` for the
    // guard-absorption branch below, otherwise a doomed do-while attempt at
    // a non-loop `start_idx` can corrupt order-dependent builder state
    // (`materialized_vns`) via out-of-order `lower_block_stmts` calls.
    let is_confirmed_loop_head = host.get_loop_body(start_idx).is_some();
    let mut idx = start_idx;
    let mut visited = HashSet::default();
    let mut path = Vec::new();
    let mut absorbed_guards: HashSet<usize> = HashSet::default();
    let (cond_idx, exit_idx) = loop {
        if host.sese_region_proof_budget_exceeded() {
            return Ok(None);
        }
        if !visited.insert(idx) {
            return Ok(None);
        }
        path.push(idx);

        let successors = host.successors().get(idx).cloned().unwrap_or_default();
        if successors.len() == 2 && successors.contains(&start_idx) {
            if host.region_has_external_entry(&visited, start_idx) {
                return Ok(None);
            }
            let Some(exit_idx) = successors
                .into_iter()
                .find(|successor| *successor != start_idx)
            else {
                return Ok(None);
            };
            break (idx, exit_idx);
        }
        let next_idx = match successors.as_slice() {
            [next_idx] => *next_idx,
            _ if is_confirmed_loop_head && idx != start_idx && successors.len() == 2 => {
                // Embedded early-return guard: one successor continues the
                // linear do-while chain, the other is a bare `return expr;`
                // block that lies outside the loop's contiguous body.
                let mut absorbed = None;
                for (i, &succ) in successors.iter().enumerate() {
                    if visited.contains(&succ) {
                        continue;
                    }
                    if trivial_return_expr(host, succ)?.is_some() {
                        absorbed = Some((succ, successors[1 - i]));
                        break;
                    }
                }
                let Some((return_target, continue_idx)) = absorbed else {
                    return Ok(None);
                };
                host.record_extra_absorbed_member(return_target);
                absorbed_guards.insert(idx);
                continue_idx
            }
            _ => return Ok(None),
        };
        if !host.can_inline_linear_successor(idx, next_idx, &visited) {
            return Ok(None);
        }
        idx = next_idx;
    };

    if diag {
        eprintln!(
            "[DIAG] lower_do_while_region: cfg proof start={} latch={} exit={} blocks={}",
            start_idx,
            cond_idx,
            exit_idx,
            path.len()
        );
    }

    let mut body = Vec::new();
    for (path_pos, block_idx) in path.iter().copied().enumerate() {
        if host.sese_region_proof_budget_exceeded() {
            return Ok(None);
        }
        body.extend(host.lower_block_stmts(block_idx)?);
        let terminator = host.lower_block_terminator(block_idx)?;
        if block_idx != cond_idx {
            let Some(expected_next) = path.get(path_pos + 1).copied() else {
                return Ok(None);
            };
            if absorbed_guards.contains(&block_idx) {
                let LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } = terminator
                else {
                    return Ok(None);
                };
                let expected_next_addr = host.block_target_key(expected_next);
                let (guard_cond, return_target_addr) = if true_target == expected_next_addr {
                    (negate_expr(cond), false_target)
                } else if false_target == Some(expected_next_addr) {
                    (cond, Some(true_target))
                } else {
                    return Ok(None);
                };
                let Some(return_target_addr) = return_target_addr else {
                    return Ok(None);
                };
                let Some(return_target_idx) = host.find_block_index_by_address(return_target_addr)
                else {
                    return Ok(None);
                };
                let Some(return_expr) = trivial_return_expr(host, return_target_idx)? else {
                    return Ok(None);
                };
                body.push(PreHirStmt::If {
                    cond: guard_cond,
                    then_body: std::rc::Rc::new(vec![PreHirStmt::Return(return_expr)]),
                    else_body: std::rc::Rc::new(Vec::new()),
                });
                continue;
            }
            let target = match terminator {
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    target
                }
                _ => return Ok(None),
            };
            if host.find_block_index_by_address(target) != Some(expected_next) {
                return Ok(None);
            }
            continue;
        }

        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = terminator
        else {
            return Ok(None);
        };
        let start_addr = host.block_target_key(start_idx);
        let exit_addr = host.block_target_key(exit_idx);
        if true_target == start_addr && false_target == Some(exit_addr) {
            return Ok(Some((body, cond, cond_idx, exit_idx)));
        }
        if false_target == Some(start_addr) && true_target == exit_addr {
            return Ok(Some((body, negate_expr(cond), cond_idx, exit_idx)));
        }
        return Ok(None);
    }

    Ok(None)
}

pub fn try_lower_multiblock_dowhile(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    if structuring_total_work_budget_exceeded(host) {
        return Ok(None);
    }
    let diag = structuring_diag_enabled();

    // After `order_tails_by_exit()` (Ghidra LoopBody::orderTails equivalent),
    // tails[0] is the preferred latch — the tail with a direct edge to the exit
    // block.  We accept multi-tail loops here: additional tails are handled as
    // mid-body break/continue edges by the subgraph lowerer.
    let (exit_idx, latch_idx, body_set, multi_tail) = {
        let Some(loop_body) = host.get_loop_body(idx) else {
            return Ok(None);
        };
        let Some(exit_idx) = loop_body.exit_idx else {
            return Ok(None);
        };
        if loop_body.tails.is_empty() {
            return Ok(None);
        }
        let multi_tail = loop_body.tails.len() > 1;
        // tails[0] is always the preferred latch after order_tails_by_exit().
        let latch_idx = loop_body.tails[0];
        let body_set: HashSet<usize> = loop_body.body.iter().copied().collect();
        (exit_idx, latch_idx, body_set, multi_tail)
    };

    let start_addr = host.block_target_key(idx);
    let exit_addr = host.block_target_key(exit_idx);

    // If the loop head itself independently branches to the loop's
    // exit (i.e. it's a `while`-style test, not just a do-while whose
    // only test is at the latch), defer entirely to `try_lower_while`.
    // Otherwise `lower_loop_body_subgraph` below re-discovers that same
    // `while(idx){...}` shape internally (its own reducer pass tries
    // `try_lower_while` on `idx` too) and this function would wrap the
    // result in a second, redundant `while(1){...}` shell whose only
    // `break`s live on the *inner* loop -- the outer shell has no exit
    // of its own, making anything lowered after it unreachable.
    if idx != latch_idx
        && let LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } = host.lower_block_terminator(idx)?
        && (true_target == exit_addr || false_target == Some(exit_addr))
    {
        return Ok(None);
    }

    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(latch_idx)?
    else {
        return Ok(None);
    };

    let _while_cond = if true_target == start_addr && false_target == Some(exit_addr) {
        cond
    } else if false_target == Some(start_addr) && true_target == exit_addr {
        negate_expr(cond)
    } else {
        return Ok(None);
    };

    if diag {
        eprintln!(
            "[DIAG] try_lower_multiblock_dowhile: attempting subgraph for idx={} multi_tail={}",
            idx, multi_tail
        );
    }

    let Some(lowered) = lower_loop_body_subgraph(host, &body_set, idx, Some(exit_idx), idx)? else {
        return Ok(None);
    };

    host.bump_loop_while_subgraph_lowered();
    if multi_tail {
        host.bump_loop_multi_tail_dowhile_lowered();
    }

    Ok(Some((
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: std::rc::Rc::new(lowered),
        },
        exit_idx,
    )))
}

// -----------------------------------------------------------------------
// For-loop pattern detection
// -----------------------------------------------------------------------

/// Attempt to recognise and lower a for-loop pattern starting at `idx`.
///
/// CFG invariants that must ALL hold:
///
/// 1. `idx` is a valid while-loop head: CBranch with one arm pointing to `exit_idx`.
/// 2. **Latch invariant**: the LoopBody has exactly one tail, and the tail is dominated
///    by the head (`dom_tree.dominates(head_idx, tail_idx)`).
/// 3. **Init invariant**: the head has exactly one predecessor that is OUTSIDE the loop
///    body (the init block), and that init block lowers to a single `Assign` statement.
/// 4. **Update invariant**: the latch block (excluding its back-edge) lowers to a single
///    `Assign` statement (the loop counter update).
/// 5. **Variable invariant**: init's LHS and update's LHS name the same variable.
///
/// On success emits `PreHirStmt::For { init, cond, update, body }` and returns
/// `(stmt, exit_idx)`. The init block is skipped by returning the adjusted `skip_to`.
pub fn try_lower_for(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    // ── Invariant 1: valid while-loop head (CBranch + LoopBody with exit) ──
    // Extract all needed data from LoopBody before taking &mut host borrows.
    let (exit_idx, latch_idx, body_set) = {
        let Some(lb) = host.get_loop_body(idx) else {
            return Ok(None);
        };
        let Some(exit_idx) = lb.exit_idx else {
            return Ok(None);
        };
        if lb.tails.len() != 1 {
            return Ok(None);
        }
        let latch_idx = lb.tails[0];
        let body_set: HashSet<usize> = lb.body.iter().copied().collect();
        (exit_idx, latch_idx, body_set)
    };

    // ── Invariant 2: latch dominated by head ──
    if !host.dom_tree().dominates(idx, latch_idx) {
        return Ok(None);
    }

    // ── Confirm head has CBranch with one arm → exit ──
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };
    let exit_addr = host.block_target_key(exit_idx);
    let (while_cond, body_addr) = if true_target == exit_addr {
        let Some(body_addr) = false_target else {
            return Ok(None);
        };
        (negate_expr(cond), body_addr)
    } else if false_target == Some(exit_addr) {
        (cond, true_target)
    } else {
        return Ok(None);
    };

    // A for-loop head contributes only its condition to the resulting HIR.
    // Prove from raw p-code that lowering the discarded prefix cannot emit
    // a side effect or nested control-flow statement. Materializing the
    // entire head merely to perform this rejection is prohibitively costly
    // for unrolled arithmetic loops and cannot improve the candidate.
    if !host.head_has_only_discardable_pure_ops(idx) {
        return Ok(None);
    }

    let diag = structuring_diag_enabled();

    // ── Invariant 3: exactly one outside-loop predecessor of head (init block) ──
    let outside_preds: Vec<usize> = host.predecessors()[idx]
        .iter()
        .copied()
        .filter(|&p| !body_set.contains(&p))
        .collect();
    if outside_preds.len() != 1 {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=outside_preds_count count={}",
                idx,
                outside_preds.len()
            );
        }
        return Ok(None);
    }
    let init_idx = outside_preds[0];

    // Init block must lower to exactly one Assign statement
    let init_stmts = host.lower_block_stmts(init_idx)?;
    if init_stmts.len() != 1 {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=init_block_stmt_count init_idx={} count={}",
                idx,
                init_idx,
                init_stmts.len()
            );
        }
        return Ok(None);
    }
    let PreHirStmt::Assign {
        lhs: ref init_lhs, ..
    } = init_stmts[0]
    else {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=init_stmt_not_assign",
                idx
            );
        }
        return Ok(None);
    };
    let init_var_name = match init_lhs {
        PreHirLValue::Var(name) => name.clone(),
        _ => {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_for reject: idx={} reason=init_lhs_not_var",
                    idx
                );
            }
            return Ok(None);
        }
    };

    // ── Invariant 4: latch lowers to exactly one Assign (the update) ──
    // We lower latch stmts only (not the back-edge terminator).
    let latch_stmts = host.lower_block_stmts(latch_idx)?;
    if latch_stmts.len() != 1 {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=latch_block_stmt_count latch_idx={} count={}",
                idx,
                latch_idx,
                latch_stmts.len()
            );
        }
        return Ok(None);
    }
    let PreHirStmt::Assign {
        lhs: ref update_lhs,
        ..
    } = latch_stmts[0]
    else {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=latch_stmt_not_assign",
                idx
            );
        }
        return Ok(None);
    };

    // ── Invariant 5: init and update assign to the same variable ──
    let update_var_name = match update_lhs {
        PreHirLValue::Var(name) => name.clone(),
        _ => {
            if diag {
                eprintln!(
                    "[DIAG] try_lower_for reject: idx={} reason=update_lhs_not_var",
                    idx
                );
            }
            return Ok(None);
        }
    };
    if init_var_name != update_var_name {
        if diag {
            eprintln!(
                "[DIAG] try_lower_for reject: idx={} reason=init_update_var_mismatch init_var={} update_var={}",
                idx, init_var_name, update_var_name
            );
        }
        return Ok(None);
    }

    // ── Lower the loop body: body_blocks = body_set \ {head, latch} ──
    let body_blocks: HashSet<usize> = body_set
        .iter()
        .copied()
        .filter(|&b| b != idx && b != latch_idx)
        .collect();

    let Some(body_start_idx) = host.find_block_index_by_address(body_addr) else {
        return Ok(None);
    };

    let for_body = if body_blocks.is_empty() {
        // Empty body (tight counter loop)
        Vec::new()
    } else {
        let Some(lowered) =
            lower_loop_body_subgraph(host, &body_blocks, body_start_idx, Some(exit_idx), idx)?
        else {
            return Ok(None);
        };
        lowered
    };

    let init_box = Box::new(init_stmts.into_iter().next().unwrap());
    let update_box = Box::new(latch_stmts.into_iter().next().unwrap());

    host.bump_loop_for_lowered();

    Ok(Some((
        PreHirStmt::For {
            init: Some(init_box),
            cond: Some(while_cond),
            update: Some(update_box),
            body: std::rc::Rc::new(for_body),
        },
        exit_idx,
    )))
}

// -----------------------------------------------------------------------
// Subgraph body lowering: lower a loop body as a CFG subgraph with
// explicit break/continue context, enabling multi-exit loops.
// -----------------------------------------------------------------------

fn lower_loop_exit_stmt(
    host: &mut impl StructuringHost,
    pred_idx: usize,
    target: u64,
) -> Result<PreHirStmt, MlilPreviewError> {
    let return_expr = host
        .find_block_index_by_address(target)
        .map(|target_idx| host.lower_return_join_expr_for_predecessor(pred_idx, target_idx))
        .transpose()?
        .flatten();
    Ok(return_expr
        .map(|expr| PreHirStmt::Return(Some(expr)))
        .unwrap_or(PreHirStmt::Break))
}

/// Structure the acyclic interior of an already-admitted cyclic owner.
/// This mirrors angr's region ordering: identify/own the loop first, then
/// run ordinary acyclic structuring inside that exact membership set.
fn lower_loop_body_via_sese(
    host: &mut impl StructuringHost,
    body_set: &HashSet<usize>,
    start_idx: usize,
    break_idx: usize,
    head_idx: usize,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    if body_set.iter().copied().min() != Some(start_idx) {
        return Ok(None);
    }
    let (mut body, _, _) = crate::sese_driver::build_sese_region_body_for_members(
        host,
        start_idx,
        host.block_count(),
        crate::HashMap::default(),
        body_set,
    )?;
    let continue_label = block_label(host.block_target_key(head_idx));
    let break_label = block_label(host.block_target_key(break_idx));
    let mut stats = LoopControlRewriteStats::default();
    rewrite_loop_control_gotos_in_stmts(
        &mut body,
        Some(&continue_label),
        Some(&break_label),
        &mut stats,
    );
    host.track_loop_control_rewrite_stats(
        stats.break_rewrites,
        stats.continue_rewrites,
        stats.skipped_nested_scope_count,
    );
    while body.last() == Some(&PreHirStmt::Continue) {
        body.pop();
    }
    Ok(Some(body))
}

/// Lower all blocks in `body_set` (the loop body excluding the head) into a HIR statement
/// sequence, treating jumps to `break_idx` as `Break` and jumps to `head_idx` as `Continue`.
///
/// Algorithm (based on natural loop structure):
/// 1. Process body blocks in sorted index order (forward dominance order for reducible CFGs).
/// 2. For each block, attempt the same structured reducers as `build_multiblock_body`.
/// 3. At the fallback terminator level, intercept exits to break/continue targets directly.
///
/// Returns `None` if the subgraph cannot be lowered (e.g. irreducible subgraph, budget
/// exceeded). Callers should fall through to the goto-based fallback in that case.
pub fn lower_loop_body_subgraph(
    host: &mut impl StructuringHost,
    body_set: &HashSet<usize>,
    start_idx: usize,
    break_idx: Option<usize>,
    head_idx: usize,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    if structuring_total_work_budget_exceeded(host) {
        return Ok(None);
    }

    if body_set.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let break_addr: Option<u64> = break_idx.map(|bi| host.block_target_key(bi));
    let break_addrs: HashSet<u64> = host
        .get_loop_body(head_idx)
        .map(|lb| {
            lb.all_exits
                .iter()
                .filter_map(|&exit| Some(host.block_start_address(exit)))
                .collect()
        })
        .filter(|exits: &HashSet<u64>| !exits.is_empty())
        .unwrap_or_else(|| break_addr.into_iter().collect());

    let break_indices: HashSet<usize> = host
        .get_loop_body(head_idx)
        .map(|lb| lb.all_exits.iter().copied().collect())
        .filter(|exits: &HashSet<usize>| !exits.is_empty())
        .unwrap_or_else(|| break_idx.into_iter().collect());

    let head_addr: u64 = host.block_target_key(head_idx);

    let targeted = host.collect_jump_targets()?;

    // Process blocks in sorted index order; this is preorder-compatible for reducible bodies.
    let mut sorted_body: Vec<usize> = body_set.iter().copied().collect();
    sorted_body.sort_unstable();

    let Some(start_pos) = sorted_body.iter().position(|&i| i == start_idx) else {
        return Ok(None);
    };
    // Rotate so the scan begins at the loop head and wraps around to any
    // body block with a *lower* raw index instead of dropping it. Loop
    // rotation codegen (routine at GCC -O1+) places the latch test before
    // the head in the instruction stream -- `jmp head; latch: ...; head:
    // ...; jcc latch` -- so the latch's block index can be lower than its
    // own head's even though it executes *after* the head on every real
    // iteration. A forward-only scan from `start_pos` silently orphaned
    // that block from this subgraph, which downstream materialized it as
    // a bogus second definition site for loop-invariant values read at
    // the head (only one of which was actually reachable, so the other
    // read them uninitialized).
    sorted_body.rotate_left(start_pos);
    let start_pos = 0;

    let mut result_stmts: Vec<PreHirStmt> = Vec::new();
    let mut emitted_labels: HashSet<u64> = HashSet::default();
    // Graph-native exclusive emission (Ghidra CollapseStructure invariant):
    // each body_set block index is lowered at most once. Nested structured
    // regions tombstone `[idx, skip_to)` so residual walk cannot re-emit.
    let mut consumed_blocks: HashSet<usize> = HashSet::default();
    // Addresses within body_set that must have a label emitted regardless of `targeted`.
    // Pre-populated by scanning:
    //   (a) terminator_cache for already-lowered terminators, and
    //   (b) raw CFG successors (always available) for body-internal edges.
    // This handles both forward and backward jump references before blocks are lowered.
    let mut force_labels: HashSet<u64> = HashSet::default();
    {
        let body_addrs: HashSet<u64> = body_set
            .iter()
            .map(|&bi| host.block_start_address(bi))
            .collect();
        for &bi in body_set.iter() {
            // (a) Cached terminator branch targets (no re-lowering).
            if let Some(targets) = host.cached_terminator_branch_targets(bi) {
                for t in targets {
                    if body_addrs.contains(&t) {
                        force_labels.insert(t);
                    }
                }
            }
            // (b) Raw CFG successors inside the body set.
            for &succ_idx in &host.successors()[bi] {
                if body_set.contains(&succ_idx) {
                    force_labels.insert(host.block_start_address(succ_idx));
                }
            }
        }
    }
    let mut last_structuring_failure = None;
    let mut pos = start_pos;

    // Helper closure: is the skip_to index within the body set or equal to break_idx?
    let is_valid_skip = |skip_to: usize| -> bool {
        body_set.contains(&skip_to) || break_indices.contains(&skip_to)
    };

    // Tombstone every body_set member in the half-open index range [from, to).
    let tombstone_range = |from: usize, to: usize, consumed: &mut HashSet<usize>| {
        for &bi in body_set.iter() {
            if bi >= from && bi < to {
                consumed.insert(bi);
            }
        }
    };

    while pos < sorted_body.len() {
        let idx = sorted_body[pos];
        if consumed_blocks.contains(&idx) {
            pos += 1;
            continue;
        }

        // --- Attempt structured reducers, but only accept if skip_to stays within bounds ---
        macro_rules! try_reducer {
            ($call:expr) => {{
                if let Some((stmt, skip_to)) =
                    capture_structuring_failure($call, &mut last_structuring_failure)?
                {
                    if is_valid_skip(skip_to)
                        && host.accept_structured_region(idx, skip_to, &targeted)
                    {
                        result_stmts.push(stmt);
                        // Exclusive emission: region owns body_set ∩ [idx, skip_to).
                        if break_indices.contains(&skip_to) {
                            for &bi in body_set.iter() {
                                if bi >= idx {
                                    consumed_blocks.insert(bi);
                                }
                            }
                            return Ok(Some(result_stmts));
                        }
                        tombstone_range(idx, skip_to, &mut consumed_blocks);
                        consumed_blocks.insert(idx);
                        pos = sorted_body
                            .iter()
                            .position(|&i| i == skip_to)
                            .unwrap_or(sorted_body.len());
                        while pos < sorted_body.len() && consumed_blocks.contains(&sorted_body[pos])
                        {
                            pos += 1;
                        }
                        continue;
                    }
                }
            }};
        }

        try_reducer!(try_lower_switch(host, idx));
        try_reducer!(try_lower_dowhile(host, idx));
        try_reducer!(try_lower_while(host, idx));
        try_reducer!(try_lower_infloop_with_break(host, idx));
        try_reducer!(try_lower_infloop(host, idx));
        try_reducer!(try_lower_short_circuit_if(host, idx));
        try_reducer!(try_lower_if_else(host, idx));
        try_reducer!(try_lower_if(host, idx));

        // --- Fallback: emit block with loop-context-aware terminator ---
        // Residual emission still exclusive: one emit per block index.
        consumed_blocks.insert(idx);
        let block_key = host.block_target_key(idx);
        if (idx == start_idx || targeted.contains(&block_key) || force_labels.contains(&block_key))
            && emitted_labels.insert(block_key)
        {
            result_stmts.push(PreHirStmt::Label(block_label(block_key)));
        }
        result_stmts.extend(host.lower_block_stmts(idx)?);

        match host.lower_block_terminator(idx)? {
            LoweredTerminator::Return(expr) => {
                result_stmts.push(PreHirStmt::Return(expr));
            }
            LoweredTerminator::Goto(target) | LoweredTerminator::Fallthrough(Some(target)) => {
                if break_addrs.contains(&target) {
                    let exit_stmt = lower_loop_exit_stmt(host, idx, target)?;
                    if matches!(exit_stmt, PreHirStmt::Break) {
                        host.bump_loop_multi_exit_break();
                    }
                    result_stmts.push(exit_stmt);
                } else if target == head_addr {
                    result_stmts.push(PreHirStmt::Continue);
                } else if host.next_block_address(idx) != Some(target) {
                    // Track this target as requiring a label if it is in the body.
                    if let Some(target_idx) = host.find_block_index_by_address(target) {
                        if body_set.contains(&target_idx) {
                            force_labels.insert(target);
                        }
                    }
                    result_stmts.push(PreHirStmt::Goto(block_label(target)));
                }
            }
            LoweredTerminator::Fallthrough(None) => {}
            LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } => {
                let next_addr = host.next_block_address(idx);
                // Check if either arm is the break or continue target
                let true_is_break = break_addrs.contains(&true_target);
                let false_is_break =
                    false_target.is_some_and(|target| break_addrs.contains(&target));
                let true_is_continue = true_target == head_addr;
                let false_is_continue = false_target == Some(head_addr);

                if true_is_break && false_is_continue {
                    let exit_stmt = lower_loop_exit_stmt(host, idx, true_target)?;
                    let is_break = matches!(exit_stmt, PreHirStmt::Break);
                    result_stmts.push(PreHirStmt::If {
                        cond,
                        then_body: std::rc::Rc::new(vec![exit_stmt]),
                        else_body: std::rc::Rc::new(vec![PreHirStmt::Continue]),
                    });
                    if is_break {
                        host.bump_loop_multi_exit_break();
                    }
                } else if false_is_break && true_is_continue {
                    let false_target = false_target.expect("false break target");
                    let exit_stmt = lower_loop_exit_stmt(host, idx, false_target)?;
                    let is_break = matches!(exit_stmt, PreHirStmt::Break);
                    result_stmts.push(PreHirStmt::If {
                        cond: negate_expr(cond),
                        then_body: std::rc::Rc::new(vec![exit_stmt]),
                        else_body: std::rc::Rc::new(vec![PreHirStmt::Continue]),
                    });
                    if is_break {
                        host.bump_loop_multi_exit_break();
                    }
                } else if true_is_break && !false_is_break {
                    // `if (cond) break;` then continue with false arm
                    let exit_stmt = lower_loop_exit_stmt(host, idx, true_target)?;
                    let is_break = matches!(exit_stmt, PreHirStmt::Break);
                    result_stmts.push(PreHirStmt::If {
                        cond,
                        then_body: std::rc::Rc::new(vec![exit_stmt]),
                        else_body: std::rc::Rc::new(Vec::new()),
                    });
                    if is_break {
                        host.bump_loop_multi_exit_break();
                    }
                } else if false_is_break && !true_is_break {
                    // `if (!cond) break;` then continue with true arm
                    let false_target = false_target.expect("false break target");
                    let exit_stmt = lower_loop_exit_stmt(host, idx, false_target)?;
                    let is_break = matches!(exit_stmt, PreHirStmt::Break);
                    result_stmts.push(PreHirStmt::If {
                        cond: negate_expr(cond),
                        then_body: std::rc::Rc::new(vec![exit_stmt]),
                        else_body: std::rc::Rc::new(Vec::new()),
                    });
                    if is_break {
                        host.bump_loop_multi_exit_break();
                    }
                } else if true_is_continue && !false_is_continue {
                    result_stmts.push(PreHirStmt::If {
                        cond,
                        then_body: std::rc::Rc::new(vec![PreHirStmt::Continue]),
                        else_body: std::rc::Rc::new(Vec::new()),
                    });
                } else if false_is_continue && !true_is_continue {
                    result_stmts.push(PreHirStmt::If {
                        cond: negate_expr(cond),
                        then_body: std::rc::Rc::new(vec![PreHirStmt::Continue]),
                        else_body: std::rc::Rc::new(Vec::new()),
                    });
                } else {
                    // General conditional: emit as if/goto like build_multiblock_body fallback
                    let then_body: Vec<PreHirStmt> = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        // Track this as requiring a label if it is in the body.
                        if let Some(target_idx) = host.find_block_index_by_address(true_target) {
                            if body_set.contains(&target_idx) {
                                force_labels.insert(true_target);
                            }
                        }
                        vec![PreHirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(ft) if Some(ft) != next_addr => {
                            // Track this as requiring a label if it is in the body.
                            if let Some(target_idx) = host.find_block_index_by_address(ft) {
                                if body_set.contains(&target_idx) {
                                    force_labels.insert(ft);
                                }
                            }
                            vec![PreHirStmt::Goto(block_label(ft))]
                        }
                        _ => Vec::new(),
                    };
                    result_stmts.push(PreHirStmt::If {
                        cond,
                        then_body: std::rc::Rc::new(then_body),
                        else_body: std::rc::Rc::new(else_body),
                    });
                }
            }
            LoweredTerminator::Unsupported { .. } => {
                // Propagate as an unsupported marker; caller will fall back.
                return Ok(None);
            }
            LoweredTerminator::Switch {
                expr,
                targets,
                default_target,
                min_val,
                proof,
            } => {
                // Switch inside loop body: emit as switch with gotos, rewrite pass will clean
                let (case_values, _used_proof_payload) =
                    recovered_switch_case_values(&targets, default_target, min_val, proof.as_ref());
                let cases = case_values
                    .into_iter()
                    .map(|(value, target)| PreHirSwitchCase {
                        values: vec![value],
                        body: std::rc::Rc::new(vec![PreHirStmt::Goto(block_label(target))]),
                    })
                    .collect();
                result_stmts.push(PreHirStmt::Switch {
                    expr,
                    cases,
                    default: std::rc::Rc::new(
                        default_target
                            .map(block_label)
                            .map(PreHirStmt::Goto)
                            .into_iter()
                            .collect(),
                    ),
                });
            }
        }

        pos += 1;
    }

    // Apply break/continue rewriting to catch any Goto labels that escaped the fallback
    // (e.g. produced by nested if/else structuring that still emits gotos).
    //
    // CFG-based: build break_labels from ALL exits of this loop body, not just the
    // canonical one.  This converts multi-exit gotos to `break` when they all exit
    // the loop, keeping the generated code clean without changing semantics.
    let continue_label_str = block_label(head_addr);
    let continue_set: std::collections::HashSet<String> =
        std::iter::once(continue_label_str.clone()).collect();
    let break_labels: std::collections::HashSet<String> = {
        if let Some(lb) = host.get_loop_body(head_idx) {
            let all_exits_labels: std::collections::HashSet<String> = lb
                .all_exits
                .iter()
                .filter_map(|&exit| Some(block_label(host.block_start_address(exit))))
                .collect();
            if !all_exits_labels.is_empty() {
                all_exits_labels
            } else if let Some(ref bstr) = break_addr.map(block_label) {
                std::iter::once(bstr.clone()).collect()
            } else {
                std::collections::HashSet::default()
            }
        } else if let Some(ref bstr) = break_addr.map(block_label) {
            std::iter::once(bstr.clone()).collect()
        } else {
            std::collections::HashSet::default()
        }
    };
    let mut stats = LoopControlRewriteStats::default();
    rewrite_loop_control_gotos_multi(&mut result_stmts, &continue_set, &break_labels, &mut stats);
    host.track_loop_control_rewrite_stats(
        stats.break_rewrites,
        stats.continue_rewrites,
        stats.skipped_nested_scope_count,
    );

    // Strip trailing `Continue` at the end of the body: the latch block naturally jumps back
    // to the head, so a Continue there is redundant. Only strip at the very end; a Continue
    // inside an if-branch earlier in the body must be preserved.
    while result_stmts.last() == Some(&PreHirStmt::Continue) {
        result_stmts.pop();
    }

    Ok(Some(result_stmts))
}

/// Structures a **multi-block infinite loop** — a loop whose `all_exits` is empty,
/// meaning no edge inside the body ever leaves the loop.
///
/// These are not caught by `try_lower_infloop` (single-block host-loop) or
/// `try_lower_while` (requires a conditional exit at the head).  This reducer
/// detects them via `LoopBody::is_infinite_loop_candidate` and emits
/// `while(true) { body }`.
pub fn try_lower_multiblock_infloop(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let body_blocks: Vec<usize> = {
        let Some(loop_body) = host.get_loop_body(idx) else {
            return Ok(None);
        };
        if !loop_body.is_infinite_loop_candidate() {
            return Ok(None);
        }
        if loop_body.body.len() < 2 {
            // Single-block infinite loops are handled by try_lower_infloop.
            return Ok(None);
        }
        loop_body.body.clone()
    };

    // Include ALL body blocks (including the head) in the subgraph so that the head
    // block's statements are naturally emitted first.  The head block is the start.
    let body_set: HashSet<usize> = body_blocks.iter().copied().collect();

    let Some(lowered) = lower_loop_body_subgraph(
        host, &body_set, idx,  // start at the loop head
        None, // no break exit — truly infinite
        idx,  // head for continue detection
    )?
    else {
        return Ok(None);
    };

    let max_body_idx = body_blocks.iter().copied().max().unwrap_or(idx);
    Ok(Some((
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: std::rc::Rc::new(lowered),
        },
        max_body_idx + 1,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt};

    #[test]
    fn rewrite_loop_control_gotos_converts_break_and_continue_targets() {
        let mut body = vec![
            PreHirStmt::Goto("block_header".to_string()),
            PreHirStmt::Goto("block_exit".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Const(1, NirType::Bool),
                then_body: vec![PreHirStmt::Goto("block_header".to_string())].into(),
                else_body: vec![PreHirStmt::Goto("block_exit".to_string())].into(),
            },
        ];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("block_header"),
            Some("block_exit"),
            &mut stats,
        );

        assert!(matches!(body[0], PreHirStmt::Continue));
        assert!(matches!(body[1], PreHirStmt::Break));
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[2]
        else {
            panic!("expected if statement in rewritten loop body");
        };
        assert!(matches!(then_body.as_slice(), [PreHirStmt::Continue]));
        assert!(matches!(else_body.as_slice(), [PreHirStmt::Break]));
        assert_eq!(stats.break_rewrites, 2);
        assert_eq!(stats.continue_rewrites, 2);
        assert_eq!(stats.skipped_nested_scope_count, 0);
    }

    #[test]
    fn rewrite_loop_control_gotos_does_not_rewrite_inside_nested_loop_or_switch() {
        let mut body = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![PreHirStmt::Goto("block_header".to_string())].into(),
            },
            PreHirStmt::Switch {
                expr: PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
                cases: vec![PreHirSwitchCase {
                    values: vec![1],
                    body: vec![PreHirStmt::Goto("block_exit".to_string())].into(),
                }],
                default: vec![PreHirStmt::Goto("block_header".to_string())].into(),
            },
        ];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("block_header"),
            Some("block_exit"),
            &mut stats,
        );

        let PreHirStmt::While {
            body: nested_while_body,
            ..
        } = &body[0]
        else {
            panic!("expected nested while");
        };
        assert!(
            matches!(nested_while_body.as_slice(), [PreHirStmt::Goto(label)] if label == "block_header")
        );

        let PreHirStmt::Switch { cases, default, .. } = &body[1] else {
            panic!("expected switch statement");
        };
        // Inside switch, outer loop break target is shielded (Goto)
        assert!(
            matches!(cases[0].body.as_slice(), [PreHirStmt::Goto(label)] if label == "block_exit")
        );
        // Inside switch, outer loop continue target is propagated (Continue)
        assert!(matches!(default.as_slice(), [PreHirStmt::Continue]));
        assert_eq!(stats.break_rewrites, 0);
        assert_eq!(stats.continue_rewrites, 1); // 1 continue propagated through switch
        assert_eq!(stats.skipped_nested_scope_count, 2);
    }

    #[test]
    fn rewrite_loop_control_gotos_with_nested_switch_converts_continue_but_preserves_break() {
        let mut body = vec![PreHirStmt::Switch {
            expr: PreHirExpr::Const(1, NirType::Bool),
            cases: vec![PreHirSwitchCase {
                values: vec![1],
                body: vec![
                    PreHirStmt::Goto("outer_continue".to_string()),
                    PreHirStmt::Goto("outer_break".to_string()),
                ]
                .into(),
            }],
            default: Vec::new().into(),
        }];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("outer_continue"),
            Some("outer_break"),
            &mut stats,
        );

        let PreHirStmt::Switch { cases, .. } = &body[0] else {
            panic!("expected switch");
        };
        let case_body = &cases[0].body;
        assert!(matches!(case_body[0], PreHirStmt::Continue)); // Outer continue is permitted in switch
        assert!(matches!(case_body[1], PreHirStmt::Goto(ref l) if l == "outer_break")); // Outer break is shielded by switch
    }

    #[test]
    fn rewrite_loop_control_gotos_with_nested_loop_preserves_both() {
        let mut body = vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: vec![
                PreHirStmt::Goto("outer_continue".to_string()),
                PreHirStmt::Goto("outer_break".to_string()),
            ]
            .into(),
        }];

        let mut stats = LoopControlRewriteStats::default();
        rewrite_loop_control_gotos_in_stmts(
            &mut body,
            Some("outer_continue"),
            Some("outer_break"),
            &mut stats,
        );

        let PreHirStmt::While {
            body: inner_body, ..
        } = &body[0]
        else {
            panic!("expected while");
        };
        // Both are shielded by the inner loop frame
        assert!(matches!(inner_body[0], PreHirStmt::Goto(ref l) if l == "outer_continue"));
        assert!(matches!(inner_body[1], PreHirStmt::Goto(ref l) if l == "outer_break"));
    }

    #[test]
    fn undefined_goto_guard_rejects_missing_label_in_structured_loop_body() {
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Const(1, NirType::Bool),
            then_body: vec![PreHirStmt::Goto("block_missing".to_string())].into(),
            else_body: Vec::new().into(),
        }];

        assert!(has_goto_to_undefined_label(&body));
    }

    #[test]
    fn undefined_goto_guard_allows_labels_defined_in_loop_body() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Const(1, NirType::Bool),
                then_body: vec![PreHirStmt::Goto("block_join".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("block_join".to_string()),
            PreHirStmt::Break,
        ];

        assert!(!has_goto_to_undefined_label(&body));
    }

    // ── try_fold_cond_prefix ────────────────────────────────────────────────

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn u32_const(v: i64) -> PreHirExpr {
        PreHirExpr::Const(
            v,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn eq_expr(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn try_fold_cond_prefix_substitutes_used_var_and_drops_dead_byproduct() {
        // Mirrors a raw x86 CMP: one dead flag byproduct ("of") plus the one
        // flag ("zf") the branch actually reads.
        let cond_prefix = vec![
            assign("of", u32_const(0)),
            assign("zf", eq_expr(var("x"), u32_const(0))),
        ];
        let cond = var("zf");
        let body: Vec<PreHirStmt> = Vec::new();

        let folded = try_fold_cond_prefix(&cond_prefix, &cond, &body)
            .expect("fold should succeed: single-use zf + unused of");
        assert_eq!(folded, eq_expr(var("x"), u32_const(0)));
    }

    #[test]
    fn try_fold_cond_prefix_bails_on_multi_use() {
        let cond_prefix = vec![assign("t", u32_const(5))];
        let cond = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("t")),
            rhs: Box::new(var("t")),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let body: Vec<PreHirStmt> = Vec::new();

        assert!(try_fold_cond_prefix(&cond_prefix, &cond, &body).is_none());
    }

    #[test]
    fn try_fold_cond_prefix_bails_when_body_reads_dropped_var_before_any_write() {
        // "t" is unused by cond (drop candidate), but body reads it first --
        // dropping the prefix def would leave that read stale.
        let cond_prefix = vec![assign("t", u32_const(5))];
        let cond = PreHirExpr::Const(1, NirType::Bool);
        let body = vec![PreHirStmt::Expr(var("t"))];

        assert!(try_fold_cond_prefix(&cond_prefix, &cond, &body).is_none());
    }

    #[test]
    fn try_fold_cond_prefix_drops_var_when_body_overwrites_before_any_read() {
        // "t" is unused by cond, and body's first touch of "t" is a clean
        // overwrite before any read -- safe to drop the prefix def.
        let cond_prefix = vec![assign("t", u32_const(5))];
        let cond = PreHirExpr::Const(1, NirType::Bool);
        let body = vec![assign("t", u32_const(9)), PreHirStmt::Expr(var("t"))];

        let folded = try_fold_cond_prefix(&cond_prefix, &cond, &body)
            .expect("fold should succeed: body clobbers t before any read");
        assert_eq!(folded, PreHirExpr::Const(1, NirType::Bool));
    }

    #[test]
    fn try_fold_cond_prefix_empty_prefix_returns_none() {
        let cond = var("zf");
        let body: Vec<PreHirStmt> = Vec::new();
        assert!(try_fold_cond_prefix(&[], &cond, &body).is_none());
    }
}
