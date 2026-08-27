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
        "stack_chk_fail" | "stack_chk_fail_local" | "stack_smash_handler"
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
}
