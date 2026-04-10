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
use super::mem_ssa::{alias_key_for_pointer_expr, nir_byte_size, AliasKey};
use super::super::*;
use std::collections::HashMap;

type LoadCache = HashMap<AliasKey, String>;

/// Apply RLE to the function.  Returns `true` if any `Load` was replaced.
pub(crate) fn apply_redundant_load_elimination(func: &mut HirFunction) -> bool {
    let mut cache = LoadCache::new();
    rle_stmts(&mut func.body, &mut cache)
}

fn rle_stmts(stmts: &mut Vec<HirStmt>, cache: &mut LoadCache) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        if rle_stmt(stmt, cache) {
            changed = true;
        }
    }
    changed
}

fn rle_stmt(stmt: &mut HirStmt, cache: &mut LoadCache) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            rewrite_loads_in_expr(rhs, cache, &mut changed);
            match lhs {
                HirLValue::Deref { ptr, ty } => {
                    let key = alias_key_for_pointer_expr(ptr, nir_byte_size(ty));
                    if matches!(key, AliasKey::Stack { .. }) {
                        cache.remove(&key);
                    }
                }
                HirLValue::Index { base, elem_ty, .. } => {
                    let key = alias_key_for_pointer_expr(base, nir_byte_size(elem_ty));
                    if matches!(key, AliasKey::Stack { .. }) {
                        cache.remove(&key);
                    }
                }
                HirLValue::Var(name) => {
                    if let HirExpr::Load { ptr, ty } = &*rhs {
                        let key = alias_key_for_pointer_expr(ptr.as_ref(), nir_byte_size(&ty));
                        if matches!(key, AliasKey::Stack { .. }) {
                            cache.insert(key, name.clone());
                        }
                    }
                }
            }
        }
        HirStmt::VaStart { va_list, .. } => {
            rewrite_loads_in_expr(va_list, cache, &mut changed);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => {
            rewrite_loads_in_expr(e, cache, &mut changed);
        }
        HirStmt::Return(None) => {}
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
            let mut tc = LoadCache::new();
            let mut ec = LoadCache::new();
            if rle_stmts(then_body, &mut tc) {
                changed = true;
            }
            if rle_stmts(else_body, &mut ec) {
                changed = true;
            }
            cache.clear();
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            rewrite_loads_in_expr(cond, cache, &mut changed);
            let mut inner = LoadCache::new();
            if rle_stmts(body, &mut inner) {
                changed = true;
            }
            cache.clear();
        }
        HirStmt::For {
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
            let mut inner = LoadCache::new();
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
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_loads_in_expr(expr, cache, &mut changed);
            for case in cases.iter_mut() {
                let mut c = LoadCache::new();
                if rle_stmts(&mut case.body, &mut c) {
                    changed = true;
                }
            }
            let mut d = LoadCache::new();
            if rle_stmts(default, &mut d) {
                changed = true;
            }
            cache.clear();
        }
        HirStmt::Block(body) => {
            if rle_stmts(body, cache) {
                changed = true;
            }
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
    changed
}

fn rewrite_loads_in_expr(expr: &mut HirExpr, cache: &LoadCache, changed: &mut bool) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            let size = nir_byte_size(&ty);
            let key = alias_key_for_pointer_expr(ptr, size);
            if let AliasKey::Stack { .. } = &key {
                if let Some(v) = cache.get(&key) {
                    *expr = HirExpr::Var(v.clone());
                    *changed = true;
                    return;
                }
            }
            rewrite_loads_in_expr(ptr.as_mut(), cache, changed);
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. } => rewrite_loads_in_expr(inner.as_mut(), cache, changed),
        HirExpr::Binary { lhs, rhs, .. } => {
            rewrite_loads_in_expr(lhs.as_mut(), cache, changed);
            rewrite_loads_in_expr(rhs.as_mut(), cache, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                rewrite_loads_in_expr(a, cache, changed);
            }
        }
        HirExpr::PtrOffset { base, .. } => rewrite_loads_in_expr(base.as_mut(), cache, changed),
        HirExpr::Index { base, index, .. } => {
            rewrite_loads_in_expr(base.as_mut(), cache, changed);
            rewrite_loads_in_expr(index.as_mut(), cache, changed);
        }
        HirExpr::AggregateCopy { src, .. } => rewrite_loads_in_expr(src.as_mut(), cache, changed),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}
