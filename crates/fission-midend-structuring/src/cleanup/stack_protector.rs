//! Dropping the stack-protector check the compiler inserted.
//!
//! `-fstack-protector` gives a function with a local buffer a guard the source
//! never wrote: load the canary in the prologue, re-read and compare it in the
//! epilogue, and call `__stack_chk_fail` when they differ.
//!
//! ```text
//! local_10 = *(unsigned long long *)(fs_offset + 40);
//! ...body...
//! xVar23 = local_28 - *(unsigned long long *)(fs_offset + 40);
//! if (xVar23) { __stack_chk_fail(...); }
//! return xVar23;
//! ```
//!
//! That `if` is a diamond in the CFG and the source graph has no node for it,
//! which is why `log_gpasswd_success_group` (source CFG: one node, no edges)
//! scored a structural distance of 10 against a body that is otherwise a
//! straight line. 61 of the 250 sample-set functions carry the guard.
//!
//! Only the branch is removed. The canary arithmetic stays: it is
//! straight-line, so it costs nothing structurally, and deleting an assignment
//! whose value the return still names would emit an undefined variable.
//!
//! # What counts as the check
//!
//! The layout puts the handler wherever the block order left it, so the check
//! does not always look like the `if` above. One rule covers the forms that
//! reach the output: **a branch exactly one side of which does nothing but
//! reach the handler is the check, and it is replaced by its other side.**
//! "Reach the handler" is the call itself, a jump to a label standing on the
//! call, or -- for an arm that is not there -- falling out of the `if` into
//! the call.
//!
//! ```text
//! if (x) { block_f: __stack_chk_fail(); }      // a label an earlier clone left
//! if (!x) { return v; } __stack_chk_fail();    // the handler laid out after
//! if (x) goto block_f;                         // every check but the nearest
//! if (c) { ...; if (!x) { return v; } } __stack_chk_fail();
//! ```
//!
//! A handler that control merely runs into is not a check and stays: behind a
//! call the lifter does not know to be no-return, the handler is the only
//! evidence that the code in front of it ends there.
//!
//! Removing a check can leave its handler with nothing reaching it. Only that
//! block is swept ([`Checks::drop_unreached_handlers`]); re-running the
//! general unreachable-tail pass here deleted dead code that had nothing to
//! do with the canary, kept alive only by jumps out of other dead code.

use fission_midend_prehir::{PreHirExpr, PreHirStmt};

use std::rc::Rc;

use crate::HashMap;

/// Whether `target` is the stack-protector failure handler.
///
/// Matched by name rather than by the no-return fixpoint: a `sub_XXXX` that
/// happens not to return is an ordinary abort path the source did write, and
/// its guard is a real branch. Only this one name is scaffolding.
fn is_stack_chk_fail(target: &str) -> bool {
    matches!(
        target.trim_start_matches('_'),
        "stack_chk_fail"
            | "stack_chk_fail_local"
            | "stack_smash_handler"
            // MSVC /GS: the cookie mismatch handler, reached the same way.
            | "report_gsfailure"
    )
}

fn is_stack_chk_fail_call(stmt: &PreHirStmt) -> bool {
    let expr = match stmt {
        PreHirStmt::Expr(expr) => expr,
        PreHirStmt::Assign { rhs, .. } => rhs,
        _ => return false,
    };
    matches!(expr, PreHirExpr::Call { target, .. } if is_stack_chk_fail(target))
}

/// The handler call each label stands for.
///
/// A function has one block that calls the handler and every check in it
/// branches there, so all but the check laid out next to that block reach it
/// by `goto`. A jump to one of these labels is the failing side of a check
/// exactly as the call itself is.
type FailLabels = HashMap<String, PreHirStmt>;

/// What the function's layout says about where a failed check goes.
#[derive(Default)]
struct Checks {
    fail_labels: FailLabels,
}

impl Checks {
    fn of(body: &[PreHirStmt]) -> Self {
        let mut checks = Self::default();
        // A label on a jump to a fail label is one too, so a pass can add to
        // what the next pass matches. Each round names at least one new
        // label, which bounds the loop by the labels in the function.
        while checks.collect_fail_labels(body) {}
        checks
    }

