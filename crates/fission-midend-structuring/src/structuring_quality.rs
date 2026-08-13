//! Comparing two structurings of the same function.
//!
//! angr keeps a transform only when the goto count strictly drops
//! (`strictly_less_gotos`). That is the right idea and the wrong measurement
//! for Fission, because **goto density cannot see the ways a structuring gets
//! worse.** Every axis below is here because a driver in this work scored an
//! improvement on gotos while emitting something worse, and the corpus
//! reported no regression:
//!
//! - **switches**: a switch dispatch is a cascade of two-way branches, so the
//!   DREAM driver describes it perfectly as nested `if`s -- and loses the
//!   `switch` the existing path recovered. Three tests caught it; the corpus
//!   called it a win.
//! - **empty `if` shells**: the match-fold driver emits `if (c) { }` where the
//!   existing path folds a short-circuit `&&`. Caught by the HIR presentation
//!   invariants, not by any goto count.
//! - **nesting depth**: trading jumps for conditions nests by construction. A
//!   9-goto function came back 9 levels deep, and a 51-node region cost 45
//!   seconds downstream. Past some depth the jump was the better answer.
//! - **guard formula size**: materializing a previously hidden transfer made
//!   four candidates replace a few jumps with thousands of repeated boolean
//!   nodes. Formula growth must stay within the measured healthy envelope or
//!   remain proportional to the transfer reduction.
//! The largest single guard is also measured for callers that open a new,
//! broader candidate funnel. Total formula size can look affordable while one
//! `if` carries most of the reaching-condition forest; the linear-fallback and
//! hierarchical funnels use that measurement as an additional admission
//! constraint.
//!
//! A candidate is taken only when it strictly wins on gotos and gives up
//! nothing on switches or empty shells. Nesting and guard formulas are the
//! price of the trade rather than defects, so each gets a budget tied to the
//! number of jumps removed. That makes "run a driver" safe by construction
//! rather than by how a particular corpus happens to score, which is what lets
//! the drivers run at all.

use fission_midend_prehir::{PreHirExpr, PreHirStmt};

/// What a structuring is worth, on every axis measurement has shown to matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StructuringQuality {
    /// Explicit jumps. Lower is better; this is the axis being optimised.
    pub gotos: usize,
    /// Recovered `switch` statements. Higher is better.
    pub switches: usize,
    /// `if` statements with neither arm populated. Lower is better.
    pub empty_if_shells: usize,
    /// Deepest nesting of conditionals and loops. Lower is better.
    pub nesting_depth: usize,
    /// Total expression nodes carried by conditional and loop guards.
    pub guard_formula_size: usize,
    /// Largest single guard expression, in expression nodes.
    pub max_guard_formula_size: usize,
}

impl StructuringQuality {
    /// Whether `self` is worth taking over `baseline`.
    ///
    /// Strictly fewer jumps, no lost switches, no new empty shells -- and
    /// nesting and guard-formula growth within budget.
    ///
    /// Nesting gets a budget rather than a ban because **it is the price, not
    /// a defect**: replacing a jump with a condition is what puts the code one
    /// level deeper, so forbidding any increase forbids the trade entirely.
    /// Banning it was tried first and rejected a candidate that took a
    /// function from ten jumps to none. One level per jump removed is the
    /// rule; the drivers additionally cap depth absolutely, so a runaway
    /// result cannot arrive here no matter how many jumps it bought.
    ///
    /// Guard formulas may stay inside the measured healthy envelope regardless
    /// of ratio. Beyond it, they receive one copy of the baseline formula
    /// forest per removed jump. A driver may therefore trade transfers for
    /// conditions, but large formulas may not grow faster than the number of
    /// transfers they eliminated.
    ///
    /// Ties on gotos lose: a rewrite that does not reduce them has no reason to
    /// displace what is already there.
    pub fn improves_on(&self, baseline: &Self) -> bool {
        let removed = baseline.gotos.saturating_sub(self.gotos);
        let guard_budget = guard_budget(baseline.guard_formula_size, removed);
        self.gotos < baseline.gotos
            && self.switches >= baseline.switches
            && self.empty_if_shells <= baseline.empty_if_shells
            && self.nesting_depth <= baseline.nesting_depth + removed
            && self.guard_formula_size <= guard_budget
    }

