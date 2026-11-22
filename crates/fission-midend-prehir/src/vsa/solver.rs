/// ValueSetSolver: forward worklist-based dataflow analysis over HIR.
///
/// Processes a function's body (flat or structured) in a forward pass,
/// propagating `CircleRange` values through assignments.  A widening
/// operator prevents infinite loops in cyclic control flow.
///
/// Algorithm:
/// 1. Seed: constants → singleton ranges; all others → top.
/// 2. Worklist forward scan: propagate assignments.
/// 3. On repeated visits (loop back-edges): apply `widen()`.
/// 4. Fixed-point reached when no range changes.
///
/// This is a simplified, non-SSA fixed-point solver suitable for HIR
/// tree-structured programs.  It is conservative (sound) but may be
/// imprecise for programs with complex aliasing.
use super::circle_range::CircleRange;
use super::transfer::{RangeEnv, eval_expr, nir_bits};
use crate::ir::{NirBinding, PreHirFunction, PreHirLValue, PreHirStmt};
use std::collections::{HashMap, HashSet};

const MAX_ITERATIONS: usize = 8;

/// Run the VSA solver on a function and return the resulting range
/// environment (variable name → CircleRange).
pub fn solve(func: &PreHirFunction) -> RangeEnv {
    let mut env: RangeEnv = HashMap::new();

    // Every parameter and local starts at Top. The environment is
    // flow-insensitive: a name has one range for the whole function, the join
    // of every assignment this function makes to it. That join says nothing
    // about a read that precedes the assignments (a live-in register, an
    // unrecovered parameter, an undefined local) or about a store this
    // function cannot see (a callee or an alias writing a global), so the
    // only sound starting point for a name is Top.
    for binding in func.params.iter().chain(func.locals.iter()) {
        let bits = nir_bits(&binding.ty).unwrap_or(64);
        env.insert(binding.name.clone(), CircleRange::top(bits));
    }

    // Iterative forward propagation with widening.
    for iter in 0..MAX_ITERATIONS {
        let mut changed = false;
        propagate_stmts(&func.body, &mut env, iter > 0, &mut changed);
        if !changed {
            break;
        }
    }

    env
}

/// Propagate ranges through a statement list.
///
/// `with_widening`: if true, apply the widening operator on assignment to
/// prevent non-termination in loops.
fn propagate_stmts(
    stmts: &[PreHirStmt],
    env: &mut RangeEnv,
    with_widening: bool,
    changed: &mut bool,
) {
    for stmt in stmts {
        propagate_stmt(stmt, env, with_widening, changed);
    }
}

fn update_env(
    env: &mut RangeEnv,
    name: &str,
    new_range: CircleRange,
    widen: bool,
    changed: &mut bool,
) {
    // A name the seed did not cover (a global, a register the builder did not
    // declare) starts at Top for the same reason a local does: this
    // function's assignments are not the only writes to it.
    let (old, aligned_new) = match env.get(name).copied() {
        Some(old) => (old, new_range.cast_unsigned(old.bits())),
        None => (CircleRange::top(new_range.bits()), new_range),
    };
    let next = if widen && !old.is_bottom() {
        aligned_new.widen(&old)
    } else {
        aligned_new.join(&old)
    };
    if next != old {
        *changed = true;
        env.insert(name.to_string(), next);
    }
}

fn propagate_stmt(stmt: &PreHirStmt, env: &mut RangeEnv, widen: bool, changed: &mut bool) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let new_range = eval_expr(rhs, env);
            if let PreHirLValue::Var(name) = lhs {
                update_env(env, name, new_range, widen, changed);
            }
            // Memory assignments (Deref/Index) are ignored — conservative.
        }
        PreHirStmt::Block(stmts) => {
            propagate_stmts(stmts, env, widen, changed);
        }
        PreHirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            // Conservative: join environments from both branches.
            let mut then_env = env.clone();
            let mut else_env = env.clone();
            propagate_stmts(then_body, &mut then_env, widen, changed);
            propagate_stmts(else_body, &mut else_env, widen, changed);
            // Merge back into env, including variables introduced only in `else_body`.
            let all_keys: HashSet<String> =
                then_env.keys().chain(else_env.keys()).cloned().collect();
            for k in all_keys {
                let joined = match (then_env.get(&k), else_env.get(&k)) {
                    (Some(t), Some(e)) => t.join(e),
                    (Some(t), None) => *t,
                    (None, Some(e)) => *e,
                    (None, None) => continue,
                };
                update_env(env, &k, joined, widen, changed);
            }
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            propagate_stmts(body, env, true, changed);
        }
        PreHirStmt::For {
            init, body, update, ..
        } => {
            if let Some(init_stmt) = init {
                propagate_stmt(init_stmt, env, widen, changed);
            }
            propagate_stmts(body, env, true, changed);
            if let Some(update_stmt) = update {
                propagate_stmt(update_stmt, env, true, changed);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let base_env = env.clone();
            let mut merged_env: RangeEnv = HashMap::new();
            for case in cases {
                let mut case_env = base_env.clone();
                propagate_stmts(&case.body, &mut case_env, widen, changed);
                merge_into(&mut merged_env, &case_env);
            }
            {
                let mut def_env = base_env.clone();
                propagate_stmts(default, &mut def_env, widen, changed);
                merge_into(&mut merged_env, &def_env);
            }
            for (k, r) in merged_env {
                update_env(env, &k, r, widen, changed);
            }
        }
        // Labels, Gotos, Break, Continue, Return don't affect ranges directly.
        _ => {}
    }
}

