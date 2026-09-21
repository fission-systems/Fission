//! Presentation-only recovery of switch-shaped HIR.
//!
//! This module owns the two compiler-lowered switch forms handled by the HIR
//! presentation fixed point: equality if-chains and select-based decision
//! trees.  Its public surface is intentionally limited to those two entry
//! points; expression purity and HIR node types remain borrowed from the
//! presentation owner.

use super::{HirBinaryOp, HirExpr, HirStmt, HirSwitchCase, HirUnaryOp, expr_is_presentation_pure};
use std::collections::HashSet;
// ── Switch recovery ─────────────────────────────────────────────────────────
/// Two cases reads just as naturally as `if`/`else if`; the payoff shows up
/// once there are enough arms that the repeated `x == ` noise dominates.
const MIN_LOWERED_SWITCH_CASES: usize = 3;

/// `if (x == c1) {..} else if (x == c2) {..} else if (x == c3) {..} else {..}`
/// → `switch (x) { case c1: ..; case c2: ..; case c3: ..; default: ..; }`.
///
/// GCC and Clang sometimes lower a `switch` with too few or too sparse
/// cases to this exact if-chain shape instead of emitting a jump table
/// (the "switch lowering" compiler transform documented in the SAILR
/// paper, USENIX 2024) -- Fission's own switch recovery
/// (`fission-midend-structuring`'s jump-table-based rule) never sees a
/// jump table to key off in this case, so the chain surfaces as nested
/// `if`/`else` instead of the switch it started as.
///
/// A real `switch` statement evaluates its expression exactly once; an
/// if-chain evaluates it up to once per untaken arm. Collapsing multiple
/// evaluations into one is only safe when the expression is provably
/// side-effect-free (`expr_is_presentation_pure`) -- otherwise a call or a
/// load that used to run on every untaken comparison would silently stop
/// running.
pub(super) fn recover_switch_from_lowered_if_chain(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_switch_from_lowered_if_chain_in_stmt(stmt);
    }
    for stmt in stmts.iter_mut() {
        if let Some(switch_stmt) = try_build_switch_from_if_chain(stmt) {
            *stmt = switch_stmt;
            changed = true;
        }
    }
    changed
}

fn recover_switch_from_lowered_if_chain_in_stmt(stmt: &mut HirStmt) -> bool {
    match stmt {
        HirStmt::Block(body) => recover_switch_from_lowered_if_chain(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let a = recover_switch_from_lowered_if_chain(then_body);
            let b = recover_switch_from_lowered_if_chain(else_body);
            a || b
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            recover_switch_from_lowered_if_chain(body)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = recover_switch_from_lowered_if_chain(body);
            if let Some(i) = init {
                changed |= recover_switch_from_lowered_if_chain_in_stmt(i);
            }
            if let Some(u) = update {
                changed |= recover_switch_from_lowered_if_chain_in_stmt(u);
            }
            changed
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                changed |= recover_switch_from_lowered_if_chain(&mut case.body);
            }
            changed |= recover_switch_from_lowered_if_chain(default);
            changed
        }
        HirStmt::Assign { .. }
        | HirStmt::Expr(_)
        | HirStmt::VaStart { .. }
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

/// `x == c` or `c == x` → `(x, c)`.
fn match_eq_const(cond: &HirExpr) -> Option<(&HirExpr, i64)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };
    if let HirExpr::Const(value, _) = rhs.as_ref() {
        return Some((lhs.as_ref(), *value));
    }
    if let HirExpr::Const(value, _) = lhs.as_ref() {
        return Some((rhs.as_ref(), *value));
    }
    None
}

fn try_build_switch_from_if_chain(stmt: &HirStmt) -> Option<HirStmt> {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    let (switch_expr, first_value) = match_eq_const(cond)?;
    if !expr_is_presentation_pure(switch_expr) {
        return None;
    }
    let mut cases = vec![HirSwitchCase {
        values: vec![first_value],
        body: then_body.clone(),
    }];
    let mut seen_values: HashSet<i64> = [first_value].into_iter().collect();
    let mut tail: &Vec<HirStmt> = else_body;
    loop {
        let [
            HirStmt::If {
                cond: next_cond,
                then_body: next_then,
                else_body: next_else,
            },
        ] = tail.as_slice()
        else {
            break;
        };
        let Some((next_expr, next_value)) = match_eq_const(next_cond) else {
            break;
        };
        if next_expr != switch_expr || seen_values.contains(&next_value) {
            break;
        }
        cases.push(HirSwitchCase {
            values: vec![next_value],
            body: next_then.clone(),
        });
        seen_values.insert(next_value);
        tail = next_else;
    }
    if cases.len() < MIN_LOWERED_SWITCH_CASES {
        return None;
    }
    Some(HirStmt::Switch {
        expr: switch_expr.clone(),
        cases,
        default: tail.clone(),
    })
}

