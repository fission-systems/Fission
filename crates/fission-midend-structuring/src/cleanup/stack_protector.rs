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

use fission_midend_prehir::{PreHirExpr, PreHirStmt};

use std::rc::Rc;

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

/// Whether `stmt` is a guard whose only job is to call `__stack_chk_fail`.
///
/// Either arm may hold the call -- which arm depends on how the comparison
/// was normalized -- but the other must be empty, so that removing the guard
/// removes nothing the source wrote.
fn is_stack_protector_guard(stmt: &PreHirStmt) -> bool {
    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmt
    else {
        return false;
    };
    let (taken, empty) = if else_body.is_empty() {
        (then_body, else_body)
    } else if then_body.is_empty() {
        (else_body, then_body)
    } else {
        return false;
    };
    empty.is_empty() && taken.len() == 1 && is_stack_chk_fail_call(&taken[0])
}

/// The arm taken when the canary is intact, when `stmt` is the stack-protector
/// check in a shape it can have *before* the layout settles.
///
/// [`is_stack_protector_guard`] matches the finished form, a one-armed `if`
/// around the call. Earlier the same check is a two-armed `if`: one arm calls
/// the handler (and may carry the `return` the lifter put after a call it
/// does not know diverges), the other is everything the function does next.
/// A pass that asks "does this region decide anything?" has to see through
/// both, because this decision is the compiler's and is about to be removed.
pub(super) fn stack_protector_pass_arm(stmt: &PreHirStmt) -> Option<&[PreHirStmt]> {
    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmt
    else {
        return None;
    };
    let is_fail_arm = |arm: &[PreHirStmt]| {
        arm.first().is_some_and(is_stack_chk_fail_call)
            && arm[1..]
                .iter()
                .all(|stmt| matches!(stmt, PreHirStmt::Return(_)))
    };
    if is_fail_arm(then_body) {
        Some(else_body.as_slice())
    } else if is_fail_arm(else_body) {
        Some(then_body.as_slice())
    } else {
        None
    }
}

/// Remove every stack-protector guard in `body`, reporting whether any went.
pub fn drop_stack_protector_guards(body: Vec<PreHirStmt>) -> (Vec<PreHirStmt>, bool) {
    let mut changed = false;
    let out = drop_in(body, &mut changed);
    (out, changed)
}

fn drop_in(body: Vec<PreHirStmt>, changed: &mut bool) -> Vec<PreHirStmt> {
    let mut out = Vec::with_capacity(body.len());
    for stmt in body {
        if is_stack_protector_guard(&stmt) {
            *changed = true;
            continue;
        }
        out.push(recurse(stmt, changed));
    }
    out
}

fn nested(body: &Rc<Vec<PreHirStmt>>, changed: &mut bool) -> Rc<Vec<PreHirStmt>> {
    Rc::new(drop_in(body.as_ref().clone(), changed))
}

fn recurse(stmt: PreHirStmt, changed: &mut bool) -> PreHirStmt {
    match stmt {
        PreHirStmt::Block(body) => PreHirStmt::Block(nested(&body, changed)),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond,
            then_body: nested(&then_body, changed),
            else_body: nested(&else_body, changed),
        },
        PreHirStmt::While { cond, body } => PreHirStmt::While {
            cond,
            body: nested(&body, changed),
        },
        PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: nested(&body, changed),
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
            body: nested(&body, changed),
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
                    case.body = nested(&case.body, changed);
                    case
                })
                .collect(),
            default: nested(&default, changed),
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
        assert_eq!(stack_protector_pass_arm(&then_fails), Some(&[ret()][..]));
        // if (!x) { return rax; } else { __stack_chk_fail(); }
        let else_fails = guard(vec![ret()], vec![call("__stack_chk_fail")]);
        assert_eq!(stack_protector_pass_arm(&else_fails), Some(&[ret()][..]));
        // The settled one-armed guard: the pass arm is the empty one.
        let settled = guard(vec![call("__stack_chk_fail")], Vec::new());
        assert_eq!(stack_protector_pass_arm(&settled), Some(&[][..]));
    }

    #[test]
    fn an_arm_that_does_more_than_fail_is_not_the_check() {
        let ret = || PreHirStmt::Return(None);
        // A source-written abort path is a real decision.
        let abort = guard(vec![call("abort"), ret()], vec![ret()]);
        assert_eq!(stack_protector_pass_arm(&abort), None);
        // Work after the handler call is not scaffolding.
        let works = guard(
            vec![call("__stack_chk_fail"), call("cleanup"), ret()],
            vec![ret()],
        );
        assert_eq!(stack_protector_pass_arm(&works), None);
        // Nor is work before it.
        let works_first = guard(vec![call("log"), call("__stack_chk_fail")], vec![ret()]);
        assert_eq!(stack_protector_pass_arm(&works_first), None);
        assert_eq!(stack_protector_pass_arm(&call("__stack_chk_fail")), None);
    }
}
