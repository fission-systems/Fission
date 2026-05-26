use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConditionAssumption {
    pub(super) expr: HirExpr,
    pub(super) value: bool,
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
        match expr {
            HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => var == name,
            HirExpr::Const(_, _) => false,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::expr_contains_var(expr, name),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_contains_var(lhs, name) || Self::expr_contains_var(rhs, name)
            }
            HirExpr::Call { args, .. } => args.iter().any(|arg| Self::expr_contains_var(arg, name)),
            HirExpr::Index { base, index, .. } => {
                Self::expr_contains_var(base, name) || Self::expr_contains_var(index, name)
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_contains_var(cond, name)
                    || Self::expr_contains_var(then_expr, name)
                    || Self::expr_contains_var(else_expr, name)
            }
        }
    }

    pub(super) fn lvalue_contains_var(lhs: &HirLValue, name: &str) -> bool {
        match lhs {
            HirLValue::Var(_) => false,
            HirLValue::Deref { ptr, .. } => Self::expr_contains_var(ptr, name),
            HirLValue::Index { base, index, .. } => {
                Self::expr_contains_var(base, name) || Self::expr_contains_var(index, name)
            }
            HirLValue::FieldAccess { base, .. } => Self::expr_contains_var(base, name),
        }
    }

    pub(super) fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
        match expr {
            HirExpr::Var(var) if var == name => *expr = replacement.clone(),
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                Self::replace_var_in_expr(expr, name, replacement);
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::replace_var_in_expr(lhs, name, replacement);
                Self::replace_var_in_expr(rhs, name, replacement);
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    Self::replace_var_in_expr(arg, name, replacement);
                }
            }
            HirExpr::Index { base, index, .. } => {
                Self::replace_var_in_expr(base, name, replacement);
                Self::replace_var_in_expr(index, name, replacement);
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::replace_var_in_expr(cond, name, replacement);
                Self::replace_var_in_expr(then_expr, name, replacement);
                Self::replace_var_in_expr(else_expr, name, replacement);
            }
        }
    }

    fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
        match lhs {
            HirLValue::Var(_) => {}
            HirLValue::Deref { ptr, .. } => Self::replace_var_in_expr(ptr, name, replacement),
            HirLValue::Index { base, index, .. } => {
                Self::replace_var_in_expr(base, name, replacement);
                Self::replace_var_in_expr(index, name, replacement);
            }
            HirLValue::FieldAccess { base, .. } => {
                Self::replace_var_in_expr(base, name, replacement);
            }
        }
    }

    pub(super) fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                Self::replace_var_in_lvalue(lhs, name, replacement);
                Self::replace_var_in_expr(rhs, name, replacement);
            }
            HirStmt::VaStart { va_list, .. } => {
                Self::replace_var_in_expr(va_list, name, replacement)
            }
            HirStmt::Expr(expr) => Self::replace_var_in_expr(expr, name, replacement),
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::While { cond, body } => {
                Self::replace_var_in_expr(cond, name, replacement);
                for stmt in body {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::DoWhile { body, cond } => {
                for stmt in body {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
                Self::replace_var_in_expr(cond, name, replacement);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                Self::replace_var_in_expr(expr, name, replacement);
                for case in cases {
                    for stmt in &mut case.body {
                        Self::replace_var_in_stmt(stmt, name, replacement);
                    }
                }
                for stmt in default {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                Self::replace_var_in_expr(cond, name, replacement);
                for stmt in then_body {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
                for stmt in else_body {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    Self::replace_var_in_stmt(init_stmt, name, replacement);
                }
                if let Some(cond) = cond {
                    Self::replace_var_in_expr(cond, name, replacement);
                }
                if let Some(update_stmt) = update {
                    Self::replace_var_in_stmt(update_stmt, name, replacement);
                }
                for stmt in body {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::Return(Some(expr)) => Self::replace_var_in_expr(expr, name, replacement),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    pub(super) fn count_var_defs_stmt(stmt: &HirStmt, target: &str) -> usize {
        match stmt {
            HirStmt::Assign { lhs, .. } => {
                usize::from(matches!(lhs, HirLValue::Var(name) if name == target))
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                .sum(),
            HirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body
                    .iter()
                    .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                    .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                init.iter()
                    .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                    .sum::<usize>()
                    + update
                        .iter()
                        .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
                    + body
                        .iter()
                        .map(|stmt| Self::count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    fn count_var_reads_expr(expr: &HirExpr, name: &str) -> usize {
        match expr {
            HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
            HirExpr::Const(_, _) => 0,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::count_var_reads_expr(expr, name),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::count_var_reads_expr(lhs, name) + Self::count_var_reads_expr(rhs, name)
            }
            HirExpr::Call { args, .. } => args
                .iter()
                .map(|arg| Self::count_var_reads_expr(arg, name))
                .sum(),
            HirExpr::Index { base, index, .. } => {
                Self::count_var_reads_expr(base, name) + Self::count_var_reads_expr(index, name)
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::count_var_reads_expr(cond, name)
                    + Self::count_var_reads_expr(then_expr, name)
                    + Self::count_var_reads_expr(else_expr, name)
            }
        }
    }

    fn count_var_reads_lvalue(lhs: &HirLValue, name: &str) -> usize {
        match lhs {
            HirLValue::Var(_) => 0,
            HirLValue::Deref { ptr, .. } => Self::count_var_reads_expr(ptr, name),
            HirLValue::Index { base, index, .. } => {
                Self::count_var_reads_expr(base, name) + Self::count_var_reads_expr(index, name)
            }
            HirLValue::FieldAccess { base, .. } => Self::count_var_reads_expr(base, name),
        }
    }

    pub(super) fn count_var_reads_stmt(stmt: &HirStmt, name: &str) -> usize {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                Self::count_var_reads_lvalue(lhs, name) + Self::count_var_reads_expr(rhs, name)
            }
            HirStmt::VaStart { va_list, .. } => Self::count_var_reads_expr(va_list, name),
            HirStmt::Expr(expr) => Self::count_var_reads_expr(expr, name),
            HirStmt::Block(stmts) | HirStmt::While { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                .sum(),
            HirStmt::DoWhile { body, cond } => {
                body.iter()
                    .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                    .sum::<usize>()
                    + Self::count_var_reads_expr(cond, name)
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                Self::count_var_reads_expr(expr, name)
                    + cases
                        .iter()
                        .map(|case| {
                            case.body
                                .iter()
                                .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                                .sum::<usize>()
                        })
                        .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                Self::count_var_reads_expr(cond, name)
                    + then_body
                        .iter()
                        .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.iter()
                    .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                    .sum::<usize>()
                    + cond
                        .as_ref()
                        .map(|expr| Self::count_var_reads_expr(expr, name))
                        .unwrap_or(0)
                    + update
                        .iter()
                        .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
                    + body
                        .iter()
                        .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            HirStmt::Return(Some(expr)) => Self::count_var_reads_expr(expr, name),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
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
                HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue
                | HirStmt::If { .. }
                | HirStmt::Switch { .. }
                | HirStmt::While { .. }
                | HirStmt::DoWhile { .. }
                | HirStmt::For { .. } => return None,
                HirStmt::Assign { .. }
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
