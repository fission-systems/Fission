/// x86 EFLAGS condition-code recovery pass.
///
/// After HIR building, branch conditions that reference raw flag variables
/// (`cf`, `zf`, `sf`, `of`, `pf`) are recovered into high-level comparisons
/// using the 16 x86 Jcc semantics:
///
/// | Jcc   | Raw HIR condition   | Recovered            |
/// |-------|---------------------|----------------------|
/// | JE    | `zf`                | `a == b`             |
/// | JNE   | `!zf`               | `a != b`             |
/// | JB    | `cf`                | `a < b` (unsigned)   |
/// | JAE   | `!cf`               | `a >= b` (unsigned)  |
/// | JBE   | `cf \|\| zf`        | `a <= b` (unsigned)  |
/// | JA    | `!cf && !zf`        | `a > b`  (unsigned)  |
/// | JL    | `sf != of`          | `a < b`  (signed)    |
/// | JGE   | `sf == of`          | `a >= b` (signed)    |
/// | JLE   | `zf \|\| sf != of`  | `a <= b` (signed)    |
/// | JG    | `!zf && sf == of`   | `a > b`  (signed)    |
/// | JS    | `sf`                | `result < 0`         |
/// | JNS   | `!sf`               | `result >= 0`        |
/// | JO    | `of`                | (overflow)           |
/// | JNO   | `!of`               | (!overflow)          |
/// | JP    | `pf`                | (parity)             |
/// | JNP   | `!pf`               | (!parity)            |
///
/// Algorithm:
/// 1. Walk straight-line statement streams with a local reaching-definition map
///    for raw flags. Conditions use the most recent flag definitions in that
///    stream. Labels and terminating control-flow statements clear the map so
///    definitions never cross a possible non-linear predecessor.
/// 2. Also keep the older single-definition scan as a whole-function fallback
///    for already-structured shapes where a flag has only one definition.
/// 3. Reconstruct the high-level expression using the flag definitions.
/// 4. Return `true` if any substitution was made (caller re-runs cleanup passes).
use crate::prelude::*;
use crate::analysis::defuse::DefUseMap;
use crate::analysis::liveness::LivenessTransfer;
use crate::cleanup::expr_has_side_effects;
use crate::{HashMap, HashSet};

/// x86 EFLAGS variable names produced by `arch::x86::unique_x86_register_name`.
const FLAG_NAMES: &[&str] = &["cf", "pf", "af", "zf", "sf", "of"];

fn is_flag_var(name: &str) -> bool {
    matches!(name, "cf" | "pf" | "af" | "zf" | "sf" | "of")
}

// ── Phase 1: Definition scan ──────────────────────────────────────────────────

/// Count how many times each flag variable is assigned in the entire body.
fn count_flag_defs(stmts: &[DirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                ..
            } if is_flag_var(name) => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            DirStmt::Block(body) => count_flag_defs(body, counts),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_flag_defs(then_body, counts);
                count_flag_defs(else_body, counts);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                count_flag_defs(body, counts)
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    count_flag_defs(std::slice::from_ref(init.as_ref()), counts);
                }
                count_flag_defs(body, counts);
                if let Some(update) = update {
                    count_flag_defs(std::slice::from_ref(update.as_ref()), counts);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_flag_defs(&case.body, counts);
                }
                count_flag_defs(default, counts);
            }
            _ => {}
        }
    }
}

/// Collect definitions for flags that have exactly ONE assignment in the body.
fn collect_single_defs(stmts: &[DirStmt]) -> HashMap<String, DirExpr> {
    // First pass: count assignments per flag.
    let mut counts: HashMap<String, usize> = HashMap::default();
    count_flag_defs(stmts, &mut counts);

    // Only retain singly-defined flags (conservative correctness).
    let single: crate::HashSet<String> = counts
        .into_iter()
        .filter(|(_, c)| *c == 1)
        .map(|(k, _)| k)
        .collect();

    if single.is_empty() {
        return HashMap::default();
    }

    // Second pass: collect the actual definition expressions.
    let mut defs: HashMap<String, DirExpr> = HashMap::default();
    collect_defs_for(stmts, &single, &mut defs);
    defs
}