    /// The axes on which `self` is worse than `baseline`, for diagnostics.
    pub fn regressions_against(&self, baseline: &Self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.switches < baseline.switches {
            out.push("switches");
        }
        if self.empty_if_shells > baseline.empty_if_shells {
            out.push("empty_if_shells");
        }
        let removed = baseline.gotos.saturating_sub(self.gotos);
        if self.nesting_depth > baseline.nesting_depth + removed {
            out.push("nesting_depth");
        }
        let guard_budget = guard_budget(baseline.guard_formula_size, removed);
        if self.guard_formula_size > guard_budget {
            out.push("guard_formula_size");
        }
        out
    }

    /// Whether growth of the largest single guard stays proportional to the
    /// transfers removed by this candidate.
    ///
    /// This is deliberately separate from [`Self::improves_on`]. Established
    /// candidates predate this measurement and have corpus-validated winners
    /// outside the narrow bound. Callers opening a broader funnel opt in
    /// without retroactively rejecting those candidates.
    pub fn has_proportional_max_guard_growth(&self, baseline: &Self) -> bool {
        let removed = baseline.gotos.saturating_sub(self.gotos);
        self.max_guard_formula_size <= max_guard_budget(baseline.max_guard_formula_size, removed)
    }
}

fn max_guard_budget(baseline_max: usize, removed_gotos: usize) -> usize {
    // One eliminated transfer exposes one predicate term. A materialised term
    // needs a boolean atom, an optional negation, and a binary connector:
    // three expression nodes. Four leaves one node of slack without allowing
    // independent paths to be concentrated into a single condition.
    const MAX_GUARD_NODES_PER_REMOVED_GOTO: usize = 4;
    baseline_max.saturating_add(removed_gotos.saturating_mul(MAX_GUARD_NODES_PER_REMOVED_GOTO))
}

fn guard_budget(baseline_size: usize, removed_gotos: usize) -> usize {
    // Full-corpus measurement before this comparator found every healthy
    // reaching-condition candidate at or below 2,008 nodes. Newly unlocked
    // candidates at 2,755..4,974 nodes visibly duplicated guard trees, while
    // the populations did not overlap. Keep the measured envelope, then make
    // the budget relative so already-large functions are judged at their own
    // scale instead of by a universal cap.
    const MEASURED_HEALTHY_GUARD_ENVELOPE: usize = 2_008;
    MEASURED_HEALTHY_GUARD_ENVELOPE
        .max(baseline_size.saturating_mul(removed_gotos.saturating_add(1)))
}

/// Measure a structured body.
pub fn measure(body: &[PreHirStmt]) -> StructuringQuality {
    let mut q = StructuringQuality::default();
    walk(body, 0, &mut q);
    q
}

fn walk(body: &[PreHirStmt], depth: usize, q: &mut StructuringQuality) {
    for stmt in body {
        match stmt {
            PreHirStmt::Goto(_) => q.gotos += 1,
            PreHirStmt::If {
                then_body,
                else_body,
                cond,
            } => {
                add_guard(q, cond);
                if then_body.is_empty() && else_body.is_empty() {
                    q.empty_if_shells += 1;
                }
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(then_body, inner, q);
                walk(else_body, inner, q);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                add_guard(q, expr);
                q.switches += 1;
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                for case in cases {
                    walk(&case.body, inner, q);
                }
                walk(default, inner, q);
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                add_guard(q, cond);
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(body, inner, q);
            }
            PreHirStmt::For { cond, body, .. } => {
                if let Some(cond) = cond {
                    add_guard(q, cond);
                }
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(body, inner, q);
            }
            PreHirStmt::Block(inner) => walk(inner, depth, q),
            _ => {}
        }
    }
}

fn add_guard(q: &mut StructuringQuality, guard: &PreHirExpr) {
    let size = expr_size(guard);
    q.guard_formula_size = q.guard_formula_size.saturating_add(size);
    q.max_guard_formula_size = q.max_guard_formula_size.max(size);
}

/// Total size of every guard in a body, counted in expression nodes.
///
/// The honest measure of what condition-based structuring costs. Depth and
/// decision count are both proxies for it: a node's reaching condition is a
/// formula over the decisions on the paths that reach it, and it is the
/// *formulas* that the rest of the pipeline has to carry, simplify, and type.
/// A region that produced 45 seconds of downstream work did so by emitting
/// guards, not by being tall.
///
/// Counting them directly means a bound can be placed on the thing that
/// actually costs, rather than on a stand-in that excludes large functions
/// whether or not their guards are big.
pub fn guard_formula_size(body: &[PreHirStmt]) -> usize {
    measure(body).guard_formula_size
}