/// `return x==10 ? 2 : x>10 ? DFLT : x==0 ? 0 : x<0 ? DFLT : x==1||x==2||x==3 ? 1 : DFLT`
/// → `switch (x) { case 10: return 2; case 0: return 0; case 1: case 2: case 3: return 1; default: return DFLT; }`.
///
/// Companion to `recover_switch_from_lowered_if_chain` for a *different*
/// real compiled shape of the same "switch lowering" deoptimization:
/// GCC's binary-search-style lowering for a small, sparse-case switch
/// builds a tree of direct comparisons against the switch variable rather
/// than a linear equality chain, and Fission's own `fold_if_else_pure_
/// returns_to_select` (which runs first, to give the linear-chain
/// recovery above a chance against shapes it *does* match) has already
/// collapsed that tree into one big nested `Select` expression by the
/// time this pass runs.
///
/// Only recurses into a `Select` as "still inside the decision tree" when
/// its condition is *directly* `switch_var CMP const` (or the bare-negation
/// zero-check idiom `!switch_var`) -- never into an unrelated or
/// arithmetic-derived condition (e.g. `x - 1 > 2`), since that could just
/// as easily be a case body's own unrelated ternary, and misreading it as
/// a tree split would silently corrupt that case's value. Every leaf the
/// walk stops at is required to be *either* a matched `==` case *or*
/// structurally identical (`HirExpr`'s derived `PartialEq`) to every other
/// such leaf -- multiple unreachable-looking default leaves are extremely
/// common in these trees (see the module-level proposal doc) precisely
/// because the compiler's search tree doesn't bottom out evenly, so this
/// only accepts the tree when it can prove there is genuinely one default
/// value, not merely the first one found.
pub(super) fn recover_switch_from_select_decision_tree(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_switch_from_select_decision_tree_in_stmt(stmt);
    }
    for stmt in stmts.iter_mut() {
        if let Some(switch_stmt) = try_build_switch_from_select_tree(stmt) {
            *stmt = switch_stmt;
            changed = true;
        }
    }
    changed
}

fn recover_switch_from_select_decision_tree_in_stmt(stmt: &mut HirStmt) -> bool {
    match stmt {
        HirStmt::Block(body) => recover_switch_from_select_decision_tree(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let a = recover_switch_from_select_decision_tree(then_body);
            let b = recover_switch_from_select_decision_tree(else_body);
            a || b
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            recover_switch_from_select_decision_tree(body)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = recover_switch_from_select_decision_tree(body);
            if let Some(i) = init {
                changed |= recover_switch_from_select_decision_tree_in_stmt(i);
            }
            if let Some(u) = update {
                changed |= recover_switch_from_select_decision_tree_in_stmt(u);
            }
            changed
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                changed |= recover_switch_from_select_decision_tree(&mut case.body);
            }
            changed |= recover_switch_from_select_decision_tree(default);
            changed
        }
        HirStmt::Assign { .. }
        | HirStmt::Expr(_)
        | HirStmt::VaStart { .. }
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

/// The non-constant operand of a direct `var CMP const` (or `const CMP
/// var`) comparison, or the operand of a bare `!var` zero-check -- used
/// only to identify *which* variable a decision tree's root is keyed on,
/// before `classify_direct_split` walks the rest of the tree against it.
fn compared_var(cond: &HirExpr) -> Option<&HirExpr> {
    if let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr,
        ..
    } = cond
    {
        return Some(expr.as_ref());
    }
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    if !matches!(
        op,
        HirBinaryOp::Eq
            | HirBinaryOp::Ne
            | HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
    ) {
        return None;
    }
    if matches!(rhs.as_ref(), HirExpr::Const(_, _)) {
        Some(lhs.as_ref())
    } else if matches!(lhs.as_ref(), HirExpr::Const(_, _)) {
        Some(rhs.as_ref())
    } else {
        None
    }
}