fn collect_defs_for(
    stmts: &[DirStmt],
    wanted: &crate::HashSet<String>,
    defs: &mut HashMap<String, DirExpr>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if wanted.contains(name.as_str()) => {
                defs.entry(name.clone()).or_insert_with(|| rhs.clone());
            }
            DirStmt::Block(body) => collect_defs_for(body, wanted, defs),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defs_for(then_body, wanted, defs);
                collect_defs_for(else_body, wanted, defs);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_defs_for(body, wanted, defs)
            }
            DirStmt::For { body, .. } => collect_defs_for(body, wanted, defs),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defs_for(&case.body, wanted, defs);
                }
                collect_defs_for(default, wanted, defs);
            }
            _ => {}
        }
    }
}

// ── Phase 2: Pattern extraction helpers ──────────────────────────────────────

/// Extract `(a, b)` from `__sborrow(a, b)` or `__scarry(a, b)`.
fn extract_sborrow_args(expr: &DirExpr) -> Option<(DirExpr, DirExpr)> {
    if let DirExpr::Call { target, args, .. } = expr {
        if (target == "__sborrow" || target == "__scarry") && args.len() == 2 {
            return Some((args[0].clone(), args[1].clone()));
        }
    }
    None
}

/// Extract `(a, b)` from `a < b` (unsigned Lt).
fn extract_lt_args(expr: &DirExpr) -> Option<(DirExpr, DirExpr)> {
    if let DirExpr::Binary {
        op: DirBinaryOp::Lt,
        lhs,
        rhs,
        ..
    } = expr
    {
        return Some((*lhs.clone(), *rhs.clone()));
    }
    None
}

/// Extract `(a, b)` from `a == b`.
fn extract_eq_args(expr: &DirExpr) -> Option<(DirExpr, DirExpr)> {
    if let DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = expr
    {
        return Some((*lhs.clone(), *rhs.clone()));
    }
    None
}

// ── Phase 3: Condition pattern matching ──────────────────────────────────────

/// Check whether `expr` is `Var(flag)`.
fn is_flag_expr(expr: &DirExpr, flag: &str) -> bool {
    matches!(expr, DirExpr::Var(n) if n == flag)
}

/// Check whether `expr` is `!Var(flag)`.
fn is_not_flag(expr: &DirExpr, flag: &str) -> bool {
    matches!(expr, DirExpr::Unary { op: DirUnaryOp::Not, expr: inner, .. }
             if is_flag_expr(inner, flag))
}

/// Check whether `expr` is `Var(sf) == Var(of)` or `Var(of) == Var(sf)`.
fn is_sf_eq_of(expr: &DirExpr) -> bool {
    matches!(expr,
        DirExpr::Binary { op: DirBinaryOp::Eq, lhs, rhs, .. }
        if (is_flag_expr(lhs, "sf") && is_flag_expr(rhs, "of"))
            || (is_flag_expr(lhs, "of") && is_flag_expr(rhs, "sf")))
}

/// Check whether `expr` is `Var(sf) != Var(of)` or `Var(of) != Var(sf)`.
fn is_sf_ne_of(expr: &DirExpr) -> bool {
    matches!(expr,
        DirExpr::Binary { op: DirBinaryOp::Ne, lhs, rhs, .. }
        if (is_flag_expr(lhs, "sf") && is_flag_expr(rhs, "of"))
            || (is_flag_expr(lhs, "of") && is_flag_expr(rhs, "sf")))
}

