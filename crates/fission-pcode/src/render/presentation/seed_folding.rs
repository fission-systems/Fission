//! Folding of seed assignments into later pure updates.
//!
//! These presentation-only rewrites compress compiler-shaped temporary
//! updates while preserving the existing purity, alias, and memory barriers
//! supplied by the parent presentation module.

use super::*;

/// Fold `x = seed; x = f(x)` into `x = f(seed)`.
///
/// The generalisation of [`fold_self_update_after_seed`], which only matched
/// `x = x ⊕ rhs`. `iVar6 = ptr[1]; iVar6 = *(uint *)(iVar6 + ..)` is the
/// shape left over, and it was 6.4% of the readable layer's body lines --
/// the largest single pattern still unfolded, and the reason a temp stays
/// visible even after `inline_single_use_pure_assigns`, which refuses any
/// name defined more than once.
///
/// Requires exactly one mention of `x` in `f`: substituting into two would
/// duplicate the seed rather than compress it.
pub(super) fn fold_seed_transform(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < stmts.len() {
        // The redefinition need not be the next statement --
        // `iVar6 = ptr[1]; uVar8 = ..; iVar6 = *(iVar6 + ..)` is the common
        // shape. Scan forward to it, and only across a span that neither
        // reads `x`, redefines what the seed depends on, nor writes memory.
        let Some(j) = next_redefinition_of_seed(stmts, i) else {
            i += 1;
            continue;
        };
        let folded = match (&stmts[i], &stmts[j]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(x1),
                    rhs: seed,
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(x2),
                    rhs: transform,
                },
            ) if x1 == x2
                && (expr_is_presentation_pure(seed) || expr_is_movable_read(seed))
                && !expr_mentions_var(seed, x1)
                && count_var_mentions(transform, x1) == 1
                && (expr_is_presentation_pure(transform) || expr_is_movable_read(transform)) =>
            {
                let mut folded = transform.clone();
                substitute_var(&mut folded, x1, seed);
                Some(HirStmt::Assign {
                    lhs: HirLValue::Var(x1.clone()),
                    rhs: folded,
                })
            }
            _ => None,
        };
        if let Some(stmt) = folded {
            stmts[j] = stmt;
            stmts.remove(i);
            changed = true;
            continue;
        }
        i += 1;
    }
    for stmt in stmts.iter_mut() {
        changed |= fold_seed_transform_in_children(stmt);
    }
    changed
}

/// The index of the statement that redefines the variable `stmts[i]` assigns,
/// if the span between is safe to fold across.
fn next_redefinition_of_seed(stmts: &[HirStmt], i: usize) -> Option<usize> {
    let HirStmt::Assign {
        lhs: HirLValue::Var(name),
        rhs: seed,
    } = &stmts[i]
    else {
        return None;
    };
    let seed_is_pure = expr_is_presentation_pure(seed);
    for j in i + 1..stmts.len() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(target),
            ..
        } = &stmts[j]
            && target == name
        {
            if pure_expr_free_var_redefined_before(stmts, i + 1, j, seed) {
                return None;
            }
            if !seed_is_pure && memory_clobbered_between(stmts, i + 1, j) {
                return None;
            }
            return Some(j);
        }
        // Any earlier read of `x`, or anything that is not a plain local
        // assignment, ends the search: this fold moves the seed forward past
        // the span, and both would make that observable.
        if count_uses_in_stmt(&stmts[j], name) > 0
            || !matches!(
                &stmts[j],
                HirStmt::Assign {
                    lhs: HirLValue::Var(_),
                    ..
                }
            )
        {
            return None;
        }
    }
    None
}

fn fold_seed_transform_in_children(stmt: &mut HirStmt) -> bool {
    match stmt {
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            fold_seed_transform(body)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => fold_seed_transform(then_body) | fold_seed_transform(else_body),
        HirStmt::For { body, .. } => fold_seed_transform(body),
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                changed |= fold_seed_transform(&mut case.body);
            }
            changed | fold_seed_transform(default)
        }
        _ => false,
    }
}

/// Fold `x = seed; x = x ⊕ rhs` into `x = seed ⊕ rhs` when pure.
pub(super) fn fold_self_update_after_seed(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < stmts.len() {
        let folded = match (&stmts[i], &stmts[i + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(x1),
                    rhs: seed,
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(x2),
                    rhs:
                        HirExpr::Binary {
                            op,
                            lhs: bin_lhs,
                            rhs: bin_rhs,
                            ty,
                        },
                },
            ) if x1 == x2
                // The seed is folded into the very next statement, so no
                // span exists for a store to intervene -- a movable read is
                // as safe here as a pure expression.
                && (expr_is_presentation_pure(seed) || expr_is_movable_read(seed))
                && !expr_mentions_var(seed, x1)
                && matches!(bin_lhs.as_ref(), HirExpr::Var(n) if n == x1)
                && (expr_is_presentation_pure(bin_rhs) || expr_is_movable_read(bin_rhs))
                && !expr_mentions_var(bin_rhs, x1) =>
            {
                Some(HirStmt::Assign {
                    lhs: HirLValue::Var(x1.clone()),
                    rhs: HirExpr::Binary {
                        op: *op,
                        lhs: Box::new(seed.clone()),
                        rhs: bin_rhs.clone(),
                        ty: ty.clone(),
                    },
                })
            }
            _ => None,
        };
        if let Some(stmt) = folded {
            stmts[i] = stmt;
            stmts.remove(i + 1);
            changed = true;
            // Re-examine from same index in case of chains.
            continue;
        }
        i += 1;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= fold_self_update_after_seed(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_self_update_after_seed(then_body);
                changed |= fold_self_update_after_seed(else_body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= fold_self_update_after_seed(b);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= fold_self_update_after_seed(b);
                    }
                }
                changed |= fold_self_update_after_seed(body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_self_update_after_seed(&mut case.body);
                }
                changed |= fold_self_update_after_seed(default);
            }
            _ => {}
        }
    }
    changed
}
