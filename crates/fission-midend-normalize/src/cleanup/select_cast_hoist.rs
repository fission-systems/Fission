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

use std::collections::HashMap;
use std::rc::Rc;

use crate::prelude::*;

pub fn hoist_shared_select_casts(func: &mut PreHirFunction) -> bool {
    let mut local_types = HashMap::new();
    for param in &func.params {
        local_types.insert(param.name.clone(), param.ty.clone());
    }
    for local in &func.locals {
        local_types.insert(local.name.clone(), local.ty.clone());
    }

    let mut changed = false;
    walk_stmts(&mut func.body, &local_types, &mut changed);
    for binding in &mut func.locals {
        if let Some(init) = binding.initializer.as_mut() {
            rewrite(init, &local_types, &mut changed);
        }
    }
    changed
}

fn walk_stmts(
    stmts: &mut Vec<PreHirStmt>,
    local_types: &HashMap<String, NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                walk_lvalue(lhs, local_types, changed);
                rewrite(rhs, local_types, changed);
            }
            PreHirStmt::Expr(expr) => rewrite(expr, local_types, changed),
            PreHirStmt::VaStart { va_list, .. } => rewrite(va_list, local_types, changed),
            PreHirStmt::Return(Some(expr)) => rewrite(expr, local_types, changed),
            PreHirStmt::Block(body) => {
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), local_types, changed)
            }
            PreHirStmt::While { cond, body } => {
                rewrite(cond, local_types, changed);
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), local_types, changed);
            }
            PreHirStmt::DoWhile { body, cond } => {
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), local_types, changed);
                rewrite(cond, local_types, changed);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init.as_mut() {
                    walk_one(init, local_types, changed);
                }
                if let Some(cond) = cond.as_mut() {
                    rewrite(cond, local_types, changed);
                }
                if let Some(update) = update.as_mut() {
                    walk_one(update, local_types, changed);
                }
                walk_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), local_types, changed);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite(cond, local_types, changed);
                walk_stmts(
                    Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    local_types,
                    changed,
                );
                walk_stmts(
                    Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    local_types,
                    changed,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite(expr, local_types, changed);
                for case in cases {
                    walk_stmts(
                        Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        local_types,
                        changed,
                    );
                }
                walk_stmts(
                    Rc::<Vec<PreHirStmt>>::make_mut(default),
                    local_types,
                    changed,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn walk_one(stmt: &mut PreHirStmt, local_types: &HashMap<String, NirType>, changed: &mut bool) {
    let mut one = vec![std::mem::replace(stmt, PreHirStmt::Break)];
    walk_stmts(&mut one, local_types, changed);
    *stmt = one.pop().expect("walk_stmts preserves length");
}

fn walk_lvalue(lhs: &mut PreHirLValue, local_types: &HashMap<String, NirType>, changed: &mut bool) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => rewrite(ptr, local_types, changed),
        PreHirLValue::Index { base, index, .. } => {
            rewrite(base, local_types, changed);
            rewrite(index, local_types, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => rewrite(base, local_types, changed),
    }
}

fn rewrite(expr: &mut PreHirExpr, local_types: &HashMap<String, NirType>, changed: &mut bool) {
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
        | PreHirExpr::FieldAccess { base: expr, .. } => rewrite(expr, local_types, changed),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite(lhs, local_types, changed);
            rewrite(rhs, local_types, changed);
        }
        PreHirExpr::Index { base, index, .. } => {
            rewrite(base, local_types, changed);
            rewrite(index, local_types, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite(arg, local_types, changed);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite(cond, local_types, changed);
            rewrite(then_expr, local_types, changed);
            rewrite(else_expr, local_types, changed);
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

    let PreHirExpr::Cast {
        expr: then_inner_ref,
        ..
    } = then_expr.as_ref()
    else {
        unreachable!("matched a Cast above")
    };
    let PreHirExpr::Cast {
        expr: else_inner_ref,
        ..
    } = else_expr.as_ref()
    else {
        unreachable!("matched a Cast above")
    };

    let then_pre_ty = expr_type_of(then_inner_ref, local_types);
    let else_pre_ty = expr_type_of(else_inner_ref, local_types);

    if then_pre_ty != else_pre_ty {
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
    let inner_ty = then_pre_ty.unwrap_or_else(|| hoisted.clone());
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
fn expr_type_of(expr: &PreHirExpr, local_types: &HashMap<String, NirType>) -> Option<NirType> {
    match expr {
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. } => Some(ty.clone()),
        PreHirExpr::Var(name) => local_types.get(name.as_str()).cloned(),
        PreHirExpr::PtrOffset { base, .. } => expr_type_of(base, local_types),
        PreHirExpr::AddressOfLocal(name) => local_types
            .get(name.as_str())
            .map(|t| NirType::Ptr(Box::new(t.clone()))),
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
            locals: vec![
                PreHirBinding {
                    name: "x".to_string(),
                    ty: ptr_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "a".to_string(),
                    ty: ptr_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "b".to_string(),
                    ty: ptr_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "d".to_string(),
                    ty: ptr_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
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

    /// When arms have different pre-cast types (e.g. signed vs unsigned or pointer vs integer),
    /// hoisting would corrupt extension or produce invalid types.
    #[test]
    fn keeps_arms_with_different_pre_cast_types() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };

        // Test 1: c ? (u64)i32_var : (u64)u32_var
        let mut f1 = PreHirFunction {
            name: "test1".to_string(),
            locals: vec![
                PreHirBinding {
                    name: "x".to_string(),
                    ty: u64_ty.clone(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "i32_var".to_string(),
                    ty: i32_ty,
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "u32_var".to_string(),
                    ty: u32_ty,
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
            body: vec![set(select(
                cast(u64_ty.clone(), var("i32_var")),
                cast(u64_ty.clone(), var("u32_var")),
                u64_ty.clone(),
            ))],
            ..Default::default()
        };
        assert!(!hoist_shared_select_casts(&mut f1));
        assert!(matches!(rhs_of(&f1), PreHirExpr::Select { .. }));

        // Test 2: c ? (u64)ptr_var : (u64)u64_var
        let mut f2 = PreHirFunction {
            name: "test2".to_string(),
            locals: vec![
                PreHirBinding {
                    name: "x".to_string(),
                    ty: u64_ty.clone(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "ptr_var".to_string(),
                    ty: ptr_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "u64_var".to_string(),
                    ty: u64_ty.clone(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
            body: vec![set(select(
                cast(u64_ty.clone(), var("ptr_var")),
                cast(u64_ty.clone(), var("u64_var")),
                u64_ty.clone(),
            ))],
            ..Default::default()
        };
        assert!(!hoist_shared_select_casts(&mut f2));
        assert!(matches!(rhs_of(&f2), PreHirExpr::Select { .. }));
    }
}