/// Helper: build `Binary { op, lhs, rhs, ty: Bool }`.
fn bool_binary(op: DirBinaryOp, lhs: DirExpr, rhs: DirExpr) -> DirExpr {
    DirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

/// Substitute any raw flag `Var` references in `expr` with their definitions.
/// Returns `Some(new_expr)` if any substitution occurred, `None` otherwise.
fn substitute_single_flags(expr: &DirExpr, defs: &HashMap<String, DirExpr>) -> Option<DirExpr> {
    match expr {
        DirExpr::Var(name) if is_flag_var(name) => defs.get(name).cloned(),
        DirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => substitute_single_flags(inner, defs).map(|new_inner| DirExpr::Unary {
            op: *op,
            expr: Box::new(new_inner),
            ty: ty.clone(),
        }),
        DirExpr::Binary { op, lhs, rhs, ty } => {
            let new_lhs = substitute_single_flags(lhs, defs);
            let new_rhs = substitute_single_flags(rhs, defs);
            if new_lhs.is_some() || new_rhs.is_some() {
                Some(DirExpr::Binary {
                    op: *op,
                    lhs: Box::new(new_lhs.unwrap_or_else(|| *lhs.clone())),
                    rhs: Box::new(new_rhs.unwrap_or_else(|| *rhs.clone())),
                    ty: ty.clone(),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Try to recover a high-level comparison from a condition that references
/// raw x86 flag variables. Returns `Some(recovered)` on success.
pub(super) fn try_recover_flag_condition(
    cond: &DirExpr,
    defs: &HashMap<String, DirExpr>,
) -> Option<DirExpr> {
    // ── JL / JGE: signed SF != OF / SF == OF ──────────────────────────────
    // JL (signed less than): SF != OF → a < b (signed)
    if is_sf_ne_of(cond) {
        if let Some(of_def) = defs.get("of") {
            if let Some((a, b)) = extract_sborrow_args(of_def) {
                return Some(bool_binary(DirBinaryOp::SLt, a, b));
            }
        }
    }
    // JGE (signed greater or equal): SF == OF → a >= b (signed) = !(a < b)
    if is_sf_eq_of(cond) {
        if let Some(of_def) = defs.get("of") {
            if let Some((a, b)) = extract_sborrow_args(of_def) {
                return Some(DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(bool_binary(DirBinaryOp::SLt, a, b)),
                    ty: NirType::Bool,
                });
            }
        }
    }

    // ── JLE / JG: zf + sf/of ─────────────────────────────────────────────
    // JLE (signed <=): ZF=1 OR (SF != OF) → a <= b signed
    // Try: LogicalOr(zf, sf_ne_of) or LogicalOr(sf_ne_of, zf)
    if let DirExpr::Binary {
        op: DirBinaryOp::LogicalOr | DirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = cond
    {
        let (lhs, rhs) = (lhs.as_ref(), rhs.as_ref());
        if (is_flag_expr(lhs, "zf") && is_sf_ne_of(rhs))
            || (is_sf_ne_of(lhs) && is_flag_expr(rhs, "zf"))
        {
            if let Some(of_def) = defs.get("of") {
                if let Some((a, b)) = extract_sborrow_args(of_def) {
                    return Some(bool_binary(DirBinaryOp::SLe, a, b));
                }
            }
        }
        // JBE (unsigned <=): CF=1 OR ZF=1 → a <= b unsigned
        if (is_flag_expr(lhs, "cf") && is_flag_expr(rhs, "zf"))
            || (is_flag_expr(lhs, "zf") && is_flag_expr(rhs, "cf"))
        {
            if let Some(cf_def) = defs.get("cf") {
                if let Some((a, b)) = extract_lt_args(cf_def) {
                    return Some(bool_binary(DirBinaryOp::Le, a, b));
                }
            }
        }
    }

    // JG (signed >): !ZF AND (SF == OF) → a > b signed = b < a
    // Try: LogicalAnd(!zf, sf_eq_of) or LogicalAnd(sf_eq_of, !zf)
    if let DirExpr::Binary {
        op: DirBinaryOp::LogicalAnd | DirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = cond
    {
        let (lhs, rhs) = (lhs.as_ref(), rhs.as_ref());
        if (is_not_flag(lhs, "zf") && is_sf_eq_of(rhs))
            || (is_sf_eq_of(lhs) && is_not_flag(rhs, "zf"))
        {
            if let Some(of_def) = defs.get("of") {
                if let Some((a, b)) = extract_sborrow_args(of_def) {
                    // a > b signed = b < a signed
                    return Some(bool_binary(DirBinaryOp::SLt, b, a));
                }
            }
        }
        // JA (unsigned >): !CF AND !ZF → a > b unsigned = b < a
        if (is_not_flag(lhs, "cf") && is_not_flag(rhs, "zf"))
            || (is_not_flag(lhs, "zf") && is_not_flag(rhs, "cf"))
        {
            if let Some(cf_def) = defs.get("cf") {
                if let Some((a, b)) = extract_lt_args(cf_def) {
                    // a > b unsigned = b < a
                    return Some(bool_binary(DirBinaryOp::Lt, b, a));
                }
            }
        }
    }

    // ── Single-flag substitution ──────────────────────────────────────────
    // For any remaining flag var references, substitute definitions directly.
    // The existing normalizer will further simplify (e.g. !(a==b) → a!=b).
    substitute_single_flags(cond, defs)
}

// ── Phase 4: Walk statements ──────────────────────────────────────────────────

fn recover_in_cond(cond: &mut DirExpr, defs: &HashMap<String, DirExpr>, changed: &mut bool) {
    if let Some(recovered) = try_recover_flag_condition(cond, defs) {
        *cond = recovered;
        *changed = true;
        // Re-normalize the substituted expression.
        super::super::pipeline::normalize_expr(cond);
    }
}

fn recover_in_stmts_box(
    stmt: &mut Box<DirStmt>,
    defs: &HashMap<String, DirExpr>,
    changed: &mut bool,
) {
    let mut tmp = vec![*stmt.clone()];
    recover_in_stmts(&mut tmp, defs, changed);
    if let Some(s) = tmp.into_iter().next() {
        **stmt = s;
    }
}

fn recover_in_stmts(stmts: &mut Vec<DirStmt>, defs: &HashMap<String, DirExpr>, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                recover_in_cond(cond, defs, changed);
                recover_in_stmts(then_body, defs, changed);
                recover_in_stmts(else_body, defs, changed);
            }
            DirStmt::While { cond, body } => {
                recover_in_cond(cond, defs, changed);
                recover_in_stmts(body, defs, changed);
            }
            DirStmt::DoWhile { body, cond } => {
                recover_in_stmts(body, defs, changed);
                recover_in_cond(cond, defs, changed);
            }
            DirStmt::For {
                cond,
                body,
                init,
                update,
                ..
            } => {
                if let Some(c) = cond {
                    recover_in_cond(c, defs, changed);
                }
                if let Some(i) = init {
                    recover_in_stmts_box(i, defs, changed);
                }
                if let Some(u) = update {
                    recover_in_stmts_box(u, defs, changed);
                }
                recover_in_stmts(body, defs, changed);
            }
            DirStmt::Block(body) => recover_in_stmts(body, defs, changed),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    recover_in_stmts(&mut case.body, defs, changed);
                }
                recover_in_stmts(default, defs, changed);
            }
            _ => {}
        }
    }
}

fn recover_in_stmts_with_reaching_defs(
    stmts: &mut Vec<DirStmt>,
    defs: &mut HashMap<String, DirExpr>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if is_flag_var(name) => {
                defs.insert(name.clone(), rhs.clone());
            }
            DirStmt::Block(body) => {
                let mut nested_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(body, &mut nested_defs, changed);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                recover_in_cond(cond, defs, changed);
                let mut then_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(then_body, &mut then_defs, changed);
                let mut else_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(else_body, &mut else_defs, changed);
            }
            DirStmt::While { cond, body } => {
                recover_in_cond(cond, defs, changed);
                let mut body_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(body, &mut body_defs, changed);
            }
            DirStmt::DoWhile { body, cond } => {
                let mut body_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(body, &mut body_defs, changed);
                recover_in_cond(cond, &body_defs, changed);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let mut loop_defs = defs.clone();
                if let Some(init) = init {
                    recover_in_stmts_box_with_reaching_defs(init, &mut loop_defs, changed);
                }
                if let Some(cond) = cond {
                    recover_in_cond(cond, &loop_defs, changed);
                }
                let mut body_defs = loop_defs.clone();
                recover_in_stmts_with_reaching_defs(body, &mut body_defs, changed);
                if let Some(update) = update {
                    recover_in_stmts_box_with_reaching_defs(update, &mut body_defs, changed);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    let mut case_defs = defs.clone();
                    recover_in_stmts_with_reaching_defs(&mut case.body, &mut case_defs, changed);
                }
                let mut default_defs = defs.clone();
                recover_in_stmts_with_reaching_defs(default, &mut default_defs, changed);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => {
                defs.clear();
            }
            _ => {}
        }
    }
}

fn recover_in_stmts_box_with_reaching_defs(
    stmt: &mut Box<DirStmt>,
    defs: &mut HashMap<String, DirExpr>,
    changed: &mut bool,
) {
    let mut tmp = vec![*stmt.clone()];
    recover_in_stmts_with_reaching_defs(&mut tmp, defs, changed);
    if let Some(s) = tmp.into_iter().next() {
        **stmt = s;
    }
}

// ── Dead flag assignment elimination ─────────────────────────────────────────

#[derive(Debug)]
struct FlagBasicBlock {
    start: usize,
    end: usize,
    successors: Vec<usize>,
}

fn collect_goto_targets(stmt: &DirStmt, targets: &mut HashSet<String>) {
    match stmt {
        DirStmt::Goto(label) => {
            targets.insert(label.clone());
        }
        DirStmt::Block(body) => {
            for stmt in body {
                collect_goto_targets(stmt, targets);
            }
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body.iter().chain(else_body) {
                collect_goto_targets(stmt, targets);
            }
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            for stmt in body {
                collect_goto_targets(stmt, targets);
            }
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_goto_targets(init, targets);
            }
            for stmt in body {
                collect_goto_targets(stmt, targets);
            }
            if let Some(update) = update {
                collect_goto_targets(update, targets);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_goto_targets(stmt, targets);
                }
            }
            for stmt in default {
                collect_goto_targets(stmt, targets);
            }
        }
        _ => {}
    }
}