    fn collect_fail_labels(&mut self, body: &[PreHirStmt]) -> bool {
        let mut grew = false;
        for (idx, stmt) in body.iter().enumerate() {
            if let PreHirStmt::Label(name) = stmt {
                if !self.fail_labels.contains_key(name) {
                    if let Some(call) = self.fail_at_front(&body[idx + 1..]).cloned() {
                        self.fail_labels.insert(name.clone(), call);
                        grew = true;
                    }
                }
            }
            for nested in child_sequences(stmt) {
                grew |= self.collect_fail_labels(nested);
            }
        }
        grew
    }

    /// The handler call `stmt` amounts to: the call, or a jump to it.
    fn fail_call<'a>(&'a self, stmt: &'a PreHirStmt) -> Option<&'a PreHirStmt> {
        match stmt {
            PreHirStmt::Goto(label) => self.fail_labels.get(label),
            call if is_stack_chk_fail_call(call) => Some(call),
            _ => None,
        }
    }

    /// The handler call control meets first on entering `stmts`, if that is
    /// the first thing it meets.
    fn fail_at_front<'a>(&'a self, stmts: &'a [PreHirStmt]) -> Option<&'a PreHirStmt> {
        stmts
            .iter()
            .find(|stmt| !matches!(stmt, PreHirStmt::Label(_)))
            .and_then(|stmt| self.fail_call(stmt))
    }

    /// Whether `arm` does nothing but fail.
    ///
    /// The `return` a lifter puts behind a call it does not know diverges is
    /// allowed after the handler; anything else there, or anything at all
    /// before it, is work the source wrote and makes the branch a real one.
    fn is_fail_arm(&self, arm: &[PreHirStmt]) -> bool {
        let mut rest = arm
            .iter()
            .skip_while(|stmt| matches!(stmt, PreHirStmt::Label(_)));
        rest.next()
            .is_some_and(|stmt| self.fail_call(stmt).is_some())
            && rest.all(|stmt| matches!(stmt, PreHirStmt::Return(_)))
    }

    /// The arm taken when the canary is intact, when `stmt` is the check.
    ///
    /// `next_fails` says that falling out of `stmt` runs into the handler.
    /// That makes an *absent* arm the failing one: the same branch, laid out
    /// with the handler block after the `if` rather than inside it.
    ///
    /// ```text
    /// if (diff) { __stack_chk_fail(); }       // handler inside
    /// return v;
    ///
    /// if (!diff) { return v; }                // handler after
    /// __stack_chk_fail();
    /// ```
    ///
    /// Exactly one arm must fail. If both do there is no passing side to
    /// keep, and the branch is left for a reader to see.
    fn pass_arm<'a>(
        &self,
        stmt: &'a PreHirStmt,
        next_fails: bool,
    ) -> Option<&'a Rc<Vec<PreHirStmt>>> {
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = stmt
        else {
            return None;
        };
        let fails = |arm: &[PreHirStmt]| {
            if arm.is_empty() {
                next_fails
            } else {
                self.is_fail_arm(arm)
            }
        };
        match (fails(then_body), fails(else_body)) {
            (true, false) => Some(else_body),
            (false, true) => Some(then_body),
            _ => None,
        }
    }

    /// Replace every check in `body` with its passing arm.
    ///
    /// `tail_fails` is `next_fails` for the last statement: whether control
    /// that runs off the end of `body` meets the handler.
    fn drop_in(
        &self,
        body: Vec<PreHirStmt>,
        tail_fails: bool,
        changed: &mut bool,
    ) -> Vec<PreHirStmt> {
        let next_fails: Vec<bool> = (1..=body.len())
            .map(|after| {
                let rest = &body[after..];
                if rest.iter().all(|stmt| matches!(stmt, PreHirStmt::Label(_))) {
                    tail_fails
                } else {
                    self.fail_at_front(rest).is_some()
                }
            })
            .collect();
        let mut out = Vec::with_capacity(body.len());
        for (stmt, next_fails) in body.into_iter().zip(next_fails) {
            if let Some(pass) = self.pass_arm(&stmt, next_fails) {
                *changed = true;
                out.extend(self.drop_in(pass.as_ref().clone(), next_fails, changed));
                continue;
            }
            out.push(map_nested(stmt, &mut |nested, shares_exit| {
                let tail_fails = shares_exit && next_fails;
                Rc::new(self.drop_in(nested.as_ref().clone(), tail_fails, changed))
            }));
        }
        out
    }

    /// Put the handler call back where a jump to it lost its target.
    ///
    /// A check's failing arm can carry the label other code jumps to. The
    /// other checks went with it, but a jump that was not a check -- the
    /// fall-through behind a call the lifter does not know diverges -- still
    /// names that label. It meant "call the handler", so it says so.
    fn restore_dangling_jumps(&self, body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
        let mut defined = Vec::new();
        defined_labels(&body, &mut defined);
        self.restore_in(body, &defined)
    }

    fn restore_in(&self, body: Vec<PreHirStmt>, defined: &[String]) -> Vec<PreHirStmt> {
        body.into_iter()
            .map(|stmt| match stmt {
                PreHirStmt::Goto(label) if !defined.contains(&label) => {
                    match self.fail_labels.get(&label) {
                        Some(call) => call.clone(),
                        None => PreHirStmt::Goto(label),
                    }
                }
                other => map_nested(other, &mut |nested, _| {
                    Rc::new(self.restore_in(nested.as_ref().clone(), defined))
                }),
            })
            .collect()
    }
}

