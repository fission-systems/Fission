//! Redundant load elimination (RLE) for stack-slot loads.
//!
//! Within each sequentially executed region, if two `Load` operations read the
//! same [`AliasKey::Stack`] location with no intervening store to that key,
//! the second load is replaced with the variable that holds the first load’s
//! result.
//!
//! This complements [`super::dead_store`] (which removes dead **stores**) and
//! [`super::cse`] (which does not consider `Load` as part of the pure
//! expression key).  Unknown / heap pointers are never cached.
//!
//! At `if` / `while` / `switch` boundaries the cache is conservatively reset
//! (or forked empty) so we do not assume memory state merges across joins.
use super::mem_ssa::{
    AliasKey, alias_key_for_pointer_expr, field_access_ptr_expr, index_access_ptr_expr,
    nir_byte_size,
};
use crate::HashMap;
use crate::prelude::*;

type LoadCache = HashMap<AliasKey, String>;

/// Apply RLE to the function.  Returns `true` if any `Load` was replaced.
pub fn apply_redundant_load_elimination(func: &mut PreHirFunction) -> bool {
    let mut cache = LoadCache::default();
    rle_stmts(&mut func.body, &mut cache)
}

fn rle_stmts(stmts: &mut Vec<PreHirStmt>, cache: &mut LoadCache) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        if rle_stmt(stmt, cache) {
            changed = true;
        }
    }
    changed
}

fn rle_stmt(stmt: &mut PreHirStmt, cache: &mut LoadCache) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            rewrite_loads_in_expr(rhs, cache, &mut changed);
            if expr_has_call(rhs) {
                cache.clear();
            }
            match lhs {
                PreHirLValue::Deref { ptr, ty } => {
                    if expr_has_call(ptr) {
                        cache.clear();
                    } else {
                        let key = alias_key_for_pointer_expr(ptr, nir_byte_size(ty));
                        invalidate_load_cache(cache, &key);
                    }
                }
                PreHirLValue::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty,
                } => {
                    if expr_has_call(base) {
                        cache.clear();
                    } else {
                        let ptr = field_access_ptr_expr(base, field_name, *offset, ty);
                        let key = alias_key_for_pointer_expr(&ptr, nir_byte_size(ty));
                        invalidate_load_cache(cache, &key);
                    }
                }
                PreHirLValue::Index {
                    base,
                    index,
                    elem_ty,
                } => {
                    if expr_has_call(base) || expr_has_call(index) {
                        cache.clear();
                    } else {
                        let ptr = index_access_ptr_expr(base, index, elem_ty);
                        let key = alias_key_for_pointer_expr(&ptr, nir_byte_size(elem_ty));
                        invalidate_load_cache(cache, &key);
                    }
                }
                PreHirLValue::Var(name) => {
                    cache.retain(|k, v| {
                        v != name && !matches!(k, AliasKey::Partition(p) if p.base_object == *name)
                    });
                    if let PreHirExpr::Load { ptr, ty } = &*rhs {
                        let key = alias_key_for_pointer_expr(ptr.as_ref(), nir_byte_size(ty));
                        if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                        {
                            cache.insert(key, name.clone());
                        }
                    }
                }
            }
        }
        PreHirStmt::VaStart { va_list, .. } => {
            rewrite_loads_in_expr(va_list, cache, &mut changed);
        }
        PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => {
            rewrite_loads_in_expr(e, cache, &mut changed);
            if expr_has_call(e) {
                cache.clear();
            }
        }
        PreHirStmt::Return(None) => {
            cache.clear();
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
            let mut tc = LoadCache::default();
            let mut ec = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), &mut tc) {
                changed = true;
            }
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), &mut ec) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::While { cond: _, body } => {
            // The condition is evaluated again after every iteration of the
            // body, so a value cached before the loop is not the value it
            // reads the second time round. It is left as it is.
            let mut inner = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), &mut inner) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::DoWhile { body, cond } => {
            cache.clear();
            let mut inner = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), &mut inner) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::For {
            init,
            cond: _,
            update,
            body,
        } => {
            if let Some(s) = init {
                if rle_stmt(s, cache) {
                    changed = true;
                }
            }
            // As for `while`, the condition runs after every iteration and is
            // not rewritten from what was cached before the loop. The update
            // runs after the body, so it sees the body's own end state -- an
            // `if` in the body clears that, which also covers a `continue`.
            let mut inner = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), &mut inner) {
                changed = true;
            }
            if let Some(s) = update {
                if rle_stmt(s, &mut inner) {
                    changed = true;
                }
            }
            cache.clear();
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_loads_in_expr(expr, cache, &mut changed);
            for case in cases.iter_mut() {
                let mut c = LoadCache::default();
                if rle_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    &mut c,
                ) {
                    changed = true;
                }
            }
            let mut d = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), &mut d) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::Block(body) => {
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), cache) {
                changed = true;
            }
        }
        PreHirStmt::Label(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {
            cache.clear();
        }
    }
    changed
}

