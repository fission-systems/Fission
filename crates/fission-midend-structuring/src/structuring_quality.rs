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
//! - **guard branch terms**: guard *size* is expression nodes, and a
//!   short-circuit `&&` is three of them -- indistinguishable from `a + b`.
//!   But in C `&&` and `||` *are* control flow: each one is a conditional
//!   branch. So a candidate could take a function from 38 jumps to zero while
//!   emitting 2,743 short-circuit operators, and every axis above reported a
//!   clean win. Measured on the 250-function sample-set, short-circuit count
//!   predicts decompiled CFG size at r=0.954 against r=0.233 for statement
//!   count, and the functions carrying them hold 97% of the corpus's total
//!   structural distance from source.
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

use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirStmt};

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
    /// Short-circuit operators (`&&`, `||`) across every guard.
    ///
    /// Each one is a conditional branch, so this is the guard axis measured in
    /// the same unit as [`Self::gotos`] -- the unit the trade is actually
    /// denominated in. [`Self::guard_formula_size`] cannot stand in for it:
    /// one 2,000-node guard may be a single wide arithmetic comparison and
    /// another a 600-term predicate chain.
    pub guard_branch_terms: usize,
    /// Statements in the body, counted through every nested construct.
    ///
    /// The axis that catches a structuring paying for its jumps with text.
    /// Nothing else here sees duplication: a copied exit block adds no guard,
    /// no nesting and no jump, so every other measure reports it as free.
    /// Measured, relaxing what a loop may absorb removed twenty jumps and grew
    /// the corpus output by 23% -- a trade no axis could refuse.
    pub statements: usize,
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
            && self.guard_branch_terms <= branch_term_budget(baseline.guard_branch_terms, removed)
            && self.statements <= statement_budget(baseline.statements, removed)
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
        if self.guard_branch_terms > branch_term_budget(baseline.guard_branch_terms, removed) {
            out.push("guard_branch_terms");
        }
        if self.statements > statement_budget(baseline.statements, removed) {
            out.push("statements");
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
    // This began at four, on the reasoning that one eliminated transfer
    // exposes one predicate term -- an atom, an optional negation, and a
    // connector -- so four nodes leave a little slack without letting
    // independent paths concentrate into one condition.
    //
    // Measured, that reasoning was refusing the largest wins available.
    // Candidates removing 20 to 39 jumps concentrate guards exactly that way,
    // and the corpus rejected 622 jumps' worth of them on the guard axes
    // alone: `bin_009` 39 to 0 and `bin_146` 38 to 0, both reaching none at
    // all. Sweeping the constant: 4 = 928 jumps, 64 = 846, 512 = 821, and
    // 8192 = 821, so 512 is the plateau. Wall clock moved 5m20s to 5m40s
    // across the whole corpus and the worst single function is 7.0s against a
    // 45s harness timeout.
    //
    // The concern the small number encoded -- a formula so large it costs more
    // downstream than the jumps were worth -- is real and was measured at
    // 160,423 nodes and 45 seconds. It is bounded by
    // `reaching_driver::MAX_GUARD_FORMULA_SIZE`, which is where it belongs:
    // an absolute ceiling on what reaches the pipeline, not a per-jump rate.
    const MAX_GUARD_NODES_PER_REMOVED_GOTO: usize = 512;
    baseline_max.saturating_add(removed_gotos.saturating_mul(MAX_GUARD_NODES_PER_REMOVED_GOTO))
}

/// How much text a structuring may add for the jumps it removes.
///
/// Duplication is the one trade the other axes cannot see, and it is a real
/// trade rather than a defect: copying a short exit block into its jump sites
/// is how the jump disappears. So it gets a budget, like nesting, rather than a
/// ban.
fn statement_budget(baseline_statements: usize, removed_gotos: usize) -> usize {
    // A jump replaced by a copied block costs that block. Sixteen statements
    // per jump is well past the six-statement ceiling `cleanup::tail_dup` uses
    // for the same trade, so this refuses only the runaway case.
    const MAX_STATEMENTS_PER_REMOVED_GOTO: usize = 16;
    baseline_statements
        .saturating_add(removed_gotos.saturating_mul(MAX_STATEMENTS_PER_REMOVED_GOTO))
}