impl Checks {
    /// Remove the handler block wherever nothing reaches it any more.
    ///
    /// Where the block sat after the check rather than inside it, the passing
    /// arm's `return` is now directly in front of it, and the checks that
    /// jumped to its label are gone. Only the handler goes: this is not a
    /// general sweep for dead code, which would also take whatever else in
    /// the function had been kept alive by a jump from code that is itself
    /// unreachable.
    ///
    /// Repeated, because the block can be reached through a label that only
    /// another such block jumps to. Each round removes a statement or stops.
    fn drop_unreached_handlers(&self, mut body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
        loop {
            let mut targets = Vec::new();
            jump_targets(&body, &mut targets);
            let mut changed = false;
            body = self.drop_unreached_in(body, &targets, &mut changed);
            if !changed {
                return body;
            }
        }
    }

    fn drop_unreached_in(
        &self,
        body: Vec<PreHirStmt>,
        targets: &[String],
        changed: &mut bool,
    ) -> Vec<PreHirStmt> {
        let mut dead = vec![false; body.len()];
        for idx in 1..body.len() {
            if dead[idx] {
                continue;
            }
            // Not reached by running into it ...
            let Some(prev) = (0..idx).rev().find(|&prev| !dead[prev]) else {
                continue;
            };
            if !leaves(&body[prev]) {
                continue;
            }
            // ... and not by a jump.
            let labels = body[idx..]
                .iter()
                .take_while(|stmt| matches!(stmt, PreHirStmt::Label(_)))
                .count();
            let handler = idx + labels;
            let unlabelled = body[idx..handler]
                .iter()
                .all(|stmt| !matches!(stmt, PreHirStmt::Label(name) if targets.contains(name)));
            if unlabelled
                && body
                    .get(handler)
                    .is_some_and(|stmt| self.fail_call(stmt).is_some())
            {
                dead[idx..=handler].fill(true);
                *changed = true;
            }
        }
        body.into_iter()
            .zip(dead)
            .filter(|(_, dead)| !dead)
            .map(|(stmt, _)| {
                map_nested(stmt, &mut |nested, _| {
                    Rc::new(self.drop_unreached_in(nested.as_ref().clone(), targets, changed))
                })
            })
            .collect()
    }
}

