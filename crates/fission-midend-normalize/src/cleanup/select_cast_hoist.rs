//! Lift a cast both arms of a conditional share out of the conditional.
//!
//! ```text
//! x = c ? (uchar *)a : (uchar *)b;   ->   x = (uchar *)(c ? a : b);
//! ```
//!
//! The two spellings compile identically -- one conversion happens either
//! way, on whichever arm the condition selects -- and the hoisted form is
//! the one a person writes. It is also, on the DecBench structure metric,
//! materially cheaper: pyjoern gives a conditional whose arms carry casts
//! its own branch in the CFG (four nodes) and a conditional over plain
//! operands a single node, so a function emitting a cast in both arms of
//! seventy conditionals pays for seventy branches that its source does not
//! have.
//!
//! Only an *identical* target type on both arms hoists. Two different casts
//! are two different conversions and there is no single outer cast that
//! means the same thing.

use std::rc::Rc;

use crate::prelude::*;

pub fn hoist_shared_select_casts(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    walk_stmts(&mut func.body, &mut changed);
    for binding in &mut func.locals {
        if let Some(init) = binding.initializer.as_mut() {
            rewrite(init, &mut changed);
        }
    }
    changed
}

fn walk_stmts(stmts: &mut Vec<PreHirStmt>, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                walk_lvalue(lhs, changed);
                rewrite(rhs, changed);
            }
            PreHirStmt::Expr(expr) => rewrite(expr, changed),
            PreHirStmt::VaStart { va_list, .. } => rewrite(va_list, changed),
            PreHirStmt::Return(Some(expr)) => rewrite(expr, changed),
            PreHirStmt::Block(body) => walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), changed),
            PreHirStmt::While { cond, body } => {
                rewrite(cond, changed);
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), changed);
            }
            PreHirStmt::DoWhile { body, cond } => {
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), changed);
                rewrite(cond, changed);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init.as_mut() {
                    walk_one(init, changed);
                }
                if let Some(cond) = cond.as_mut() {
                    rewrite(cond, changed);
                }
                if let Some(update) = update.as_mut() {
                    walk_one(update, changed);
                }
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), changed);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite(cond, changed);
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(then_body), changed);
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(else_body), changed);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite(expr, changed);
                for case in cases {
                    walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body), changed);
                }
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(default), changed);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn walk_one(stmt: &mut PreHirStmt, changed: &mut bool) {
    let mut one = vec![std::mem::replace(stmt, PreHirStmt::Break)];
    walk_stmts(&mut one, changed);
    *stmt = one.pop().expect("walk_stmts preserves length");
}

fn walk_lvalue(lhs: &mut PreHirLValue, changed: &mut bool) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => rewrite(ptr, changed),
        PreHirLValue::Index { base, index, .. } => {
            rewrite(base, changed);
            rewrite(index, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => rewrite(base, changed),
    }
}

fn rewrite(expr: &mut PreHirExpr, changed: &mut bool) {
    // Children first, so an inner conditional is already hoisted when this
    // one is examined.
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => return,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => rewrite(expr, changed),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite(lhs, changed);
            rewrite(rhs, changed);
        }
        PreHirExpr::Index { base, index, .. } => {
            rewrite(base, changed);
            rewrite(index, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite(arg, changed);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite(cond, changed);
            rewrite(then_expr, changed);
            rewrite(else_expr, changed);
        }
    }

    let PreHirExpr::Select {
        cond,
        then_expr,
        else_expr,
        ty,
    } = expr
    else {
        return;
    };
    let (PreHirExpr::Cast { ty: then_ty, .. }, PreHirExpr::Cast { ty: else_ty, .. }) =
        (then_expr.as_ref(), else_expr.as_ref())
    else {
        return;
    };
    if then_ty != else_ty {
        return;
    }
    let hoisted = then_ty.clone();
    let PreHirExpr::Cast {
        expr: then_inner, ..
    } = then_expr.as_mut()
    else {
        unreachable!("matched a Cast above")
    };
    let then_inner = std::mem::replace(then_inner.as_mut(), PreHirExpr::Const(0, ty.clone()));
    let PreHirExpr::Cast {
        expr: else_inner, ..
    } = else_expr.as_mut()
    else {
        unreachable!("matched a Cast above")
    };
    let else_inner = std::mem::replace(else_inner.as_mut(), PreHirExpr::Const(0, ty.clone()));

    // The conditional now yields the arms' *pre-cast* value, so its own type
    // is theirs, and the hoisted cast restores what the surrounding
    // expression was given before.
    let inner_ty = expr_type_of(&then_inner).unwrap_or_else(|| hoisted.clone());
    *expr = PreHirExpr::Cast {
        ty: hoisted,
        expr: Box::new(PreHirExpr::Select {
            cond: cond.clone(),
            then_expr: Box::new(then_inner),
            else_expr: Box::new(else_inner),
            ty: inner_ty,
        }),
    };
    *changed = true;
}

