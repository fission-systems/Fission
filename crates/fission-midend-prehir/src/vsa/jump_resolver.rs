/// Jump table resolution using VSA range information.
///
/// After solving ranges for all variables in a function, this module
/// attempts to refine `PreHirStmt::Switch` expressions by:
///
/// 1. Detecting switch index variables whose range is `[0, N)`.
/// 2. Propagating the inferred case count down to switches that were
///    previously emitted with a synthetic contiguous range.
/// 3. Dead-branch elimination: if the switch discriminant is a singleton,
///    replace the entire switch with the matching case body.
///
/// References:
/// - Ghidra `jumptable.hh`: `JumpModel`, `PathMeld`
/// - Ghidra `rangeutil.hh`: `CircleRange`, `ValueSetSolver`
use super::solver::solve;
use super::transfer::{RangeEnv, eval_expr};
use crate::ir::{PreHirFunction, PreHirStmt, PreHirSwitchCase};
use fission_midend_core::wave_stats::{
    add_dispatcher_shape_recoveries, add_indirect_target_set_refinements,
};
use std::collections::HashSet;

/// Apply VSA-based switch refinement to a function's body.
///
/// Returns `true` if any changes were made.
pub fn apply_jump_resolver_pass(func: &mut PreHirFunction) -> bool {
    if jump_resolver_candidate_count(&func.body) == 0 {
        return false;
    }
    let mut targets = HashSet::new();
    collect_goto_targets(&func.body, &mut targets);
    let env = solve(func);
    refine_stmts(&mut func.body, &env, &targets)
}

pub fn jump_resolver_candidate_count(stmts: &[PreHirStmt]) -> usize {
    fn count_opt_stmt(stmt: &Option<Box<PreHirStmt>>) -> usize {
        stmt.as_deref().map_or(0, count_stmt)
    }

    fn count_stmt(stmt: &PreHirStmt) -> usize {
        match stmt {
            PreHirStmt::Switch { cases, default, .. } => {
                let local_cost = 1 + cases.len().min(8) + usize::from(!default.is_empty());
                local_cost
                    + cases
                        .iter()
                        .map(|case| jump_resolver_candidate_count(&case.body))
                        .sum::<usize>()
                    + jump_resolver_candidate_count(default)
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                jump_resolver_candidate_count(then_body) + jump_resolver_candidate_count(else_body)
            }
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::Block(body) => jump_resolver_candidate_count(body),
            PreHirStmt::For {
                init, body, update, ..
            } => {
                count_opt_stmt(init) + jump_resolver_candidate_count(body) + count_opt_stmt(update)
            }
            _ => 0,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

fn collect_goto_targets(stmts: &[PreHirStmt], targets: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Goto(target) => {
                targets.insert(target.clone());
            }
            PreHirStmt::Block(body) => collect_goto_targets(body, targets),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_goto_targets(then_body, targets);
                collect_goto_targets(else_body, targets);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_goto_targets(body, targets);
            }
            PreHirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    collect_goto_targets(std::slice::from_ref(s.as_ref()), targets);
                }
                collect_goto_targets(body, targets);
                if let Some(s) = update {
                    collect_goto_targets(std::slice::from_ref(s.as_ref()), targets);
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    collect_goto_targets(&c.body, targets);
                }
                collect_goto_targets(default, targets);
            }
            _ => {}
        }
    }
}

