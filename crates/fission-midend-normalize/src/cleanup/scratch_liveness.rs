//! Retire scratch assignments that cannot reach observable output.
//!
//! Runs on the **structured** body, after `run_structuring_pipeline`. That
//! placement is the point: the pre-structuring `copy_propagation_pass` decides
//! what to keep from a whole-function definition count, and a definition count
//! cannot retire loop-carried residue of the shape
//!
//! ```text
//! xVar1 = xVar2;
//! ...
//! xVar2 = xVar1 + 1;
//! ```
//!
//! Every name there has a reader even when the whole graph only feeds itself.
//! Reachability has to run the other way: from what the function actually
//! observes, backwards through the dependency graph, keeping only what a root
//! transitively needs.
//!
//! A **root** is any temp read by something whose evaluation is observable —
//! a branch or loop condition, a switch discriminant, a store's address or
//! value, a call target or argument, a returned value, or the right-hand side
//! of a write to a binding that is not itself scratch. A scratch definition
//! whose own right-hand side contains a `Call` is a root regardless of whether
//! anything reads it, because removing it would delete the call.
//!
//! `TempPreserved` deliberately does **not** veto here. That marker holds a
//! materialisation point for builder-stage consumers — predicate proofs,
//! loop-carried initializer seeding, cross-block replacement — and all of them
//! have run and consumed it long before structuring produces the body this
//! pass sees. Widening the marker itself was measured and rejected; see
//! `docs/proposals/2026-08-17-ast-stage-copy-propagation.md`.
//!
//! The pass is fail-closed: a statement form it does not model aborts the whole
//! function rather than deleting dataflow across semantics it cannot read.
use crate::prelude::*;
use crate::analysis::defuse::collect_expr_vars;
use crate::{HashMap, HashSet};

/// Remove scratch assignments outside the observable dependency closure.
/// Returns `true` if anything was removed.
pub fn prune_unobservable_scratch(func: &mut PreHirFunction) -> bool {
    let scratch: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like() && is_builder_minted_temp(&b.name))
        .map(|b| b.name.clone())
        .collect();
    if scratch.is_empty() {
        return false;
    }

    let mut ctx = Collect {
        scratch: &scratch,
        deps: HashMap::default(),
        roots: HashSet::default(),
        unmodelled: false,
    };
    ctx.body(&func.body);
    if ctx.unmodelled {
        return false;
    }

    // Backwards reachability from the roots over the dependency graph.
    let mut live: HashSet<String> = HashSet::default();
    let mut stack: Vec<String> = ctx.roots.iter().cloned().collect();
    while let Some(name) = stack.pop() {
        if !live.insert(name.clone()) {
            continue;
        }
        if let Some(deps) = ctx.deps.get(&name) {
            for d in deps {
                if !live.contains(d) {
                    stack.push(d.clone());
                }
            }
        }
    }

    let dead: HashSet<String> = scratch.difference(&live).cloned().collect();
    if dead.is_empty() {
        return false;
    }

    let mut changed = false;
    prune(&mut func.body, &dead, &mut changed);
    if changed {
        func.locals.retain(|b| !dead.contains(&b.name));
    }
    changed
}

struct Collect<'a> {
    scratch: &'a HashSet<String>,
    /// scratch name -> the names its defining expression reads
    deps: HashMap<String, HashSet<String>>,
    /// names whose value is observed
    roots: HashSet<String>,
    unmodelled: bool,
}

