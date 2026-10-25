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
use super::mem_ssa::{AliasKey, alias_key_for_pointer_expr, nir_byte_size};
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
                    let key = alias_key_for_pointer_expr(ptr, nir_byte_size(ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.retain(|k, _| k.is_no_alias(&key));
                    } else {
                        cache.clear();
                    }
                }
                PreHirLValue::FieldAccess { base, ty, .. } => {
                    let key = alias_key_for_pointer_expr(base, nir_byte_size(ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.retain(|k, _| k.is_no_alias(&key));
                    } else {
                        cache.clear();
                    }
                }
                PreHirLValue::Index { base, elem_ty, .. } => {
                    let key = alias_key_for_pointer_expr(base, nir_byte_size(elem_ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.retain(|k, _| k.is_no_alias(&key));
                    } else {
                        cache.clear();
                    }
                }
                PreHirLValue::Var(name) => {
                    cache.retain(|_, v| v != name);
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
        PreHirStmt::While { cond, body } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
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
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                if rle_stmt(s, cache) {
                    changed = true;
                }
            }
            if let Some(e) = cond {
                rewrite_loads_in_expr(e, cache, &mut changed);
            }
            let mut inner = LoadCache::default();
            if rle_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), &mut inner) {
                changed = true;
            }
            if let Some(s) = update {
                if rle_stmt(s, cache) {
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
}