fn contains_referenced_label(stmts: &[PreHirStmt], targets: &HashSet<String>) -> bool {
    if targets.is_empty() {
        return false;
    }
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(name) if targets.contains(name) => return true,
            PreHirStmt::Block(body) => {
                if contains_referenced_label(body, targets) {
                    return true;
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if contains_referenced_label(then_body, targets)
                    || contains_referenced_label(else_body, targets)
                {
                    return true;
                }
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                if contains_referenced_label(body, targets) {
                    return true;
                }
            }
            PreHirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    if contains_referenced_label(std::slice::from_ref(s.as_ref()), targets) {
                        return true;
                    }
                }
                if contains_referenced_label(body, targets) {
                    return true;
                }
                if let Some(s) = update {
                    if contains_referenced_label(std::slice::from_ref(s.as_ref()), targets) {
                        return true;
                    }
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    if contains_referenced_label(&c.body, targets) {
                        return true;
                    }
                }
                if contains_referenced_label(default, targets) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn contains_any_label(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        PreHirStmt::Label(_) => true,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => contains_any_label(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => contains_any_label(then_body) || contains_any_label(else_body),
        PreHirStmt::For {
            init, body, update, ..
        } => {
            init.as_deref()
                .is_some_and(|s| contains_any_label(std::slice::from_ref(s)))
                || contains_any_label(body)
                || update
                    .as_deref()
                    .is_some_and(|s| contains_any_label(std::slice::from_ref(s)))
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| contains_any_label(&c.body)) || contains_any_label(default)
        }
        _ => false,
    })
}

fn contains_switch_owned_break(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        PreHirStmt::Break => true,
        PreHirStmt::Block(inner) => contains_switch_owned_break(inner),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => contains_switch_owned_break(then_body) || contains_switch_owned_break(else_body),
        // Loops and inner switches own their own `Break` statements.
        PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Switch { .. } => false,
        _ => false,
    })
}

fn arm_always_exits_switch(stmts: &[PreHirStmt]) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    match last {
        PreHirStmt::Break | PreHirStmt::Return(_) | PreHirStmt::Goto(_) => true,
        PreHirStmt::Block(inner) => arm_always_exits_switch(inner),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => arm_always_exits_switch(then_body) && arm_always_exits_switch(else_body),
        _ => false,
    }
}

fn strip_tail_switch_breaks(stmts: &mut Vec<PreHirStmt>) {
    while let Some(last) = stmts.last() {
        match last {
            PreHirStmt::Break => {
                stmts.pop();
            }
            PreHirStmt::Block(inner) => {
                let mut inner_vec = std::rc::Rc::unwrap_or_clone(inner.clone());
                strip_switch_breaks(&mut inner_vec);
                if inner_vec.is_empty() {
                    stmts.pop();
                } else {
                    *stmts.last_mut().unwrap() = PreHirStmt::Block(std::rc::Rc::new(inner_vec));
                    break;
                }
            }
            PreHirStmt::If { .. } => {
                if let Some(PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                }) = stmts.last_mut()
                {
                    strip_switch_breaks(std::rc::Rc::make_mut(then_body));
                    strip_switch_breaks(std::rc::Rc::make_mut(else_body));
                }
                break;
            }
            _ => break,
        }
    }
}

fn fold_early_break_ifs(stmts: &mut Vec<PreHirStmt>) {
    for idx in (0..stmts.len()).rev() {
        if idx + 1 >= stmts.len() {
            continue;
        }
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &stmts[idx]
        else {
            continue;
        };
        let tail = &stmts[idx + 1..];
        if contains_any_label(tail) || contains_switch_owned_break(tail) {
            continue;
        }
        let then_exits = arm_always_exits_switch(then_body);
        let else_exits = arm_always_exits_switch(else_body);
        let then_has_break = contains_switch_owned_break(then_body);
        let else_has_break = contains_switch_owned_break(else_body);

        if then_exits && else_exits && (then_has_break || else_has_break) {
            let mut stripped_then = (**then_body).clone();
            let mut stripped_else = (**else_body).clone();
            strip_switch_breaks(&mut stripped_then);
            strip_switch_breaks(&mut stripped_else);
            if !contains_switch_owned_break(&stripped_then)
                && !contains_switch_owned_break(&stripped_else)
            {
                stmts.truncate(idx + 1);
                if let PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } = &mut stmts[idx]
                {
                    *std::rc::Rc::make_mut(then_body) = stripped_then;
                    *std::rc::Rc::make_mut(else_body) = stripped_else;
                }
            }
        } else if then_exits && then_has_break && !else_has_break {
            let mut stripped_then = (**then_body).clone();
            strip_switch_breaks(&mut stripped_then);
            if !contains_switch_owned_break(&stripped_then) {
                let tail_stmts = stmts.split_off(idx + 1);
                if let PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } = &mut stmts[idx]
                {
                    *std::rc::Rc::make_mut(then_body) = stripped_then;
                    std::rc::Rc::make_mut(else_body).extend(tail_stmts);
                }
            }
        } else if else_exits && else_has_break && !then_has_break {
            let mut stripped_else = (**else_body).clone();
            strip_switch_breaks(&mut stripped_else);
            if !contains_switch_owned_break(&stripped_else) {
                let tail_stmts = stmts.split_off(idx + 1);
                if let PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } = &mut stmts[idx]
                {
                    std::rc::Rc::make_mut(then_body).extend(tail_stmts);
                    *std::rc::Rc::make_mut(else_body) = stripped_else;
                }
            }
        }
    }
}