impl Collect<'_> {
    fn expr_vars(expr: &PreHirExpr) -> HashSet<String> {
        let mut out = HashSet::default();
        collect_expr_vars(expr, &mut out);
        out
    }

    /// Everything this expression reads is observed.
    fn root_expr(&mut self, expr: &PreHirExpr) {
        self.roots.extend(Self::expr_vars(expr));
    }

    /// An lvalue that is not a bare `Var` is a store: the address it computes
    /// is evaluated, so everything in it is observed.
    fn root_lvalue_address(&mut self, lhs: &PreHirLValue) {
        match lhs {
            PreHirLValue::Var(_) => {}
            PreHirLValue::Deref { ptr, .. } => self.root_expr(ptr),
            PreHirLValue::Index { base, index, .. } => {
                self.root_expr(base);
                self.root_expr(index);
            }
            PreHirLValue::FieldAccess { base, .. } => self.root_expr(base),
        }
    }

    fn body(&mut self, stmts: &[PreHirStmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &PreHirStmt) {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                self.root_lvalue_address(lhs);
                match lhs {
                    // A scratch definition records a dependency edge instead of
                    // rooting its operands; whether they survive depends on
                    // whether anything reaches this name.
                    PreHirLValue::Var(name) if self.scratch.contains(name) => {
                        self.deps
                            .entry(name.clone())
                            .or_default()
                            .extend(Self::expr_vars(rhs));
                        // Deleting a call is not a dataflow decision.
                        if expr_contains_call(rhs) {
                            self.roots.insert(name.clone());
                        }
                    }
                    // A write to anything else -- a real local, a parameter, a
                    // store -- is observable output.
                    _ => self.root_expr(rhs),
                }
            }
            PreHirStmt::Expr(expr) => self.root_expr(expr),
            PreHirStmt::Return(Some(expr)) => self.root_expr(expr),
            PreHirStmt::Return(None) => {}
            PreHirStmt::VaStart { va_list, .. } => self.root_expr(va_list),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.root_expr(cond);
                self.body(then_body);
                self.body(else_body);
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                self.root_expr(cond);
                self.body(body);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                // The header is a boxed statement this pass will not rewrite,
                // so treat its whole dataflow as observable and prune only the
                // body. A `for (i = 0; ...; i++)` induction variable must never
                // be retired out from under its own loop.
                for header in [init, update].into_iter().flatten() {
                    self.root_header(header);
                }
                if let Some(cond) = cond {
                    self.root_expr(cond);
                }
                self.body(body);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.root_expr(expr);
                for case in cases {
                    self.body(&case.body);
                }
                self.body(default);
            }
            PreHirStmt::Block(body) => self.body(body),
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }

    /// A `for` header statement: both what it reads and what it writes stay.
    fn root_header(&mut self, stmt: &PreHirStmt) {
        if let PreHirStmt::Assign { lhs, rhs } = stmt {
            if let PreHirLValue::Var(name) = lhs {
                self.roots.insert(name.clone());
            }
            self.root_lvalue_address(lhs);
            self.root_expr(rhs);
        } else {
            // Anything else in a header is not modelled; refuse the function.
            self.unmodelled = true;
        }
    }
}

