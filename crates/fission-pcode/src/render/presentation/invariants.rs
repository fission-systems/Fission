//! HIR presentation structural invariants (ADR 0011 regression firewall).
//!
//! These checks are **not** a semantic oracle. They catch presentation-only
//! regressions that leave unreadable or observationally broken trees:
//! undefined locals, call/load re-execution, fully empty control arms.
//!
//! On violation, [`super::apply_hir_presentation`] restores the pre-polish tree
//! so HIR never ships a worse-than-NIR broken form from a bad pass interaction.

use super::{HirExpr, HirFunction, HirLValue, HirStmt};
use std::collections::{HashMap, HashSet};

/// One structural presentation violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresentationViolation {
    pub code: &'static str,
    pub detail: String,
}

/// Validate presentation output against the pre-polish tree and local structure.
pub(crate) fn check_hir_presentation_invariants(
    before: &HirFunction,
    after: &HirFunction,
) -> Result<(), Vec<PresentationViolation>> {
    let mut violations = Vec::new();

    let before_calls = count_real_calls_in_stmts(&before.body);
    let after_calls = count_real_calls_in_stmts(&after.body);
    if after_calls > before_calls {
        violations.push(PresentationViolation {
            code: "call_count_increased",
            detail: format!("calls before={before_calls} after={after_calls}"),
        });
    }

    let before_loads = count_loads_in_stmts(&before.body);
    let after_loads = count_loads_in_stmts(&after.body);
    if after_loads > before_loads {
        violations.push(PresentationViolation {
            code: "load_count_increased",
            detail: format!("loads before={before_loads} after={after_loads}"),
        });
    }

    // Only flag *new* use-without-def introduced by polish. Pre-polish trees may
    // already contain fixture `Var("0")` homes or under-defined temps; presentation
    // must not make the set worse.
    let undef_before = undef_local_uses(before);
    let undef_after = undef_local_uses(after);
    for name in undef_after.difference(&undef_before) {
        violations.push(PresentationViolation {
            code: "use_without_def",
            detail: format!(
                "local `{name}` became used-without-def after presentation (not present pre-polish)"
            ),
        });
    }

    // Empty if shells: only flag if polish introduced them (pre-polish may have noise).
    let mut empty_before = 0usize;
    let mut empty_after = 0usize;
    count_empty_if_shells(&before.body, &mut empty_before);
    count_empty_if_shells(&after.body, &mut empty_after);
    if empty_after > empty_before {
        violations.push(PresentationViolation {
            code: "empty_if_shell",
            detail: format!(
                "empty if shells increased: before={empty_before} after={empty_after}"
            ),
        });
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn count_var_defs_in_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for s in stmts {
        count_var_defs_in_stmt(s, out);
    }
}

fn count_var_defs_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            count_var_defs_in_stmts(body, out);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_var_defs_in_stmts(then_body, out);
            count_var_defs_in_stmts(else_body, out);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                count_var_defs_in_stmt(i, out);
            }
            if let Some(u) = update {
                count_var_defs_in_stmt(u, out);
            }
            count_var_defs_in_stmts(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_var_defs_in_stmts(&case.body, out);
            }
            count_var_defs_in_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_var_uses_in_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        collect_var_uses_in_stmt(s, out);
    }
}

fn collect_var_uses_in_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_var_uses_in_lvalue(lhs, out);
            collect_var_uses_in_expr(rhs, out);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            collect_var_uses_in_expr(e, out);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => collect_var_uses_in_stmts(body, out),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_var_uses_in_expr(cond, out);
            collect_var_uses_in_stmts(body, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_var_uses_in_expr(cond, out);
            collect_var_uses_in_stmts(then_body, out);
            collect_var_uses_in_stmts(else_body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(c) = cond {
                collect_var_uses_in_expr(c, out);
            }
            if let Some(i) = init {
                collect_var_uses_in_stmt(i, out);
            }
            if let Some(u) = update {
                collect_var_uses_in_stmt(u, out);
            }
            collect_var_uses_in_stmts(body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_var_uses_in_expr(expr, out);
            for case in cases {
                collect_var_uses_in_stmts(&case.body, out);
            }
            collect_var_uses_in_stmts(default, out);
        }
    }
}