fn strip_switch_breaks(stmts: &mut Vec<PreHirStmt>) {
    strip_tail_switch_breaks(stmts);
    fold_early_break_ifs(stmts);
}

fn refine_stmts(stmts: &mut Vec<PreHirStmt>, env: &RangeEnv, targets: &HashSet<String>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if refine_stmt(&mut stmts[i], env, targets) {
            changed = true;
        }
        // If the stmt was reduced to a singleton-constant switch,
        // inline the matching case in place.
        if let PreHirStmt::Switch {
            expr,
            cases,
            default,
        } = &stmts[i]
            && !crate::util::expr_has_side_effecting_call(expr)
        {
            let range = eval_expr(expr, env);
            if let Some(v) = range.singleton_value() {
                // `v` is the discriminant's bits, zero-extended; a case value
                // is a signed integer. `-1` and `0xFFFFFFFF` are the same
                // 32-bit value, so compare at the discriminant's width.
                let bits = range.bits();
                let matches = |c: &PreHirSwitchCase| {
                    c.values
                        .iter()
                        .any(|value| case_value_matches(*value, v, bits))
                };
                let matching_case = cases.iter().find(|c| matches(c));
                let other_branches_have_label = cases
                    .iter()
                    .filter(|c| !matches(c))
                    .any(|c| contains_referenced_label(&c.body, targets))
                    || (matching_case.is_some() && contains_referenced_label(default, targets));
                if !other_branches_have_label {
                    // Find the matching case.
                    let replacement = matching_case
                        .map(|c| c.body.clone())
                        .unwrap_or_else(|| default.clone());
                    let mut replacement_stmts = std::rc::Rc::unwrap_or_clone(replacement);
                    strip_switch_breaks(&mut replacement_stmts);
                    if !contains_switch_owned_break(&replacement_stmts) {
                        stmts.splice(i..=i, replacement_stmts);
                        add_dispatcher_shape_recoveries(1);
                        changed = true;
                        // Don't advance i — the replacement stmts need processing.
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    changed
}

/// Does the switch case value `case` denote the `bits`-wide value `value`?
fn case_value_matches(case: i64, value: u64, bits: u32) -> bool {
    let mask = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    (case as u64) & mask == value & mask
}

fn refine_stmt(stmt: &mut PreHirStmt, env: &RangeEnv, targets: &HashSet<String>) -> bool {
    match stmt {
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            let mut changed = false;

            // Recurse into case bodies.
            for case in cases.iter_mut() {
                if refine_stmts(std::rc::Rc::make_mut(&mut case.body), env, targets) {
                    changed = true;
                }
            }
            if refine_stmts(std::rc::Rc::make_mut(default), env, targets) {
                changed = true;
            }

            // Attempt to narrow the switch to known live cases.
            // If expr's range is [lo, hi) we can prune cases outside that range.
            let range = eval_expr(expr, env);
            if !range.is_top() && !range.is_bottom() {
                let before = cases.len();
                cases.retain(|c| {
                    if contains_referenced_label(&c.body, targets) {
                        return true;
                    }
                    c.values.iter().any(|&v| {
                        let v_u = v as u64;
                        // Check if v_u is in range [lo, hi).
                        let lo = range.lo();
                        let hi = range.hi();
                        let mask = if range.bits() >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << range.bits()) - 1
                        };
                        if lo <= hi {
                            lo <= (v_u & mask) && (v_u & mask) < hi
                        } else {
                            // Wrapped arc.
                            (v_u & mask) >= lo || (v_u & mask) < hi
                        }
                    })
                });
                if cases.len() != before {
                    add_indirect_target_set_refinements(1);
                    add_dispatcher_shape_recoveries(1);
                    changed = true;
                }
            }
            changed
        }
        PreHirStmt::If {
            then_body,
            else_body,
            cond,
        } => {
            let mut changed = refine_stmts(std::rc::Rc::make_mut(then_body), env, targets);
            if refine_stmts(std::rc::Rc::make_mut(else_body), env, targets) {
                changed = true;
            }

            // Constant condition elimination.
            if !crate::util::expr_has_side_effecting_call(cond) {
                let range = eval_expr(cond, env);
                if let Some(v) = range.singleton_value() {
                    let (active, dead) = if v != 0 {
                        (then_body, else_body)
                    } else {
                        (else_body, then_body)
                    };
                    if !contains_referenced_label(dead, targets) {
                        let replacement =
                            std::rc::Rc::make_mut(active).drain(..).collect::<Vec<_>>();
                        *stmt = PreHirStmt::Block(std::rc::Rc::new(replacement));
                        return true;
                    }
                }
            }
            changed
        }
        PreHirStmt::While { body, cond } => {
            let changed = refine_stmts(std::rc::Rc::make_mut(body), env, targets);
            // If condition is provably false, remove the loop.
            if !crate::util::expr_has_side_effecting_call(cond) {
                let range = eval_expr(cond, env);
                if range.singleton_value() == Some(0) && !contains_referenced_label(body, targets) {
                    *stmt = PreHirStmt::Block(std::rc::Rc::new(vec![]));
                    return true;
                }
            }
            changed
        }
        PreHirStmt::DoWhile { body, cond: _ } => {
            refine_stmts(std::rc::Rc::make_mut(body), env, targets)
        }
        PreHirStmt::For {
            init: _,
            body,
            update: _,
            ..
        } => refine_stmts(std::rc::Rc::make_mut(body), env, targets),
        PreHirStmt::Block(stmts) => refine_stmts(std::rc::Rc::make_mut(stmts), env, targets),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        NirType, PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt,
        PreHirSwitchCase,
    };
    use std::rc::Rc;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    #[test]
    fn jump_resolver_strips_switch_breaks_on_inline() {
        let mut func = PreHirFunction {
            name: "test_switch_break".to_string(),
            params: vec![],
            locals: vec![
                PreHirBinding {
                    name: "x".to_string(),
                    ty: u32_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: Some(PreHirExpr::Const(1, u32_ty())),
                },
                PreHirBinding {
                    name: "y".to_string(),
                    ty: u32_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
            body: vec![PreHirStmt::Switch {
                expr: PreHirExpr::Const(1, u32_ty()),
                cases: vec![PreHirSwitchCase {
                    values: vec![1],
                    body: Rc::new(vec![
                        PreHirStmt::Assign {
                            lhs: PreHirLValue::Var("y".to_string()),
                            rhs: PreHirExpr::Const(42, u32_ty()),
                        },
                        PreHirStmt::Break,
                    ]),
                }],
                default: Rc::new(vec![PreHirStmt::Break]),
            }],
            ..Default::default()
        };

        assert!(apply_jump_resolver_pass(&mut func));
        assert_eq!(func.body.len(), 1);
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } if name == "y"
        ));
    }

    #[test]
    fn jump_resolver_preserves_referenced_labels_in_if_and_switch() {
        let mut func = PreHirFunction {
            name: "test_labels".to_string(),
            params: vec![],
            locals: vec![],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Const(0, u32_ty()),
                    then_body: Rc::new(vec![PreHirStmt::Label("alive_target".to_string())]),
                    else_body: Rc::new(vec![]),
                },
                PreHirStmt::Goto("alive_target".to_string()),
            ],
            ..Default::default()
        };

        // When dead branch contains referenced label, condition folding must not delete it
        apply_jump_resolver_pass(&mut func);
        assert!(matches!(&func.body[0], PreHirStmt::If { .. }));
    }
}