fn build_flag_cfg(stmts: &[DirStmt]) -> Vec<FlagBasicBlock> {
    if stmts.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0];
    for (index, stmt) in stmts.iter().enumerate() {
        if index != 0 && matches!(stmt, DirStmt::Label(_)) {
            starts.push(index);
        }
        if index + 1 < stmts.len() && matches!(stmt, DirStmt::Goto(_) | DirStmt::Return(_)) {
            starts.push(index + 1);
        }
    }
    starts.sort_unstable();
    starts.dedup();

    let mut blocks: Vec<FlagBasicBlock> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| FlagBasicBlock {
            start: *start,
            end: starts.get(index + 1).copied().unwrap_or(stmts.len()),
            successors: Vec::new(),
        })
        .collect();

    let mut label_blocks = HashMap::default();
    for (block_index, block) in blocks.iter().enumerate() {
        for stmt in &stmts[block.start..block.end] {
            if let DirStmt::Label(label) = stmt {
                label_blocks.insert(label.clone(), block_index);
            }
        }
    }

    for block_index in 0..blocks.len() {
        let start = blocks[block_index].start;
        let end = blocks[block_index].end;
        let mut targets = HashSet::default();
        for stmt in &stmts[start..end] {
            collect_goto_targets(stmt, &mut targets);
        }
        let mut successors: Vec<usize> = targets
            .into_iter()
            .filter_map(|target| label_blocks.get(&target).copied())
            .collect();

        let terminal = &stmts[end - 1];
        if !matches!(terminal, DirStmt::Goto(_) | DirStmt::Return(_))
            && block_index + 1 < blocks.len()
        {
            successors.push(block_index + 1);
        }
        successors.sort_unstable();
        successors.dedup();
        blocks[block_index].successors = successors;
    }
    blocks
}

