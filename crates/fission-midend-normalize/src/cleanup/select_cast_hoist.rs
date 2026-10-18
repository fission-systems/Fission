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
//! means the same thing. The pre-cast source types must also agree: moving a
//! shared destination cast outside a select whose arms require different
//! promotions changes which conversion happens before selection.

use std::collections::HashMap;
use std::rc::Rc;

use crate::prelude::*;

type TypeMap = HashMap<String, NirType>;

pub fn hoist_shared_select_casts(func: &mut PreHirFunction) -> bool {
    let type_map: TypeMap = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect();
    let mut changed = false;
    walk_stmts(&mut func.body, &type_map, &mut changed);
    for binding in &mut func.locals {
        if let Some(init) = binding.initializer.as_mut() {
            rewrite(init, &type_map, &mut changed);
        }
    }
    changed
}

fn walk_stmts(stmts: &mut Vec<PreHirStmt>, type_map: &TypeMap, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                walk_lvalue(lhs, type_map, changed);
                rewrite(rhs, type_map, changed);
            }
            PreHirStmt::Expr(expr) => rewrite(expr, type_map, changed),
            PreHirStmt::VaStart { va_list, .. } => rewrite(va_list, type_map, changed),
            PreHirStmt::Return(Some(expr)) => rewrite(expr, type_map, changed),
            PreHirStmt::Block(body) => {
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), type_map, changed)
            }
            PreHirStmt::While { cond, body } => {
                rewrite(cond, type_map, changed);
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), type_map, changed);
            }
            PreHirStmt::DoWhile { body, cond } => {
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), type_map, changed);
                rewrite(cond, type_map, changed);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init.as_mut() {
                    walk_one(init, type_map, changed);
                }
                if let Some(cond) = cond.as_mut() {
                    rewrite(cond, type_map, changed);
                }
                if let Some(update) = update.as_mut() {
                    walk_one(update, type_map, changed);
                }
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), type_map, changed);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite(cond, type_map, changed);
                walk_stmts(
                    Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    type_map,
                    changed,
                );
                walk_stmts(
                    Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    type_map,
                    changed,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite(expr, type_map, changed);
                for case in cases {
                    walk_stmts(
                        Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        type_map,
                        changed,
                    );
                }
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(default), type_map, changed);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn walk_one(stmt: &mut PreHirStmt, type_map: &TypeMap, changed: &mut bool) {
    let mut one = vec![std::mem::replace(stmt, PreHirStmt::Break)];
    walk_stmts(&mut one, type_map, changed);
    *stmt = one.pop().expect("walk_stmts preserves length");
}

fn walk_lvalue(lhs: &mut PreHirLValue, type_map: &TypeMap, changed: &mut bool) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => rewrite(ptr, type_map, changed),
        PreHirLValue::Index { base, index, .. } => {
            rewrite(base, type_map, changed);
            rewrite(index, type_map, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => rewrite(base, type_map, changed),
    }
}

fn rewrite(expr: &mut PreHirExpr, type_map: &TypeMap, changed: &mut bool) {
    // Children first, so an inner conditional is already hoisted when this
    // one is examined.
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => return,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => rewrite(expr, type_map, changed),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite(lhs, type_map, changed);
            rewrite(rhs, type_map, changed);
        }
        PreHirExpr::Index { base, index, .. } => {
            rewrite(base, type_map, changed);
            rewrite(index, type_map, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite(arg, type_map, changed);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite(cond, type_map, changed);
            rewrite(then_expr, type_map, changed);
            rewrite(else_expr, type_map, changed);
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

    let hoisted = match (then_expr.as_ref(), else_expr.as_ref()) {
        (PreHirExpr::Cast { ty: then_ty, .. }, PreHirExpr::Cast { ty: else_ty, .. })
            if then_ty == else_ty =>
        {
            then_ty.clone()
        }
        _ => return,
    };

    // Commuting the casts with the select is sound only when both arms are
    // converted from the same source type. Otherwise the original form may
    // sign-extend one arm and zero-extend the other (or perform pointer vs.
    // integer conversion) before selection, while the hoisted form would
    // force both through one inferred inner select type.
    let inner_ty = match (then_expr.as_ref(), else_expr.as_ref()) {
        (
            PreHirExpr::Cast {
                expr: then_inner, ..
            },
            PreHirExpr::Cast {
                expr: else_inner, ..
            },
        ) => {
            let Some(then_source_ty) = expr_type_of(then_inner, type_map) else {
                return;
            };
            let Some(else_source_ty) = expr_type_of(else_inner, type_map) else {
                return;
            };
            if then_source_ty != else_source_ty {
                return;
            }
            then_source_ty
        }
        _ => unreachable!("matched two casts above"),
    };

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

/// The declared type of an expression where it carries one. Variables get
/// their type from the function bindings captured before rewriting.
fn expr_type_of(expr: &PreHirExpr, type_map: &TypeMap) -> Option<NirType> {
    match expr {
        PreHirExpr::Var(name) => type_map.get(name).cloned(),
        PreHirExpr::AddressOfLocal(name) => type_map
            .get(name)
            .cloned()
            .map(|ty| NirType::Ptr(Box::new(ty))),
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. }
        | PreHirExpr::FieldAccess { ty, .. } => Some(ty.clone()),
        PreHirExpr::PtrOffset { base, .. } => expr_type_of(base, type_map),
        PreHirExpr::Index { elem_ty, .. } => Some(elem_ty.clone()),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AggregateCopy { .. } => None,
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

    fn int_ty(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn u32_ty() -> NirType {
        int_ty(32, false)
    }

    fn binding(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: None,
            initializer: None,
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
            params: vec![
                binding("a", ptr_ty()),
                binding("b", ptr_ty()),
                binding("c", u32_ty()),
                binding("d", ptr_ty()),
            ],
            int_param_offsets: Vec::new(),
            locals: vec![binding("x", ptr_ty())],
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

    /// Equal destination casts are still distinct conversions when their
    /// source types differ. Hoisting would make the select choose one source
    /// type and could turn sign extension into zero extension (or vice versa).
    #[test]
    fn keeps_shared_casts_with_different_source_types() {
        let u64_ty = int_ty(64, false);
        let mut f = PreHirFunction {
            name: "t".to_string(),
            params: vec![
                binding("signed", int_ty(32, true)),
                binding("unsigned", int_ty(32, false)),
                binding("c", u32_ty()),
            ],
            locals: vec![binding("x", u64_ty.clone())],
            body: vec![set(select(
                cast(u64_ty.clone(), var("signed")),
                cast(u64_ty.clone(), var("unsigned")),
                u64_ty,
            ))],
            ..Default::default()
        };

        assert!(!hoist_shared_select_casts(&mut f));
        assert!(matches!(rhs_of(&f), PreHirExpr::Select { .. }));
    }

    /// If a source expression's type cannot be proven, keep the casts on the
    /// arms rather than guessing an inner select type.
    #[test]
    fn keeps_shared_casts_when_a_source_type_is_unknown() {
        let mut f = func(vec![set(select(
            cast(ptr_ty(), PreHirExpr::AddressOfGlobal("g".to_string())),
            cast(ptr_ty(), var("b")),
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
