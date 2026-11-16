//! Switch structuring free functions (`try_lower_switch`, compare-chain parse).
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`.

use crate::guarded_tail_pure::{count_var_reads_expr, count_var_reads_stmt};
use crate::helpers::{
    block_label, detect_and_patch_case_fallthrough, extract_eq_const_for_case,
    extract_range_guard_for_chain, merge_equivalent_switch_cases, recovered_switch_case_values,
};
use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, LoweredTerminator, structuring_diag_enabled};
use crate::regions::EmitReadyDecision;
use crate::{HashMap, HashSet};
use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
use fission_midend_core::ir::{
    DispatcherCaseMapSource, DispatcherLegality, DispatcherProofScope, DispatcherProofUnit,
    MlilPreviewError, SelectorNormalization,
};
use fission_midend_core::wave_stats;
use fission_midend_prehir::util::format_expr_key;
use fission_midend_prehir::util::strip_casts;
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt, PreHirSwitchCase};

/// Soft budget for compare-chain switch parsing steps.
pub const SWITCH_CHAIN_PARSE_BUDGET_MAX: usize = 16;

pub fn try_lower_switch(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    if let Some(direct) = try_lower_direct_dispatcher_switch(host, idx)? {
        return Ok(Some(direct));
    }

    let Some(parsed) = parse_switch_chain(host, idx)? else {
        return Ok(None);
    };
    if !compare_chain_switch_candidate(&parsed) {
        return Ok(None);
    }
    let emit_ready = EmitReadyDecision::from_dispatcher_proof(Some(&parsed.proof));
    if !emit_ready.emit_ready {
        if structuring_diag_enabled() {
            eprintln!(
                "[DIAG] switch chain at {} emit_ready failed: {:?}",
                host.block_target_key(idx),
                emit_ready.failure
            );
        }
        host.bump_switch_emit_ready_failed();
        return Ok(None);
    }
    let mut seen_case_values = HashSet::default();
    if !parsed
        .cases
        .iter()
        .all(|(value, _)| seen_case_values.insert(*value))
    {
        return Ok(None);
    }

    let mut exits = parsed
        .cases
        .iter()
        .map(|(_, block_idx)| *block_idx)
        .collect::<Vec<_>>();
    exits.push(parsed.default_idx);
    let Some(exit) = host.shared_exit_for_indices(&exits)? else {
        return Ok(None);
    };

    let mut case_targets = std::collections::HashSet::default();
    for case_idx in &exits {
        case_targets.insert(*case_idx);
    }
    let old_targets = std::mem::replace(host.active_switch_targets_mut(), case_targets);

    let mut cases = Vec::new();
    let mut max_skip = 0usize;
    for (value, case_idx) in parsed.cases {
        let Some((case_body, skip_to)) = host.lower_linear_body(case_idx, exit)? else {
            *host.active_switch_targets_mut() = old_targets;
            return Ok(None);
        };
        let mut case_body = std::rc::Rc::unwrap_or_clone(case_body);
        max_skip = max_skip.max(skip_to).max(case_idx + 1);
        let expected_label = block_label(host.block_start_address(case_idx));
        if !matches!(
            case_body.first(),
            Some(PreHirStmt::Label(l)) if l == &expected_label
        ) {
            case_body.insert(0, PreHirStmt::Label(expected_label));
        }
        cases.push(PreHirSwitchCase {
            values: vec![value],
            body: std::rc::Rc::new(case_body),
        });
    }
    merge_equivalent_switch_cases(&mut cases);
    let Some((default_body, default_skip)) = host.lower_linear_body(parsed.default_idx, exit)?
    else {
        *host.active_switch_targets_mut() = old_targets;
        return Ok(None);
    };
    let mut default_body = std::rc::Rc::unwrap_or_clone(default_body);
    let expected_default_label = block_label(host.block_start_address(parsed.default_idx));
    if !matches!(
        default_body.first(),
        Some(PreHirStmt::Label(l)) if l == &expected_default_label
    ) {
        default_body.insert(0, PreHirStmt::Label(expected_default_label));
    }
    let ft_count = detect_and_patch_case_fallthrough(&mut cases, Some(&default_body));
    max_skip = max_skip.max(default_skip).max(parsed.default_idx + 1);

    *host.active_switch_targets_mut() = old_targets;
    host.bump_switch_fallthrough_detected(ft_count);

    let skip_to = match exit {
        LinearExit::Join(join_idx) => join_idx,
        LinearExit::Return | LinearExit::End => max_skip,
    };
    let stmt = PreHirStmt::Switch {
        expr: parsed.selector,
        cases,
        default: std::rc::Rc::new(default_body),
    };
    if !switch_owns_what_it_skips(host, idx, skip_to, &stmt) {
        return Ok(None);
    }
    // The later compare blocks are only looked at, on a copy of the host:
    // lowering a block ahead of its turn would fix what it lowers to.
    let later_compare_blocks = parsed.compare_blocks.get(1..).unwrap_or_default().to_vec();
    let doing_work = host.lower_observed(|observed| {
        for block in later_compare_blocks {
            let stmts = observed.lower_block_stmts(block)?;
            if !stmts.is_empty() && !compare_block_work_goes_unread(observed, block, &stmts)? {
                return Ok(Some(block));
            }
        }
        Ok(None)
    })?;
    if let Some(block) = doing_work {
        if structuring_diag_enabled() {
            eprintln!(
                "[DIAG] switch at block {idx} rejected: compare block {block} does work something reads"
            );
        }
        return Ok(None);
    }
    let head_stmts = match parsed.compare_blocks.first() {
        Some(head) => host.lower_block_stmts(*head)?,
        None => Vec::new(),
    };
    wave_stats::add_compare_chain_dispatcher_count(1);
    wave_stats::add_dispatcher_proof_units(1);
    wave_stats::add_dispatcher_proof_completed(1);
    wave_stats::add_dispatcher_shape_recoveries(1);
    Ok(Some((switch_behind_its_head(head_stmts, stmt), skip_to)))
}

/// A recovered switch with the statements of the block that dispatches in
/// front of it.
///
/// The switch replaces a block's *terminator*. The block's statements -- what
/// it computes before it dispatches, which is often the selector itself -- are
/// not part of that and are printed nowhere else: the caller owns the region
/// from this block on and resumes behind it.
fn switch_behind_its_head(head_stmts: Vec<PreHirStmt>, switch: PreHirStmt) -> PreHirStmt {
    if head_stmts.is_empty() {
        return switch;
    }
    let mut wrapped = head_stmts;
    wrapped.push(switch);
    PreHirStmt::Block(std::rc::Rc::new(wrapped))
}

/// How many blocks the search for a reader of a compare block's work may
/// enter before it stops and answers that there is one.
const COMPARE_WORK_READER_SEARCH_BLOCKS: usize = 128;

/// Whether dropping the statements of a compare block behind the first
/// changes nothing.
///
/// The first compare block's statements run on every path and go in front of
/// the switch. A later one's run only when the compares before it failed, and
/// a `switch` has no place for that. What such a block usually holds is the
/// compare itself -- the loaded operand and the flags -- which the selector
/// already stands for and nothing reads again. That can be dropped; anything
/// else cannot, and the chain is left to lower as the `if`s it is.
///
/// Two things make it droppable: the block writes registers and temporaries
/// only (no store, no call), so every later read of what it wrote is a read of
/// a name; and no path from it reads one of those names before writing it.
fn compare_block_work_goes_unread(
    host: &mut impl StructuringHost,
    block: usize,
    stmts: &[PreHirStmt],
) -> Result<bool, MlilPreviewError> {
    Ok(
        host.head_has_only_discardable_pure_ops(block)
            && assignments_go_unread(host, block, stmts)?,
    )
}

/// Whether `stmts`, the statements of `block`, are assignments to variables
/// that no path from the block reads before writing them again.
///
/// The caller has to know the block writes registers and temporaries only;
/// a name stands for the whole of what was written only then.
fn assignments_go_unread(
    host: &mut impl StructuringHost,
    block: usize,
    stmts: &[PreHirStmt],
) -> Result<bool, MlilPreviewError> {
    let Some(names) = names_a_block_assigns(stmts) else {
        return Ok(false);
    };
    let starts = host.successors().get(block).cloned().unwrap_or_default();
    let read = something_reads_before_writing(&starts, &names, &mut |idx| {
        if idx >= host.block_count() {
            return Ok(None);
        }
        let stmts = host.lower_block_stmts(idx)?;
        let terminator = host.lower_block_terminator(idx)?;
        let successors = host.successors().get(idx).cloned().unwrap_or_default();
        Ok(Some((stmts, terminator, successors)))
    })?;
    Ok(!read)
}

/// The variables `stmts` assign, when assigning variables is all they do.
fn names_a_block_assigns(stmts: &[PreHirStmt]) -> Option<Vec<&str>> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// A block as the reader search sees it: its statements, how it ends, and
/// the blocks it leads to.
type BlockForReaderSearch = (Vec<PreHirStmt>, LoweredTerminator, Vec<usize>);

/// Whether some path from one of `starts` reads one of `names` before
/// writing it.
///
/// A name stops being followed down a path at the first statement that
/// assigns it outright; a write inside a branch or a loop does not count. A
/// block `block_at` cannot supply, a terminator that is not understood, and a
/// search that runs past its budget all answer `true`.
fn something_reads_before_writing(
    starts: &[usize],
    names: &[&str],
    block_at: &mut dyn FnMut(usize) -> Result<Option<BlockForReaderSearch>, MlilPreviewError>,
) -> Result<bool, MlilPreviewError> {
    let mut followed_into: HashMap<usize, HashSet<&str>> = HashMap::default();
    let mut pending: Vec<(usize, Vec<&str>)> = starts
        .iter()
        .map(|start| (*start, names.to_vec()))
        .collect();
    let mut entered = 0usize;
    while let Some((idx, arriving)) = pending.pop() {
        let already = followed_into.entry(idx).or_default();
        let mut unwritten: Vec<&str> = arriving
            .into_iter()
            .filter(|name| already.insert(*name))
            .collect();
        if unwritten.is_empty() {
            continue;
        }
        entered += 1;
        if entered > COMPARE_WORK_READER_SEARCH_BLOCKS {
            return Ok(true);
        }
        let Some((stmts, terminator, successors)) = block_at(idx)? else {
            return Ok(true);
        };
        for stmt in &stmts {
            if unwritten
                .iter()
                .any(|name| count_var_reads_stmt(stmt, name) > 0)
            {
                return Ok(true);
            }
            if let PreHirStmt::Assign {
                lhs: PreHirLValue::Var(written),
                ..
            } = stmt
            {
                unwritten.retain(|name| name != written);
            }
        }
        if unwritten.is_empty() {
            continue;
        }
        if terminator_reads(&terminator, &unwritten) {
            return Ok(true);
        }
        for successor in successors {
            pending.push((successor, unwritten.clone()));
        }
    }
    Ok(false)
}

/// Whether the way a block ends reads one of `names`. An ending that is not
/// understood is taken to read all of them.
fn terminator_reads(terminator: &LoweredTerminator, names: &[&str]) -> bool {
    let expr = match terminator {
        LoweredTerminator::Fallthrough(_)
        | LoweredTerminator::Goto(_)
        | LoweredTerminator::Return(None) => return false,
        LoweredTerminator::Cond { cond, .. } => cond,
        LoweredTerminator::Switch { expr, .. } => expr,
        LoweredTerminator::Return(Some(expr)) => expr,
        LoweredTerminator::Unsupported { .. } => return true,
    };
    names
        .iter()
        .any(|name| count_var_reads_expr(expr, name) > 0)
}

/// Whether a recovered switch at `idx` holds every block the walk steps over
/// when it resumes at `skip_to`.
///
/// The caller owns `[idx, skip_to)` once it accepts the switch: those blocks
/// are printed inside the statement or not at all. When the arms share a join
/// the range ends there. When every arm leaves the function there is no join,
/// and the range runs to the last block any arm reached -- which says nothing
/// about the blocks in between. Two things have to hold for them:
///
/// - each is reached only through the switch (`idx` dominates it), so nothing
///   outside jumps to a block that is no longer printed;
/// - no `goto` inside the statement names one of them without the statement
///   also defining that label: an arm that could not inline a block leaves a
///   `goto` to it, expecting the walk to print the block later.
///
/// A switch that fails either is not one this layout can express, and its
/// blocks lower the ordinary way.
pub(crate) fn switch_owns_what_it_skips(
    host: &impl StructuringHost,
    idx: usize,
    skip_to: usize,
    stmt: &PreHirStmt,
) -> bool {
    let end = skip_to.min(host.block_count());
    let owned = skipped_blocks_are_owned(
        idx + 1..end,
        &|block| {
            host.predecessors()
                .get(block)
                .is_some_and(|preds| preds.is_empty())
                || host.dom_tree().dominates(idx, block)
        },
        &|block| block_label(host.block_start_address(block)),
        stmt,
    );
    if !owned && structuring_diag_enabled() {
        eprintln!(
            "[DIAG] switch at block {idx} rejected: skips to {skip_to} over blocks it does not hold"
        );
    }
    owned
}

/// The two conditions of [`switch_owns_what_it_skips`] over the skipped
/// `blocks`, with the graph facts passed in.
fn skipped_blocks_are_owned(
    blocks: std::ops::Range<usize>,
    entered_only_through_switch: &dyn Fn(usize) -> bool,
    label_of: &dyn Fn(usize) -> String,
    stmt: &PreHirStmt,
) -> bool {
    if blocks.is_empty() {
        return true;
    }
    if !blocks.clone().all(entered_only_through_switch) {
        return false;
    }
    let stmts = std::slice::from_ref(stmt);
    let referenced = crate::cleanup::collect_referenced_labels(stmts);
    let defined = crate::cleanup::collect_defined_labels(stmts);
    blocks.into_iter().all(|block| {
        let label = label_of(block);
        !referenced.contains(&label) || defined.contains(&label)
    })
}

pub fn try_lower_direct_dispatcher_switch(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let LoweredTerminator::Switch {
        expr,
        targets,
        default_target,
        min_val,
        proof,
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };
    let emit_ready = EmitReadyDecision::from_dispatcher_proof(proof.as_ref());
    if !emit_ready.emit_ready {
        if structuring_diag_enabled() {
            eprintln!(
                "[DIAG] direct switch at {} emit_ready failed: {:?}",
                host.block_target_key(idx),
                emit_ready.failure
            );
        }
        host.bump_switch_emit_ready_failed();
        return Ok(None);
    }

    let (case_values, used_proof_payload) =
        recovered_switch_case_values(&targets, default_target, min_val, proof.as_ref());
    if case_values.len() < 2 {
        return Ok(None);
    }

    let mut seen_case_values = HashSet::default();
    if !case_values
        .iter()
        .all(|(value, _)| seen_case_values.insert(*value))
    {
        return Ok(None);
    }

    let mut exits = Vec::new();
    for (_, target) in &case_values {
        let Some(case_idx) = host.find_block_index_by_address(*target) else {
            return Ok(None);
        };
        let canon = canonicalize_switch_target(host, case_idx);
        if !exits.contains(&canon) {
            exits.push(canon);
        }
    }
    if let Some(default_target) = default_target {
        let Some(default_idx) = host.find_block_index_by_address(default_target) else {
            return Ok(None);
        };
        let canon = canonicalize_switch_target(host, default_idx);
        if !exits.contains(&canon) {
            exits.push(canon);
        }
    }
    let Some(exit) = host.shared_exit_for_indices(&exits)? else {
        return Ok(None);
    };

    let mut case_targets = std::collections::HashSet::default();
    for case_idx in &exits {
        case_targets.insert(*case_idx);
    }
    let old_targets = std::mem::replace(host.active_switch_targets_mut(), case_targets);

    let mut cases = Vec::new();
    let mut max_skip = idx + 1;
    let mut success = true;
    for (value, target) in case_values {
        if Some(target) == default_target {
            continue;
        }
        let Some(case_idx) = host.find_block_index_by_address(target) else {
            success = false;
            break;
        };
        let case_idx = canonicalize_switch_target(host, case_idx);
        let Some((case_body, skip_to)) = host.lower_linear_body(case_idx, exit)? else {
            success = false;
            break;
        };
        let mut case_body = std::rc::Rc::unwrap_or_clone(case_body);
        max_skip = max_skip.max(skip_to).max(case_idx + 1);
        let expected_label = block_label(host.block_start_address(case_idx));
        if !matches!(
            case_body.first(),
            Some(PreHirStmt::Label(l)) if l == &expected_label
        ) {
            case_body.insert(0, PreHirStmt::Label(expected_label));
        }
        cases.push(PreHirSwitchCase {
            values: vec![value],
            body: std::rc::Rc::new(case_body),
        });
    }
    if !success || cases.len() < 2 {
        *host.active_switch_targets_mut() = old_targets;
        return Ok(None);
    }
    merge_equivalent_switch_cases(&mut cases);

    let default = if let Some(default_target) = default_target {
        let Some(default_idx) = host.find_block_index_by_address(default_target) else {
            *host.active_switch_targets_mut() = old_targets;
            return Ok(None);
        };
        let default_idx = canonicalize_switch_target(host, default_idx);
        let Some((default_body, skip_to)) = host.lower_linear_body(default_idx, exit)? else {
            *host.active_switch_targets_mut() = old_targets;
            return Ok(None);
        };
        let mut default_body = std::rc::Rc::unwrap_or_clone(default_body);
        let expected_default_label = block_label(host.block_start_address(default_idx));
        if !matches!(
            default_body.first(),
            Some(PreHirStmt::Label(l)) if l == &expected_default_label
        ) {
            default_body.insert(0, PreHirStmt::Label(expected_default_label));
        }
        max_skip = max_skip.max(skip_to).max(default_idx + 1);
        std::rc::Rc::new(default_body)
    } else {
        std::rc::Rc::new(Vec::new())
    };
    let ft_count = detect_and_patch_case_fallthrough(&mut cases, Some(default.as_slice()));

    *host.active_switch_targets_mut() = old_targets;
    host.bump_switch_fallthrough_detected(ft_count);

    let skip_to = match exit {
        LinearExit::Join(join_idx) => join_idx,
        LinearExit::Return | LinearExit::End => max_skip,
    };

    if used_proof_payload {
        host.bump_proof_payload_direct_emit();
    }
    let stmt = PreHirStmt::Switch {
        expr,
        cases,
        default,
    };
    if !switch_owns_what_it_skips(host, idx, skip_to, &stmt) {
        return Ok(None);
    }
    let head_stmts = host.lower_block_stmts(idx)?;
    wave_stats::add_dispatcher_proof_units(1);
    wave_stats::add_dispatcher_proof_completed(1);
    wave_stats::add_dispatcher_shape_recoveries(1);
    Ok(Some((switch_behind_its_head(head_stmts, stmt), skip_to)))
}

pub fn parse_switch_chain(
    host: &mut impl StructuringHost,
    start_idx: usize,
) -> Result<Option<ParsedSwitch>, MlilPreviewError> {
    let mut current_idx = start_idx;
    let mut current_term = host.lower_block_terminator(current_idx)?;
    let mut selector: Option<PreHirExpr> = None;
    let mut cases = Vec::new();
    let mut guarded_default_idx: Option<usize> = None;
    let mut saw_range_guard = false;
    let mut visited = HashSet::default();
    let mut compare_blocks = Vec::new();
    let max_chain_steps = host
        .successors()
        .len()
        .min(SWITCH_CHAIN_PARSE_BUDGET_MAX)
        .max(1);

    for _ in 0..max_chain_steps {
        if !visited.insert(current_idx) {
            return Ok(None);
        }
        compare_blocks.push(current_idx);

        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target,
        } = current_term
        else {
            return Ok(None);
        };

        let mut case_target = None;
        let mut next_compare_target = None;
        let mut case_on_true = true;
        let mut matched_case = None;

        if let Some((case_selector, value)) = extract_eq_const_for_case(&cond, true) {
            case_target = Some(true_target);
            next_compare_target = false_target;
            case_on_true = true;
            matched_case = Some((case_selector, value));
        } else if let Some((case_selector, value)) = extract_eq_const_for_case(&cond, false) {
            case_target = false_target;
            next_compare_target = Some(true_target);
            case_on_true = false;
            matched_case = Some((case_selector, value));
        } else if !saw_range_guard {
            if let Some(range_selector) = extract_range_guard_for_chain(&cond, true) {
                case_target = false_target;
                next_compare_target = Some(true_target);
                case_on_true = false;

                if let Some(existing) = &selector {
                    if strip_casts(existing) != strip_casts(&range_selector) {
                        return Ok(None);
                    }
                } else {
                    selector = Some(range_selector);
                }
                let Some(case_target_addr) = case_target else {
                    return Ok(None);
                };
                let Some(default_idx) = host.find_block_index_by_address(case_target_addr) else {
                    return Ok(None);
                };
                guarded_default_idx = Some(canonicalize_switch_target(host, default_idx));
                saw_range_guard = true;
            } else if let Some(range_selector) = extract_range_guard_for_chain(&cond, false) {
                case_target = Some(true_target);
                next_compare_target = false_target;
                case_on_true = true;

                if let Some(existing) = &selector {
                    if strip_casts(existing) != strip_casts(&range_selector) {
                        return Ok(None);
                    }
                } else {
                    selector = Some(range_selector);
                }
                let Some(case_target_addr) = case_target else {
                    return Ok(None);
                };
                let Some(default_idx) = host.find_block_index_by_address(case_target_addr) else {
                    return Ok(None);
                };
                guarded_default_idx = Some(canonicalize_switch_target(host, default_idx));
                saw_range_guard = true;
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }

        if let Some((case_selector, value)) = matched_case {
            let Some(case_target_addr) = case_target else {
                return Ok(None);
            };
            let Some(case_idx) = host.find_block_index_by_address(case_target_addr) else {
                return Ok(None);
            };
            let case_idx = canonicalize_switch_target(host, case_idx);
            if let Some(existing) = &selector {
                if strip_casts(existing) != strip_casts(&case_selector) {
                    return Ok(None);
                }
            } else {
                selector = Some(case_selector);
            }
            cases.push((value, case_idx));
        }

        let Some(next_compare_addr) = next_compare_target else {
            return Ok(None);
        };
        let Some(next_idx) = host.find_block_index_by_address(next_compare_addr) else {
            return Ok(None);
        };

        let next_term = host.lower_block_terminator(next_idx)?;
        match next_term {
            LoweredTerminator::Cond { .. } => {
                current_idx = next_idx;
                current_term = next_term;
                continue;
            }
            _ => {
                let Some(selector) = selector else {
                    return Ok(None);
                };
                let default_idx = canonicalize_switch_target(host, next_idx);
                if let Some(guarded_default_idx) = guarded_default_idx
                    && guarded_default_idx != default_idx
                {
                    return Ok(None);
                }
                let default_idx = guarded_default_idx.unwrap_or(default_idx);
                if !compare_chain_switch_candidate_values(&cases, default_idx) {
                    return Ok(None);
                }
                let proof =
                    build_compare_chain_proof(host, start_idx, &selector, &cases, default_idx);
                return Ok(Some(ParsedSwitch {
                    selector,
                    cases,
                    default_idx,
                    compare_blocks,
                    proof,
                }));
            }
        }
    }

    Ok(None)
}

/// The block a switch arm really starts at: `start_idx`, or the block behind
/// the forwarding blocks it leads through.
///
/// A forwarding block that is stepped over is printed nowhere, so one is
/// stepped over only when it writes nothing. An arm that sets the value the
/// function returns and jumps to the shared exit stays the arm's first block.
pub fn canonicalize_switch_target(host: &impl StructuringHost, start_idx: usize) -> usize {
    const MAX_SWITCH_TARGET_CANON_STEPS: usize = 32;
    let mut current = start_idx;
    let mut visited = HashSet::default();
    for _ in 0..MAX_SWITCH_TARGET_CANON_STEPS {
        if !visited.insert(current) {
            break;
        }
        if host.successors()[current].len() != 1 {
            break;
        }
        let next_idx = host.successors()[current][0];
        if !host.is_trivial_forwarding_block(current, next_idx)
            || !host.block_writes_nothing(current)
        {
            break;
        }
        current = next_idx;
    }
    current
}

pub fn compare_chain_switch_candidate_values(cases: &[(i64, usize)], default_idx: usize) -> bool {
    if cases.len() < 2 {
        return false;
    }
    let mut targets: HashSet<usize> = cases.iter().map(|(_, block_idx)| *block_idx).collect();
    targets.insert(default_idx);
    targets.len() >= 2
}

pub fn compare_chain_switch_candidate(parsed: &ParsedSwitch) -> bool {
    compare_chain_switch_candidate_values(&parsed.cases, parsed.default_idx)
}

#[derive(Debug, Clone)]
pub struct ParsedSwitch {
    pub selector: PreHirExpr,
    pub cases: Vec<(i64, usize)>,
    pub default_idx: usize,
    /// The blocks whose terminators were read as the chain's compares, in
    /// chain order; the first is the block the chain starts at.
    pub compare_blocks: Vec<usize>,
    pub proof: DispatcherProofUnit,
}

pub fn build_compare_chain_proof(
    host: &impl StructuringHost,
    start_idx: usize,
    selector: &PreHirExpr,
    cases: &[(i64, usize)],
    default_idx: usize,
) -> DispatcherProofUnit {
    let recovered_cases = cases
        .iter()
        .map(|(value, block_idx)| (*value, host.block_target_key(*block_idx)))
        .collect::<Vec<_>>();
    let mut guard_bounds = Vec::new();
    if !cases.is_empty() {
        let min_case = cases.iter().map(|(value, _)| *value).min();
        let max_case = cases.iter().map(|(value, _)| *value).max();
        guard_bounds.push((min_case, max_case));
    }
    DispatcherProofUnit {
        selector_expr: format_expr_key(selector),
        rendered_selector_expr: Some(format_expr_key(selector)),
        candidate_targets: recovered_cases.iter().map(|(_, target)| *target).collect(),
        recovered_cases,
        selector_cardinality: cases.len(),
        target_cardinality: cases
            .iter()
            .map(|(_, block_idx)| host.block_target_key(*block_idx))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        case_map_source: DispatcherCaseMapSource::CompareChainRecovered,
        default_target: Some(host.block_target_key(default_idx)),
        guard_set: vec!["compare_chain".to_string(), "shared_selector".to_string()],
        follow_block: Some(host.block_target_key(default_idx)),
        normalization: Some(SelectorNormalization {
            base_subtract: None,
            mask: None,
            stride: None,
            width: None,
            address_space: None,
            guard_bounds,
        }),
        legality_witness: Some(DispatcherLegality {
            follow_block: Some(host.block_target_key(default_idx)),
            postdom_ok: true,
            side_effect_free_selector: true,
            ordinal_domain_complete: true,
            shared_tail_conflict: false,
            valid: true,
        }),
        proof_scope: if start_idx == 0 {
            DispatcherProofScope::OuterDispatch
        } else {
            DispatcherProofScope::NestedDispatch
        },
        proof_complete: true,
        failure_family: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(block: usize) -> String {
        format!("block_{block}")
    }

    fn switch_whose_cases_are(bodies: Vec<Vec<PreHirStmt>>) -> PreHirStmt {
        PreHirStmt::Switch {
            expr: PreHirExpr::Var("x".to_string()),
            cases: bodies
                .into_iter()
                .enumerate()
                .map(|(value, body)| PreHirSwitchCase {
                    values: vec![value as i64],
                    body: std::rc::Rc::new(body),
                })
                .collect(),
            default: std::rc::Rc::new(Vec::new()),
        }
    }

    /// Arms that hold their blocks: each skipped block is reached only
    /// through the switch, and nothing is left behind a `goto`.
    #[test]
    fn a_switch_that_holds_every_skipped_block_owns_them() {
        let stmt = switch_whose_cases_are(vec![
            vec![PreHirStmt::Label(label(2)), PreHirStmt::Return(None)],
            vec![PreHirStmt::Label(label(3)), PreHirStmt::Return(None)],
        ]);
        assert!(skipped_blocks_are_owned(2..4, &|_| true, &label, &stmt));
    }

    /// An arm that could not inline a block leaves a `goto` to it. The walk
    /// was going to print that block; stepping over it leaves the `goto`
    /// pointing at nothing.
    #[test]
    fn a_goto_to_a_skipped_block_the_switch_does_not_define_is_not_owned() {
        let stmt = switch_whose_cases_are(vec![
            vec![PreHirStmt::Goto(label(3))],
            vec![PreHirStmt::Label(label(2)), PreHirStmt::Return(None)],
        ]);
        assert!(!skipped_blocks_are_owned(2..5, &|_| true, &label, &stmt));
    }

    /// The same `goto` is fine when the statement defines the label itself,
    /// or when it leaves the skipped range altogether.
    #[test]
    fn a_goto_the_switch_resolves_or_that_leaves_the_range_is_owned() {
        let inside = switch_whose_cases_are(vec![
            vec![PreHirStmt::Goto(label(3))],
            vec![PreHirStmt::Label(label(3)), PreHirStmt::Return(None)],
        ]);
        assert!(skipped_blocks_are_owned(2..5, &|_| true, &label, &inside));
        let outside = switch_whose_cases_are(vec![
            vec![PreHirStmt::Goto(label(9))],
            vec![PreHirStmt::Return(None)],
        ]);
        assert!(skipped_blocks_are_owned(2..5, &|_| true, &label, &outside));
    }

    /// A skipped block something outside the switch also reaches is not the
    /// switch's to take, whatever the statement looks like.
    fn work(name: &str) -> PreHirStmt {
        PreHirStmt::Goto(name.to_string())
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs,
        }
    }

    /// Runs the reader search over `blocks`, each a list of statements, an
    /// ending and its successors, starting at block 0.
    fn read_before_written(names: &[&str], blocks: Vec<BlockForReaderSearch>) -> bool {
        something_reads_before_writing(&[0], names, &mut |idx| Ok(blocks.get(idx).cloned()))
            .expect("the search itself does not fail")
    }

    #[test]
    fn a_block_that_only_assigns_variables_names_them() {
        let stmts = vec![assign("zf", var("a")), assign("tmp", var("b"))];
        assert_eq!(names_a_block_assigns(&stmts), Some(vec!["zf", "tmp"]));
    }

    #[test]
    fn a_block_that_does_anything_else_names_nothing() {
        let stmts = vec![assign("zf", var("a")), work("elsewhere")];
        assert_eq!(names_a_block_assigns(&stmts), None);
        let store = PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(var("p")),
                ty: fission_midend_core::ir::NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
            rhs: var("a"),
        };
        assert_eq!(names_a_block_assigns(&[store]), None);
    }

    #[test]
    fn a_name_nothing_reads_is_not_read() {
        let blocks = vec![
            (
                vec![assign("x", var("y"))],
                LoweredTerminator::Goto(1),
                vec![1],
            ),
            (
                Vec::new(),
                LoweredTerminator::Return(Some(var("x"))),
                Vec::new(),
            ),
        ];
        assert!(!read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_read_on_one_of_two_paths_is_a_read() {
        let cond = LoweredTerminator::Cond {
            cond: var("c"),
            true_target: 1,
            false_target: Some(2),
        };
        let blocks = vec![
            (Vec::new(), cond, vec![1, 2]),
            (Vec::new(), LoweredTerminator::Return(None), Vec::new()),
            (
                vec![assign("x", var("zf"))],
                LoweredTerminator::Return(None),
                Vec::new(),
            ),
        ];
        assert!(read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_write_ends_the_search_down_that_path() {
        let blocks = vec![
            (
                vec![assign("zf", var("a"))],
                LoweredTerminator::Goto(1),
                vec![1],
            ),
            (
                vec![assign("x", var("zf"))],
                LoweredTerminator::Return(None),
                Vec::new(),
            ),
        ];
        assert!(!read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_statement_that_reads_the_name_it_writes_is_a_read() {
        let blocks = vec![(
            vec![assign("zf", var("zf"))],
            LoweredTerminator::Return(None),
            Vec::new(),
        )];
        assert!(read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_write_inside_a_branch_does_not_end_the_search() {
        let guarded = PreHirStmt::If {
            cond: var("c"),
            then_body: std::rc::Rc::new(vec![assign("zf", var("a"))]),
            else_body: std::rc::Rc::new(Vec::new()),
        };
        let blocks = vec![
            (vec![guarded], LoweredTerminator::Goto(1), vec![1]),
            (
                Vec::new(),
                LoweredTerminator::Return(Some(var("zf"))),
                Vec::new(),
            ),
        ];
        assert!(read_before_written(&["zf"], blocks));
    }

    #[test]
    fn the_way_a_block_ends_can_be_the_read() {
        let cond = LoweredTerminator::Cond {
            cond: var("zf"),
            true_target: 1,
            false_target: None,
        };
        assert!(read_before_written(
            &["zf"],
            vec![(Vec::new(), cond, Vec::new())]
        ));
    }

    #[test]
    fn a_loop_with_no_reader_ends() {
        let blocks = vec![(Vec::new(), LoweredTerminator::Goto(0), vec![0])];
        assert!(!read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_block_that_cannot_be_looked_at_counts_as_a_read() {
        let blocks = vec![(Vec::new(), LoweredTerminator::Goto(7), vec![7])];
        assert!(read_before_written(&["zf"], blocks));
    }

    #[test]
    fn a_search_past_its_budget_counts_as_a_read() {
        let long = COMPARE_WORK_READER_SEARCH_BLOCKS + 1;
        let blocks: Vec<BlockForReaderSearch> = (0..long)
            .map(|idx| {
                let next = if idx + 1 < long {
                    vec![idx + 1]
                } else {
                    Vec::new()
                };
                (Vec::new(), LoweredTerminator::Goto(0), next)
            })
            .collect();
        assert!(read_before_written(&["zf"], blocks));
        let short: Vec<BlockForReaderSearch> = (0..COMPARE_WORK_READER_SEARCH_BLOCKS)
            .map(|idx| {
                let next = if idx + 1 < COMPARE_WORK_READER_SEARCH_BLOCKS {
                    vec![idx + 1]
                } else {
                    Vec::new()
                };
                (Vec::new(), LoweredTerminator::Goto(0), next)
            })
            .collect();
        assert!(!read_before_written(&["zf"], short));
    }

    #[test]
    fn a_switch_with_nothing_ahead_of_it_is_not_wrapped() {
        let bare = switch_behind_its_head(Vec::new(), switch_whose_cases_are(vec![vec![]]));
        assert!(matches!(bare, PreHirStmt::Switch { .. }));
        let wrapped = switch_behind_its_head(vec![work("a")], switch_whose_cases_are(vec![vec![]]));
        let PreHirStmt::Block(stmts) = wrapped else {
            panic!("expected the head and the switch in one block");
        };
        assert!(matches!(
            stmts.as_slice(),
            [PreHirStmt::Goto(_), PreHirStmt::Switch { .. }]
        ));
    }

    #[test]
    fn a_skipped_block_entered_from_elsewhere_is_not_owned() {
        let stmt = switch_whose_cases_are(vec![
            vec![PreHirStmt::Return(None)],
            vec![PreHirStmt::Return(None)],
        ]);
        assert!(!skipped_blocks_are_owned(
            2..5,
            &|block| block != 3,
            &label,
            &stmt
        ));
        // Nothing is skipped when the walk resumes at the next block.
        assert!(skipped_blocks_are_owned(2..2, &|_| false, &label, &stmt));
    }
    use crate::helpers::extract_eq_const_operands;
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirBinaryOp, PreHirUnaryOp};

    #[test]
    fn merge_equivalent_switch_cases_merges_non_adjacent_equal_bodies() {
        let mut cases = vec![
            PreHirSwitchCase {
                values: vec![1],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))]
                .into(),
            },
            PreHirSwitchCase {
                values: vec![2],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))]
                .into(),
            },
            PreHirSwitchCase {
                values: vec![3],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )))]
                .into(),
            },
        ];

        merge_equivalent_switch_cases(&mut cases);

        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].values, vec![1, 3]);
        assert_eq!(cases[1].values, vec![2]);
    }

    #[test]
    fn extract_eq_const_operands_normalizes_subtracted_selector() {
        let selector = PreHirExpr::Var("msg".to_string());
        let shifted = PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(PreHirExpr::Const(
                160,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let recovered = extract_eq_const_operands(
            &shifted,
            &PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        )
        .expect("normalized selector");
        assert_eq!(recovered.0, selector);
        assert_eq!(recovered.1, 160);
    }

    #[test]
    fn extract_range_guard_for_chain_normalizes_affine_selector() {
        let selector = PreHirExpr::Var("msg".to_string());
        let shifted = PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(selector.clone()),
            rhs: Box::new(PreHirExpr::Const(
                160,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let cond = PreHirExpr::Binary {
            op: PreHirBinaryOp::Le,
            lhs: Box::new(shifted),
            rhs: Box::new(PreHirExpr::Const(
                95,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Bool,
        };

        let recovered =
            extract_range_guard_for_chain(&cond, true).expect("normalized range guard selector");
        assert_eq!(recovered, selector);
    }

    /// A switch with two cases where case 0 ends with a goto targeting case 1's label.
    /// The goto should be replaced with the __fallthrough sentinel.
    #[test]
    fn test_switch_fallthrough_detection_patches_goto_to_next_label() {
        let next_label = "block_0x1000".to_string();
        let mut cases = vec![
            PreHirSwitchCase {
                values: vec![0],
                body: vec![
                    PreHirStmt::Label("block_0x0000".to_string()),
                    PreHirStmt::Goto(next_label.clone()),
                ]
                .into(),
            },
            PreHirSwitchCase {
                values: vec![1],
                body: vec![
                    PreHirStmt::Label(next_label.clone()),
                    PreHirStmt::Return(None),
                ]
                .into(),
            },
        ];

        let patched = detect_and_patch_case_fallthrough(&mut cases, None);
        assert_eq!(patched, 1, "Expected 1 fallthrough patched");
        // The goto in case[0] should now be the sentinel.
        assert!(
            matches!(&cases[0].body[1], PreHirStmt::Goto(lbl) if lbl == SWITCH_FALLTHROUGH_SENTINEL),
            "Expected __fallthrough sentinel, got: {:?}",
            &cases[0].body[1]
        );
        // Case[1] should be unchanged.
        assert!(matches!(&cases[1].body[0], PreHirStmt::Label(lbl) if lbl == &next_label),);
    }

    /// A switch where case 0's goto targets a label NOT in case 1 — must not be patched.
    #[test]
    fn test_switch_fallthrough_detection_ignores_non_adjacent_goto() {
        let mut cases = vec![
            PreHirSwitchCase {
                values: vec![0],
                body: vec![
                    PreHirStmt::Label("block_a".to_string()),
                    PreHirStmt::Goto("block_x".to_string()), // points somewhere else
                ]
                .into(),
            },
            PreHirSwitchCase {
                values: vec![1],
                body: vec![
                    PreHirStmt::Label("block_b".to_string()),
                    PreHirStmt::Return(None),
                ]
                .into(),
            },
        ];
        let patched = detect_and_patch_case_fallthrough(&mut cases, None);
        assert_eq!(patched, 0, "Expected 0 fallthroughs for non-adjacent goto");
    }

    #[test]
    fn test_switch_fallthrough_detection_patches_last_case_to_default_and_checks_only_head_label() {
        let mut cases = vec![
            PreHirSwitchCase {
                values: vec![0],
                body: vec![
                    PreHirStmt::Label("block_0x1000".to_string()),
                    PreHirStmt::Goto("inner_loop_0x2000".to_string()),
                ]
                .into(),
            },
            PreHirSwitchCase {
                values: vec![1],
                body: vec![
                    PreHirStmt::Label("block_0x2000".to_string()),
                    PreHirStmt::Label("inner_loop_0x2000".to_string()),
                    PreHirStmt::Goto("block_0x3000".to_string()),
                ]
                .into(),
            },
        ];
        let default = vec![
            PreHirStmt::Label("block_0x3000".to_string()),
            PreHirStmt::Return(None),
        ];

        let patched = detect_and_patch_case_fallthrough(&mut cases, Some(&default));
        assert_eq!(
            patched, 1,
            "Only last case -> default head label should be patched"
        );
        assert!(
            matches!(&cases[0].body[1], PreHirStmt::Goto(lbl) if lbl == "inner_loop_0x2000"),
            "Non-head label in next case must not be treated as case entry fallthrough"
        );
        assert!(
            matches!(&cases[1].body[2], PreHirStmt::Goto(lbl) if lbl == SWITCH_FALLTHROUGH_SENTINEL),
            "Last case goto to default head label should be patched to fallthrough sentinel"
        );
    }

    #[test]
    pub fn compare_chain_switch_candidate_requires_two_cases_and_distinct_targets() {
        assert!(!compare_chain_switch_candidate_values(&[(1, 3)], 4));
        assert!(!compare_chain_switch_candidate_values(&[(1, 3), (2, 3)], 3));
        assert!(compare_chain_switch_candidate_values(&[(1, 3), (2, 4)], 5));
    }
}