#[cfg(test)]
mod signed_case_regressions {
    use super::*;
    use crate::ir::{NirType, PreHirExpr, PreHirLValue, PreHirSwitchCase};
    use std::rc::Rc;

    fn int(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    /// `switch (<const>) { case <case_value>: y = 1; default: y = 2; }`
    fn resolve(const_value: i64, bits: u32, case_value: i64) -> u64 {
        let ty = int(bits, true);
        let assign = |v: i64| PreHirStmt::Assign {
            lhs: PreHirLValue::Var("y".to_string()),
            rhs: PreHirExpr::Const(v, int(32, true)),
        };
        let mut func = PreHirFunction {
            name: "signed_case".to_string(),
            body: vec![PreHirStmt::Switch {
                expr: PreHirExpr::Const(const_value, ty),
                cases: vec![PreHirSwitchCase {
                    values: vec![case_value],
                    body: Rc::new(vec![assign(1), PreHirStmt::Break]),
                }],
                default: Rc::new(vec![assign(2), PreHirStmt::Break]),
            }],
            ..Default::default()
        };
        assert!(apply_jump_resolver_pass(&mut func));
        match &func.body[0] {
            PreHirStmt::Assign {
                rhs: PreHirExpr::Const(v, _),
                ..
            } => *v as u64,
            other => panic!("switch not resolved to an assignment: {other:?}"),
        }
    }

    #[test]
    fn a_narrow_negative_constant_selects_the_negative_case() {
        // The discriminant is the 32-bit value 0xFFFFFFFF, i.e. -1, and the
        // case says -1. Comparing the zero-extended 4294967295 with -1 missed
        // the case and inlined `default`: a different branch of the program.
        assert_eq!(resolve(-1, 32, -1), 1);
        assert_eq!(resolve(-128, 8, -128), 1);
    }

    #[test]
    fn a_narrow_constant_that_matches_no_case_still_selects_default() {
        // Positive control: the fix compares at the discriminant's width, it
        // does not make every case match.
        assert_eq!(resolve(-1, 32, 5), 2);
        assert_eq!(resolve(7, 32, 7), 1);
    }
}

#[cfg(test)]
mod pr_review_regressions {
    use super::*;
    use crate::ir::{
        NirType, PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirLValue, PreHirSwitchCase,
    };
    use std::rc::Rc;
    #[test]
    fn review_bottom_seeding_must_not_enable_bool_width_comparison_misfold() {
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ret = |v| PreHirStmt::Return(Some(PreHirExpr::Const(v, ty.clone())));
        let mut func = PreHirFunction {
            locals: vec![PreHirBinding {
                name: "x".into(),
                ty: ty.clone(),
                initializer: None,
                surface_type_name: None,
                origin: None,
            }],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".into()),
                    rhs: PreHirExpr::Const(1, ty.clone()),
                },
                PreHirStmt::Switch {
                    expr: PreHirExpr::Binary {
                        op: PreHirBinaryOp::SLt,
                        lhs: Box::new(PreHirExpr::Var("x".into())),
                        rhs: Box::new(PreHirExpr::Const(0, ty.clone())),
                        ty: NirType::Bool,
                    },
                    cases: vec![
                        PreHirSwitchCase {
                            values: vec![0],
                            body: vec![ret(11)].into(),
                        },
                        PreHirSwitchCase {
                            values: vec![1],
                            body: vec![ret(22)].into(),
                        },
                    ],
                    default: vec![ret(33)].into(),
                },
            ],
            ..Default::default()
        };
        apply_jump_resolver_pass(&mut func);
        assert!(
            !matches!(
                &func.body[1],
                PreHirStmt::Return(Some(PreHirExpr::Const(22, _)))
            ),
            "x=1; signed x<0 is false, but Bool width truncation selected the true arm"
        );
    }

    #[test]
    fn strip_switch_breaks_removes_tail_if_breaks_inside_enclosing_loop() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let assign = |name: &str, v: i64| PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(v, u32_ty.clone()),
        };
        let mut func = PreHirFunction {
            name: "nested_if_breaks".to_string(),
            body: vec![PreHirStmt::While {
                cond: PreHirExpr::Var("loop_cond".to_string()),
                body: Rc::new(vec![PreHirStmt::Switch {
                    expr: PreHirExpr::Const(1, u32_ty.clone()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: Rc::new(vec![PreHirStmt::If {
                            cond: PreHirExpr::Var("c".to_string()),
                            then_body: Rc::new(vec![assign("y", 1), PreHirStmt::Break]),
                            else_body: Rc::new(vec![assign("y", 2), PreHirStmt::Break]),
                        }]),
                    }],
                    default: Rc::new(vec![PreHirStmt::Break]),
                }]),
            }],
            ..Default::default()
        };

        assert!(apply_jump_resolver_pass(&mut func));
        let PreHirStmt::While { body, .. } = &func.body[0] else {
            panic!("expected while");
        };
        assert_eq!(body.len(), 1);
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[0]
        else {
            panic!("expected inlined if, got {body:?}");
        };
        assert_eq!(then_body.as_slice(), &[assign("y", 1)]);
        assert_eq!(else_body.as_slice(), &[assign("y", 2)]);
    }

    #[test]
    fn strip_switch_breaks_folds_early_exit_if_before_trailing_stmt() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let assign = |name: &str, v: i64| PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(v, u32_ty.clone()),
        };
        let mut func = PreHirFunction {
            name: "early_if_break".to_string(),
            body: vec![PreHirStmt::While {
                cond: PreHirExpr::Var("loop_cond".to_string()),
                body: Rc::new(vec![PreHirStmt::Switch {
                    expr: PreHirExpr::Const(1, u32_ty.clone()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: Rc::new(vec![
                            PreHirStmt::If {
                                cond: PreHirExpr::Var("c".to_string()),
                                then_body: Rc::new(vec![assign("y", 1), PreHirStmt::Break]),
                                else_body: Rc::new(vec![]),
                            },
                            assign("y", 2),
                            PreHirStmt::Break,
                        ]),
                    }],
                    default: Rc::new(vec![PreHirStmt::Break]),
                }]),
            }],
            ..Default::default()
        };

        assert!(apply_jump_resolver_pass(&mut func));
        let PreHirStmt::While { body, .. } = &func.body[0] else {
            panic!("expected while");
        };
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[0]
        else {
            panic!("expected inlined if, got {body:?}");
        };
        assert_eq!(then_body.as_slice(), &[assign("y", 1)]);
        assert_eq!(else_body.as_slice(), &[assign("y", 2)]);
    }

    #[test]
    fn strip_switch_breaks_preserves_inner_loop_break_and_blocks_partial_non_tail_inline() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let assign = |name: &str, v: i64| PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(v, u32_ty.clone()),
        };

        // Inner loop break must stay intact while outer switch break is stripped.
        let mut with_inner_loop = PreHirFunction {
            name: "inner_loop_break".to_string(),
            body: vec![PreHirStmt::While {
                cond: PreHirExpr::Var("outer".to_string()),
                body: Rc::new(vec![PreHirStmt::Switch {
                    expr: PreHirExpr::Const(1, u32_ty.clone()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: Rc::new(vec![
                            PreHirStmt::While {
                                cond: PreHirExpr::Var("inner".to_string()),
                                body: Rc::new(vec![PreHirStmt::Break]),
                            },
                            PreHirStmt::Break,
                        ]),
                    }],
                    default: Rc::new(vec![PreHirStmt::Break]),
                }]),
            }],
            ..Default::default()
        };
        assert!(apply_jump_resolver_pass(&mut with_inner_loop));
        let PreHirStmt::While { body, .. } = &with_inner_loop.body[0] else {
            panic!("expected outer while");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            PreHirStmt::While { body: inner, .. } if inner.as_slice() == [PreHirStmt::Break]
        ));

        // Partial nested conditional break followed by another statement cannot drop the
        // switch break without leaking into the outer loop or skipping fallthrough.
        let mut with_partial_non_tail = PreHirFunction {
            name: "partial_non_tail_break".to_string(),
            body: vec![PreHirStmt::While {
                cond: PreHirExpr::Var("outer".to_string()),
                body: Rc::new(vec![PreHirStmt::Switch {
                    expr: PreHirExpr::Const(1, u32_ty.clone()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: Rc::new(vec![
                            PreHirStmt::If {
                                cond: PreHirExpr::Var("c1".to_string()),
                                then_body: Rc::new(vec![PreHirStmt::If {
                                    cond: PreHirExpr::Var("c2".to_string()),
                                    then_body: Rc::new(vec![assign("y", 1), PreHirStmt::Break]),
                                    else_body: Rc::new(vec![]),
                                }]),
                                else_body: Rc::new(vec![]),
                            },
                            assign("y", 2),
                            PreHirStmt::Break,
                        ]),
                    }],
                    default: Rc::new(vec![]),
                }]),
            }],
            ..Default::default()
        };
        assert!(!apply_jump_resolver_pass(&mut with_partial_non_tail));
        let PreHirStmt::While { body, .. } = &with_partial_non_tail.body[0] else {
            panic!("expected outer while");
        };
        assert!(matches!(&body[0], PreHirStmt::Switch { .. }));
    }

    #[test]
    fn jump_resolver_abstains_on_signed_div_float_and_side_effecting_calls() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let f32_ty = NirType::Float { bits: 32 };
        let ret = |v: i64| PreHirStmt::Return(Some(PreHirExpr::Const(v, i32_ty.clone())));

        // Signed Div `-4 / 2` must NOT fold to unsigned `0x7fff_fffe`.
        let mut div_func = PreHirFunction {
            name: "signed_div_guard".to_string(),
            body: vec![PreHirStmt::Switch {
                expr: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Div,
                    lhs: Box::new(PreHirExpr::Const(-4, i32_ty.clone())),
                    rhs: Box::new(PreHirExpr::Const(2, i32_ty.clone())),
                    ty: i32_ty.clone(),
                },
                cases: vec![
                    PreHirSwitchCase {
                        values: vec![-2],
                        body: Rc::new(vec![ret(1)]),
                    },
                    PreHirSwitchCase {
                        values: vec![0x7fff_fffe],
                        body: Rc::new(vec![ret(2)]),
                    },
                ],
                default: Rc::new(vec![ret(3)]),
            }],
            ..Default::default()
        };
        assert!(!apply_jump_resolver_pass(&mut div_func));

        // Float arithmetic inside a Switch condition must NOT fold as integer bits.
        let one_f = PreHirExpr::Const(i64::from(1.0_f32.to_bits()), f32_ty.clone());
        let two_f = PreHirExpr::Const(i64::from(2.0_f32.to_bits()), f32_ty.clone());
        let float_eq = PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(one_f.clone()),
                rhs: Box::new(one_f),
                ty: f32_ty,
            }),
            rhs: Box::new(two_f),
            ty: NirType::Bool,
        };
        let mut float_func = PreHirFunction {
            name: "float_guard".to_string(),
            body: vec![PreHirStmt::Switch {
                expr: float_eq,
                cases: vec![
                    PreHirSwitchCase {
                        values: vec![0],
                        body: Rc::new(vec![ret(0)]),
                    },
                    PreHirSwitchCase {
                        values: vec![1],
                        body: Rc::new(vec![ret(1)]),
                    },
                ],
                default: Rc::new(vec![ret(2)]),
            }],
            ..Default::default()
        };
        assert!(!apply_jump_resolver_pass(&mut float_func));

        // Side-effecting call masked by `& 0` must NOT be dropped by Switch/If folding.
        let call_masked = PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs: Box::new(PreHirExpr::Call {
                origin: None,
                target: "side_effect".to_string(),
                args: vec![],
                ty: i32_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Const(0, i32_ty.clone())),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        };
        let mut call_func = PreHirFunction {
            name: "side_effect_guard".to_string(),
            body: vec![PreHirStmt::Switch {
                expr: call_masked,
                cases: vec![PreHirSwitchCase {
                    values: vec![0],
                    body: Rc::new(vec![ret(1)]),
                }],
                default: Rc::new(vec![ret(2)]),
            }],
            ..Default::default()
        };
        assert!(!apply_jump_resolver_pass(&mut call_func));
    }
}