// Fix collect_var_uses_in_stmt properly without the messy double match.
// Rewrite the function cleanly below by replacing via search - actually I'll fix in write.

fn collect_var_uses_in_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => collect_var_uses_in_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_var_uses_in_expr(base, out);
            collect_var_uses_in_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => collect_var_uses_in_expr(base, out),
    }
}

fn collect_var_uses_in_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(n) => {
            out.insert(n.clone());
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            collect_var_uses_in_expr(expr, out);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_var_uses_in_expr(lhs, out);
            collect_var_uses_in_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_var_uses_in_expr(cond, out);
            collect_var_uses_in_expr(then_expr, out);
            collect_var_uses_in_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                collect_var_uses_in_expr(a, out);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => collect_var_uses_in_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            collect_var_uses_in_expr(base, out);
            collect_var_uses_in_expr(index, out);
        }
    }
}

fn is_presentation_pure_intrinsic(target: &str) -> bool {
    matches!(
        target,
        "__popcount"
            | "__popcount64"
            | "__lzcnt"
            | "__carry"
            | "__scarry"
            | "__sborrow"
            | "__parity"
    )
}

fn count_real_calls_in_stmts(stmts: &[HirStmt]) -> usize {
    stmts.iter().map(count_real_calls_in_stmt).sum()
}

