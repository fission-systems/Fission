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
    // range if present. Locals/globals whose address escapes (`&x`) can be
    // mutated through a pointer, and non-integer bindings (`Float`, `Aggregate`,
    // `Unknown`, >64-bit ints) stay at Top.
    let address_taken = address_taken_vars(func);
    for binding in &func.locals {
        let initial_range = match nir_bits(&binding.ty) {
            Some(bits) if CircleRange::supports_intervals(bits) => {
                if address_taken.contains(binding.name.as_str()) {
                    CircleRange::top(bits)
                } else if let Some(init_expr) = &binding.initializer {
                    eval_expr(init_expr, &env).cast_unsigned(bits)
                } else {
                    CircleRange::bottom(bits)
                }
            }
            Some(bits) => CircleRange::top(bits),
            None => CircleRange::top(64),
        };
        env.insert(binding.name.clone(), initial_range);
    }
    for name in &address_taken {
        env.entry((*name).to_string())
            .or_insert_with(|| CircleRange::top(64));
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

/// Names of locals/globals whose address is taken anywhere in the function.
///
/// Bodies are shared `Rc` DAGs, so each statement list is visited once.
fn address_taken_vars(func: &PreHirFunction) -> HashSet<&str> {
    fn expr<'a>(e: &'a PreHirExpr, out: &mut HashSet<&'a str>) {
        match e {
            PreHirExpr::AddressOfLocal(name) | PreHirExpr::AddressOfGlobal(name) => {
                out.insert(name.as_str());
            }
            PreHirExpr::Var(_) | PreHirExpr::Const(..) => {}
            PreHirExpr::Cast { expr: inner, .. }
            | PreHirExpr::Unary { expr: inner, .. }
            | PreHirExpr::Load { ptr: inner, .. }
            | PreHirExpr::PtrOffset { base: inner, .. }
            | PreHirExpr::FieldAccess { base: inner, .. }
            | PreHirExpr::AggregateCopy { src: inner, .. } => expr(inner, out),
            PreHirExpr::Binary { lhs, rhs, .. }
            | PreHirExpr::Index {
                base: lhs,
                index: rhs,
                ..
            } => {
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
    for binding in func.params.iter().chain(func.locals.iter()) {
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

fn update_env(
    env: &mut RangeEnv,
    name: &str,
    new_range: CircleRange,
    widen: bool,
    changed: &mut bool,
) {
    let (old, aligned_new) = match env.get(name).copied() {
        Some(old) => (old, new_range.cast_unsigned(old.bits())),
        None if name.starts_with("DAT_") => (CircleRange::top(new_range.bits()), new_range),
        None => (CircleRange::bottom(new_range.bits()), new_range),
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
    use crate::ir::{NirType, PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};
    use std::rc::Rc;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn binding(name: &str, ty: NirType, initializer: Option<PreHirExpr>) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: None,
            initializer,
        }
    }

    fn make_func(
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
    fn solve_refines_locals_from_bottom_while_keeping_params_top() {
        let func = make_func(
            vec![binding("p", u32_ty(), None)],
            vec![binding("x", u32_ty(), None)],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(5, u32_ty()),
            }],
        );
        let env = solve(&func);
        assert_eq!(env.get("x").and_then(|r| r.singleton_value()), Some(5));
        assert!(env.get("p").copied().expect("p in env").is_top());
    }

    #[test]
    fn solve_keeps_address_taken_locals_and_globals_at_top() {
        let func = make_func(
            vec![],
            vec![binding("x", u32_ty(), None)],
            vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(5, u32_ty()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "init".to_string(),
                    args: vec![PreHirExpr::AddressOfLocal("x".to_string())],
                    ty: u32_ty(),
                }),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("DAT_140008000".to_string()),
                    rhs: PreHirExpr::Const(1, u32_ty()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("g_escaped".to_string()),
                    rhs: PreHirExpr::Const(2, u32_ty()),
                },
                PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "take_global".to_string(),
                    args: vec![PreHirExpr::AddressOfGlobal("g_escaped".to_string())],
                    ty: u32_ty(),
                }),
            ],
        );
        let env = solve(&func);
        assert!(env.get("x").copied().expect("x in env").is_top());
        assert!(eval_expr(&PreHirExpr::Var("DAT_140008000".to_string()), &env).is_top());
        assert!(
            env.get("g_escaped")
                .copied()
                .expect("escaped global in env")
                .is_top()
        );
    }

    #[test]
    fn solve_tracks_unseeded_vars_and_else_only_if_vars() {
        let func = make_func(
            vec![],
            vec![],
            vec![PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: Rc::new(vec![]),
                else_body: Rc::new(vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("else_only".to_string()),
                    rhs: PreHirExpr::Const(7, u32_ty()),
                }]),
            }],
        );
        let env = solve(&func);
        assert_eq!(
            env.get("else_only").and_then(|r| r.singleton_value()),
            Some(7),
            "else-only var must not be dropped or forced to Top: {env:?}"
        );
    }

    #[test]
    fn solve_joins_if_branches_for_initialized_and_then_only_vars() {
        let func = make_func(
            vec![],
            vec![binding(
                "x",
                u32_ty(),
                Some(PreHirExpr::Const(10, u32_ty())),
            )],
            vec![PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: Rc::new(vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("x".to_string()),
                        rhs: PreHirExpr::Const(20, u32_ty()),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("then_only".to_string()),
                        rhs: PreHirExpr::Const(3, u32_ty()),
                    },
                ]),
                else_body: Rc::new(vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(21, u32_ty()),
                }]),
            }],
        );
        let env = solve(&func);
        let x = env.get("x").copied().expect("x in env");
        assert_eq!(x, CircleRange::interval(10, 22, 32));
        assert_eq!(
            env.get("then_only").and_then(|r| r.singleton_value()),
            Some(3)
        );
    }

    #[test]
    fn solve_widens_changing_loop_vars_but_preserves_loop_invariant_assigns() {
        let func = make_func(
            vec![],
            vec![
                binding("i", u32_ty(), Some(PreHirExpr::Const(0, u32_ty()))),
                binding("inv", u32_ty(), None),
            ],
            vec![PreHirStmt::While {
                cond: PreHirExpr::Var("c".to_string()),
                body: Rc::new(vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("inv".to_string()),
                        rhs: PreHirExpr::Const(42, u32_ty()),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: crate::ir::PreHirBinaryOp::Add,
                            lhs: Box::new(PreHirExpr::Var("i".to_string())),
                            rhs: Box::new(PreHirExpr::Const(1, u32_ty())),
                            ty: u32_ty(),
                        },
                    },
                ]),
            }],
        );
        let env = solve(&func);
        assert!(env.get("i").copied().expect("i in env").is_top());
        assert_eq!(env.get("inv").and_then(|r| r.singleton_value()), Some(42));
    }

    #[test]
    fn solve_keeps_float_locals_at_top_even_if_assigned_int_zero() {
        let f32_ty = NirType::Float { bits: 32 };
        let func = make_func(
            vec![],
            vec![binding("f", f32_ty, None)],
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("f".to_string()),
                rhs: PreHirExpr::Const(0, u32_ty()),
            }],
        );
        let env = solve(&func);
        assert!(env.get("f").copied().expect("f in env").is_top());
    }
}
