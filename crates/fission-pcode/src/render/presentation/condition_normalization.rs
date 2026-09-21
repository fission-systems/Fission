//! Condition canonicalization and redundant select/return-join recovery.

use super::*;

/// Normalize condition/comparison presentation forms in place.
pub(super) fn canonicalize_presentation_conditions(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= canonicalize_conditions_in_stmt(stmt);
    }
    changed
}

fn canonicalize_conditions_in_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { rhs, .. } => {
            changed |= canonicalize_conditions_in_expr(rhs);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            changed |= canonicalize_conditions_in_expr(e);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            changed |= canonicalize_conditions_in_expr(cond);
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= canonicalize_conditions_in_expr(cond);
            for s in then_body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
            for s in else_body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(c) = cond {
                changed |= canonicalize_conditions_in_expr(c);
            }
            if let Some(i) = init {
                changed |= canonicalize_conditions_in_stmt(i);
            }
            if let Some(u) = update {
                changed |= canonicalize_conditions_in_stmt(u);
            }
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= canonicalize_conditions_in_expr(expr);
            for case in cases {
                for s in case.body.iter_mut() {
                    changed |= canonicalize_conditions_in_stmt(s);
                }
            }
            for s in default.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
    }
    changed
}

pub(super) fn canonicalize_conditions_in_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            changed |= canonicalize_conditions_in_expr(expr);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= canonicalize_conditions_in_expr(lhs);
            changed |= canonicalize_conditions_in_expr(rhs);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= canonicalize_conditions_in_expr(cond);
            changed |= canonicalize_conditions_in_expr(then_expr);
            changed |= canonicalize_conditions_in_expr(else_expr);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                changed |= canonicalize_conditions_in_expr(a);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => {
            changed |= canonicalize_conditions_in_expr(ptr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= canonicalize_conditions_in_expr(base);
            changed |= canonicalize_conditions_in_expr(index);
        }
    }
    // Post-order local rewrites so nested forms normalize first.
    changed |= rewrite_presentation_condition_form(expr);
    changed
}