fn flag_uses(stmt: &DirStmt) -> HashSet<String> {
    LivenessTransfer::for_stmt(stmt)
        .uses_before_definition()
        .map(str::to_string)
        .filter(|name| is_flag_var(name))
        .collect()
}

fn flag_definition(stmt: &DirStmt) -> Option<(&str, &DirExpr)> {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } if is_flag_var(name) => Some((name, rhs)),
        _ => None,
    }
}

fn transfer_flag_block(
    block: &FlagBasicBlock,
    stmt_uses: &[HashSet<String>],
    stmts: &[DirStmt],
    live_out: &HashSet<String>,
) -> HashSet<String> {
    let mut live = live_out.clone();
    for index in (block.start..block.end).rev() {
        if let Some((name, _)) = flag_definition(&stmts[index]) {
            live.remove(name);
        }
        live.extend(stmt_uses[index].iter().cloned());
    }
    live
}

/// Find pure top-level flag definitions that are dead under the label/goto CFG.
///
/// Nested structured statements are summarized conservatively as uses and are
/// cleaned only when the flag has no use anywhere. This avoids inventing a
/// second structured-CFG engine inside normalization.
fn dead_flag_definition_sites(stmts: &[DirStmt]) -> HashSet<usize> {
    let blocks = build_flag_cfg(stmts);
    if blocks.is_empty() {
        return HashSet::default();
    }

    let stmt_uses: Vec<HashSet<String>> = stmts.iter().map(flag_uses).collect();
    let mut live_in = vec![HashSet::default(); blocks.len()];
    loop {
        let mut changed = false;
        for block_index in (0..blocks.len()).rev() {
            let mut live_out = HashSet::default();
            for successor in &blocks[block_index].successors {
                live_out.extend(live_in[*successor].iter().cloned());
            }
            let next = transfer_flag_block(&blocks[block_index], &stmt_uses, stmts, &live_out);
            if next != live_in[block_index] {
                live_in[block_index] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut dead = HashSet::default();
    for block in &blocks {
        let mut live = HashSet::default();
        for successor in &block.successors {
            live.extend(live_in[*successor].iter().cloned());
        }
        for index in (block.start..block.end).rev() {
            if let Some((name, rhs)) = flag_definition(&stmts[index]) {
                if !live.contains(name) && !expr_has_side_effects(rhs) {
                    dead.insert(std::ptr::from_ref(&stmts[index]) as usize);
                }
                live.remove(name);
            }
            live.extend(stmt_uses[index].iter().cloned());
        }
    }
    dead
}

fn remove_dead_sites(stmts: &mut Vec<DirStmt>, dead_sites: &HashSet<usize>, changed: &mut bool) {
    stmts.retain(|stmt| {
        let remove = dead_sites.contains(&(std::ptr::from_ref(stmt) as usize));
        *changed |= remove;
        !remove
    });
}

fn remove_globally_unused_flags(
    stmts: &mut Vec<DirStmt>,
    uses: &HashMap<String, usize>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) => remove_globally_unused_flags(body, uses, changed),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                remove_globally_unused_flags(then_body, uses, changed);
                remove_globally_unused_flags(else_body, uses, changed);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                remove_globally_unused_flags(body, uses, changed);
            }
            DirStmt::For { body, .. } => remove_globally_unused_flags(body, uses, changed),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    remove_globally_unused_flags(&mut case.body, uses, changed);
                }
                remove_globally_unused_flags(default, uses, changed);
            }
            _ => {}
        }
    }
    stmts.retain(|stmt| {
        let remove = flag_definition(stmt).is_some_and(|(name, rhs)| {
            uses.get(name).copied().unwrap_or(0) == 0 && !expr_has_side_effects(rhs)
        });
        *changed |= remove;
        !remove
    });
}