fn invalidate_load_cache(cache: &mut LoadCache, key: &AliasKey) {
    if matches!(key, AliasKey::Partition(partition) if partition.is_promotable_stack_like()) {
        cache.retain(|k, _| k.is_no_alias(key));
    } else {
        cache.clear();
    }
}

fn expr_has_call(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => expr_has_call(inner),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_has_call(lhs) || expr_has_call(rhs),
        PreHirExpr::Index { base, index, .. } => expr_has_call(base) || expr_has_call(index),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => expr_has_call(cond) || expr_has_call(then_expr) || expr_has_call(else_expr),
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
    }
}

fn rewrite_loads_in_expr(expr: &mut PreHirExpr, cache: &LoadCache, changed: &mut bool) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            let size = nir_byte_size(&ty);
            let key = alias_key_for_pointer_expr(ptr, size);
            if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
            {
                if let Some(v) = cache.get(&key) {
                    *expr = PreHirExpr::Var(v.clone());
                    *changed = true;
                    return;
                }
            }
            rewrite_loads_in_expr(ptr.as_mut(), cache, changed);
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            rewrite_loads_in_expr(inner.as_mut(), cache, changed)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_loads_in_expr(lhs.as_mut(), cache, changed);
            rewrite_loads_in_expr(rhs.as_mut(), cache, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                rewrite_loads_in_expr(a, cache, changed);
            }
        }
        PreHirExpr::PtrOffset { base, .. } => rewrite_loads_in_expr(base.as_mut(), cache, changed),
        PreHirExpr::Index { base, index, .. } => {
            rewrite_loads_in_expr(base.as_mut(), cache, changed);
            rewrite_loads_in_expr(index.as_mut(), cache, changed);
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            rewrite_loads_in_expr(src.as_mut(), cache, changed)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_loads_in_expr(cond.as_mut(), cache, changed);
            rewrite_loads_in_expr(then_expr.as_mut(), cache, changed);
            rewrite_loads_in_expr(else_expr.as_mut(), cache, changed);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    #[test]
    fn invalidates_cache_when_cached_variable_is_reassigned() {
        let mut func = PreHirFunction {
            name: "rle_reassign".into(),
            locals: vec![
                PreHirBinding {
                    name: "slot".into(),
                    ty: NirType::Ptr(Box::new(u32_ty())),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::StackOffset(-0x10)),
                    initializer: None,
                },
                PreHirBinding {
                    name: "v".into(),
                    ty: u32_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
                    name: "w".into(),
                    ty: u32_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("v".into()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("slot".into())),
                        ty: u32_ty(),
                    },
                },
                // v is reassigned with a different value
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("v".into()),
                    rhs: PreHirExpr::Const(99, u32_ty()),
                },
                // Subsequent load of slot must NOT be replaced with v
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("w".into()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("slot".into())),
                        ty: u32_ty(),
                    },
                },
            ],
            ..Default::default()
        };

        apply_redundant_load_elimination(&mut func);
        // The third statement must remain a Load, not Var("v")
        let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
            panic!()
        };
        assert!(
            matches!(rhs, PreHirExpr::Load { .. }),
            "reassigned v must not be substituted for later load"
        );
    }
    fn loop_fixture(body: Vec<PreHirStmt>) -> PreHirFunction {
        let local = |name: &str, ty: NirType, origin| PreHirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin,
            initializer: None,
        };
        PreHirFunction {
            name: "rle_loop".into(),
            locals: vec![
                local(
                    "local_10",
                    u32_ty(),
                    Some(NirBindingOrigin::StackOffset(-0x10)),
                ),
                local("w", u32_ty(), None),
                local("v", u32_ty(), None),
            ],
            body,
            ..Default::default()
        }
    }

    fn load_slot() -> PreHirExpr {
        PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::AddressOfLocal("local_10".into())),
            ty: u32_ty(),
        }
    }

    fn store_slot(value: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::AddressOfLocal("local_10".into())),
                ty: u32_ty(),
            },
            rhs: value,
        }
    }

    fn cache_v() -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("v".into()),
            rhs: load_slot(),
        }
    }

    fn lt_ten(lhs: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Lt,
            lhs: Box::new(lhs),
            rhs: Box::new(PreHirExpr::Const(10, u32_ty())),
            ty: NirType::Bool,
        }
    }

    fn mentions_v(expr: &PreHirExpr) -> bool {
        format!("{expr:?}").contains("Var(\"v\")")
    }

    /// `v = *slot; while (*slot < 10) { *slot = 5; }` -- the condition is
    /// evaluated again after every body, so it must keep reading the slot.
    #[test]
    fn a_while_condition_is_not_rewritten_from_before_the_loop() {
        let mut func = loop_fixture(vec![
            cache_v(),
            PreHirStmt::While {
                cond: lt_ten(load_slot()),
                body: std::rc::Rc::new(vec![store_slot(PreHirExpr::Const(5, u32_ty()))]),
            },
        ]);
        apply_redundant_load_elimination(&mut func);
        let PreHirStmt::While { cond, .. } = &func.body[1] else {
            panic!()
        };
        assert!(!mentions_v(cond), "the body rewrites the slot: {cond:?}");
    }

    /// The update runs after the body, so it sees the body's stores.
    #[test]
    fn a_for_update_sees_the_stores_of_the_body() {
        let mut func = loop_fixture(vec![PreHirStmt::For {
            init: Some(Box::new(cache_v())),
            cond: Some(lt_ten(PreHirExpr::Var("v".into()))),
            update: Some(Box::new(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("v".into()),
                rhs: load_slot(),
            })),
            body: std::rc::Rc::new(vec![store_slot(PreHirExpr::Const(5, u32_ty()))]),
        }]);
        apply_redundant_load_elimination(&mut func);
        let PreHirStmt::For { update, .. } = &func.body[0] else {
            panic!()
        };
        let Some(update) = update else { panic!() };
        let PreHirStmt::Assign { rhs, .. } = update.as_ref() else {
            panic!()
        };
        assert!(
            matches!(rhs, PreHirExpr::Load { .. }),
            "the body stores to the slot before the update runs: {rhs:?}"
        );
    }

    /// A loop that never touches the slot still lets its body reuse nothing
    /// from before it, but must not break the straight-line case.
    #[test]
    fn straight_line_reload_after_the_loop_is_not_reused_across_it() {
        let mut func = loop_fixture(vec![
            cache_v(),
            PreHirStmt::While {
                cond: lt_ten(PreHirExpr::Var("v".into())),
                body: std::rc::Rc::new(vec![store_slot(PreHirExpr::Const(5, u32_ty()))]),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("v".into()),
                rhs: load_slot(),
            },
        ]);
        apply_redundant_load_elimination(&mut func);
        let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
            panic!()
        };
        assert!(matches!(rhs, PreHirExpr::Load { .. }));
    }

    /// Positive control: without it the loop tests could pass because nothing
    /// is ever cached.
    #[test]
    fn control_a_straight_line_reload_is_reused() {
        let mut func = loop_fixture(vec![
            cache_v(),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("w".into()),
                rhs: load_slot(),
            },
        ]);
        apply_redundant_load_elimination(&mut func);
        let PreHirStmt::Assign { rhs, .. } = &func.body[1] else {
            panic!()
        };
        assert!(
            matches!(rhs, PreHirExpr::Var(name) if name == "v"),
            "{rhs:?}"
        );
    }

    #[test]
    fn field_access_and_index_stores_invalidate_overlapping_or_dynamic_cached_loads() {
        let local_20_plus_4 = || PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::AddressOfLocal("local_20".into())),
            offset: 4,
        };
        let load_plus_4 = |dst: &str| PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst.into()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(local_20_plus_4()),
                ty: u32_ty(),
            },
        };

        // Case 1: local_20.field_4 = 99 overlaps *(local_20 + 4), whereas local_20.field_0 does not.
        for (offset, expect_cached) in [(0u32, true), (4u32, false)] {
            let mut func = loop_fixture(vec![
                load_plus_4("w1"),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::FieldAccess {
                        base: Box::new(PreHirExpr::AddressOfLocal("local_20".into())),
                        field_name: format!("field_{offset}"),
                        offset,
                        ty: u32_ty(),
                    },
                    rhs: PreHirExpr::Const(99, u32_ty()),
                },
                load_plus_4("w2"),
            ]);
            apply_redundant_load_elimination(&mut func);
            let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
                panic!()
            };
            if expect_cached {
                assert!(matches!(rhs, PreHirExpr::Var(v) if v == "w1"), "{rhs:?}");
            } else {
                assert!(
                    matches!(rhs, PreHirExpr::Load { .. }),
                    "field_4 store must invalidate *(local_20 + 4): {rhs:?}"
                );
            }
        }

        // Case 2: local_20[1] = 99 (const index overlapping +4) and local_20[i] = 99 (dynamic index)
        // both invalidate *(local_20 + 4), whereas local_20[0] = 99 preserves it.
        for (index, expect_cached) in [
            (PreHirExpr::Const(0, u32_ty()), true),
            (PreHirExpr::Const(1, u32_ty()), false),
            (PreHirExpr::Var("i".into()), false),
        ] {
            let mut func = loop_fixture(vec![
                load_plus_4("w1"),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Index {
                        base: Box::new(PreHirExpr::AddressOfLocal("local_20".into())),
                        index: Box::new(index.clone()),
                        elem_ty: u32_ty(),
                    },
                    rhs: PreHirExpr::Const(99, u32_ty()),
                },
                load_plus_4("w2"),
            ]);
            apply_redundant_load_elimination(&mut func);
            let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
                panic!()
            };
            if expect_cached {
                assert!(matches!(rhs, PreHirExpr::Var(v) if v == "w1"), "{rhs:?}");
            } else {
                assert!(
                    matches!(rhs, PreHirExpr::Load { .. }),
                    "index {index:?} store must invalidate *(local_20 + 4): {rhs:?}"
                );
            }
        }
    }
}