fn rewrite_presentation_condition_form(expr: &mut HirExpr) -> bool {
    // `!(x == 0)` → `x != 0`, `!(x != 0)` → `x == 0`.
    // `!!e` only when the outer result is Bool (value `!!x` ≠ `x` for int x∉{0,1}).
    if let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr: inner,
        ty: outer_ty,
    } = expr
    {
        if matches!(outer_ty, NirType::Bool) {
            if let HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: inner2,
                ..
            } = inner.as_ref()
            {
                *expr = inner2.as_ref().clone();
                return true;
            }
            // De Morgan's: `!(A && B)` → `!A || !B`, `!(A || B)` → `!A && !B`.
            // Each operand still appears exactly once either way, so this
            // can't change evaluation count or order.
            if let HirExpr::Binary {
                op: op @ (HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr),
                lhs,
                rhs,
                ty,
            } = inner.as_ref()
            {
                let pushed_op = if *op == HirBinaryOp::LogicalAnd {
                    HirBinaryOp::LogicalOr
                } else {
                    HirBinaryOp::LogicalAnd
                };
                let new_lhs = HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: lhs.clone(),
                    ty: NirType::Bool,
                };
                let new_rhs = HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: rhs.clone(),
                    ty: NirType::Bool,
                };
                *expr = HirExpr::Binary {
                    op: pushed_op,
                    lhs: Box::new(new_lhs),
                    rhs: Box::new(new_rhs),
                    ty: ty.clone(),
                };
                return true;
            }
        }
        if let HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ty,
        } = inner.as_ref()
        {
            *expr = HirExpr::Binary {
                op: HirBinaryOp::Ne,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                ty: ty.clone(),
            };
            return true;
        }
        if let HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ty,
        } = inner.as_ref()
        {
            *expr = HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                ty: ty.clone(),
            };
            return true;
        }
    }

    // Const-left comparisons → var/expr-left with flipped op.
    if let HirExpr::Binary { op, lhs, rhs, ty } = expr {
        let lhs_is_const = matches!(lhs.as_ref(), HirExpr::Const(_, _));
        let rhs_is_const = matches!(rhs.as_ref(), HirExpr::Const(_, _));
        if lhs_is_const && !rhs_is_const {
            if let Some(flipped) = flip_comparison_op(*op) {
                let new_lhs = std::mem::replace(rhs.as_mut(), HirExpr::Const(0, NirType::Unknown));
                let new_rhs = std::mem::replace(lhs.as_mut(), HirExpr::Const(0, NirType::Unknown));
                *op = flipped;
                *lhs.as_mut() = new_lhs;
                *rhs.as_mut() = new_rhs;
                let _ = ty;
                return true;
            }
            if matches!(*op, HirBinaryOp::Eq | HirBinaryOp::Ne) {
                std::mem::swap(lhs, rhs);
                return true;
            }
        }
    }

    // `(a | b) == 0` → `(a == 0) && (b == 0)`; `(a | b) != 0` → `(a != 0) || (b != 0)`.
    // A bitwise-flags check reads as boolean logic instead of a mask compare.
    // `a` and `b` each still appear exactly once, so evaluation count is
    // unchanged; only reachable once the const-left flip above has already
    // put the zero constant on the right.
    if let HirExpr::Binary {
        op: cmp_op @ (HirBinaryOp::Eq | HirBinaryOp::Ne),
        lhs,
        rhs,
        ty,
    } = expr
    {
        if let (
            HirExpr::Binary {
                op: HirBinaryOp::Or,
                lhs: a,
                rhs: b,
                ..
            },
            HirExpr::Const(0, _),
        ) = (lhs.as_ref(), rhs.as_ref())
        {
            let new_left = HirExpr::Binary {
                op: *cmp_op,
                lhs: a.clone(),
                rhs: rhs.clone(),
                ty: NirType::Bool,
            };
            let new_right = HirExpr::Binary {
                op: *cmp_op,
                lhs: b.clone(),
                rhs: rhs.clone(),
                ty: NirType::Bool,
            };
            let logic_op = if *cmp_op == HirBinaryOp::Eq {
                HirBinaryOp::LogicalAnd
            } else {
                HirBinaryOp::LogicalOr
            };
            *expr = HirExpr::Binary {
                op: logic_op,
                lhs: Box::new(new_left),
                rhs: Box::new(new_right),
                ty: ty.clone(),
            };
            return true;
        }
    }

    // `Select(cond, a, b) == a` → `cond`; `Select(cond, a, b) == b` → `!cond`
    // (and negated for `!=`), restricted to `a`/`b` both being the exact
    // constant being compared against -- constants have no side effects, so
    // dropping their (and the select's) evaluation can't change observable
    // behavior, unlike doing this for arbitrary `a`/`b`.
    if let HirExpr::Binary {
        op: cmp_op @ (HirBinaryOp::Eq | HirBinaryOp::Ne),
        lhs,
        rhs,
        ..
    } = expr
    {
        if let (
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            },
            HirExpr::Const(rhs_val, _),
        ) = (lhs.as_ref(), rhs.as_ref())
        {
            let then_matches = matches!(then_expr.as_ref(), HirExpr::Const(v, _) if v == rhs_val);
            let else_matches = matches!(else_expr.as_ref(), HirExpr::Const(v, _) if v == rhs_val);
            let negate = match (*cmp_op, then_matches, else_matches) {
                (HirBinaryOp::Eq, true, _) => Some(false),
                (HirBinaryOp::Eq, false, true) => Some(true),
                (HirBinaryOp::Ne, true, _) => Some(true),
                (HirBinaryOp::Ne, false, true) => Some(false),
                _ => None,
            };
            if let Some(negate) = negate {
                *expr = if negate {
                    HirExpr::Unary {
                        op: HirUnaryOp::Not,
                        expr: cond.clone(),
                        ty: NirType::Bool,
                    }
                } else {
                    cond.as_ref().clone()
                };
                return true;
            }
        }
    }

    false
}

fn flip_comparison_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    Some(match op {
        HirBinaryOp::Lt => HirBinaryOp::Gt,
        HirBinaryOp::Le => HirBinaryOp::Ge,
        HirBinaryOp::Gt => HirBinaryOp::Lt,
        HirBinaryOp::Ge => HirBinaryOp::Le,
        HirBinaryOp::SLt => HirBinaryOp::SGt,
        HirBinaryOp::SLe => HirBinaryOp::SGe,
        HirBinaryOp::SGt => HirBinaryOp::SLt,
        HirBinaryOp::SGe => HirBinaryOp::SLe,
        _ => return None,
    })
}

pub(super) fn body_is_effectively_empty(stmts: &[HirStmt]) -> bool {
    stmts.iter().all(|s| match s {
        HirStmt::Block(inner) => body_is_effectively_empty(inner),
        HirStmt::Label(_) => true,
        _ => false,
    })
}

fn is_presentation_noise_stmt(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Label(_) => true,
        HirStmt::Block(b) if b.is_empty() => true,
        _ => false,
    }
}

pub(super) fn single_var_assign(stmts: &[HirStmt]) -> Option<(&str, &HirExpr)> {
    let meaningful: Vec<&HirStmt> = stmts
        .iter()
        .filter(|s| !is_presentation_noise_stmt(s))
        .collect();
    match meaningful.as_slice() {
        [
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            },
        ] => Some((name.as_str(), rhs)),
        [HirStmt::Block(inner)] => single_var_assign(inner),
        _ => None,
    }
}