/// The declared type of an expression where it carries one. `None` for the
/// shapes whose type only the surrounding context knows.
fn expr_type_of(expr: &PreHirExpr) -> Option<NirType> {
    match expr {
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. } => Some(ty.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::PreHirBinding;

    fn ptr_ty() -> NirType {
        NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }))
    }

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn cast(ty: NirType, expr: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Cast {
            ty,
            expr: Box::new(expr),
        }
    }

    fn select(then_expr: PreHirExpr, else_expr: PreHirExpr, ty: NirType) -> PreHirExpr {
        PreHirExpr::Select {
            cond: Box::new(var("c")),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
            ty,
        }
    }

    fn func(body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            name: "t".to_string(),
            int_param_offsets: Vec::new(),
            locals: vec![PreHirBinding {
                name: "x".to_string(),
                ty: ptr_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body,
            ..Default::default()
        }
    }

    fn set(rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_string()),
            rhs,
        }
    }

    fn rhs_of(f: &PreHirFunction) -> &PreHirExpr {
        match &f.body[0] {
            PreHirStmt::Assign { rhs, .. } => rhs,
            other => panic!("shape changed: {other:?}"),
        }
    }

    /// The shape the metric pays for: one conversion written twice.
    #[test]
    fn hoists_a_cast_both_arms_share() {
        let mut f = func(vec![set(select(
            cast(ptr_ty(), var("a")),
            cast(ptr_ty(), var("b")),
            ptr_ty(),
        ))]);
        assert!(hoist_shared_select_casts(&mut f));
        let PreHirExpr::Cast { ty, expr } = rhs_of(&f) else {
            panic!("expected an outer cast, got {:?}", rhs_of(&f));
        };
        assert_eq!(*ty, ptr_ty());
        let PreHirExpr::Select {
            then_expr,
            else_expr,
            ..
        } = expr.as_ref()
        else {
            panic!("expected the conditional under the cast");
        };
        assert_eq!(**then_expr, var("a"));
        assert_eq!(**else_expr, var("b"));
    }

    /// Two different target types are two different conversions; no single
    /// outer cast means the same thing.
    #[test]
    fn keeps_arms_whose_casts_differ() {
        let mut f = func(vec![set(select(
            cast(ptr_ty(), var("a")),
            cast(u32_ty(), var("b")),
            ptr_ty(),
        ))]);
        assert!(!hoist_shared_select_casts(&mut f));
        assert!(matches!(rhs_of(&f), PreHirExpr::Select { .. }));
    }

    /// One bare arm has no conversion to share.
    #[test]
    fn keeps_a_conditional_with_one_bare_arm() {
        let mut f = func(vec![set(select(
            cast(ptr_ty(), var("a")),
            var("b"),
            ptr_ty(),
        ))]);
        assert!(!hoist_shared_select_casts(&mut f));
        assert!(matches!(rhs_of(&f), PreHirExpr::Select { .. }));
    }

    /// Nested conditionals hoist from the inside out.
    #[test]
    fn hoists_through_a_nested_conditional() {
        let inner = select(cast(ptr_ty(), var("a")), cast(ptr_ty(), var("b")), ptr_ty());
        let mut f = func(vec![set(select(
            cast(ptr_ty(), inner),
            cast(ptr_ty(), var("d")),
            ptr_ty(),
        ))]);
        assert!(hoist_shared_select_casts(&mut f));
        let PreHirExpr::Cast { expr, .. } = rhs_of(&f) else {
            panic!("expected an outer cast");
        };
        let PreHirExpr::Select { then_expr, .. } = expr.as_ref() else {
            panic!("expected the conditional under the cast");
        };
        // The inner conditional hoisted its own shared cast first, so what
        // the outer arm holds is a cast over a bare conditional.
        assert!(
            matches!(then_expr.as_ref(), PreHirExpr::Cast { .. }),
            "{then_expr:?}"
        );
    }

    /// The walk reaches conditionals inside nested bodies, not just the top
    /// statement list.
    #[test]
    fn hoists_inside_a_branch_body() {
        let mut f = func(vec![PreHirStmt::If {
            cond: Box::new(var("c")).as_ref().clone(),
            then_body: Rc::new(vec![set(select(
                cast(ptr_ty(), var("a")),
                cast(ptr_ty(), var("b")),
                ptr_ty(),
            ))]),
            else_body: Rc::new(Vec::new()),
        }]);
        assert!(hoist_shared_select_casts(&mut f));
        let PreHirStmt::If { then_body, .. } = &f.body[0] else {
            panic!("shape changed");
        };
        let PreHirStmt::Assign { rhs, .. } = &then_body[0] else {
            panic!("shape changed");
        };
        assert!(matches!(rhs, PreHirExpr::Cast { .. }), "{rhs:?}");
    }
}
