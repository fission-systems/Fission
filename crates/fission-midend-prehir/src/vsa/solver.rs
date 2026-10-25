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
use crate::ir::{NirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};
use std::collections::{HashMap, HashSet};

const MAX_ITERATIONS: usize = 8;

/// Run the VSA solver on a function and return the resulting range
/// environment (variable name → CircleRange).
pub fn solve(func: &PreHirFunction) -> RangeEnv {
    let mut env: RangeEnv = HashMap::new();

    // Parameters start at Top (can receive arbitrary caller arguments).
    for binding in &func.params {
        let bits = nir_bits(&binding.ty).unwrap_or(64);
        env.insert(binding.name.clone(), CircleRange::top(bits));
    }

    // Locals start at Bottom (unassigned / unvisited), or their initializer
    // range if present. A local whose address escapes (`&x`) can be written
    // through that pointer, which the assignment transfer never sees, so it
    // stays at Top like a parameter.
    let address_taken = address_taken_locals(func);
    for binding in &func.locals {
        let bits = nir_bits(&binding.ty).unwrap_or(64);
        let initial_range = if address_taken.contains(binding.name.as_str()) {
            CircleRange::top(bits)
        } else if let Some(init_expr) = &binding.initializer {
            eval_expr(init_expr, &env)
        } else {
            CircleRange::bottom(bits)
        };
        env.insert(binding.name.clone(), initial_range);
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

/// Names of locals whose address is taken anywhere in the function.
///
/// Bodies are shared `Rc` DAGs, so each statement list is visited once.
fn address_taken_locals(func: &PreHirFunction) -> HashSet<&str> {
    fn expr<'a>(e: &'a PreHirExpr, out: &mut HashSet<&'a str>) {
        match e {
            PreHirExpr::AddressOfLocal(name) => {
                out.insert(name.as_str());
            }
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => {}
            PreHirExpr::Cast { expr: inner, .. }
            | PreHirExpr::Unary { expr: inner, .. }
            | PreHirExpr::Load { ptr: inner, .. }
            | PreHirExpr::PtrOffset { base: inner, .. }
            | PreHirExpr::FieldAccess { base: inner, .. }
            | PreHirExpr::AggregateCopy { src: inner, .. } => expr(inner, out),
            PreHirExpr::Binary { lhs, rhs, .. } | PreHirExpr::Index { base: lhs, index: rhs, .. } => {
                expr(lhs, out);
                expr(rhs, out);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                expr(cond, out);
                expr(then_expr, out);
                expr(else_expr, out);
            }
            PreHirExpr::Call { args, .. } => args.iter().for_each(|a| expr(a, out)),
        }
    }
    fn lvalue<'a>(lv: &'a PreHirLValue, out: &mut HashSet<&'a str>) {
        match lv {
            PreHirLValue::Var(_) => {}
            PreHirLValue::Deref { ptr, .. } => expr(ptr, out),
            PreHirLValue::FieldAccess { base, .. } => expr(base, out),
            PreHirLValue::Index { base, index, .. } => {
                expr(base, out);
                expr(index, out);
            }
        }
    }
    fn stmts<'a>(
        list: &'a std::rc::Rc<Vec<PreHirStmt>>,
        seen: &mut HashSet<*const Vec<PreHirStmt>>,
        out: &mut HashSet<&'a str>,
    ) {
        if seen.insert(std::rc::Rc::as_ptr(list)) {
            list.iter().for_each(|s| stmt(s, seen, out));
        }
    }
    fn stmt<'a>(
        s: &'a PreHirStmt,
        seen: &mut HashSet<*const Vec<PreHirStmt>>,
        out: &mut HashSet<&'a str>,
    ) {
        match s {
            PreHirStmt::Assign { lhs, rhs } => {
                lvalue(lhs, out);
                expr(rhs, out);
            }
            PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => expr(e, out),
            PreHirStmt::VaStart { va_list, .. } => expr(va_list, out),
            PreHirStmt::Block(body) => stmts(body, seen, out),
            PreHirStmt::Switch {
                expr: scrutinee,
                cases,
                default,
            } => {
                expr(scrutinee, out);
                cases.iter().for_each(|c| stmts(&c.body, seen, out));
                stmts(default, seen, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                expr(cond, out);
                stmts(then_body, seen, out);
                stmts(else_body, seen, out);
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                expr(cond, out);
                stmts(body, seen, out);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    stmt(init, seen, out);
                }
                if let Some(cond) = cond {
                    expr(cond, out);
                }
                if let Some(update) = update {
                    stmt(update, seen, out);
                }
                stmts(body, seen, out);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }

    let mut out = HashSet::new();
    let mut seen = HashSet::new();
    for s in &func.body {
        stmt(s, &mut seen, &mut out);
    }
    for binding in &func.locals {
        if let Some(init) = &binding.initializer {
            expr(init, &mut out);
        }
    }
    out
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

fn propagate_stmt(stmt: &PreHirStmt, env: &mut RangeEnv, widen: bool, changed: &mut bool) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let new_range = eval_expr(rhs, env);
            if let PreHirLValue::Var(name) = lhs {
                let old = env
                    .get(name.as_str())
                    .copied()
                    .unwrap_or_else(|| CircleRange::bottom(new_range.bits()));
                let next = if widen && !old.is_bottom() {
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
            // Merge back into env.
            let all_keys: std::collections::HashSet<_> =
                then_env.keys().chain(else_env.keys()).cloned().collect();
            for k in all_keys {
                let then_r = then_env
                    .get(&k)
                    .copied()
                    .unwrap_or_else(|| CircleRange::bottom(64));
                let else_r = else_env
                    .get(&k)
                    .copied()
                    .unwrap_or_else(|| CircleRange::bottom(then_r.bits()));
                let joined = then_r.join(&else_r);
                let old = env
                    .get(&k)
                    .copied()
                    .unwrap_or_else(|| CircleRange::bottom(joined.bits()));
                let next = if widen && !old.is_bottom() {
                    joined.widen(&old)
                } else {
                    joined.join(&old)
                };
                if next != old {
                    *changed = true;
                    env.insert(k, next);
                }
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
                    .unwrap_or_else(|| CircleRange::bottom(r.bits()));
                let next = if widen && !old.is_bottom() {
                    r.widen(&old)
                } else {
                    r.join(&old)
                };
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
pub fn range_of<'a>(env: &'a RangeEnv, binding: &NirBinding) -> CircleRange {
    env.get(binding.name.as_str())
        .copied()
        .unwrap_or_else(|| CircleRange::top(nir_bits(&binding.ty).unwrap_or(64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{NirType, PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};

    fn make_test_func(
        params: Vec<PreHirBinding>,
        locals: Vec<PreHirBinding>,
        body: Vec<PreHirStmt>,
    ) -> PreHirFunction {
        PreHirFunction {
            name: "test_fn".to_string(),
            params,
            locals,
            body,
            ..Default::default()
        }
    }

    #[test]
    fn solve_keeps_address_taken_local_at_top() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let local_x = PreHirBinding {
            name: "x".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        // x = 5; init(&x);  -- the call may overwrite x through the pointer.
        let func = make_test_func(
            vec![],
            vec![local_x],
            vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(5, u32_ty.clone()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "init".to_string(),
                    args: vec![PreHirExpr::AddressOfLocal("x".to_string())],
                    ty: u32_ty,
                }),
            ],
        );

        let env = solve(&func);
        assert!(env.get("x").copied().expect("x in env").is_top());
    }

    #[test]
    fn solve_infers_singleton_for_local_assignment() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let local_x = PreHirBinding {
            name: "x".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        let param_p = PreHirBinding {
            name: "p".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        let func = make_test_func(
            vec![param_p],
            vec![local_x],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(5, u32_ty),
            }],
        );

        let env = solve(&func);
        let x_range = env.get("x").copied().expect("x in env");
        assert!(x_range.is_singleton());
        assert_eq!(x_range.singleton_value(), Some(5));

        let p_range = env.get("p").copied().expect("p in env");
        assert!(p_range.is_top());
    }

    #[test]
    fn solve_infers_range_with_initializer_and_if_branches() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let local_x = PreHirBinding {
            name: "x".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: Some(PreHirExpr::Const(10, u32_ty.clone())),
        };
        let func = make_test_func(
            vec![],
            vec![local_x],
            vec![PreHirStmt::If {
                cond: PreHirExpr::Const(1, u32_ty.clone()),
                then_body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(20, u32_ty.clone()),
                }]),
                else_body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(21, u32_ty),
                }]),
            }],
        );

        let env = solve(&func);
        let x_range = env.get("x").copied().expect("x in env");
        assert!(!x_range.is_top());
        assert!(!x_range.is_bottom());
        assert_eq!(x_range.lo(), 10);
        assert_eq!(x_range.hi(), 22);
    }
}