pub(super) fn single_return_expr(stmts: &[HirStmt]) -> Option<&HirExpr> {
    let meaningful: Vec<&HirStmt> = stmts
        .iter()
        .filter(|s| !is_presentation_noise_stmt(s))
        .collect();
    match meaningful.as_slice() {
        [HirStmt::Return(Some(expr))] => Some(expr),
        [HirStmt::Block(inner)] => single_return_expr(inner),
        _ => None,
    }
}

pub(super) fn expr_result_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Cast { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } | HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) => {
            NirType::Ptr(Box::new(NirType::Unknown))
        }
        HirExpr::Var(_) | HirExpr::AggregateCopy { .. } => NirType::Unknown,
    }
}

/// Fold null-check join sugar:
/// `x = c ? a : b; return ~c ? x : a` → `return x` (then collapse even with call).
pub(super) fn fold_redundant_select_return_join(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_redundant_select_return_join(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_redundant_select_return_join(then_body);
                changed |= fold_redundant_select_return_join(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_redundant_select_return_join(&mut case.body);
                }
                changed |= fold_redundant_select_return_join(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let can_fold_to_x = match (&stmts[i], &stmts[i + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(x),
                    rhs:
                        HirExpr::Select {
                            cond: c1,
                            then_expr: t1,
                            else_expr: _e1,
                            ..
                        },
                },
                HirStmt::Return(Some(HirExpr::Select {
                    cond: c2,
                    then_expr: t2,
                    else_expr: e2,
                    ..
                })),
            ) => {
                let x_var = |e: &HirExpr| matches!(e, HirExpr::Var(n) if n == x);
                // apply_binop: x = !p ? 0 : call; return !(p==0) ? x : 0
                // c1 nullish, c2 non-nullish, t2=x, e2=t1
                (cond_are_negations(c1, c2) && x_var(t2) && t1.as_ref() == e2.as_ref())
                    // x = c ? a : b; return c ? a : x
                    || (cond_logically_same(c1, c2) && t1.as_ref() == t2.as_ref() && x_var(e2))
                    // x = c ? a : b; return ~c ? a : x  (less common)
                    || (cond_are_negations(c1, c2) && t1.as_ref() == t2.as_ref() && x_var(e2))
            }
            _ => false,
        };
        if can_fold_to_x {
            if let HirStmt::Assign {
                lhs: HirLValue::Var(x),
                ..
            } = &stmts[i]
            {
                let x = x.clone();
                stmts[i + 1] = HirStmt::Return(Some(HirExpr::Var(x)));
                changed = true;
            }
        }
        // Collapse `x = rhs; return x` for any rhs (call/select safe: single eval).
        if let (
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            },
            HirStmt::Return(Some(HirExpr::Var(ret_name))),
        ) = (&stmts[i], &stmts[i + 1])
        {
            if name == ret_name {
                stmts[i] = HirStmt::Return(Some(rhs.clone()));
                stmts.remove(i + 1);
                changed = true;
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn cond_logically_same(a: &HirExpr, b: &HirExpr) -> bool {
    if a == b {
        return true;
    }
    normalize_truthiness(a) == normalize_truthiness(b)
}

fn cond_are_negations(a: &HirExpr, b: &HirExpr) -> bool {
    if let Some(inner) = peel_logical_not(a) {
        if cond_logically_same(&inner, b) {
            return true;
        }
    }
    if let Some(inner) = peel_logical_not(b) {
        if cond_logically_same(&inner, a) {
            return true;
        }
    }
    match (normalize_truthiness(a), normalize_truthiness(b)) {
        (Truthiness::Zero(x), Truthiness::NonZero(y))
        | (Truthiness::NonZero(x), Truthiness::Zero(y)) => x == y,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Truthiness {
    /// Expression is true when `var == 0` / nullish.
    Zero(String),
    /// Expression is true when `var != 0` / non-null.
    NonZero(String),
    Other,
}

fn normalize_truthiness(expr: &HirExpr) -> Truthiness {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => match normalize_truthiness(expr) {
            Truthiness::Zero(v) => Truthiness::NonZero(v),
            Truthiness::NonZero(v) => Truthiness::Zero(v),
            Truthiness::Other => Truthiness::Other,
        },
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (HirExpr::Var(v), HirExpr::Const(0, _)) | (HirExpr::Const(0, _), HirExpr::Var(v)) => {
                Truthiness::Zero(v.clone())
            }
            _ => Truthiness::Other,
        },
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (HirExpr::Var(v), HirExpr::Const(0, _)) | (HirExpr::Const(0, _), HirExpr::Var(v)) => {
                Truthiness::NonZero(v.clone())
            }
            _ => Truthiness::Other,
        },
        // Bare `var` used as condition ⇒ non-zero / non-null.
        HirExpr::Var(v) => Truthiness::NonZero(v.clone()),
        // `!var` handled above via Unary Not.
        HirExpr::Cast { expr, .. } => normalize_truthiness(expr),
        _ => Truthiness::Other,
    }
}

fn peel_logical_not(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => Some(expr.as_ref().clone()),
        _ => None,
    }
}
