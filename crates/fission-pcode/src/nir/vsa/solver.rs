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
use crate::nir::{HirFunction, HirLValue, HirStmt, NirBinding};
use std::collections::HashMap;

const MAX_ITERATIONS: usize = 8;

/// Run the VSA solver on a function and return the resulting range
/// environment (variable name → CircleRange).
pub(crate) fn solve(func: &HirFunction) -> RangeEnv {
    let mut env: RangeEnv = HashMap::new();

    // Seed with parameter and local types.
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
fn propagate_stmts(stmts: &[HirStmt], env: &mut RangeEnv, with_widening: bool, changed: &mut bool) {
    for stmt in stmts {
        propagate_stmt(stmt, env, with_widening, changed);
    }
}

fn propagate_stmt(stmt: &HirStmt, env: &mut RangeEnv, widen: bool, changed: &mut bool) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let new_range = eval_expr(rhs, env);
            if let HirLValue::Var(name) = lhs {
                let old = env
                    .get(name.as_str())
                    .copied()
                    .unwrap_or_else(|| CircleRange::top(new_range.bits()));
                let next = if widen {
                    new_range.widen(&old)
                } else {
                    new_range.join(&old)
                };
                if next != old {
                    *changed = true;
                    env.insert(name.clone(), next);
                }
            }
            // Memory assignments (Deref/Index) are ignored — conservative.
        }
        HirStmt::Block(stmts) => {
            propagate_stmts(stmts, env, widen, changed);
        }
        HirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            // Conservative: join environments from both branches.
            let mut then_env = env.clone();
            let mut else_env = env.clone();
            propagate_stmts(then_body, &mut then_env, widen, changed);
            propagate_stmts(else_body, &mut else_env, widen, changed);
            // Merge back into env.
            for (k, then_r) in &then_env {
                let else_r = else_env
                    .get(k.as_str())
                    .copied()
                    .unwrap_or_else(|| CircleRange::top(then_r.bits()));
                let joined = then_r.join(&else_r);
                let old = env
                    .get(k.as_str())
                    .copied()
                    .unwrap_or_else(|| CircleRange::top(joined.bits()));
                let next = if widen {
                    joined.widen(&old)
                } else {
                    joined.join(&old)
                };
                if next != old {
                    *changed = true;
                    env.insert(k.clone(), next);
                }
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            propagate_stmts(body, env, true, changed);
        }
        HirStmt::For {
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
        HirStmt::Switch { cases, default, .. } => {
            let base_env = env.clone();
            let mut merged_env: RangeEnv = HashMap::new();
            for case in cases {
                let mut case_env = base_env.clone();
                propagate_stmts(&case.body, &mut case_env, widen, changed);
                merge_into(&mut merged_env, &case_env, widen, changed);
            }
            {
                let mut def_env = base_env.clone();
                propagate_stmts(default, &mut def_env, widen, changed);
                merge_into(&mut merged_env, &def_env, widen, changed);
            }
            for (k, r) in merged_env {
                let old = env
                    .get(k.as_str())
                    .copied()
                    .unwrap_or_else(|| CircleRange::top(r.bits()));
                let next = if widen { r.widen(&old) } else { r.join(&old) };
                if next != old {
                    *changed = true;
                    env.insert(k, next);
                }
            }
        }
        // Labels, Gotos, Break, Continue, Return don't affect ranges directly.
        _ => {}
    }
}

fn merge_into(dst: &mut RangeEnv, src: &RangeEnv, _widen: bool, _changed: &mut bool) {
    for (k, r) in src {
        let entry = dst
            .entry(k.clone())
            .or_insert_with(|| CircleRange::bottom(r.bits()));
        *entry = entry.join(r);
    }
}

/// Query the range for a binding, defaulting to top.
pub(crate) fn range_of<'a>(env: &'a RangeEnv, binding: &NirBinding) -> CircleRange {
    env.get(binding.name.as_str())
        .copied()
        .unwrap_or_else(|| CircleRange::top(nir_bits(&binding.ty).unwrap_or(64)))
}
