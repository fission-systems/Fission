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
//!
//! A candidate is taken only when it strictly wins on gotos and gives up
//! nothing on switches or empty shells. Nesting is the exception: it is the
//! *price* of the trade rather than a defect, so it gets a budget of one level
//! per jump removed. That makes "run a driver" safe by construction rather
//! than by how a particular corpus happens to score, which is what lets the
//! drivers run at all.

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
}

impl StructuringQuality {
    /// Whether `self` is worth taking over `baseline`.
    ///
    /// Strictly fewer jumps, no lost switches, no new empty shells -- and
    /// nesting within budget.
    ///
    /// Nesting gets a budget rather than a ban because **it is the price, not
    /// a defect**: replacing a jump with a condition is what puts the code one
    /// level deeper, so forbidding any increase forbids the trade entirely.
    /// Banning it was tried first and rejected a candidate that took a
    /// function from ten jumps to none. One level per jump removed is the
    /// rule; the drivers additionally cap depth absolutely, so a runaway
    /// result cannot arrive here no matter how many jumps it bought.
    ///
    /// Ties on gotos lose: a rewrite that does not reduce them has no reason
    /// to displace what is already there.
    pub fn improves_on(&self, baseline: &Self) -> bool {
        let removed = baseline.gotos.saturating_sub(self.gotos);
        self.gotos < baseline.gotos
            && self.switches >= baseline.switches
            && self.empty_if_shells <= baseline.empty_if_shells
            && self.nesting_depth <= baseline.nesting_depth + removed
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
        out
    }
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
                ..
            } => {
                if then_body.is_empty() && else_body.is_empty() {
                    q.empty_if_shells += 1;
                }
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(then_body, inner, q);
                walk(else_body, inner, q);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                q.switches += 1;
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                for case in cases {
                    walk(&case.body, inner, q);
                }
                walk(default, inner, q);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(body, inner, q);
            }
            PreHirStmt::For { body, .. } => {
                let inner = depth + 1;
                q.nesting_depth = q.nesting_depth.max(inner);
                walk(body, inner, q);
            }
            PreHirStmt::Block(inner) => walk(inner, depth, q),
            _ => {}
        }
    }
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
    let mut total = 0;
    guards(body, &mut total);
    total
}

fn guards(body: &[PreHirStmt], total: &mut usize) {
    for stmt in body {
        match stmt {
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                *total += expr_size(cond);
                guards(then_body, total);
                guards(else_body, total);
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                *total += expr_size(cond);
                guards(body, total);
            }
            PreHirStmt::For { cond, body, .. } => {
                if let Some(cond) = cond {
                    *total += expr_size(cond);
                }
                guards(body, total);
            }
            PreHirStmt::Switch { expr, cases, default } => {
                *total += expr_size(expr);
                for case in cases {
                    guards(&case.body, total);
                }
                guards(default, total);
            }
            PreHirStmt::Block(inner) => guards(inner, total),
            _ => {}
        }
    }
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
        assert_eq!(q.nesting_depth, 1, "the block itself is not a nesting level");
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
}