/// Whether control cannot continue past `stmt` to the next statement.
///
/// Decided by the last statement of each arm, not by any statement in it: an
/// arm can leave and then be re-entered at a label further down.
fn leaves(stmt: &PreHirStmt) -> bool {
    let last_leaves = |body: &[PreHirStmt]| body.last().is_some_and(leaves);
    match stmt {
        PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {
            true
        }
        PreHirStmt::Block(body) => last_leaves(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => last_leaves(then_body) && last_leaves(else_body),
        other => super::unreachable_tail::is_diverging_call(other),
    }
}

fn jump_targets(stmts: &[PreHirStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        if let PreHirStmt::Goto(name) = stmt {
            out.push(name.clone());
        }
        for nested in child_sequences(stmt) {
            jump_targets(nested, out);
        }
    }
}

fn defined_labels(stmts: &[PreHirStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        if let PreHirStmt::Label(name) = stmt {
            out.push(name.clone());
        }
        for nested in child_sequences(stmt) {
            defined_labels(nested, out);
        }
    }
}

/// The arm taken when the canary is intact, when `stmts[idx]` is the
/// stack-protector check in a shape it can have *before* the layout settles.
///
/// A pass that asks "does this region decide anything?" has to see through
/// the check, because this decision is the compiler's and is about to be
/// removed. It knows only the statements it was handed, so a check that
/// reaches the handler by a jump out of them is not recognised here.
pub(super) fn stack_protector_pass_arm(stmts: &[PreHirStmt], idx: usize) -> Option<&[PreHirStmt]> {
    let checks = Checks::default();
    let next_fails = checks.fail_at_front(&stmts[idx + 1..]).is_some();
    checks
        .pass_arm(stmts.get(idx)?, next_fails)
        .map(|arm| arm.as_slice())
}

/// Replace every stack-protector check in `body` with what the function does
/// when the canary is intact, reporting whether any went.
///
/// The handler call is left wherever control reaches it without a check --
/// behind a call the lifter does not know diverges. That is a statement the
/// binary runs into as this decompiler models it, and deleting it would send
/// that path on into whatever follows.
pub fn drop_stack_protector_guards(body: Vec<PreHirStmt>) -> (Vec<PreHirStmt>, bool) {
    let checks = Checks::of(&body);
    let mut changed = false;
    let out = checks.drop_in(body, false, &mut changed);
    if !changed {
        return (out, false);
    }
    let out = checks.restore_dangling_jumps(out);
    (checks.drop_unreached_handlers(out), true)
}

/// The statement lists nested directly in `stmt`.
fn child_sequences(stmt: &PreHirStmt) -> Vec<&[PreHirStmt]> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => vec![body.as_slice()],
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![then_body.as_slice(), else_body.as_slice()],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&[PreHirStmt]> =
                cases.iter().map(|case| case.body.as_slice()).collect();
            out.push(default.as_slice());
            out
        }
        _ => Vec::new(),
    }
}