/// How many short-circuit branches a structuring may add for the jumps it
/// removes.
///
/// The one budget denominated in the same unit as the thing being optimised.
/// Every other guard budget counts expression nodes, which cannot tell
/// `a && b` from `a + b` -- so a candidate could take a function from 38 jumps
/// to zero by emitting 2,743 short-circuit operators and every axis reported a
/// clean win. It is not one: `&&` is a conditional branch, so that trade
/// removed 38 branches and added 2,743.
///
/// One per removed jump, because a jump is a branch and `&&` is a branch. At
/// that rate the budget states an invariant rather than a tolerance -- a
/// candidate may add at most as many short-circuits as it removed jumps, so
/// **the total number of branches never increases**. It still admits the
/// legitimate fold, since turning `if (a) if (b) X;` into `if (a && b) X;`
/// spends exactly one term for the one transfer it removes, while refusing a
/// reaching-condition formula, which spends hundreds.
///
/// Swept over the 250-function sample-set, counting both kinds of branch:
///
/// ```text
/// per-goto   gotos   short-circuit   total branches
///        0    1,297              85           1,382
///        1    1,123             228           1,351   <- minimum
///        2    1,030             537           1,567
///        4      963           1,347           2,310
///        8      936           1,983           2,919
///       32      729          11,570          12,299
///      512      617          27,866          28,483   <- previous behaviour
/// ```
///
/// The old setting bought 506 fewer jumps for 27,638 more branch terms: 55
/// branches spent per branch removed. Measured on the decompiled CFGs
/// themselves rather than on this proxy -- short-circuit count predicts CFG
/// size at r=0.954 -- the same sweep moves the structural mass the corpus
/// carries:
///
/// ```text
/// per-goto   CFG nodes+edges   mean/median   largest function
///      512            87,073         15.3x        4,721 nodes
///        8            18,895          3.3x          491 nodes
///        2            14,172          2.8x          281 nodes
///        1            13,191          2.7x          281 nodes
/// ```
///
/// Every other decompiler on DecBench sits between 1.8x and 3.4x, so 8 already
/// reaches the band. 1 is chosen over it for the invariant rather than the
/// last 0.6x: it is the only rate that is not a tuned number, and it still
/// carries 30% less structural mass than 8 does.
///
/// A per-guard ceiling was tried instead and is dominated: capping one guard
/// at 16 terms gives 1,040 jumps and 763 terms against this rate at 2 giving
/// 1,030 and 537 -- worse on both axes, because rejecting a candidate for its
/// largest guard discards all of its jump removals at once.
fn branch_term_budget(baseline_terms: usize, removed_gotos: usize) -> usize {
    const MAX_BRANCH_TERMS_PER_REMOVED_GOTO: usize = 1;
    baseline_terms.saturating_add(removed_gotos.saturating_mul(MAX_BRANCH_TERMS_PER_REMOVED_GOTO))
}

fn guard_budget(baseline_size: usize, removed_gotos: usize) -> usize {
    // The 2,008-node envelope this started from described the candidates a
    // weaker driver produced. Region identification and hierarchical reaching
    // conditions produce larger, denser formulas that are worth their size:
    // the ones between 2,000 and 8,000 nodes are the ones taking functions to
    // zero jumps. The envelope now matches
    // `reaching_driver::MAX_GUARD_FORMULA_SIZE`, so a candidate is refused for
    // its guards in one place rather than two, and the relative term still
    // judges an already-large function at its own scale.
    const MEASURED_HEALTHY_GUARD_ENVELOPE: usize = 8_000;
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
        q.statements += 1;
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
    q.guard_branch_terms = q
        .guard_branch_terms
        .saturating_add(expr_branch_terms(guard));
}

/// Short-circuit operators in `e`, each of which is one conditional branch.
pub fn expr_branch_terms(e: &PreHirExpr) -> usize {
    match e {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(..) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_branch_terms(expr),
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            let own = usize::from(matches!(
                op,
                PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::LogicalOr
            ));
            own + expr_branch_terms(lhs) + expr_branch_terms(rhs)
        }
        PreHirExpr::Index { base, index, .. } => expr_branch_terms(base) + expr_branch_terms(index),
        // `?:` branches too, and its arms may carry their own short circuits.
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            1 + expr_branch_terms(cond)
                + expr_branch_terms(then_expr)
                + expr_branch_terms(else_expr)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(expr_branch_terms).sum(),
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
    measure(body).guard_formula_size
}