fn merge_into(dst: &mut RangeEnv, src: &RangeEnv) {
    for (k, r) in src {
        let entry = dst
            .entry(k.clone())
            .or_insert_with(|| CircleRange::bottom(r.bits()));
        *entry = entry.join(r);
    }
}

/// Query the range for a binding, defaulting to top.
pub fn range_of<'a>(env: &'a RangeEnv, binding: &NirBinding) -> CircleRange {
    env.get(binding.name.as_str())
        .copied()
        .unwrap_or_else(|| CircleRange::top(nir_bits(&binding.ty).unwrap_or(64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{PreHirBinding, PreHirExpr, PreHirSwitchCase};
    use crate::vsa::apply_jump_resolver_pass;
    use std::rc::Rc;

    fn u32_ty() -> crate::ir::NirType {
        crate::ir::NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn assign(name: &str, value: i64) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(value, u32_ty()),
        }
    }

    /// `switch (<name>) { case 0: y = 10; break; case 1: y = 11; break; }`
    fn switch_on(name: &str) -> PreHirStmt {
        PreHirStmt::Switch {
            expr: PreHirExpr::Var(name.to_string()),
            cases: vec![
                PreHirSwitchCase {
                    values: vec![0],
                    body: Rc::new(vec![assign("y", 10), PreHirStmt::Break]),
                },
                PreHirSwitchCase {
                    values: vec![1],
                    body: Rc::new(vec![assign("y", 11), PreHirStmt::Break]),
                },
            ],
            default: Rc::new(vec![]),
        }
    }

    fn assert_switch_kept(func: &PreHirFunction, index: usize) {
        match &func.body[index] {
            PreHirStmt::Switch { cases, .. } => assert_eq!(
                cases
                    .iter()
                    .flat_map(|c| c.values.iter().copied())
                    .collect::<Vec<_>>(),
                vec![0, 1],
                "both arms are reachable"
            ),
            other => panic!("switch was folded into one arm: {other:?}"),
        }
    }

    #[test]
    fn a_global_written_by_a_callee_keeps_every_switch_arm() {
        // counter = 0; reset(); switch (counter) { case 0: ...; case 1: ...; }
        //
        // `reset` may store 1 to `counter`. The only assignment this function
        // makes is `= 0`, so a solver that starts the name at Bottom proves
        // `counter == 0` and replaces the switch with the case-0 arm.
        let mut func = PreHirFunction {
            name: "calls_reset".to_string(),
            body: vec![
                assign("counter", 0),
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "reset".to_string(),
                    args: vec![],
                    ty: u32_ty(),
                }),
                switch_on("counter"),
            ],
            ..Default::default()
        };
        apply_jump_resolver_pass(&mut func);
        assert_switch_kept(&func, 2);
        assert!(eval_expr(&PreHirExpr::Var("counter".to_string()), &solve(&func)).is_top());
    }

    #[test]
    fn a_local_read_before_its_assignment_keeps_every_switch_arm() {
        // switch (x) { ... } x = 1;
        //
        // The switch reads `x` before this function assigns it: a live-in
        // register or an unrecovered parameter. The later `x = 1` says nothing
        // about that read.
        let mut func = PreHirFunction {
            name: "reads_live_in".to_string(),
            locals: vec![PreHirBinding {
                name: "x".to_string(),
                ty: u32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![switch_on("x"), assign("x", 1)],
            ..Default::default()
        };
        apply_jump_resolver_pass(&mut func);
        assert_switch_kept(&func, 0);
        assert!(eval_expr(&PreHirExpr::Var("x".to_string()), &solve(&func)).is_top());
    }
}