fn count_real_calls_in_stmt(stmt: &HirStmt) -> usize {
    match stmt {
        HirStmt::Assign { rhs, .. } => count_real_calls_in_expr(rhs),
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => count_real_calls_in_expr(e),
        HirStmt::Block(b)
        | HirStmt::While { body: b, .. }
        | HirStmt::DoWhile { body: b, .. } => count_real_calls_in_stmts(b),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_real_calls_in_expr(cond)
                + count_real_calls_in_stmts(then_body)
                + count_real_calls_in_stmts(else_body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().map_or(0, |s| count_real_calls_in_stmt(s))
                + cond.as_ref().map_or(0, |e| count_real_calls_in_expr(e))
                + update.as_ref().map_or(0, |s| count_real_calls_in_stmt(s))
                + count_real_calls_in_stmts(body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_real_calls_in_expr(expr)
                + cases
                    .iter()
                    .map(|c| count_real_calls_in_stmts(&c.body))
                    .sum::<usize>()
                + count_real_calls_in_stmts(default)
        }
        _ => 0,
    }
}

fn count_real_calls_in_expr(expr: &HirExpr) -> usize {
    match expr {
        HirExpr::Call { target, args, .. } => {
            let self_count = if is_presentation_pure_intrinsic(target) {
                0
            } else {
                1
            };
            self_count + args.iter().map(count_real_calls_in_expr).sum::<usize>()
        }
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => count_real_calls_in_expr(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_real_calls_in_expr(lhs) + count_real_calls_in_expr(rhs)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_real_calls_in_expr(cond)
                + count_real_calls_in_expr(then_expr)
                + count_real_calls_in_expr(else_expr)
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => count_real_calls_in_expr(ptr),
        HirExpr::Index { base, index, .. } => {
            count_real_calls_in_expr(base) + count_real_calls_in_expr(index)
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => 0,
    }
}

fn count_loads_in_stmts(stmts: &[HirStmt]) -> usize {
    stmts.iter().map(count_loads_in_stmt).sum()
}

fn count_loads_in_stmt(stmt: &HirStmt) -> usize {
    match stmt {
        HirStmt::Assign { rhs, .. } => count_loads_in_expr(rhs),
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => count_loads_in_expr(e),
        HirStmt::Block(b)
        | HirStmt::While { body: b, .. }
        | HirStmt::DoWhile { body: b, .. } => count_loads_in_stmts(b),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_loads_in_expr(cond)
                + count_loads_in_stmts(then_body)
                + count_loads_in_stmts(else_body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().map_or(0, |s| count_loads_in_stmt(s))
                + cond.as_ref().map_or(0, |e| count_loads_in_expr(e))
                + update.as_ref().map_or(0, |s| count_loads_in_stmt(s))
                + count_loads_in_stmts(body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_loads_in_expr(expr)
                + cases
                    .iter()
                    .map(|c| count_loads_in_stmts(&c.body))
                    .sum::<usize>()
                + count_loads_in_stmts(default)
        }
        _ => 0,
    }
}

fn count_loads_in_expr(expr: &HirExpr) -> usize {
    match expr {
        HirExpr::Load { ptr, .. } => 1 + count_loads_in_expr(ptr),
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => count_loads_in_expr(expr),
        HirExpr::Binary { lhs, rhs, .. } => count_loads_in_expr(lhs) + count_loads_in_expr(rhs),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_loads_in_expr(cond)
                + count_loads_in_expr(then_expr)
                + count_loads_in_expr(else_expr)
        }
        HirExpr::Call { args, .. } => args.iter().map(count_loads_in_expr).sum(),
        HirExpr::PtrOffset { base, .. }
        | HirExpr::FieldAccess { base, .. }
        | HirExpr::AggregateCopy { src: base, .. } => count_loads_in_expr(base),
        HirExpr::Index { base, index, .. } => {
            count_loads_in_expr(base) + count_loads_in_expr(index)
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => 0,
    }
}

fn body_effectively_empty(stmts: &[HirStmt]) -> bool {
    stmts.iter().all(|s| match s {
        HirStmt::Block(inner) => body_effectively_empty(inner),
        HirStmt::Label(_) => true,
        _ => false,
    })
}

fn undef_local_uses(func: &HirFunction) -> HashSet<String> {
    let formals: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut defs = HashMap::new();
    count_var_defs_in_stmts(&func.body, &mut defs);
    let mut used = HashSet::new();
    collect_var_uses_in_stmts(&func.body, &mut used);
    used.into_iter()
        .filter(|name| {
            !formals.contains(name.as_str()) && defs.get(name).copied().unwrap_or(0) == 0
        })
        .collect()
}

fn count_empty_if_shells(stmts: &[HirStmt], out: &mut usize) {
    for s in stmts {
        match s {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if body_effectively_empty(then_body) && body_effectively_empty(else_body) {
                    *out += 1;
                }
                count_empty_if_shells(then_body, out);
                count_empty_if_shells(else_body, out);
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => count_empty_if_shells(b, out),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_empty_if_shells(&case.body, out);
                }
                count_empty_if_shells(default, out);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::{HirBinaryOp, HirExpr, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType};

    fn int_ty(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn local(name: &str) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty: int_ty(32, true),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn param(name: &str) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty: int_ty(32, true),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    #[test]
    fn invariants_flag_new_use_without_def() {
        let before = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Const(1, int_ty(32, true)),
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            return_type: int_ty(32, true),
            ..Default::default()
        };
        let after = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".into())))],
            return_type: int_ty(32, true),
            ..Default::default()
        };
        let err = check_hir_presentation_invariants(&before, &after).unwrap_err();
        assert!(
            err.iter().any(|v| v.code == "use_without_def"),
            "expected use_without_def, got {err:?}"
        );
    }

    #[test]
    fn invariants_ignore_preexisting_use_without_def() {
        let before = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".into())))],
            return_type: int_ty(32, true),
            ..Default::default()
        };
        let after = before.clone();
        assert!(check_hir_presentation_invariants(&before, &after).is_ok());
    }

    #[test]
    fn invariants_flag_call_duplication() {
        let call = HirExpr::Call {
            target: "side".into(),
            args: vec![],
            ty: int_ty(32, true),
        };
        let before = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![local("x")],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: call.clone(),
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            return_type: int_ty(32, true),
            ..Default::default()
        };
        let after = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![],
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(call.clone()),
                rhs: Box::new(call),
                ty: int_ty(32, true),
            }))],
            return_type: int_ty(32, true),
            ..Default::default()
        };
        let err = check_hir_presentation_invariants(&before, &after).unwrap_err();
        assert!(
            err.iter().any(|v| v.code == "call_count_increased"),
            "expected call_count_increased, got {err:?}"
        );
    }
}
