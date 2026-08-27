//! Run-scoped copy propagation on the structured body.
//!
//! The pre-structuring `copy_propagation_pass` has to reason about a body whose
//! control flow is still gotos and labels, so it proves safety with a
//! whole-function definition count plus a `TempPreserved` veto. After
//! structuring, control flow is `If`/`While`/`DoWhile`/`For`/`Switch` nodes and
//! a *statement list is a straight-line run*. That makes a much simpler
//! argument available, and it is the one this pass uses:
//!
//! > A copy is carried forward only within one statement list, and the active
//! > set is dropped at anything that can transfer control or clobber state --
//! > a nested construct, a `Label`, a `Goto`, or any statement containing a
//! > `Call`. Nested bodies are walked with their own empty set.
//!
//! Nothing crosses an edge, so no dataflow analysis is needed to justify the
//! substitution. `TempPreserved` gets no veto here for the reason given in
//! `scratch_liveness`: the marker holds a materialisation point for
//! builder-stage consumers that have all run by the time this body exists.
//!
//! Two transforms share that scope:
//!
//! 1. **Alias propagation.** `x = y` where `y` is a variable, constant, or a
//!    cast over those. Later reads of `x` in the same run become `y`, and the
//!    copy is dropped once nothing in the run still reads it.
//! 2. **Single-read expression folding.** A builder temp written once and read
//!    exactly once *in the whole function* can have its defining expression
//!    moved to that one use, which reassembles a split address computation
//!    (`t = i * 4; p = base + t; *p`) into one expression. The read must be
//!    inside the same run and must come after the definition with nothing that
//!    could change the operands in between.
//!
//! Both are bounded and rerun to a fixpoint, because folding exposes new
//! single-read temps that a first pass could not see.
use crate::analysis::defuse::collect_expr_vars;
use crate::prelude::*;
use crate::{HashMap, HashSet};

use super::scratch_liveness::is_builder_minted_temp;

const MAX_ROUNDS: usize = 8;

/// Propagate aliases and fold single-read temps within straight-line runs.
pub fn propagate_copies_in_runs(func: &mut PreHirFunction) -> bool {
    let scratch: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like() && is_builder_minted_temp(&b.name))
        .map(|b| b.name.clone())
        .collect();
    if scratch.is_empty() {
        return false;
    }
    // A pointer-typed temp carries type information its defining expression
    // does not. `xVar12` declared `uchar *` makes `(ulonglong)xVar12` a
    // pointer-to-integer cast; fold the definition in and the same cast can
    // become redundant-looking and be dropped, leaving `ptr + ptr`. Restrict
    // expression folding to temps whose declared type is not a pointer.
    // Alias propagation is unaffected: it substitutes a variable or constant,
    // not a computed expression. This is the untyped half of the design; the
    // typed variant needs recovered types to prove the destination is scalar.
    let foldable: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| scratch.contains(&b.name) && !matches!(b.ty, NirType::Ptr(_)))
        .map(|b| b.name.clone())
        .collect();

    let mut any = false;
    for _ in 0..MAX_ROUNDS {
        let reads = count_reads(&func.body);
        let mut changed = false;
        run(&mut func.body, &scratch, &foldable, &reads, &mut changed);
        if !changed {
            break;
        }
        any = true;
    }
    if any {
        let live = count_reads(&func.body);
        func.locals
            .retain(|b| !scratch.contains(&b.name) || live.contains_key(&b.name));
    }
    any
}