/// If `cond` is a direct comparison of `switch_var` against a constant
/// (`switch_var CMP const` or `const CMP switch_var`, no arithmetic on
/// `switch_var`), or the bare zero-check idiom `!switch_var`, returns
/// `Some(Eq(value))` for an equality split (a genuine case boundary),
/// `Some(Range{..})` for the compiler's biased-subtraction unsigned range
/// check idiom (see below), or `Some(NotEq)` for any other direct
/// comparison (a range-narrowing split that doesn't itself name a case
/// value or a clean finite range). `None` for anything else -- including a
/// direct comparison on some *other* variable, or arithmetic on
/// `switch_var` other than the specific biased-subtraction shape -- which
/// stops the walk at this node rather than risk misreading an unrelated
/// condition as part of the tree.
enum DirectSplitOnVar {
    Eq(i64),
    NotEq,
    /// `(switch_var - bias) UNSIGNED_CMP bound`, recognized as an exact
    /// range test: the "in-range" arm covers exactly
    /// `switch_var ∈ [range_lo, range_hi]` (inclusive); `in_range_is_then`
    /// records which arm (`then` or `else`) that is.
    ///
    /// This is GCC/Clang's standard "bias and compare" range-check idiom
    /// (`(unsigned)(x - lo) <= (hi - lo)` for `lo <= x <= hi`, folding the
    /// lower-bound check into the upper-bound one via unsigned wraparound
    /// when `x < lo`) -- confirmed on the dev corpus's own `classify_range`
    /// (`gcc -O0`): its case-1/2/3 grouping compiles to exactly
    /// `uVarN = param_1; uVarN--; if (2 < uVarN) goto default;`, an
    /// **unsigned** temp (Fission's own `uVar` naming already reflects
    /// `NirType::Int { signed: false }`) compared with a plain relational
    /// op. Only accepted for the genuinely unsigned ops (`Gt`/`Ge`/`Lt`/`Le`)
    /// -- the signed variants (`SGt`/etc.) don't have this wraparound
    /// property, so a biased *signed* comparison is left as `NotEq`
    /// instead (recursed into structurally, not range-expanded).
    Range {
        range_lo: i64,
        range_hi: i64,
        in_range_is_then: bool,
    },
}

/// Cap on how many individual case values a single biased-range split is
/// allowed to expand into. A genuine switch-lowering range is small by
/// construction (it came from a handful of adjacent `case` labels); a much
/// larger bound is more likely an unrelated bounds check that only
/// coincidentally matches the bias-subtraction shape, and expanding it
/// would just flood the reconstructed switch with case labels.
const MAX_RANGE_SPLIT_EXPANSION: i64 = 64;

fn classify_direct_split(cond: &HirExpr, switch_var: &HirExpr) -> Option<DirectSplitOnVar> {
    if let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr,
        ..
    } = cond
    {
        if expr.as_ref() == switch_var {
            return Some(DirectSplitOnVar::Eq(0));
        }
        return None;
    }
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    if !matches!(
        op,
        HirBinaryOp::Eq
            | HirBinaryOp::Ne
            | HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
    ) {
        return None;
    }
    // Biased-subtraction unsigned range check: `(switch_var - bias) CMP
    // bound`, const-right canonical form (guaranteed by
    // `canonicalize_presentation_conditions` running every iteration of
    // the same fixed-point loop this pass is part of). Deliberately
    // requires an *explicit* `Sub` -- a bare `switch_var CMP const` must
    // stay on the generic `NotEq` path below, since that one recurses
    // into *both* arms unconditionally, while a range split only commits
    // to expanding the in-range arm when it proves out as a single flat
    // leaf (see `collect_select_tree_leaves`); silently reclassifying an
    // ordinary bounds check this way could turn a working recursive split
    // into a declined one.
    if matches!(
        op,
        HirBinaryOp::Gt | HirBinaryOp::Ge | HirBinaryOp::Lt | HirBinaryOp::Le
    ) && let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: base,
        rhs: bias_expr,
        ..
    } = lhs.as_ref()
        && base.as_ref() == switch_var
        && let (HirExpr::Const(bias, _), HirExpr::Const(bound, _)) =
            (bias_expr.as_ref(), rhs.as_ref())
    {
        let range = match op {
            HirBinaryOp::Gt => bias.checked_add(*bound).map(|hi| (*bias, hi, false)),
            HirBinaryOp::Ge => bias
                .checked_add(*bound)
                .and_then(|hi| hi.checked_sub(1))
                .map(|hi| (*bias, hi, false)),
            HirBinaryOp::Lt => bias
                .checked_add(*bound)
                .and_then(|hi| hi.checked_sub(1))
                .map(|hi| (*bias, hi, true)),
            HirBinaryOp::Le => bias.checked_add(*bound).map(|hi| (*bias, hi, true)),
            _ => unreachable!(),
        };
        if let Some((range_lo, range_hi, in_range_is_then)) = range
            && range_lo <= range_hi
            && range_hi - range_lo < MAX_RANGE_SPLIT_EXPANSION
        {
            return Some(DirectSplitOnVar::Range {
                range_lo,
                range_hi,
                in_range_is_then,
            });
        }
    }
    let const_value = if lhs.as_ref() == switch_var {
        match rhs.as_ref() {
            HirExpr::Const(v, _) => Some(*v),
            _ => None,
        }
    } else if rhs.as_ref() == switch_var {
        match lhs.as_ref() {
            HirExpr::Const(v, _) => Some(*v),
            _ => None,
        }
    } else {
        None
    };
    let value = const_value?;
    if matches!(op, HirBinaryOp::Eq) {
        Some(DirectSplitOnVar::Eq(value))
    } else {
        Some(DirectSplitOnVar::NotEq)
    }
}