/// Remove pure x86 flag definitions using CFG liveness for unstructured HIR
/// plus structured liveness summaries for nested bodies.
fn remove_dead_flag_assigns(func: &mut DirFunction) {
    let dead_sites = dead_flag_definition_sites(&func.body);
    let mut changed = false;
    remove_dead_sites(&mut func.body, &dead_sites, &mut changed);

    // Flag definitions form dependency chains (for example CF feeds ZF/PF).
    // Removing the terminal dead definition can expose its predecessors, so
    // recompute uses until no more pure flag definitions become dead.
    loop {
        let uses = DefUseMap::build(&func.body);
        let mut round_changed = false;
        remove_globally_unused_flags(&mut func.body, &uses.use_count, &mut round_changed);
        changed |= round_changed;
        if !round_changed {
            break;
        }
    }

    let uses = DefUseMap::build(&func.body);
    let mut assigned = HashMap::default();
    count_flag_defs(&func.body, &mut assigned);
    func.locals.retain(|binding| {
        !is_flag_var(&binding.name)
            || uses.use_count.get(&binding.name).copied().unwrap_or(0) > 0
            || assigned.contains_key(&binding.name)
    });
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Apply the x86 EFLAGS condition-code recovery pass to `func`.
///
/// Returns `true` if any branch condition was rewritten **or** dead flag
/// assignments were removed. Always runs dead-flag cleanup: flag writes may
/// be pure noise even when no condition was rewritten in this pass (e.g. the
/// compare was already high-level while CF/OF/SF/ZF/PF stores remain).
pub fn apply_flag_recovery_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;
    let mut reaching_defs = HashMap::default();
    recover_in_stmts_with_reaching_defs(&mut func.body, &mut reaching_defs, &mut changed);

    let defs = collect_single_defs(&func.body);
    if !defs.is_empty() {
        recover_in_stmts(&mut func.body, &defs, &mut changed);
    }
    // Always scrub dead flag stores — not only when a condition was rewritten.
    // Leaving `cf = …; of = …;` without rvalue uses produces undeclared-ident
    // compile errors once flags are named (SLA 0x200 layout).
    let before = flag_assign_count(&func.body);
    remove_dead_flag_assigns(func);
    let after = flag_assign_count(&func.body);
    changed || before != after
}