/// Whole-function rvalue read counts. A fold needs the *global* count: a temp
/// read once in this run may also be read after the loop that contains it.
fn count_reads(stmts: &[PreHirStmt]) -> HashMap<String, usize> {
    let mut out = HashMap::default();
    fn expr(e: &PreHirExpr, out: &mut HashMap<String, usize>) {
        let mut names = HashSet::default();
        collect_expr_vars(e, &mut names);
        // collect_expr_vars dedupes per expression; count occurrences instead
        // so `x + x` keeps `x` out of the single-read class.
        let _ = &names;
        count_occurrences(e, out);
    }
    fn walk(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
        for s in stmts {
            match s {
                PreHirStmt::Assign { lhs, rhs } => {
                    match lhs {
                        PreHirLValue::Var(_) => {}
                        PreHirLValue::Deref { ptr, .. } => expr(ptr, out),
                        PreHirLValue::Index { base, index, .. } => {
                            expr(base, out);
                            expr(index, out);
                        }
                        PreHirLValue::FieldAccess { base, .. } => expr(base, out),
                    }
                    expr(rhs, out);
                }
                PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => expr(e, out),
                PreHirStmt::VaStart { va_list, .. } => expr(va_list, out),
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    expr(cond, out);
                    walk(then_body, out);
                    walk(else_body, out);
                }
                PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
                    expr(cond, out);
                    walk(body, out);
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    for h in [init, update].into_iter().flatten() {
                        walk(std::slice::from_ref(h), out);
                    }
                    if let Some(c) = cond {
                        expr(c, out);
                    }
                    walk(body, out);
                }
                PreHirStmt::Switch {
                    expr: d,
                    cases,
                    default,
                } => {
                    expr(d, out);
                    for c in cases {
                        walk(&c.body, out);
                    }
                    walk(default, out);
                }
                PreHirStmt::Block(body) => walk(body, out),
                PreHirStmt::Return(None)
                | PreHirStmt::Label(_)
                | PreHirStmt::Goto(_)
                | PreHirStmt::Break
                | PreHirStmt::Continue => {}
            }
        }
    }
    walk(stmts, &mut out);
    out
}

/// Read counts contributed by one statement, used to decide whether a value is
/// observed before it is overwritten.
fn walk_one(s: &PreHirStmt, out: &mut HashMap<String, usize>) {
    let slice = std::slice::from_ref(s);
    for (k, v) in count_reads(slice) {
        *out.entry(k).or_insert(0) += v;
    }
}