/// Recursively decompose a `Select` decision tree into `(case_value,
/// result_expr)` pairs plus every "opaque" leaf reached (candidate default
/// values). Only ever recurses through `classify_direct_split`-recognized
/// nodes; anything else is a leaf, whether or not it happens to itself be
/// a `Select` (an unrelated ternary inside a case body must stay intact).
fn collect_select_tree_leaves<'a>(
    expr: &'a HirExpr,
    switch_var: &HirExpr,
    cases: &mut Vec<(i64, &'a HirExpr)>,
    default_leaves: &mut Vec<&'a HirExpr>,
) {
    let HirExpr::Select {
        cond,
        then_expr,
        else_expr,
        ..
    } = expr
    else {
        default_leaves.push(expr);
        return;
    };
    match classify_direct_split(cond, switch_var) {
        Some(DirectSplitOnVar::Eq(value)) => {
            cases.push((value, then_expr.as_ref()));
            descend_or_treat_as_leaf(else_expr, switch_var, cases, default_leaves);
        }
        Some(DirectSplitOnVar::NotEq) => {
            descend_or_treat_as_leaf(then_expr, switch_var, cases, default_leaves);
            descend_or_treat_as_leaf(else_expr, switch_var, cases, default_leaves);
        }
        Some(DirectSplitOnVar::Range {
            range_lo,
            range_hi,
            in_range_is_then,
        }) => {
            let (in_range_expr, out_of_range_expr) = if in_range_is_then {
                (then_expr.as_ref(), else_expr.as_ref())
            } else {
                (else_expr.as_ref(), then_expr.as_ref())
            };
            // The whole `[range_lo, range_hi]` can only be attributed to
            // one shared expression unambiguously if that side is a
            // single flat leaf with no case/range splits of its own --
            // otherwise which specific values within the range go where
            // isn't something this pass can resolve, so decline the
            // range read entirely (both arms) rather than guess.
            let mut in_range_cases = Vec::new();
            let mut in_range_defaults = Vec::new();
            collect_select_tree_leaves(
                in_range_expr,
                switch_var,
                &mut in_range_cases,
                &mut in_range_defaults,
            );
            if in_range_cases.is_empty() && in_range_defaults.len() == 1 {
                let leaf = in_range_defaults[0];
                for value in range_lo..=range_hi {
                    cases.push((value, leaf));
                }
                descend_or_treat_as_leaf(out_of_range_expr, switch_var, cases, default_leaves);
            } else {
                default_leaves.push(expr);
            }
        }
        None => default_leaves.push(expr),
    }
}

/// Only descend into a range/inequality split's arm if it actually leads
/// to a genuine case boundary (an `==` split, or a `Range` split whose
/// in-range arm resolves to one flat leaf) somewhere inside; otherwise the
/// whole arm is just the *default*'s own internal logic (e.g.
/// `x < 0 ? -1 : 3`, matching a real `default: return value < 0 ? -1 : 3;`
/// clause) and must be kept intact as one leaf, not flattened into
/// separately-tracked (and then spuriously "inconsistent") pieces. An `Eq`
/// split always descends unconditionally (it *is* the case boundary), so
/// this is only used for `NotEq`/`Range` arms.
fn descend_or_treat_as_leaf<'a>(
    expr: &'a HirExpr,
    switch_var: &HirExpr,
    cases: &mut Vec<(i64, &'a HirExpr)>,
    default_leaves: &mut Vec<&'a HirExpr>,
) {
    if subtree_has_real_case_boundary(expr, switch_var) {
        collect_select_tree_leaves(expr, switch_var, cases, default_leaves);
    } else {
        default_leaves.push(expr);
    }
}