/// Late cleanup: drop unused flag assignments after other normalize waves may
/// have left residual CF/OF/SF/ZF/PF stores.
pub fn apply_dead_flag_cleanup_pass(func: &mut DirFunction) -> bool {
    let before = flag_assign_count(&func.body);
    remove_dead_flag_assigns(func);
    flag_assign_count(&func.body) != before
}

fn flag_assign_count(stmts: &[DirStmt]) -> usize {
    let mut n = 0;
    count_flag_assigns(stmts, &mut n);
    n
}

fn count_flag_assigns(stmts: &[DirStmt], n: &mut usize) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                ..
            } if is_flag_var(name) => *n += 1,
            DirStmt::Block(body) => count_flag_assigns(body, n),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_flag_assigns(then_body, n);
                count_flag_assigns(else_body, n);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                count_flag_assigns(body, n)
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    count_flag_assigns(std::slice::from_ref(init.as_ref()), n);
                }
                count_flag_assigns(body, n);
                if let Some(update) = update {
                    count_flag_assigns(std::slice::from_ref(update.as_ref()), n);
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_flag_assigns(&case.body, n);
                }
                count_flag_assigns(default, n);
            }
            _ => {}
        }
    }
}

// ── Helpers used by other passes ─────────────────────────────────────────────

/// Returns `true` if `name` is an x86 flag variable name.
pub(super) fn is_x86_flag_variable(name: &str) -> bool {
    is_flag_var(name)
}