/// Expression nodes in `e`, counting every operand.
pub fn expr_size(e: &PreHirExpr) -> usize {
    match e {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => 1,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => 1 + expr_size(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => 1 + expr_size(lhs) + expr_size(rhs),
        PreHirExpr::Index { base, index, .. } => 1 + expr_size(base) + expr_size(index),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => 1 + expr_size(cond) + expr_size(then_expr) + expr_size(else_expr),
        PreHirExpr::Call { args, .. } => 1 + args.iter().map(expr_size).sum::<usize>(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::PreHirSwitchCase;
    use std::rc::Rc;

    fn goto(n: &str) -> PreHirStmt {
        PreHirStmt::Goto(n.to_string())
    }

    fn cond() -> PreHirExpr {
        PreHirExpr::Var("c".to_string())
    }

    fn if_stmt(then_body: Vec<PreHirStmt>, else_body: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: cond(),
            then_body: Rc::new(then_body),
            else_body: Rc::new(else_body),
        }
    }

    fn switch_stmt(body: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::Switch {
            expr: cond(),
            cases: vec![PreHirSwitchCase {
                values: vec![1],
                body: Rc::new(body),
            }],
            default: Rc::new(Vec::new()),
        }
    }

    #[test]
    fn fewer_gotos_alone_is_enough_when_nothing_else_moves() {
        let baseline = measure(&[goto("a"), goto("b")]);
        let candidate = measure(&[goto("a")]);
        assert!(candidate.improves_on(&baseline));
    }

    #[test]
    fn a_tie_on_gotos_does_not_displace_the_existing_structuring() {
        let baseline = measure(&[goto("a")]);
        let candidate = measure(&[goto("b")]);
        assert!(!candidate.improves_on(&baseline));
    }

    #[test]
    fn losing_a_switch_is_not_paid_for_by_removing_gotos() {
        // Exactly the DREAM driver's switch failure: the dispatch becomes
        // nested ifs, the jumps vanish, and the result is worse.
        let baseline = measure(&[switch_stmt(vec![goto("a")])]);
        let candidate = measure(&[if_stmt(vec![if_stmt(vec![], vec![])], vec![])]);
        assert!(candidate.gotos < baseline.gotos, "the jump did go away");
        assert!(!candidate.improves_on(&baseline));
        assert!(
            candidate
                .regressions_against(&baseline)
                .contains(&"switches")
        );
    }

    #[test]
    fn an_empty_if_shell_is_not_paid_for_by_removing_gotos() {
        // The match-fold driver's short-circuit failure.
        let baseline = measure(&[goto("a"), goto("b")]);
        let candidate = measure(&[if_stmt(Vec::new(), Vec::new())]);
        assert_eq!(candidate.empty_if_shells, 1);
        assert!(!candidate.improves_on(&baseline));
    }

    #[test]
    fn nesting_may_grow_by_one_level_per_jump_removed() {
        // Two jumps become one, buying one level: depth 1 is affordable.
        let baseline = measure(&[goto("a"), goto("b")]);
        let affordable = measure(&[if_stmt(vec![goto("a")], vec![])]);
        assert_eq!(affordable.nesting_depth, 1);
        assert!(affordable.improves_on(&baseline));

        // The same one-jump saving does not buy two levels.
        let overdrawn = measure(&[if_stmt(vec![if_stmt(vec![goto("a")], vec![])], vec![])]);
        assert_eq!(overdrawn.nesting_depth, 2);
        assert!(!overdrawn.improves_on(&baseline));
        assert!(
            overdrawn
                .regressions_against(&baseline)
                .contains(&"nesting_depth")
        );
    }

    #[test]
    fn trading_every_jump_away_affords_real_depth() {
        // The measured case the ban rejected: ten jumps to none. Depth is
        // still capped absolutely by the drivers, but the comparator must not
        // be what refuses it.
        let baseline = measure(&(0..10).map(|i| goto(&format!("l{i}"))).collect::<Vec<_>>());
        let mut deep = vec![goto("x")];
        for _ in 0..6 {
            deep = vec![if_stmt(deep, Vec::new())];
        }
        let candidate = measure(&deep);
        assert_eq!(candidate.nesting_depth, 6);
        assert_eq!(candidate.gotos, 1);
        assert!(candidate.improves_on(&baseline));
    }

    #[test]
    fn nesting_is_measured_through_every_construct() {
        let body = vec![PreHirStmt::While {
            cond: cond(),
            body: Rc::new(vec![switch_stmt(vec![if_stmt(vec![goto("x")], vec![])])]),
        }];
        let q = measure(&body);
        assert_eq!(q.nesting_depth, 3, "while > switch > if");
        assert_eq!(q.switches, 1);
        assert_eq!(q.gotos, 1);
    }

    #[test]
    fn a_block_does_not_count_as_a_level() {
        let q = measure(&[PreHirStmt::Block(Rc::new(vec![if_stmt(
            vec![goto("x")],
            vec![],
        )]))]);
        assert_eq!(
            q.nesting_depth, 1,
            "the block itself is not a nesting level"
        );
    }

    #[test]
    fn guard_size_counts_every_operand_of_every_guard() {
        let big = PreHirExpr::Binary {
            op: fission_midend_prehir::PreHirBinaryOp::LogicalAnd,
            lhs: Box::new(cond()),
            rhs: Box::new(cond()),
            ty: fission_midend_core::ir::NirType::Bool,
        };
        assert_eq!(expr_size(&big), 3, "the and plus both operands");

        let body = vec![PreHirStmt::If {
            cond: big.clone(),
            then_body: Rc::new(vec![PreHirStmt::If {
                cond: big,
                then_body: Rc::new(Vec::new()),
                else_body: Rc::new(Vec::new()),
            }]),
            else_body: Rc::new(Vec::new()),
        }];
        assert_eq!(guard_formula_size(&body), 6, "nested guards both count");
    }

    #[test]
    fn a_body_without_guards_costs_nothing() {
        assert_eq!(guard_formula_size(&[goto("a"), goto("b")]), 0);
    }

    #[test]
    fn guard_size_sees_depth_and_breadth_the_same_way() {
        // Two shallow guards and two nested ones cost the same, which is the
        // point: depth was only ever a proxy for the formulas being carried.
        let flat = vec![if_stmt(vec![], vec![]), if_stmt(vec![], vec![])];
        let nested = vec![if_stmt(vec![if_stmt(vec![], vec![])], vec![])];
        assert_eq!(guard_formula_size(&flat), guard_formula_size(&nested));
    }

    fn doubled_cond(depth: usize) -> PreHirExpr {
        if depth == 0 {
            return cond();
        }
        let inner = doubled_cond(depth - 1);
        PreHirExpr::Binary {
            op: fission_midend_prehir::PreHirBinaryOp::LogicalAnd,
            lhs: Box::new(inner.clone()),
            rhs: Box::new(inner),
            ty: fission_midend_core::ir::NirType::Bool,
        }
    }

    #[test]
    fn guard_formulas_stay_inside_the_healthy_or_proportional_budget() {
        let baseline = measure(&[if_stmt(vec![goto("a"), goto("b")], vec![])]);
        assert_eq!(baseline.guard_formula_size, 1);

        let inside_envelope = measure(&[PreHirStmt::If {
            cond: doubled_cond(9), // 1,023 expression nodes.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(inside_envelope.improves_on(&baseline));

        let over_budget = measure(&[PreHirStmt::If {
            cond: doubled_cond(11), // 4,095 nodes for the same one-jump saving.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(!over_budget.improves_on(&baseline));
        assert!(
            over_budget
                .regressions_against(&baseline)
                .contains(&"guard_formula_size")
        );
    }

    #[test]
    fn one_guard_cannot_absorb_more_than_one_term_per_removed_jump() {
        let baseline = measure(&[if_stmt(vec![goto("a"), goto("b")], vec![])]);

        let affordable = measure(&[PreHirStmt::If {
            cond: doubled_cond(1), // Three nodes: one boolean term.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(affordable.improves_on(&baseline));
        assert!(affordable.has_proportional_max_guard_growth(&baseline));

        let concentrated = measure(&[PreHirStmt::If {
            cond: doubled_cond(3), // Fifteen nodes for the same one-jump saving.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(concentrated.improves_on(&baseline));
        assert!(!concentrated.has_proportional_max_guard_growth(&baseline));
    }

    #[test]
    fn an_unconditioned_baseline_uses_the_measured_healthy_envelope() {
        let baseline = measure(&[goto("a"), goto("b")]);
        let candidate = measure(&[if_stmt(vec![goto("a")], vec![])]);
        assert_eq!(baseline.guard_formula_size, 0);
        assert!(candidate.improves_on(&baseline));
    }
}
