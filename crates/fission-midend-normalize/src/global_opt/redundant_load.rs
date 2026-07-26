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
use crate::prelude::*;
use super::mem_ssa::{AliasKey, alias_key_for_pointer_expr, nir_byte_size};
use crate::HashMap;

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
            match lhs {
                PreHirLValue::Deref { ptr, ty } => {
                    let key = alias_key_for_pointer_expr(ptr, nir_byte_size(ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.remove(&key);
                    }
                }
                PreHirLValue::FieldAccess { base, ty, .. } => {
                    let key = alias_key_for_pointer_expr(base, nir_byte_size(ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.remove(&key);
                    }
                }
                PreHirLValue::Index { base, elem_ty, .. } => {
                    let key = alias_key_for_pointer_expr(base, nir_byte_size(elem_ty));
                    if matches!(&key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
                    {
                        cache.remove(&key);
                    }
                }
                PreHirLValue::Var(name) => {
                    if let PreHirExpr::Load { ptr, ty } = &*rhs {
                        let key = alias_key_for_pointer_expr(ptr.as_ref(), nir_byte_size(&ty));
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
        }
        PreHirStmt::Return(None) => {}
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
            let mut tc = LoadCache::default();
            let mut ec = LoadCache::default();
            if rle_stmts(then_body, &mut tc) {
                changed = true;
            }
            if rle_stmts(else_body, &mut ec) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
            let mut inner = LoadCache::default();
            if rle_stmts(body, &mut inner) {
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
            if rle_stmts(body, &mut inner) {
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
                if rle_stmts(&mut case.body, &mut c) {
                    changed = true;
                }
            }
            let mut d = LoadCache::default();
            if rle_stmts(default, &mut d) {
                changed = true;
            }
            cache.clear();
        }
        PreHirStmt::Block(body) => {
            if rle_stmts(body, cache) {
                changed = true;
            }
        }
        PreHirStmt::Label(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {}
    }
    changed
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
        PreHirExpr::AggregateCopy { src, .. } => rewrite_loads_in_expr(src.as_mut(), cache, changed),
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
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}