/// Expression nodes in `e`, counting every operand.
pub fn expr_size(e: &PreHirExpr) -> usize {
    match e {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(..) => 1,
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

    /// Wide in expression nodes, and free of short-circuit branches, so the
    /// size axis can be exercised on its own.
    fn doubled_arith(depth: usize) -> PreHirExpr {
        if depth == 0 {
            return cond();
        }
        let inner = doubled_arith(depth - 1);
        PreHirExpr::Binary {
            op: fission_midend_prehir::PreHirBinaryOp::Add,
            lhs: Box::new(inner.clone()),
            rhs: Box::new(inner),
            ty: fission_midend_core::ir::NirType::Bool,
        }
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
    fn guard_formulas_stay_inside_the_measured_envelope() {
        let baseline = measure(&[if_stmt(vec![goto("a"), goto("b")], vec![])]);
        assert_eq!(baseline.guard_formula_size, 1);

        // 4,095 nodes for one jump was over budget under the 2,008-node
        // envelope, which described a weaker driver's output. Candidates in
        // this range are the ones now reaching zero jumps.
        // Built from arithmetic rather than `&&`, because size and branching
        // are separate axes: a guard this wide is affordable, and the same
        // width spent on short-circuits is not (see
        // `branches_are_budgeted_against_the_jumps_they_replace`).
        let inside_envelope = measure(&[PreHirStmt::If {
            cond: doubled_arith(11), // 4,095 expression nodes, no branches.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert_eq!(inside_envelope.guard_branch_terms, 0);
        assert!(inside_envelope.improves_on(&baseline));

        let over_budget = measure(&[PreHirStmt::If {
            cond: doubled_arith(14), // 32,767 nodes for the same one-jump saving.
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
    fn one_guard_may_absorb_terms_but_not_without_limit() {
        let baseline = measure(&[if_stmt(vec![goto("a"), goto("b")], vec![])]);

        // Concentrating several terms into one guard is what taking a function
        // to zero jumps looks like, so it has to be affordable.
        let concentrated = measure(&[PreHirStmt::If {
            cond: doubled_arith(3), // Fifteen nodes for a one-jump saving.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(concentrated.improves_on(&baseline));
        assert!(concentrated.has_proportional_max_guard_growth(&baseline));

        // A formula large enough to cost more downstream than the jump was
        // worth is still refused -- the measured failure was 160,423 nodes and
        // 45 seconds.
        let runaway = measure(&[PreHirStmt::If {
            cond: doubled_arith(14), // 32,767 nodes for the same one-jump saving.
            then_body: Rc::new(vec![goto("a")]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(!runaway.has_proportional_max_guard_growth(&baseline));
    }

    #[test]
    fn branches_are_budgeted_against_the_jumps_they_replace() {
        // Three jumps in, and the candidate spends one short-circuit to remove
        // one of them -- what folding `if (a) if (b) X;` into `if (a && b) X;`
        // costs.
        let baseline = measure(&[goto("a"), goto("b"), goto("c")]);
        let folded = measure(&[
            PreHirStmt::If {
                cond: doubled_cond(1), // One `&&`.
                then_body: Rc::new(vec![goto("a")]),
                else_body: Rc::new(Vec::new()),
            },
            goto("b"),
        ]);
        assert_eq!(folded.guard_branch_terms, 1);
        assert!(folded.improves_on(&baseline));

        // The reaching-condition shape: the jumps do disappear, and every
        // other axis reports a win, but the branches they were made of come
        // back as predicate terms and there are more of them than there were
        // jumps. Measured at its extreme on the corpus, 38 jumps removed for
        // 2,743 short-circuit operators.
        let path_conditions = measure(&[PreHirStmt::If {
            cond: doubled_cond(3), // Seven `&&` for three jumps.
            then_body: Rc::new(vec![PreHirStmt::Return(None)]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert_eq!(path_conditions.gotos, 0);
        assert!(path_conditions.guard_formula_size <= 8_000);
        assert!(!path_conditions.improves_on(&baseline));
        assert!(
            path_conditions
                .regressions_against(&baseline)
                .contains(&"guard_branch_terms")
        );
    }

    #[test]
    fn an_unconditioned_baseline_uses_the_measured_healthy_envelope() {
        let baseline = measure(&[goto("a"), goto("b")]);
        let candidate = measure(&[if_stmt(vec![goto("a")], vec![])]);
        assert_eq!(baseline.guard_formula_size, 0);
        assert!(candidate.improves_on(&baseline));
    }
}