/// Did the builder mint this name as a temporary?
///
/// `origin: Temp` alone is not enough to prove a name is scratch. A resolved
/// global symbol is registered with the same origin -- an aarch64 store to
/// `0x2000` becomes a binding named `result_sink` carrying `Temp` -- and
/// writing to it is observable memory traffic, not a dead value. Only
/// `next_temp_name` mints names, and it mints exactly `bVar`/`iVar`/`uVar`/
/// `xVar` followed by a decimal id, so anything else came from a symbol table
/// or another pass and is left alone.
pub(super) fn is_builder_minted_temp(name: &str) -> bool {
    const PREFIXES: [&str; 4] = ["bVar", "iVar", "uVar", "xVar"];
    PREFIXES.iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn expr_contains_call(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_contains_call(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_contains_call(lhs) || expr_contains_call(rhs),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_call(base) || expr_contains_call(index)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_call(cond)
                || expr_contains_call(then_expr)
                || expr_contains_call(else_expr)
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_contains_call(src),
    }
}

fn prune(stmts: &mut Vec<PreHirStmt>, dead: &HashSet<String>, changed: &mut bool) {
    let before = stmts.len();
    stmts.retain(|stmt| {
        !matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(name), .. }
            if dead.contains(name))
    });
    if stmts.len() != before {
        *changed = true;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                prune(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    dead,
                    changed,
                );
                prune(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    dead,
                    changed,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                prune(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    dead,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    prune(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        dead,
                        changed,
                    );
                }
                prune(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    dead,
                    changed,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::{NirBindingOrigin, NirType};
    use fission_midend_prehir::PreHirBinding;
    use std::rc::Rc;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn temp(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: u32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    /// The marker this pass deliberately ignores. If a `TempPreserved` binding
    /// is retired here it is because nothing observes it, not because the
    /// marker was consulted.
    fn preserved(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: u32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }
    }

    fn local(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: u32_ty(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    fn set(dst: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst.to_string()),
            rhs,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn konst(v: i64) -> PreHirExpr {
        PreHirExpr::Const(v, u32_ty())
    }

    fn func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            name: "t".to_string(),
            int_param_offsets: Vec::new(),
            locals,
            body,
            ..Default::default()
        }
    }

    fn names(f: &PreHirFunction) -> Vec<String> {
        f.body
            .iter()
            .filter_map(|s| match s {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(n),
                    ..
                } => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    /// The shape a whole-function definition count cannot retire: every name
    /// has a reader, but the graph only feeds itself.
    #[test]
    fn retires_a_closed_scratch_cycle_no_definition_count_can_see() {
        let mut f = func(
            vec![temp("xVar1"), temp("xVar2"), local("local_4")],
            vec![
                set("xVar1", konst(0)),
                set("xVar2", var("xVar1")),
                set("xVar1", var("xVar2")),
                set("local_4", konst(7)),
                PreHirStmt::Return(Some(var("local_4"))),
            ],
        );
        assert!(prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["local_4"], "{:?}", f.body);
        assert!(f.locals.iter().all(|b| !b.name.starts_with("xVar")));
    }

    /// A scratch value that reaches a returned expression is observable.
    #[test]
    fn keeps_scratch_reaching_a_return() {
        let mut f = func(
            vec![temp("xVar1")],
            vec![set("xVar1", konst(3)), PreHirStmt::Return(Some(var("xVar1")))],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["xVar1"]);
    }

    /// Reaching a store's *address* counts, not just its value.
    #[test]
    fn keeps_scratch_reaching_a_store_address() {
        let mut f = func(
            vec![temp("xVar1")],
            vec![
                set("xVar1", konst(16)),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(var("xVar1")),
                        ty: u32_ty(),
                    },
                    rhs: konst(1),
                },
            ],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f).len(), 1);
    }

    /// Removing an unread definition must not remove the call inside it.
    #[test]
    fn keeps_an_unread_scratch_definition_that_calls() {
        let mut f = func(
            vec![temp("xVar1")],
            vec![
                set(
                    "xVar1",
                    PreHirExpr::Call {
                        target: "side_effect".to_string(),
                        args: vec![],
                        ty: u32_ty(),
                    },
                ),
                PreHirStmt::Return(None),
            ],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["xVar1"]);
    }

    /// A `for` induction variable must never be retired out from under its loop.
    #[test]
    fn never_prunes_a_for_header_induction_variable() {
        let mut f = func(
            vec![temp("xVar1")],
            vec![PreHirStmt::For {
                init: Some(Box::new(set("xVar1", konst(0)))),
                cond: Some(var("xVar1")),
                update: Some(Box::new(set("xVar1", konst(1)))),
                body: Rc::new(Vec::new()),
            }],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert!(f.locals.iter().any(|b| b.name == "xVar1"));
    }

    /// A write to a non-scratch binding is observable output, so what feeds it
    /// stays even though nothing reads the destination afterwards.
    #[test]
    fn keeps_scratch_feeding_a_named_local() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4")],
            vec![set("xVar1", konst(5)), set("local_4", var("xVar1"))],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["xVar1", "local_4"]);
    }

    /// `TempPreserved` gets no veto at this stage -- only observability decides.
    #[test]
    fn a_preserved_marker_does_not_by_itself_keep_a_dead_scratch_value() {
        let mut f = func(
            vec![preserved("xVar1"), local("local_4")],
            vec![
                set("xVar1", konst(9)),
                set("local_4", konst(1)),
                PreHirStmt::Return(Some(var("local_4"))),
            ],
        );
        assert!(prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["local_4"]);
    }

    /// A resolved global symbol is registered with `origin: Temp` too. Writing
    /// to it is observable memory traffic, so name shape -- not origin alone --
    /// decides what counts as scratch.
    #[test]
    fn never_prunes_a_write_to_a_resolved_global_symbol() {
        let mut f = func(
            vec![temp("xVar0"), temp("result_sink")],
            vec![
                set("xVar0", konst(10)),
                set("result_sink", var("xVar0")),
                PreHirStmt::Return(Some(var("xVar0"))),
            ],
        );
        assert!(!prune_unobservable_scratch(&mut f));
        assert_eq!(names(&f), vec!["xVar0", "result_sink"], "{:?}", f.body);
    }

    #[test]
    fn builder_minted_temp_names_are_exactly_the_next_temp_name_shapes() {
        for ok in ["xVar0", "uVar12", "iVar7", "bVar100"] {
            assert!(is_builder_minted_temp(ok), "{ok} should be scratch");
        }
        for no in ["result_sink", "local_4", "xVar", "xVarA", "param_1", "home_0", ""] {
            assert!(!is_builder_minted_temp(no), "{no} must not be scratch");
        }
    }

    /// Prunes inside nested structured bodies, not just at the top level.
    #[test]
    fn prunes_inside_a_nested_body() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4")],
            vec![
                PreHirStmt::If {
                    cond: var("local_4"),
                    then_body: Rc::new(vec![set("xVar1", konst(2))]),
                    else_body: Rc::new(Vec::new()),
                },
                PreHirStmt::Return(Some(var("local_4"))),
            ],
        );
        assert!(prune_unobservable_scratch(&mut f));
        let PreHirStmt::If { then_body, .. } = &f.body[0] else {
            panic!("expected If");
        };
        assert!(then_body.is_empty(), "{:?}", then_body);
    }
}