fn count_occurrences(e: &PreHirExpr, out: &mut HashMap<String, usize>) {
    match e {
        PreHirExpr::Var(n) => *out.entry(n.clone()).or_insert(0) += 1,
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => count_occurrences(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_occurrences(lhs, out);
            count_occurrences(rhs, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            count_occurrences(base, out);
            count_occurrences(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_occurrences(cond, out);
            count_occurrences(then_expr, out);
            count_occurrences(else_expr, out);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args {
                count_occurrences(a, out);
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => count_occurrences(src, out),
    }
}

/// Is this expression safe to carry forward as an alias?
fn is_pure_copyable(e: &PreHirExpr) -> bool {
    match e {
        PreHirExpr::Var(_) | PreHirExpr::Const(..) | PreHirExpr::AddressOfGlobal(_) => true,
        PreHirExpr::Cast { expr, .. } => is_pure_copyable(expr),
        _ => false,
    }
}

fn expr_has_call(e: &PreHirExpr) -> bool {
    match e {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_has_call(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_has_call(lhs) || expr_has_call(rhs),
        PreHirExpr::Index { base, index, .. } => expr_has_call(base) || expr_has_call(index),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => expr_has_call(cond) || expr_has_call(then_expr) || expr_has_call(else_expr),
        PreHirExpr::AggregateCopy { src, .. } => expr_has_call(src),
    }
}

fn stmt_has_call(s: &PreHirStmt) -> bool {
    match s {
        PreHirStmt::Assign { rhs, .. } => expr_has_call(rhs),
        PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => expr_has_call(e),
        _ => false,
    }
}

fn subst_expr(e: &mut PreHirExpr, map: &HashMap<String, PreHirExpr>, changed: &mut bool) {
    if let PreHirExpr::Var(n) = e {
        if let Some(rep) = map.get(n) {
            *e = rep.clone();
            *changed = true;
            return;
        }
    }
    match e {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(..) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => subst_expr(expr, map, changed),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            subst_expr(lhs, map, changed);
            subst_expr(rhs, map, changed);
        }
        PreHirExpr::Index { base, index, .. } => {
            subst_expr(base, map, changed);
            subst_expr(index, map, changed);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            subst_expr(cond, map, changed);
            subst_expr(then_expr, map, changed);
            subst_expr(else_expr, map, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args {
                subst_expr(a, map, changed);
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => subst_expr(src, map, changed),
    }
}

fn subst_stmt(s: &mut PreHirStmt, map: &HashMap<String, PreHirExpr>, changed: &mut bool) {
    match s {
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Var(_) => {}
                PreHirLValue::Deref { ptr, .. } => subst_expr(ptr, map, changed),
                PreHirLValue::Index { base, index, .. } => {
                    subst_expr(base, map, changed);
                    subst_expr(index, map, changed);
                }
                PreHirLValue::FieldAccess { base, .. } => subst_expr(base, map, changed),
            }
            subst_expr(rhs, map, changed);
        }
        PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => subst_expr(e, map, changed),
        PreHirStmt::VaStart { va_list, .. } => subst_expr(va_list, map, changed),
        _ => {}
    }
}

/// Names written by a statement, at any depth.
fn writes_of(s: &PreHirStmt, out: &mut HashSet<String>) {
    match s {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(n),
            ..
        } => {
            out.insert(n.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for b in [then_body, else_body] {
                for s in b.iter() {
                    writes_of(s, out);
                }
            }
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                writes_of(s, out);
            }
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            for h in [init, update].into_iter().flatten() {
                writes_of(h, out);
            }
            for s in body.iter() {
                writes_of(s, out);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for c in cases.iter() {
                for s in c.body.iter() {
                    writes_of(s, out);
                }
            }
            for s in default.iter() {
                writes_of(s, out);
            }
        }
        _ => {}
    }
}

fn run(
    stmts: &mut Vec<PreHirStmt>,
    scratch: &HashSet<String>,
    foldable: &HashSet<String>,
    reads: &HashMap<String, usize>,
    changed: &mut bool,
) {
    // Recurse first so inner runs are simplified before this one substitutes.
    for s in stmts.iter_mut() {
        match s {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                run(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    scratch,
                    foldable,
                    reads,
                    changed,
                );
                run(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    scratch,
                    foldable,
                    reads,
                    changed,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                run(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    scratch,
                    foldable,
                    reads,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for c in cases.iter_mut() {
                    run(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut c.body),
                        scratch,
                        foldable,
                        reads,
                        changed,
                    );
                }
                run(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    scratch,
                    foldable,
                    reads,
                    changed,
                );
            }
            _ => {}
        }
    }

    // What this run reads before it is rewritten, so a later comparison against
    // the whole-function count can tell "nobody reads it" from "somebody
    // outside this list reads it".
    let run_reads = count_reads(stmts);

    let mut map: HashMap<String, PreHirExpr> = HashMap::default();
    let mut folded: HashSet<String> = HashSet::default();

    for i in 0..stmts.len() {
        // Clear the active set at anything that can transfer control or clobber
        // state. Nothing is carried across such a boundary.
        let boundary = matches!(
            stmts[i],
            PreHirStmt::Label(_)
                | PreHirStmt::Goto(_)
                | PreHirStmt::Break
                | PreHirStmt::Continue
                | PreHirStmt::If { .. }
                | PreHirStmt::While { .. }
                | PreHirStmt::DoWhile { .. }
                | PreHirStmt::For { .. }
                | PreHirStmt::Switch { .. }
                | PreHirStmt::Block(_)
        ) || stmt_has_call(&stmts[i]);

        if boundary {
            // The statement still gets whatever is already known substituted
            // into the expressions it evaluates, then the set is dropped.
            subst_stmt(&mut stmts[i], &map, changed);
            map.clear();
            continue;
        }

        subst_stmt(&mut stmts[i], &map, changed);

        // Anything this statement writes invalidates a carried copy that names
        // it, on either side.
        let mut written = HashSet::default();
        writes_of(&stmts[i], &mut written);
        if !written.is_empty() {
            map.retain(|dst, src| {
                if written.contains(dst) {
                    return false;
                }
                let mut names = HashSet::default();
                collect_expr_vars(src, &mut names);
                names.is_disjoint(&written)
            });
        }

        // Record a new copy.
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst),
            rhs,
        } = &stmts[i]
        {
            let mut src_names = HashSet::default();
            collect_expr_vars(rhs, &mut src_names);
            if src_names.contains(dst) {
                continue; // self-referential
            }
            if is_pure_copyable(rhs) {
                map.insert(dst.clone(), rhs.clone());
            } else if foldable.contains(dst) && reads.get(dst).copied().unwrap_or(0) == 1 {
                // Single-read expression folding: the one consumer must be in
                // this same run, which the substitution below enforces because
                // the map is dropped at every boundary.
                map.insert(dst.clone(), rhs.clone());
                folded.insert(dst.clone());
            }
        }
    }

    // Copy propagation exposes dead stores: once the reload it fed was folded
    // away, `x = A; x = B;` writes A into a slot nothing observes before B
    // overwrites it. Drop the first write, within this run only, and never
    // when its right-hand side calls.
    let mut drop_idx: Vec<usize> = Vec::new();
    for i in 0..stmts.len() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst),
            rhs,
        } = &stmts[i]
        else {
            continue;
        };
        if !scratch.contains(dst) || expr_has_call(rhs) {
            continue;
        }
        // A `for` header reads names defined in its body -- `for (; ; p = t)`
        // over a body that computes `t`. Those reads are not in this statement
        // list, so a run-local count says zero and the definition looks dead.
        // Anything read more than this run can see is off limits.
        if reads.get(dst).copied().unwrap_or(0) > run_reads.get(dst).copied().unwrap_or(0) {
            continue;
        }
        for j in i + 1..stmts.len() {
            // Any boundary ends the window: control may not reach the
            // overwrite, so the first write is not provably dead.
            let ends = matches!(
                stmts[j],
                PreHirStmt::Label(_)
                    | PreHirStmt::Goto(_)
                    | PreHirStmt::Break
                    | PreHirStmt::Continue
                    | PreHirStmt::If { .. }
                    | PreHirStmt::While { .. }
                    | PreHirStmt::DoWhile { .. }
                    | PreHirStmt::For { .. }
                    | PreHirStmt::Switch { .. }
                    | PreHirStmt::Block(_)
            );
            let mut reads_here = HashMap::default();
            walk_one(&stmts[j], &mut reads_here);
            if reads_here.contains_key(dst) {
                break; // observed before any overwrite
            }
            if ends {
                break;
            }
            if matches!(&stmts[j], PreHirStmt::Assign { lhs: PreHirLValue::Var(d2), .. } if d2 == dst)
            {
                drop_idx.push(i);
                break;
            }
        }
    }
    if !drop_idx.is_empty() {
        let mut i = 0usize;
        stmts.retain(|_| {
            let keep = !drop_idx.contains(&i);
            i += 1;
            keep
        });
        *changed = true;
    }

    // A copy whose destination no longer has any reader in this run is dead.
    // Recount over the (already substituted) run rather than trusting the
    // pre-pass count.
    let after = count_reads(stmts);
    let before = stmts.len();
    stmts.retain(|s| match s {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst),
            rhs,
        } => {
            // `after` only sees this statement list. A name this run never
            // reads may still be read by an enclosing `for` header, so the
            // whole-function count has to agree that it is dead.
            !(scratch.contains(dst)
                && after.get(dst).copied().unwrap_or(0) == 0
                && reads.get(dst).copied().unwrap_or(0) <= run_reads.get(dst).copied().unwrap_or(0)
                && !expr_has_call(rhs))
        }
        _ => true,
    });
    if stmts.len() != before {
        *changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::{NirBindingOrigin, NirType};
    use fission_midend_prehir::PreHirBinding;
    use std::rc::Rc;

    fn ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }
    fn temp(n: &str) -> PreHirBinding {
        PreHirBinding {
            name: n.into(),
            ty: ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }
    fn local(n: &str) -> PreHirBinding {
        PreHirBinding {
            name: n.into(),
            ty: ty(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }
    fn set(d: &str, r: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(d.into()),
            rhs: r,
        }
    }
    fn var(n: &str) -> PreHirExpr {
        PreHirExpr::Var(n.into())
    }
    fn k(v: i64) -> PreHirExpr {
        PreHirExpr::Const(v, ty())
    }
    fn add(a: PreHirExpr, b: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(a),
            rhs: Box::new(b),
            ty: ty(),
        }
    }
    fn func(l: Vec<PreHirBinding>, b: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            name: "t".into(),
            int_param_offsets: Vec::new(),
            locals: l,
            body: b,
            ..Default::default()
        }
    }
    fn render(f: &PreHirFunction) -> String {
        format!("{:?}", f.body)
    }

    /// `xVar1 = local_4; local_8 = xVar1;` -- the alias folds and the copy goes.
    #[test]
    fn folds_a_pure_alias_within_one_run() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![set("xVar1", var("local_4")), set("local_8", var("xVar1"))],
        );
        assert!(propagate_copies_in_runs(&mut f));
        assert_eq!(f.body.len(), 1, "{}", render(&f));
        assert!(render(&f).contains("local_4"));
    }

    /// The split address computation Glaurung's module doc names:
    /// `t = i + 4; p = base + t` becomes one expression.
    #[test]
    fn folds_a_single_read_expression_into_its_one_consumer() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", add(var("local_4"), k(4))),
                set("local_8", add(var("local_8"), var("xVar1"))),
            ],
        );
        assert!(propagate_copies_in_runs(&mut f));
        assert_eq!(f.body.len(), 1, "{}", render(&f));
    }

    /// A temp read twice is not single-read and must keep its own statement.
    #[test]
    fn keeps_an_expression_temp_that_is_read_twice() {
        let mut f = func(
            vec![temp("xVar1"), local("local_8")],
            vec![
                set("xVar1", add(var("local_8"), k(1))),
                set("local_8", add(var("xVar1"), var("xVar1"))),
            ],
        );
        assert!(!propagate_copies_in_runs(&mut f));
        assert_eq!(f.body.len(), 2, "{}", render(&f));
    }

    /// A Label ends the run: nothing may be carried past a jump target.
    #[test]
    fn does_not_carry_a_copy_across_a_label() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", var("local_4")),
                PreHirStmt::Label("L1".into()),
                set("local_8", var("xVar1")),
            ],
        );
        propagate_copies_in_runs(&mut f);
        assert!(
            render(&f).contains("xVar1"),
            "alias must survive the label: {}",
            render(&f)
        );
    }

    /// A Goto ends the run for the same reason.
    #[test]
    fn does_not_carry_a_copy_across_a_goto() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", var("local_4")),
                PreHirStmt::Goto("L1".into()),
                set("local_8", var("xVar1")),
            ],
        );
        propagate_copies_in_runs(&mut f);
        assert!(render(&f).contains("xVar1"), "{}", render(&f));
    }

    /// A call can clobber caller-saved state, so the set is dropped there.
    #[test]
    fn does_not_carry_a_copy_across_a_call() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", var("local_4")),
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "f".into(),
                    args: vec![],
                    ty: ty(),
                }),
                set("local_8", var("xVar1")),
            ],
        );
        propagate_copies_in_runs(&mut f);
        assert!(render(&f).contains("xVar1"), "{}", render(&f));
    }

    /// A copy must not be carried into a loop body: the source may be updated
    /// by the body and is therefore not invariant on later iterations.
    #[test]
    fn does_not_carry_a_copy_into_a_loop_body() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", var("local_4")),
                PreHirStmt::While {
                    cond: var("local_4"),
                    body: Rc::new(vec![set("local_8", var("xVar1"))]),
                },
            ],
        );
        propagate_copies_in_runs(&mut f);
        assert!(render(&f).contains("xVar1"), "{}", render(&f));
    }

    /// Rewriting the source invalidates the carried copy.
    #[test]
    fn invalidates_a_copy_when_its_source_is_rewritten() {
        let mut f = func(
            vec![temp("xVar1"), local("local_4"), local("local_8")],
            vec![
                set("xVar1", var("local_4")),
                set("local_4", k(99)),
                set("local_8", var("xVar1")),
            ],
        );
        propagate_copies_in_runs(&mut f);
        let out = render(&f);
        assert!(
            out.contains("xVar1"),
            "must not substitute the new local_4: {out}"
        );
    }

    /// A `for` header reads names its body defines. The body is a separate
    /// statement list, so a run-local read count sees zero for such a name and
    /// would retire the definition out from under the loop -- which is what
    /// happened to `list_sum` at clang -O0, where `for (; ; p = xVar26)` lost
    /// both statements that computed `xVar26`.
    #[test]
    fn keeps_a_definition_that_only_the_enclosing_for_header_reads() {
        let mut f = func(
            vec![temp("xVar26"), local("local_18")],
            vec![PreHirStmt::For {
                init: Some(Box::new(set("local_18", k(1)))),
                cond: Some(var("local_18")),
                update: Some(Box::new(set("local_18", var("xVar26")))),
                body: Rc::new(vec![set("xVar26", add(var("local_18"), k(8)))]),
            }],
        );
        propagate_copies_in_runs(&mut f);
        let out = render(&f);
        assert!(
            out.contains("xVar26"),
            "the body's definition of xVar26 must survive: {out}"
        );
        let PreHirStmt::For { body, .. } = &f.body[0] else {
            panic!("expected For");
        };
        assert!(
            !body.is_empty(),
            "loop body must still define the update's source: {out}"
        );
    }

    /// A global symbol registered as `Temp` is not builder-minted, so it is
    /// never treated as a foldable scratch value.
    #[test]
    fn never_folds_a_resolved_global_symbol() {
        let mut f = func(
            vec![temp("result_sink"), local("local_4")],
            vec![
                set("result_sink", var("local_4")),
                PreHirStmt::Return(Some(var("local_4"))),
            ],
        );
        assert!(!propagate_copies_in_runs(&mut f));
        assert!(render(&f).contains("result_sink"), "{}", render(&f));
    }
}