/// Returns the set of all x86 flag variable names.
pub(super) fn x86_flag_names() -> &'static [&'static str] {
    FLAG_NAMES
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    fn int(value: i64) -> DirExpr {
        DirExpr::Const(
            value,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )
    }

    fn eq(lhs: DirExpr, rhs: DirExpr) -> DirExpr {
        bool_binary(DirBinaryOp::Eq, lhs, rhs)
    }

    fn not(expr: DirExpr) -> DirExpr {
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: Box::new(expr),
            ty: NirType::Bool,
        }
    }

    fn assign(name: &str, rhs: DirExpr) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn first_if_cond(func: &DirFunction) -> &DirExpr {
        func.body
            .iter()
            .find_map(|stmt| match stmt {
                DirStmt::If { cond, .. } => Some(cond),
                _ => None,
            })
            .expect("function contains if")
    }

    #[test]
    fn local_reaching_flag_recovery_uses_latest_straight_line_definition() {
        let mut func = DirFunction {
            body: vec![
                assign("zf", eq(var("stale_row"), var("stale_limit"))),
                assign("tmp", int(0)),
                assign("zf", eq(var("row"), var("limit"))),
                DirStmt::If {
                    cond: not(var("zf")),
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
            ],
            ..DirFunction::default()
        };

        assert!(apply_flag_recovery_pass(&mut func));
        assert_eq!(
            first_if_cond(&func),
            &bool_binary(DirBinaryOp::Ne, var("row"), var("limit"))
        );
    }

    #[test]
    fn local_reaching_flag_recovery_does_not_cross_label_boundary() {
        let mut func = DirFunction {
            body: vec![
                assign("zf", eq(var("stale_row"), var("stale_limit"))),
                DirStmt::Label("join".to_string()),
                DirStmt::If {
                    cond: not(var("zf")),
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
                assign("zf", eq(var("later_row"), var("later_limit"))),
            ],
            ..DirFunction::default()
        };

        // The label still blocks expression substitution. The pass reports a
        // change only because the definition after the condition is dead.
        assert!(apply_flag_recovery_pass(&mut func));
        assert_eq!(first_if_cond(&func), &not(var("zf")));
        assert_eq!(flag_assign_count(&func.body), 1);
    }

    #[test]
    fn dead_flag_cleanup_removes_only_overwritten_definition() {
        let live_rhs = eq(var("current_value"), var("current_limit"));
        let mut func = DirFunction {
            body: vec![
                assign("cf", eq(var("stale_value"), var("stale_limit"))),
                assign("cf", live_rhs.clone()),
                DirStmt::Return(Some(var("cf"))),
            ],
            ..DirFunction::default()
        };

        assert!(apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 1);
        assert!(matches!(
            &func.body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if name == "cf" && rhs == &live_rhs
        ));
    }

    #[test]
    fn dead_flag_cleanup_removes_entry_definition_hidden_by_loop_definition() {
        let live_rhs = eq(var("current_value"), var("current_limit"));
        let mut func = DirFunction {
            body: vec![
                assign("cf", eq(var("entry_stack"), int(16))),
                DirStmt::While {
                    cond: int(1),
                    body: vec![
                        assign("cf", live_rhs.clone()),
                        DirStmt::If {
                            cond: var("cf"),
                            then_body: Vec::new(),
                            else_body: Vec::new(),
                        },
                    ],
                },
            ],
            ..DirFunction::default()
        };

        assert!(apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 1);
        assert!(matches!(
            &func.body[0],
            DirStmt::While { body, .. }
                if matches!(
                    &body[0],
                    DirStmt::Assign {
                        lhs: DirLValue::Var(name),
                        rhs,
                    } if name == "cf" && rhs == &live_rhs
                )
        ));
    }

    #[test]
    fn dead_flag_cleanup_reaches_fixed_point_across_flag_dependencies() {
        let mut func = DirFunction {
            body: vec![
                assign("cf", eq(var("entry_stack"), int(16))),
                assign("zf", not(var("cf"))),
                assign("pf", var("zf")),
            ],
            ..DirFunction::default()
        };

        assert!(apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 0);
    }

    #[test]
    fn dead_flag_cleanup_keeps_definitions_from_both_branch_predecessors() {
        let mut func = DirFunction {
            body: vec![
                DirStmt::If {
                    cond: var("selector"),
                    then_body: vec![assign("cf", eq(var("left"), var("limit")))],
                    else_body: vec![assign("cf", eq(var("right"), var("limit")))],
                },
                DirStmt::Return(Some(var("cf"))),
            ],
            ..DirFunction::default()
        };

        assert!(!apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 2);
    }

    #[test]
    fn dead_flag_cleanup_keeps_definitions_at_label_cfg_join() {
        let mut func = DirFunction {
            body: vec![
                DirStmt::If {
                    cond: var("selector"),
                    then_body: vec![DirStmt::Goto("left_path".to_string())],
                    else_body: vec![DirStmt::Goto("right_path".to_string())],
                },
                DirStmt::Label("left_path".to_string()),
                assign("cf", eq(var("left"), var("limit"))),
                DirStmt::Goto("join".to_string()),
                DirStmt::Label("right_path".to_string()),
                assign("cf", eq(var("right"), var("limit"))),
                DirStmt::Goto("join".to_string()),
                DirStmt::Label("join".to_string()),
                DirStmt::Return(Some(var("cf"))),
            ],
            ..DirFunction::default()
        };

        assert!(!apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 2);
    }

    #[test]
    fn dead_flag_cleanup_follows_goto_edges_and_definition_kills() {
        let live_rhs = eq(var("live_value"), var("live_limit"));
        let mut func = DirFunction {
            body: vec![
                assign("cf", eq(var("stale_value"), var("stale_limit"))),
                DirStmt::Goto("redefine_flag".to_string()),
                DirStmt::Label("redefine_flag".to_string()),
                assign("cf", live_rhs.clone()),
                DirStmt::Goto("use_flag".to_string()),
                DirStmt::Label("use_flag".to_string()),
                DirStmt::Return(Some(var("cf"))),
            ],
            ..DirFunction::default()
        };

        assert!(apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 1);
        assert!(func.body.iter().any(|stmt| matches!(
            stmt,
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if name == "cf" && rhs == &live_rhs
        )));
    }

    #[test]
    fn dead_flag_cleanup_retains_effectful_dead_definition() {
        let mut func = DirFunction {
            body: vec![assign(
                "cf",
                DirExpr::Call {
                    target: "observe_machine_state".to_string(),
                    args: Vec::new(),
                    ty: NirType::Bool,
                },
            )],
            ..DirFunction::default()
        };

        assert!(!apply_dead_flag_cleanup_pass(&mut func));
        assert_eq!(flag_assign_count(&func.body), 1);
    }
}
