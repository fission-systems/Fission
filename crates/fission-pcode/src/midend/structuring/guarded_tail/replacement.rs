use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConditionAssumption {
    pub(super) expr: HirExpr,
    pub(super) value: bool,
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::expr_contains_var(expr, name)
    }


    pub(super) fn lvalue_contains_var(lhs: &HirLValue, name: &str) -> bool {
        fission_midend_structuring::guarded_tail_pure::lvalue_contains_var(lhs, name)
    }


    pub(super) fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_expr(expr, name, replacement)
    }


    fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_lvalue(lhs, name, replacement)
    }


    pub(super) fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
        fission_midend_structuring::guarded_tail_pure::replace_var_in_stmt(stmt, name, replacement)
    }


    pub(super) fn count_var_defs_stmt(stmt: &HirStmt, target: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_defs_stmt(stmt, target)
    }


    fn count_var_reads_expr(expr: &HirExpr, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_expr(expr, name)
    }


    fn count_var_reads_lvalue(lhs: &HirLValue, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_lvalue(lhs, name)
    }


    pub(super) fn count_var_reads_stmt(stmt: &HirStmt, name: &str) -> usize {
        fission_midend_structuring::guarded_tail_pure::count_var_reads_stmt(stmt, name)
    }


    pub(super) fn find_guarded_tail_preexisting_source(
        body: &[HirStmt],
        if_idx: usize,
        binding_name: &str,
    ) -> Option<HirExpr> {
        for stmt in body[..if_idx].iter().rev() {
            match stmt {
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    rhs,
                } if name == binding_name && Self::expr_is_pure_value(rhs) => {
                    return Some(rhs.clone());
                }
                HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue
                | HirStmt::If { .. }
                | HirStmt::Switch { .. }
                | HirStmt::While { .. }
                | HirStmt::DoWhile { .. }
                | HirStmt::For { .. } => return None,
                HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Assign { .. }
                | HirStmt::VaStart { .. }
                | HirStmt::Expr(_)
                | HirStmt::Block(_) => {}
            }
        }
        None
    }

    pub(super) fn resolve_guarded_tail_else_source(
        body: &[HirStmt],
        if_idx: usize,
        binding_name: &str,
        cache: &mut GuardedTailReplacementCache,
    ) -> Option<HirExpr> {
        if let Some(expr) = cache.else_sources.get(binding_name) {
            return Some(expr.clone());
        }
        let expr = Self::find_guarded_tail_preexisting_source(body, if_idx, binding_name)?;
        cache
            .else_sources
            .insert(binding_name.to_string(), expr.clone());
        Some(expr)
    }

    pub(super) fn classify_stmt_read_kind(
        stmt: &HirStmt,
        name: &str,
    ) -> Option<GuardedTailReadKind> {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if Self::expr_contains_var(rhs, name) {
                    Some(GuardedTailReadKind::AssignRhs)
                } else if Self::lvalue_contains_var(lhs, name) {
                    Some(GuardedTailReadKind::NestedExpr)
                } else {
                    None
                }
            }
            HirStmt::Expr(HirExpr::Call { args, .. })
                if args.iter().any(|arg| Self::expr_contains_var(arg, name)) =>
            {
                Some(GuardedTailReadKind::CallArg)
            }
            HirStmt::Expr(expr) if Self::expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::NestedExpr)
            }
            HirStmt::Expr(_) => None,
            HirStmt::If { cond, .. } if Self::expr_contains_var(cond, name) => {
                Some(GuardedTailReadKind::ConditionExpr)
            }
            HirStmt::Switch { expr, .. } if Self::expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::SwitchSelector)
            }
            HirStmt::Return(Some(expr)) if Self::expr_contains_var(expr, name) => {
                Some(GuardedTailReadKind::ReturnExpr)
            }
            HirStmt::Return(_) => None,
            HirStmt::Block(stmts) | HirStmt::While { body: stmts, .. } => stmts
                .iter()
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name)),
            HirStmt::DoWhile { body, cond } => body
                .iter()
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name))
                .or_else(|| {
                    if Self::expr_contains_var(cond, name) {
                        Some(GuardedTailReadKind::ConditionExpr)
                    } else {
                        None
                    }
                }),
            HirStmt::Switch { cases, default, .. } => cases
                .iter()
                .flat_map(|case| case.body.iter())
                .chain(default.iter())
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name)),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => then_body
                .iter()
                .chain(else_body.iter())
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name)),
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => init
                .iter()
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name))
                .or_else(|| {
                    cond.as_ref()
                        .filter(|expr| Self::expr_contains_var(expr, name))
                        .map(|_| GuardedTailReadKind::ConditionExpr)
                })
                .or_else(|| {
                    update
                        .iter()
                        .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name))
                })
                .or_else(|| {
                    body.iter()
                        .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name))
                }),
            HirStmt::VaStart { va_list, .. } if Self::expr_contains_var(va_list, name) => {
                Some(GuardedTailReadKind::NestedExpr)
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Break
            | HirStmt::Continue
            | HirStmt::VaStart { .. } => None,
        }
    }

    fn condition_matches_assumption(
        expr: &HirExpr,
        assumption: &ConditionAssumption,
    ) -> Option<bool> {
        if expr == &assumption.expr {
            return Some(assumption.value);
        }
        if let HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: inner,
            ..
        } = expr
            && inner.as_ref() == &assumption.expr
        {
            return Some(!assumption.value);
        }
        if let HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: inner,
            ..
        } = &assumption.expr
            && inner.as_ref() == expr
        {
            return Some(!assumption.value);
        }
        None
    }

    pub(super) fn evaluate_condition_assumptions(
        expr: &HirExpr,
        assumptions: &[ConditionAssumption],
    ) -> Option<bool> {
        assumptions
            .iter()
            .find_map(|assumption| Self::condition_matches_assumption(expr, assumption))
    }

    pub(super) fn local_forward_branch_target(
        then_body: &[HirStmt],
        else_body: &[HirStmt],
    ) -> Option<(String, bool)> {
        if let Some(label) = single_goto_target(then_body)
            && else_body.is_empty()
        {
            return Some((label.to_string(), true));
        }
        if let Some(label) = single_goto_target(else_body)
            && then_body.is_empty()
        {
            return Some((label.to_string(), false));
        }
        None
    }
}