/// `true` if `expr` cannot be decomposed any further by
/// `classify_direct_split` -- i.e. `collect_select_tree_leaves` would
/// record it as exactly one default leaf and stop, whether or not it
/// happens to itself be a `Select` (an unrelated leaf ternary counts as
/// flat too, same as any other non-decomposable value).
fn is_flat_leaf(expr: &HirExpr, switch_var: &HirExpr) -> bool {
    let HirExpr::Select { cond, .. } = expr else {
        return true;
    };
    classify_direct_split(cond, switch_var).is_none()
}

/// `true` if `expr` is (or, through a chain of `classify_direct_split`
/// splits on `switch_var`, reaches) a genuine case boundary anywhere --
/// either an `==` split, or a `Range` split usable exactly the way
/// `collect_select_tree_leaves` would use it (its in-range arm resolves to
/// one flat leaf, the same precondition that function itself checks before
/// actually expanding a range).
fn subtree_has_real_case_boundary(expr: &HirExpr, switch_var: &HirExpr) -> bool {
    let HirExpr::Select {
        cond,
        then_expr,
        else_expr,
        ..
    } = expr
    else {
        return false;
    };
    match classify_direct_split(cond, switch_var) {
        Some(DirectSplitOnVar::Eq(_)) => true,
        Some(DirectSplitOnVar::NotEq) => {
            subtree_has_real_case_boundary(then_expr, switch_var)
                || subtree_has_real_case_boundary(else_expr, switch_var)
        }
        Some(DirectSplitOnVar::Range {
            in_range_is_then, ..
        }) => {
            let (in_range_expr, out_of_range_expr) = if in_range_is_then {
                (then_expr.as_ref(), else_expr.as_ref())
            } else {
                (else_expr.as_ref(), then_expr.as_ref())
            };
            is_flat_leaf(in_range_expr, switch_var)
                || subtree_has_real_case_boundary(out_of_range_expr, switch_var)
        }
        None => false,
    }
}

fn try_build_switch_from_select_tree(stmt: &HirStmt) -> Option<HirStmt> {
    let HirStmt::Return(Some(root)) = stmt else {
        return None;
    };
    let HirExpr::Select { cond, .. } = root else {
        return None;
    };
    let switch_var = compared_var(cond)?;
    if !expr_is_presentation_pure(switch_var) {
        return None;
    }
    let mut raw_cases: Vec<(i64, &HirExpr)> = Vec::new();
    let mut default_leaves: Vec<&HirExpr> = Vec::new();
    collect_select_tree_leaves(root, switch_var, &mut raw_cases, &mut default_leaves);

    // Every default leaf must be the exact same expression -- otherwise
    // the tree's true fallthrough behavior is ambiguous and it's not safe
    // to collapse to one `default:` arm.
    let default_expr = match default_leaves.split_first() {
        None => return None,
        Some((first, rest)) => {
            if rest.iter().any(|leaf| *leaf != *first) {
                return None;
            }
            *first
        }
    };

    // Group by identical result expression (mainly for a range split's
    // expansion, where many values legitimately share one body) so the
    // rebuilt switch reads as `case 1: case 2: case 3: return 1;` rather
    // than three separate cases each repeating the same body + `break;`.
    let mut grouped: Vec<(&HirExpr, Vec<i64>)> = Vec::new();
    let mut seen_values: HashSet<i64> = HashSet::default();
    for (value, result) in raw_cases {
        if !seen_values.insert(value) {
            // Same value reached twice with a different tree shape --
            // ambiguous (or the tree isn't the clean partition this
            // recovery assumes); decline rather than guess which wins.
            return None;
        }
        match grouped.iter_mut().find(|(expr, _)| *expr == result) {
            Some((_, values)) => values.push(value),
            None => grouped.push((result, vec![value])),
        }
    }
    if seen_values.len() < MIN_LOWERED_SWITCH_CASES {
        return None;
    }
    let cases: Vec<HirSwitchCase> = grouped
        .into_iter()
        .map(|(result, mut values)| {
            values.sort_unstable();
            HirSwitchCase {
                values,
                body: vec![HirStmt::Return(Some(result.clone()))],
            }
        })
        .collect();
    Some(HirStmt::Switch {
        expr: switch_var.clone(),
        cases,
        default: vec![HirStmt::Return(Some(default_expr.clone()))],
    })
}