/// Rebuild `stmt` with each nested statement list passed through `f`.
///
/// The flag tells `f` whether control that runs off the end of that list
/// continues with whatever follows `stmt`. It does for an `if` arm and a
/// block. It does not for a loop body, which goes round again, and it is not
/// claimed for a `switch` arm, which may fall into the next one.
fn map_nested(
    stmt: PreHirStmt,
    f: &mut dyn FnMut(&Rc<Vec<PreHirStmt>>, bool) -> Rc<Vec<PreHirStmt>>,
) -> PreHirStmt {
    match stmt {
        PreHirStmt::Block(body) => PreHirStmt::Block(f(&body, true)),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond,
            then_body: f(&then_body, true),
            else_body: f(&else_body, true),
        },
        PreHirStmt::While { cond, body } => PreHirStmt::While {
            cond,
            body: f(&body, false),
        },
        PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: f(&body, false),
            cond,
        },
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => PreHirStmt::For {
            init,
            cond,
            update,
            body: f(&body, false),
        },
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => PreHirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.body = f(&case.body, false);
                    case
                })
                .collect(),
            default: f(&default, false),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(target: &str) -> PreHirStmt {
        PreHirStmt::Expr(PreHirExpr::Call {
            origin: None,
            target: target.to_string(),
            args: Vec::new(),
            ty: fission_midend_core::ir::NirType::Unknown,
        })
    }

    fn guard(then: Vec<PreHirStmt>, els: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: PreHirExpr::Var("xVar23".to_string()),
            then_body: Rc::new(then),
            else_body: Rc::new(els),
        }
    }

    /// The pass arm of a check with nothing after it.
    fn pass_arm_of(stmt: &PreHirStmt) -> Option<&[PreHirStmt]> {
        stack_protector_pass_arm(std::slice::from_ref(stmt), 0)
    }

    #[test]
    fn guard_in_the_then_arm_goes() {
        let body = vec![guard(vec![call("__stack_chk_fail")], vec![]), call("f")];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![call("f")]);
    }

    #[test]
    fn an_msvc_gs_failure_guard_goes() {
        let body = vec![guard(vec![call("__report_gsfailure")], vec![]), call("f")];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![call("f")]);
    }

    #[test]
    fn guard_in_the_else_arm_goes() {
        let body = vec![guard(vec![], vec![call("__stack_chk_fail")])];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert!(out.is_empty());
    }

    #[test]
    fn a_guard_that_also_does_real_work_stays() {
        let body = vec![guard(vec![call("f"), call("__stack_chk_fail")], vec![])];
        let (out, changed) = drop_stack_protector_guards(body.clone());
        assert!(!changed);
        assert_eq!(out, body);
    }

    #[test]
    fn an_ordinary_abort_guard_stays() {
        let body = vec![guard(vec![call("abort")], vec![])];
        let (out, changed) = drop_stack_protector_guards(body.clone());
        assert!(!changed);
        assert_eq!(out, body);
    }

    #[test]
    fn a_nested_guard_goes() {
        let inner = guard(vec![call("__stack_chk_fail")], vec![]);
        let body = vec![PreHirStmt::While {
            cond: PreHirExpr::Var("c".to_string()),
            body: Rc::new(vec![inner, call("f")]),
        }];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        let PreHirStmt::While { body, .. } = &out[0] else {
            panic!("expected the loop to survive");
        };
        assert_eq!(body.as_ref(), &vec![call("f")]);
    }

    #[test]
    fn the_pass_arm_is_found_in_either_arm_and_past_a_trailing_return() {
        let ret = || PreHirStmt::Return(Some(PreHirExpr::Var("rax".to_string())));
        // if (x) { __stack_chk_fail(); return; } else { return rax; }
        let then_fails = guard(
            vec![call("__stack_chk_fail"), PreHirStmt::Return(None)],
            vec![ret()],
        );
        assert_eq!(pass_arm_of(&then_fails), Some(&[ret()][..]));
        // if (!x) { return rax; } else { __stack_chk_fail(); }
        let else_fails = guard(vec![ret()], vec![call("__stack_chk_fail")]);
        assert_eq!(pass_arm_of(&else_fails), Some(&[ret()][..]));
        // The settled one-armed guard: the pass arm is the empty one.
        let settled = guard(vec![call("__stack_chk_fail")], Vec::new());
        assert_eq!(pass_arm_of(&settled), Some(&[][..]));
    }

    #[test]
    fn an_arm_that_does_more_than_fail_is_not_the_check() {
        let ret = || PreHirStmt::Return(None);
        // A source-written abort path is a real decision.
        let abort = guard(vec![call("abort"), ret()], vec![ret()]);
        assert_eq!(pass_arm_of(&abort), None);
        // Work after the handler call is not scaffolding.
        let works = guard(
            vec![call("__stack_chk_fail"), call("cleanup"), ret()],
            vec![ret()],
        );
        assert_eq!(pass_arm_of(&works), None);
        // Nor is work before it.
        let works_first = guard(vec![call("log"), call("__stack_chk_fail")], vec![ret()]);
        assert_eq!(pass_arm_of(&works_first), None);
        assert_eq!(pass_arm_of(&call("__stack_chk_fail")), None);
    }

    fn fail() -> PreHirStmt {
        call("__stack_chk_fail")
    }

    fn ret(name: &str) -> PreHirStmt {
        PreHirStmt::Return(Some(PreHirExpr::Var(name.to_string())))
    }

    fn label(name: &str) -> PreHirStmt {
        PreHirStmt::Label(name.to_string())
    }

    fn goto(name: &str) -> PreHirStmt {
        PreHirStmt::Goto(name.to_string())
    }

    fn if_else(cond: &str, then: Vec<PreHirStmt>, els: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: PreHirExpr::Var(cond.to_string()),
            then_body: Rc::new(then),
            else_body: Rc::new(els),
        }
    }

    /// The whole of what the finished body loses, in the order it loses it.
    fn settle(body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
        super::super::settle_finished_body(body, &|_| false)
    }

    #[test]
    fn a_check_with_the_handler_after_it_goes() {
        // if (ok) { return v; }
        // __stack_chk_fail();
        let body = vec![if_else("ok", vec![ret("v")], vec![]), fail()];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        // The handler is then behind a `return` and nothing reaches it.
        assert_eq!(out, vec![ret("v")]);
    }

    #[test]
    fn a_handler_control_still_runs_into_stays() {
        // if (x) { __stack_chk_fail(); }
        // f();
        // __stack_chk_fail();
        //
        // `f` is a call the lifter does not know diverges; the handler block
        // is simply the next thing in the binary.
        let body = vec![if_else("x", vec![fail()], vec![]), call("f"), fail()];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![call("f"), fail()]);
    }

    #[test]
    fn only_the_handler_is_swept_not_other_dead_code() {
        // if (x) { __stack_chk_fail(); }
        // return v;
        // g();
        //
        // What else is unreachable is for the pass that owns reachability.
        let body = vec![if_else("x", vec![fail()], vec![]), ret("v"), call("g")];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![ret("v"), call("g")]);
    }

    #[test]
    fn an_arm_that_leaves_and_is_entered_again_does_not_hide_the_handler() {
        // if (x) { __stack_chk_fail(); }
        // if (a) { goto out; back: g(); } else { return w; }
        // __stack_chk_fail();
        // out: goto back;
        //
        // The second `if` has a `goto` in one arm and a `return` in the
        // other, but control re-enters at `back` and falls out of it into
        // the handler.
        let reentered = if_else(
            "a",
            vec![goto("out"), label("back"), call("g")],
            vec![ret("w")],
        );
        let body = vec![
            if_else("x", vec![fail()], vec![]),
            reentered.clone(),
            fail(),
            label("out"),
            goto("back"),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![reentered, fail(), label("out"), goto("back")]);
    }

    #[test]
    fn a_two_armed_check_becomes_its_passing_arm() {
        // if (ok) { return v; } else { __stack_chk_fail(); }
        let body = vec![call("f"), if_else("ok", vec![ret("v")], vec![fail()])];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![call("f"), ret("v")]);
    }

    #[test]
    fn a_check_sees_the_handler_past_the_end_of_the_arm_it_is_in() {
        // if (a) { if (ok) { return v; } } else { f(); if (ok) { return w; } }
        // __stack_chk_fail();
        let body = vec![
            if_else(
                "a",
                vec![if_else("ok", vec![ret("v")], vec![])],
                vec![call("f"), if_else("ok", vec![ret("w")], vec![])],
            ),
            fail(),
        ];
        let want = if_else("a", vec![ret("v")], vec![call("f"), ret("w")]);
        assert_eq!(settle(body), vec![want]);
    }

    #[test]
    fn a_loop_body_does_not_run_into_what_follows_the_loop() {
        // while (c) { if (x) { return v; } }
        // __stack_chk_fail();
        //
        // Leaving the `if` goes round the loop, not on to the handler, so
        // this `if` is the program's.
        let body = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Var("c".to_string()),
                body: Rc::new(vec![if_else("x", vec![ret("v")], vec![])]),
            },
            fail(),
        ];
        let (out, changed) = drop_stack_protector_guards(body.clone());
        assert!(!changed);
        assert_eq!(out, body);
    }

    #[test]
    fn a_branch_with_no_passing_side_stays() {
        // if (x) { __stack_chk_fail(); }
        // __stack_chk_fail();
        let body = vec![if_else("x", vec![fail()], vec![]), fail()];
        let (out, changed) = drop_stack_protector_guards(body.clone());
        assert!(!changed);
        assert_eq!(out, body);
    }

    #[test]
    fn a_label_on_the_handler_does_not_hide_the_check() {
        // if (x) { fail_block: __stack_chk_fail(); }
        // return v;
        let body = vec![
            if_else("x", vec![label("fail_block"), fail()], vec![]),
            ret("v"),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![ret("v")]);
    }

    #[test]
    fn a_check_that_jumps_to_the_handler_goes() {
        // if (x) { goto fail_block; }
        // f();
        // if (y) { fail_block: __stack_chk_fail(); }
        // return v;
        let body = vec![
            if_else("x", vec![goto("fail_block")], vec![]),
            call("f"),
            if_else("y", vec![label("fail_block"), fail()], vec![]),
            ret("v"),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![call("f"), ret("v")]);
    }

    #[test]
    fn a_label_on_a_jump_to_the_handler_is_followed() {
        // if (x) { goto via; }
        // return v;
        // via: goto fail_block;
        // fail_block: __stack_chk_fail();
        //
        // `via` comes first in the body, so it is only known to fail once
        // `fail_block` is.
        let body = vec![
            if_else("x", vec![goto("via")], vec![]),
            ret("v"),
            label("via"),
            goto("fail_block"),
            label("fail_block"),
            fail(),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        // With the check gone nothing jumps to `via`, and once `via` is gone
        // nothing jumps to `fail_block`.
        assert_eq!(out, vec![ret("v")]);
    }

    #[test]
    fn a_jump_that_was_not_a_check_gets_the_call_it_stood_for() {
        // if (c) { g(); goto fail_block; }
        // if (x) { fail_block: __stack_chk_fail(); }
        // return v;
        //
        // The first `if` does work before it reaches the handler, so it is
        // not a check and stays -- but its target went with the check.
        let body = vec![
            if_else("c", vec![call("g"), goto("fail_block")], vec![]),
            if_else("x", vec![label("fail_block"), fail()], vec![]),
            ret("v"),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(
            out,
            vec![if_else("c", vec![call("g"), fail()], vec![]), ret("v")]
        );
    }

    #[test]
    fn a_jump_to_a_handler_that_is_still_there_stays_a_jump() {
        // if (x) { __stack_chk_fail(); }
        // if (c) { g(); goto fail_block; }
        // return v;
        // fail_block: __stack_chk_fail();
        //
        // Only a jump whose target went is rewritten.
        let jump = if_else("c", vec![call("g"), goto("fail_block")], vec![]);
        let body = vec![
            if_else("x", vec![fail()], vec![]),
            jump.clone(),
            ret("v"),
            label("fail_block"),
            fail(),
        ];
        let (out, changed) = drop_stack_protector_guards(body);
        assert!(changed);
        assert_eq!(out, vec![jump, ret("v"), label("fail_block"), fail()]);
    }

    #[test]
    fn the_handler_does_not_unwrap_an_else_the_source_wrote() {
        // if (c) { f(); if (ok) { return v; } __stack_chk_fail(); }
        // else { g(); return w; }
        //
        // The `then` arm ends in a call that never returns, which is what
        // the `else` unwrap looks for. It is the compiler's call: without
        // the check the arm ends in `return`, and that `else` is kept.
        let body = vec![if_else(
            "c",
            vec![call("f"), if_else("ok", vec![ret("v")], vec![]), fail()],
            vec![call("g"), ret("w")],
        )];
        let want = if_else("c", vec![call("f"), ret("v")], vec![call("g"), ret("w")]);
        assert_eq!(settle(body), vec![want]);
    }

    #[test]
    fn the_pass_arm_is_found_when_the_handler_follows_the_check() {
        // if (ok) { return v; }
        // __stack_chk_fail();
        let stmts = [if_else("ok", vec![ret("v")], vec![]), fail()];
        assert_eq!(stack_protector_pass_arm(&stmts, 0), Some(&[ret("v")][..]));
        // Not when something else comes first.
        let stmts = [if_else("ok", vec![ret("v")], vec![]), call("f"), fail()];
        assert_eq!(stack_protector_pass_arm(&stmts, 0), None);
    }
}
