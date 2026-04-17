use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct GuardedTailRewriteResult {
    stmts: Vec<HirStmt>,
    exits_to_join: bool,
    unresolved_join_refs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConditionAssumption {
    expr: HirExpr,
    value: bool,
}

impl<'a> PreviewBuilder<'a> {
    fn guarded_tail_diag_enabled() -> bool {
        std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
    }

    fn map_guarded_tail_canonicalization_rejection(
        reason: GuardedTailCanonicalizationFailure,
    ) -> GuardedTailWitnessRejection {
        match reason {
            GuardedTailCanonicalizationFailure::InterleavedJoinUses
            | GuardedTailCanonicalizationFailure::AliasNotFallthrough
            | GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors
            | GuardedTailCanonicalizationFailure::AliasHasNonlocalRef
            | GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                GuardedTailWitnessRejection::AliasInterleaveConflict
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                GuardedTailWitnessRejection::AmbiguousFollow
            }
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries
            | GuardedTailCanonicalizationFailure::NestedTailEscape => {
                GuardedTailWitnessRejection::NonCanonicalLayout
            }
        }
    }

    fn map_promotion_gate_rejection(reason: PromotionGateRejection) -> GuardedTailWitnessRejection {
        match reason {
            PromotionGateRejection::MustEmitLabel
            | PromotionGateRejection::MustEmitLabelSurvivingMiddleRef
            | PromotionGateRejection::MustEmitLabelSurvivingExternalRef
            | PromotionGateRejection::MustEmitLabelOwnerConflict
            | PromotionGateRejection::ExternalEntry
            | PromotionGateRejection::NotSinglePredSucc
            | PromotionGateRejection::LoopOrSwitchTarget => {
                GuardedTailWitnessRejection::SideEntryConflict
            }
        }
    }

    pub(super) fn classify_must_emit_label_rejection(
        body: &[HirStmt],
        middle: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
        outside_refs: usize,
        middle_refs: usize,
    ) -> Option<PromotionGateRejection> {
        let effective_middle_refs = PreviewBuilder::effective_middle_refs_for_promotion(
            middle,
            label,
            middle_refs,
        );
        if effective_middle_refs > 0 {
            return Some(PromotionGateRejection::MustEmitLabelSurvivingMiddleRef);
        }
        if outside_refs > 1 {
            if Self::outside_refs_preserve_forward_owner(body, if_idx, label_idx, label) {
                return Some(PromotionGateRejection::MustEmitLabelSurvivingExternalRef);
            }
            return Some(PromotionGateRejection::MustEmitLabelOwnerConflict);
        }
        if outside_refs == 1 {
            if Self::outside_refs_are_elidable_next_flow(body, if_idx, label_idx, label) {
                return None;
            }
            return Some(PromotionGateRejection::MustEmitLabelSurvivingExternalRef);
        }
        None
    }

    pub(super) fn mark_promotion_shape_rejection(&mut self, reason: PromotionShapeRejection) {
        self.promotion_rejected_by_shape_count += 1;
        match reason {
            PromotionShapeRejection::MissingTerminalJoinTarget => {
                self.promotion_rejected_by_shape_missing_terminal_join_target_count += 1;
            }
            PromotionShapeRejection::EmptyNonterminalTail => {
                self.promotion_rejected_by_shape_empty_nonterminal_tail_count += 1;
            }
        }
    }

    pub(super) fn mark_noncanonical_layout_rejection(&mut self) {
        self.discovery_rejected_noncanonical_layout_count += 1;
        self.promotion_rejected_by_shape_count += 1;
    }

    pub(super) fn mark_guarded_tail_witness_rejection(
        &mut self,
        reason: GuardedTailWitnessRejection,
    ) {
        match reason {
            GuardedTailWitnessRejection::MissingTerminalJoin => {
                self.guarded_tail_rejected_missing_terminal_join_count += 1;
            }
            GuardedTailWitnessRejection::SideEntryConflict => {
                self.guarded_tail_rejected_side_entry_conflict_count += 1;
            }
            GuardedTailWitnessRejection::AliasInterleaveConflict => {
                self.guarded_tail_rejected_alias_interleave_conflict_count += 1;
            }
            GuardedTailWitnessRejection::AmbiguousFollow => {
                self.guarded_tail_rejected_ambiguous_follow_count += 1;
            }
            GuardedTailWitnessRejection::NonCanonicalLayout => {}
        }
    }

    fn mark_guarded_tail_execution_rejection(
        &mut self,
        reason: GuardedTailExecutionRejection,
    ) {
        match reason {
            GuardedTailExecutionRejection::Witness(reason) => {
                self.mark_guarded_tail_witness_rejection(reason);
            }
            GuardedTailExecutionRejection::ReplacementIncomplete => {
                self.region_emit_ready_failed_count += 1;
                self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
            }
            GuardedTailExecutionRejection::MustEmitLabelConflict => {
                self.region_emit_ready_failed_count += 1;
                self.guarded_tail_replacement_plan_rejected_unstable_read_count += 1;
            }
        }
    }

    fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
        match expr {
            HirExpr::Var(var) => var == name,
            HirExpr::Const(_, _) => false,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::expr_contains_var(expr, name),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_contains_var(lhs, name) || Self::expr_contains_var(rhs, name)
            }
            HirExpr::Call { args, .. } => args.iter().any(|arg| Self::expr_contains_var(arg, name)),
            HirExpr::Index { base, index, .. } => {
                Self::expr_contains_var(base, name) || Self::expr_contains_var(index, name)
            }
        }
    }

    fn lvalue_contains_var(lhs: &HirLValue, name: &str) -> bool {
        match lhs {
            HirLValue::Var(_) => false,
            HirLValue::Deref { ptr, .. } => Self::expr_contains_var(ptr, name),
            HirLValue::Index { base, index, .. } => {
                Self::expr_contains_var(base, name) || Self::expr_contains_var(index, name)
            }
        }
    }

    fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
        match expr {
            HirExpr::Var(var) if var == name => *expr = replacement.clone(),
            HirExpr::Var(_) | HirExpr::Const(_, _) => {}
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
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
        }
    }

    fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                Self::replace_var_in_lvalue(lhs, name, replacement);
                Self::replace_var_in_expr(rhs, name, replacement);
            }
            HirStmt::VaStart { va_list, .. } => Self::replace_var_in_expr(va_list, name, replacement),
            HirStmt::Expr(expr) => Self::replace_var_in_expr(expr, name, replacement),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                for stmt in stmts {
                    Self::replace_var_in_stmt(stmt, name, replacement);
                }
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
                ..
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
            }
            HirStmt::Return(Some(expr)) => Self::replace_var_in_expr(expr, name, replacement),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    fn count_var_defs_stmt(stmt: &HirStmt, target: &str) -> usize {
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
                init,
                update,
                body,
                ..
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
            HirExpr::Var(var) => usize::from(var == name),
            HirExpr::Const(_, _) => 0,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
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
        }
    }

    fn count_var_reads_lvalue(lhs: &HirLValue, name: &str) -> usize {
        match lhs {
            HirLValue::Var(_) => 0,
            HirLValue::Deref { ptr, .. } => Self::count_var_reads_expr(ptr, name),
            HirLValue::Index { base, index, .. } => {
                Self::count_var_reads_expr(base, name) + Self::count_var_reads_expr(index, name)
            }
        }
    }

    fn count_var_reads_stmt(stmt: &HirStmt, name: &str) -> usize {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                Self::count_var_reads_lvalue(lhs, name) + Self::count_var_reads_expr(rhs, name)
            }
            HirStmt::VaStart { va_list, .. } => Self::count_var_reads_expr(va_list, name),
            HirStmt::Expr(expr) => Self::count_var_reads_expr(expr, name),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| Self::count_var_reads_stmt(stmt, name))
                .sum(),
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

    fn find_guarded_tail_preexisting_source(
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

    fn resolve_guarded_tail_else_source(
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

    fn classify_stmt_read_kind(stmt: &HirStmt, name: &str) -> Option<GuardedTailReadKind> {
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
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => stmts
                .iter()
                .find_map(|stmt| Self::classify_stmt_read_kind(stmt, name)),
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
                .or_else(|| update.iter().find_map(|stmt| Self::classify_stmt_read_kind(stmt, name)))
                .or_else(|| body.iter().find_map(|stmt| Self::classify_stmt_read_kind(stmt, name))),
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

    fn condition_matches_assumption(expr: &HirExpr, assumption: &ConditionAssumption) -> Option<bool> {
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

    fn evaluate_condition_assumptions(
        expr: &HirExpr,
        assumptions: &[ConditionAssumption],
    ) -> Option<bool> {
        assumptions
            .iter()
            .find_map(|assumption| Self::condition_matches_assumption(expr, assumption))
    }

    fn local_forward_branch_target(
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

    fn rewrite_guarded_tail_sequence(
        stmts: &[HirStmt],
        join_label: &str,
        assumptions: &[ConditionAssumption],
    ) -> GuardedTailRewriteResult {
        let mut out = Vec::with_capacity(stmts.len());
        let mut idx = 0usize;
        while idx < stmts.len() {
            match &stmts[idx] {
                HirStmt::Goto(target) if target == join_label => {
                    return GuardedTailRewriteResult {
                        stmts: out,
                        exits_to_join: true,
                        unresolved_join_refs: 0,
                    };
                }
                HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    if let Some((branch_label, branch_when_true)) =
                        Self::local_forward_branch_target(then_body, else_body)
                        && branch_label != join_label
                        && let Some(label_pos) = (idx + 1..stmts.len()).find(|pos| {
                            matches!(
                                stmts.get(*pos),
                                Some(HirStmt::Label(candidate)) if candidate == &branch_label
                            )
                        })
                    {
                        let mut target_assumptions = assumptions.to_vec();
                        target_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: branch_when_true,
                        });
                        let target_rewritten = Self::rewrite_guarded_tail_sequence(
                            &stmts[label_pos + 1..],
                            join_label,
                            &target_assumptions,
                        );

                        let mut fallthrough_assumptions = assumptions.to_vec();
                        fallthrough_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value: !branch_when_true,
                        });
                        let fallthrough_rewritten = Self::rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..label_pos],
                            join_label,
                            &fallthrough_assumptions,
                        );

                        let target_body = target_rewritten.stmts.clone();
                        let target_exits = target_rewritten.exits_to_join;

                        let (then_result, then_exits, else_result, else_exits) = if branch_when_true
                        {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                target_body,
                                target_exits,
                                fallthrough_body,
                                fallthrough_exits,
                            )
                        } else {
                            let mut fallthrough_body = fallthrough_rewritten.stmts;
                            let fallthrough_exits = if fallthrough_rewritten.exits_to_join {
                                true
                            } else {
                                fallthrough_body.extend(target_rewritten.stmts);
                                target_exits
                            };
                            (
                                fallthrough_body,
                                fallthrough_exits,
                                target_body,
                                target_exits,
                            )
                        };

                        out.push(HirStmt::If {
                            cond: cond.clone(),
                            then_body: then_result,
                            else_body: else_result,
                        });
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: then_exits && else_exits,
                            unresolved_join_refs: target_rewritten.unresolved_join_refs
                                + fallthrough_rewritten.unresolved_join_refs,
                        };
                    }

                    if let Some(value) = Self::evaluate_condition_assumptions(cond, assumptions) {
                        let mut next_assumptions = assumptions.to_vec();
                        next_assumptions.push(ConditionAssumption {
                            expr: cond.clone(),
                            value,
                        });
                        let chosen = if value { then_body } else { else_body };
                        let rewritten = Self::rewrite_guarded_tail_sequence(
                            chosen,
                            join_label,
                            &next_assumptions,
                        );
                        out.extend(rewritten.stmts);
                        if rewritten.exits_to_join {
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: rewritten.unresolved_join_refs,
                            };
                        }
                        idx += 1;
                        continue;
                    }

                    let mut then_assumptions = assumptions.to_vec();
                    then_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: true,
                    });
                    let then_rewritten =
                        Self::rewrite_guarded_tail_sequence(then_body, join_label, &then_assumptions);
                    let mut else_assumptions = assumptions.to_vec();
                    else_assumptions.push(ConditionAssumption {
                        expr: cond.clone(),
                        value: false,
                    });
                    let else_rewritten =
                        Self::rewrite_guarded_tail_sequence(else_body, join_label, &else_assumptions);

                    if then_rewritten.exits_to_join || else_rewritten.exits_to_join {
                        let rest = Self::rewrite_guarded_tail_sequence(
                            &stmts[idx + 1..],
                            join_label,
                            assumptions,
                        );
                        if then_rewritten.exits_to_join && else_rewritten.exits_to_join {
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: else_rewritten.stmts,
                            });
                            return GuardedTailRewriteResult {
                                stmts: out,
                                exits_to_join: true,
                                unresolved_join_refs: then_rewritten.unresolved_join_refs
                                    + else_rewritten.unresolved_join_refs
                                    + rest.unresolved_join_refs,
                            };
                        }

                        if then_rewritten.exits_to_join {
                            let mut continue_body = else_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: then_rewritten.stmts,
                                else_body: continue_body,
                            });
                        } else {
                            let mut continue_body = then_rewritten.stmts;
                            continue_body.extend(rest.stmts);
                            out.push(HirStmt::If {
                                cond: cond.clone(),
                                then_body: continue_body,
                                else_body: else_rewritten.stmts,
                            });
                        }
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: false,
                            unresolved_join_refs: then_rewritten.unresolved_join_refs
                                + else_rewritten.unresolved_join_refs
                                + rest.unresolved_join_refs,
                        };
                    }

                    out.push(HirStmt::If {
                        cond: cond.clone(),
                        then_body: then_rewritten.stmts,
                        else_body: else_rewritten.stmts,
                    });
                }
                HirStmt::Goto(target) => {
                    out.push(HirStmt::Goto(target.clone()));
                }
                HirStmt::Block(inner) => {
                    let rewritten =
                        Self::rewrite_guarded_tail_sequence(inner, join_label, assumptions);
                    out.push(HirStmt::Block(rewritten.stmts));
                    if rewritten.exits_to_join {
                        return GuardedTailRewriteResult {
                            stmts: out,
                            exits_to_join: true,
                            unresolved_join_refs: rewritten.unresolved_join_refs,
                        };
                    }
                }
                stmt => out.push(stmt.clone()),
            }
            idx += 1;
        }

        let unresolved_join_refs = out
            .iter()
            .map(|stmt| Self::stmt_contains_goto_label(stmt, join_label))
            .sum();
        GuardedTailRewriteResult {
            stmts: out,
            exits_to_join: false,
            unresolved_join_refs,
        }
    }

    fn collect_guarded_tail_exported_bindings(
        &mut self,
        middle: &[HirStmt],
        follow_tail: &[HirStmt],
    ) -> Result<Vec<GuardedTailExportedBinding>, GuardedTailExecutionRejection> {
        let mut bindings = Vec::new();
        for (def_stmt_idx, stmt) in middle.iter().enumerate() {
            let HirStmt::Assign {
                lhs: HirLValue::Var(binding_name),
                rhs,
            } = stmt
            else {
                continue;
            };
            let mut read_sites = Vec::new();
            let mut follow_redefined = false;
            let mut nondominated_reads = 0usize;
            for (stmt_idx, stmt) in follow_tail.iter().enumerate() {
                let reads_here = Self::classify_stmt_read_kind(stmt, binding_name);
                let defs_here = Self::count_var_defs_stmt(stmt, binding_name);
                if follow_redefined {
                    if reads_here.is_some() {
                        nondominated_reads += 1;
                    }
                    continue;
                }
                if let Some(kind) = reads_here {
                    read_sites.push(GuardedTailReplacementRead { stmt_idx, kind });
                }
                if defs_here > 0 {
                    follow_redefined = true;
                }
            }
            if read_sites.is_empty() {
                continue;
            }
            self.guarded_tail_exported_binding_count += 1;
            self.guarded_tail_replacement_read_count += read_sites.len();

            if !Self::expr_is_pure_value(rhs) {
                self.guarded_tail_replacement_read_rejected_nonremovable_op_count +=
                    read_sites.len();
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if middle
                .iter()
                .map(|stmt| Self::count_var_defs_stmt(stmt, binding_name))
                .sum::<usize>()
                != 1
            {
                self.guarded_tail_replacement_read_rejected_nondominated_count += read_sites.len();
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }
            if nondominated_reads > 0 {
                self.guarded_tail_replacement_read_rejected_nondominated_count +=
                    read_sites.len() + nondominated_reads;
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            }

            bindings.push(GuardedTailExportedBinding {
                def_stmt_idx,
                binding_name: binding_name.clone(),
                replacement_source: rhs.clone(),
                read_sites,
            });
        }
        Ok(bindings)
    }

    pub(super) fn mark_guarded_tail_canonicalization_failure(
        &mut self,
        reason: GuardedTailCanonicalizationFailure,
    ) {
        self.mark_noncanonical_layout_rejection();
        match reason {
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries => {
                self.canonicalization_failed_multiple_payload_entries += 1;
            }
            GuardedTailCanonicalizationFailure::InterleavedJoinUses => {
                self.canonicalization_failed_interleaved_join_uses += 1;
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                self.canonicalization_failed_nonterminal_join_label += 1;
            }
            GuardedTailCanonicalizationFailure::NestedTailEscape => {
                self.canonicalization_failed_nested_tail_escape += 1;
            }
            GuardedTailCanonicalizationFailure::AliasNotFallthrough => {
                self.canonicalization_failed_alias_not_fallthrough_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors => {
                self.canonicalization_failed_alias_has_multiple_internal_predecessors_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasNonlocalRef => {
                self.canonicalization_failed_alias_has_nonlocal_ref_count += 1;
            }
            GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                self.canonicalization_failed_payload_crosses_join_count += 1;
            }
        }
    }

    pub(super) fn mark_promotion_gate_rejection(&mut self, reason: PromotionGateRejection) {
        self.promotion_rejected_by_gate_count += 1;
        match reason {
            PromotionGateRejection::MustEmitLabel => self.rejected_must_emit_label += 1,
            PromotionGateRejection::MustEmitLabelSurvivingMiddleRef => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_surviving_middle_ref += 1;
            }
            PromotionGateRejection::MustEmitLabelSurvivingExternalRef => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_surviving_external_ref += 1;
            }
            PromotionGateRejection::MustEmitLabelOwnerConflict => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_owner_conflict += 1;
            }
            PromotionGateRejection::NotSinglePredSucc => self.rejected_not_single_pred_succ += 1,
            PromotionGateRejection::ExternalEntry => self.rejected_external_entry += 1,
            PromotionGateRejection::LoopOrSwitchTarget => self.rejected_loop_or_switch_target += 1,
        }
    }

    fn try_build_guarded_tail_witness(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<RegionShapeWitness, GuardedTailWitnessRejection>> {
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[idx]
        else {
            return None;
        };

        let (target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
            let Some(label) = single_goto_target(then_body) else {
                return None;
            };
            (label.to_string(), false)
        } else if then_body.is_empty() {
            let Some(label) = single_goto_target(else_body) else {
                return None;
            };
            (label.to_string(), true)
        } else {
            return None;
        };

        let Some(original_label_idx) = Self::find_top_level_label_after(body, idx, &target_label)
        else {
            return None;
        };
        if !has_non_ignorable_payload(&body[idx + 1..original_label_idx]) {
            self.mark_noncanonical_layout_rejection();
            return Some(Err(GuardedTailWitnessRejection::NonCanonicalLayout));
        }
        let original_tail_end = (original_label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if original_tail_end < body.len()
            && body[original_label_idx + 1..original_tail_end]
                .iter()
                .all(is_ignorable_discovery_stmt)
        {
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        let Some((target_label, label_idx)) =
            self.resolve_terminal_join_target(body, idx, &target_label, referenced)
        else {
            return Some(Err(GuardedTailWitnessRejection::MissingTerminalJoin));
        };

        let (middle, external_redirects) = match self.canonicalize_guarded_tail_segment(
            &body[idx + 1..label_idx],
            body,
            idx + 1,
            referenced,
        ) {
            Ok(middle) => middle,
            Err(reason) => {
                self.mark_guarded_tail_canonicalization_failure(reason);
                return Some(Err(Self::map_guarded_tail_canonicalization_rejection(
                    reason,
                )));
            }
        };
        if middle.is_empty() {
            self.mark_guarded_tail_canonicalization_failure(
                GuardedTailCanonicalizationFailure::InterleavedJoinUses,
            );
            return Some(Err(GuardedTailWitnessRejection::AliasInterleaveConflict));
        }

        let tail_end = (label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if body[label_idx + 1..tail_end].is_empty() && label_idx + 1 != body.len() {
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        Some(Ok(RegionShapeWitness {
            target_label,
            label_idx,
            keep_middle_when_cond_true,
            middle,
            external_redirects,
            terminal_join_present: true,
            follow_witness: true,
            side_entry_free: true,
            alias_interleave_legal: true,
        }))
    }

    fn collect_guarded_tail_candidate_reads(
        body: &[HirStmt],
        middle: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> Vec<GuardedTailReplacementRead> {
        let mut reads = Vec::new();
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if stmt_idx >= if_idx && stmt_idx <= label_idx {
                continue;
            }
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::ExternalForwardGoto,
                });
            }
        }
        for (stmt_idx, stmt) in middle.iter().enumerate() {
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            for _ in 0..ref_count {
                reads.push(GuardedTailReplacementRead {
                    stmt_idx,
                    kind: GuardedTailReadKind::MiddleGoto,
                });
            }
        }
        reads
    }

    fn try_build_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<GuardedTailTrial, GuardedTailWitnessRejection>> {
        let witness = self.try_build_guarded_tail_witness(body, idx, referenced)?;
        if Self::guarded_tail_diag_enabled() {
            match &witness {
                Ok(witness) => eprintln!(
                    "[DIAG] guarded-tail trial idx={} label={} middle_stmts={} redirects={}",
                    idx,
                    witness.target_label,
                    witness.middle.len(),
                    witness.external_redirects.len(),
                ),
                Err(reason) => eprintln!(
                    "[DIAG] guarded-tail trial idx={} rejected={:?}",
                    idx, reason
                ),
            }
        }
        Some(witness.map(|witness| GuardedTailTrial {
            follow_block: Some(witness.target_label.clone()),
            candidate_reads: Self::collect_guarded_tail_candidate_reads(
                body,
                &witness.middle,
                idx,
                witness.label_idx,
                &witness.target_label,
            ),
            witness,
        }))
    }

    fn trim_guarded_tail_fallthrough_gotos(mut middle: Vec<HirStmt>, label: &str) -> Vec<HirStmt> {
        while matches!(middle.last(), Some(HirStmt::Goto(target)) if target == label) {
            middle.pop();
        }
        middle
    }

    fn guarded_tail_stmt_is_execution_safe(stmt: &HirStmt, label: &str) -> bool {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } => Self::expr_is_pure_value(rhs),
            HirStmt::VaStart { .. } => true,
            HirStmt::Expr(expr) => Self::expr_is_pure_value(expr),
            HirStmt::Goto(target) => target == label,
            HirStmt::Block(body) => Self::guarded_tail_middle_is_execution_safe(body, label),
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                Self::expr_is_pure_value(cond)
                    && Self::guarded_tail_middle_is_execution_safe(then_body, label)
                    && Self::guarded_tail_middle_is_execution_safe(else_body, label)
            }
            HirStmt::Label(_)
            | HirStmt::Switch { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => false,
            HirStmt::Assign { .. } => false,
        }
    }

    fn guarded_tail_middle_is_execution_safe(middle: &[HirStmt], label: &str) -> bool {
        middle.iter().all(|stmt| match stmt {
            _ => Self::guarded_tail_stmt_is_execution_safe(stmt, label),
        })
    }

    fn verify_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
    ) -> GuardedTailVerification {
        let witness = &trial.witness;
        let legality = witness.region_legality();
        self.guarded_tail_replacement_plan_candidate_count += 1;
        let follow_tail = if witness.label_idx + 1 < body.len() {
            &body[witness.label_idx + 1..]
        } else {
            &[]
        };
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} legality={:?}",
                idx, witness.target_label, legality
            );
        }

        if !legality.is_complete_for(RegionKind::GuardedTail) {
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} incomplete_legality",
                    idx, witness.target_label
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: witness.middle.clone(),
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::Witness(
                    GuardedTailWitnessRejection::NonCanonicalLayout,
                )),
            };
        }

        let (outside_refs, middle_refs) = Self::surviving_label_refs_after_guarded_tail_promotion(
            body,
            &witness.middle,
            idx,
            witness.label_idx,
            &witness.target_label,
        );
        let effective_middle_refs = Self::effective_middle_refs_for_promotion(
            &witness.middle,
            &witness.target_label,
            middle_refs,
        );
        let rewritten = Self::rewrite_guarded_tail_sequence(
            &witness.middle,
            &witness.target_label,
            &[],
        );
        let execution_safe =
            Self::guarded_tail_middle_is_execution_safe(&rewritten.stmts, &witness.target_label);
        let post_label_refs: usize = body[witness.label_idx + 1..]
            .iter()
            .map(|stmt| Self::stmt_contains_goto_label(stmt, &witness.target_label))
            .sum();
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} outside_refs={} middle_refs={} effective_middle_refs={} post_label_refs={} unresolved_join_refs={} execution_safe={}",
                idx,
                witness.target_label,
                outside_refs,
                middle_refs,
                effective_middle_refs,
                post_label_refs,
                rewritten.unresolved_join_refs,
                execution_safe,
            );
        }
        if post_label_refs > 0 {
            self.mark_promotion_gate_rejection(
                PromotionGateRejection::MustEmitLabelSurvivingExternalRef,
            );
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=MustEmitLabelConflict(post_label_refs)",
                    idx, witness.target_label
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: rewritten.stmts,
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::MustEmitLabelConflict),
            };
        }
        if outside_refs > 0 {
            self.mark_promotion_gate_rejection(PromotionGateRejection::MustEmitLabelSurvivingExternalRef);
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=MustEmitLabelConflict(outside_refs={})",
                    idx, witness.target_label, outside_refs
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal: false,
                rewritten_middle: rewritten.stmts,
                exported_bindings: Vec::new(),
                rejection_reason: Some(GuardedTailExecutionRejection::MustEmitLabelConflict),
            };
        }
        let removable_ops_legal = execution_safe
            && !has_top_level_label(&rewritten.stmts)
            && rewritten.unresolved_join_refs == 0;
        let exported_bindings = match self.collect_guarded_tail_exported_bindings(
            &rewritten.stmts,
            follow_tail,
        ) {
            Ok(bindings) => bindings,
            Err(reason) => {
                if Self::guarded_tail_diag_enabled() {
                    eprintln!(
                        "[DIAG] guarded-tail verify idx={} label={} exported_bindings_rejected={:?}",
                        idx, witness.target_label, reason
                    );
                }
                return GuardedTailVerification {
                    region_legality: legality,
                    replacement_complete: false,
                    removable_ops_legal,
                    rewritten_middle: rewritten.stmts,
                    exported_bindings: Vec::new(),
                    rejection_reason: Some(reason),
                };
            }
        };
        let replacement_complete = removable_ops_legal && effective_middle_refs == 0;

        if replacement_complete
            && exported_bindings.iter().any(|binding| {
                !binding.read_sites.is_empty()
                    && Self::find_guarded_tail_preexisting_source(body, idx, &binding.binding_name)
                        .is_none()
            })
        {
            self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
            if Self::guarded_tail_diag_enabled() {
                let missing = exported_bindings
                    .iter()
                    .filter(|binding| {
                        !binding.read_sites.is_empty()
                            && Self::find_guarded_tail_preexisting_source(
                                body,
                                idx,
                                &binding.binding_name,
                            )
                            .is_none()
                    })
                    .map(|binding| binding.binding_name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=ReplacementIncomplete(missing_else_source=[{}])",
                    idx, witness.target_label, missing
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: false,
                removable_ops_legal,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: Some(GuardedTailExecutionRejection::ReplacementIncomplete),
            };
        }

        if replacement_complete {
            self.guarded_tail_replacement_plan_completed_count += 1;
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} replacement_complete exported_bindings={}",
                    idx,
                    witness.target_label,
                    exported_bindings.len()
                );
            }
            return GuardedTailVerification {
                region_legality: legality,
                replacement_complete: true,
                removable_ops_legal: true,
                rewritten_middle: rewritten.stmts,
                exported_bindings,
                rejection_reason: None,
            };
        }

        if !removable_ops_legal || effective_middle_refs > 0 {
            self.guarded_tail_replacement_plan_rejected_unstable_read_count += 1;
        }
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} rejected={:?} removable_ops_legal={} effective_middle_refs={}",
                idx,
                witness.target_label,
                if !removable_ops_legal {
                    GuardedTailExecutionRejection::MustEmitLabelConflict
                } else {
                    GuardedTailExecutionRejection::ReplacementIncomplete
                },
                removable_ops_legal,
                effective_middle_refs
            );
        }

        GuardedTailVerification {
            region_legality: legality,
            replacement_complete,
            removable_ops_legal,
            rewritten_middle: rewritten.stmts,
            exported_bindings,
            rejection_reason: Some(if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
                GuardedTailExecutionRejection::ReplacementIncomplete
            }),
        }
    }

    fn build_guarded_tail_execution_plan(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
        verification: &GuardedTailVerification,
    ) -> Result<GuardedTailExecutionPlan, GuardedTailExecutionRejection> {
        let mut rewritten_middle = verification.rewritten_middle.clone();
        let mut synthetic_merges = Vec::new();
        let mut replacement_cache = GuardedTailReplacementCache::default();
        let mut exported_bindings = verification.exported_bindings.clone();
        exported_bindings.sort_by_key(|binding| binding.def_stmt_idx);
        let mut obsolete_defs = Vec::new();

        for binding_idx in 0..exported_bindings.len() {
            let binding_name = exported_bindings[binding_idx].binding_name.clone();
            let replacement_source = exported_bindings[binding_idx].replacement_source.clone();
            let def_stmt_idx = exported_bindings[binding_idx].def_stmt_idx;
            for stmt in rewritten_middle
                .iter_mut()
                .skip(def_stmt_idx.saturating_add(1))
            {
                Self::replace_var_in_stmt(stmt, &binding_name, &replacement_source);
            }
            for later_binding in exported_bindings.iter_mut().skip(binding_idx + 1) {
                Self::replace_var_in_expr(
                    &mut later_binding.replacement_source,
                    &binding_name,
                    &replacement_source,
                );
            }
            if rewritten_middle
                .iter()
                .skip(def_stmt_idx.saturating_add(1))
                .all(|stmt| Self::count_var_reads_stmt(stmt, &binding_name) == 0)
            {
                obsolete_defs.push(def_stmt_idx);
            }

            let else_value = if exported_bindings[binding_idx].read_sites.is_empty() {
                continue;
            } else if let Some(expr) = Self::resolve_guarded_tail_else_source(
                body,
                idx,
                &binding_name,
                &mut replacement_cache,
            ) {
                expr
            } else {
                self.guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
                return Err(GuardedTailExecutionRejection::ReplacementIncomplete);
            };

            let ty = expr_type(&replacement_source);
            let replacement_target = next_temp_name(&ty, &mut self.temp_next_id);
            self.temps.insert(
                replacement_target.clone(),
                NirBinding {
                    name: replacement_target.clone(),
                    ty,
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::TempPreserved),
                    initializer: None,
                },
            );
            self.guarded_tail_replacement_plan_merge_created_count += 1;
            synthetic_merges.push(GuardedTailSyntheticMerge {
                binding_name,
                replacement_target,
                then_value: replacement_source,
                else_value,
                read_sites: exported_bindings[binding_idx].read_sites.clone(),
            });
        }
        obsolete_defs.sort_unstable();
        obsolete_defs.dedup();
        for def_idx in obsolete_defs.into_iter().rev() {
            if def_idx < rewritten_middle.len() {
                rewritten_middle.remove(def_idx);
            }
        }
        Ok(GuardedTailExecutionPlan {
            synthetic_merges,
            redirects: trial.witness.external_redirects.clone(),
            rewritten_middle,
        })
    }

    fn apply_guarded_tail_replacement_read(
        stmt: &mut HirStmt,
        merge: &GuardedTailSyntheticMerge,
    ) {
        let replacement_expr = HirExpr::Var(merge.replacement_target.clone());
        Self::replace_var_in_stmt(stmt, &merge.binding_name, &replacement_expr);
    }

    fn execute_guarded_tail_plan(
        &mut self,
        body: &mut Vec<HirStmt>,
        idx: usize,
        trial: GuardedTailTrial,
        plan: GuardedTailExecutionPlan,
        cond: HirExpr,
    ) {
        let mut then_body = plan.rewritten_middle;
        let mut else_body = Vec::new();
        for merge in &plan.synthetic_merges {
            then_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.then_value.clone(),
            });
            else_body.push(HirStmt::Assign {
                lhs: HirLValue::Var(merge.replacement_target.clone()),
                rhs: merge.else_value.clone(),
            });
        }
        let replacement = HirStmt::If {
            cond: if trial.witness.keep_middle_when_cond_true {
                cond
            } else {
                negate_expr(cond)
            },
            then_body,
            else_body,
        };

        for (from, to) in &plan.redirects {
            Self::rewrite_goto_label_in_stmts(body, from, to);
        }

        body[idx] = replacement;
        body.drain(idx + 1..=trial.witness.label_idx);
        let tail_start = idx + 1;
        for merge in &plan.synthetic_merges {
            for read in &merge.read_sites {
                let stmt_idx = tail_start + read.stmt_idx;
                if let Some(stmt) = body.get_mut(stmt_idx) {
                    Self::apply_guarded_tail_replacement_read(stmt, merge);
                    self.guarded_tail_replacement_read_rewritten_count += 1;
                }
            }
        }
        self.guarded_tail_promoted_count += 1;
        self.promoted_region_count += 1;
    }

    pub(crate) fn promote_single_entry_guarded_tail_regions(
        &mut self,
        body: &mut Vec<HirStmt>,
    ) -> bool {
        let (normalized, alias_rewrites) = normalize_guarded_tail_layout(std::mem::take(body));
        *body = normalized;
        let referenced = collect_referenced_label_counts(body);
        let mut changed = alias_rewrites > 0;
        let mut idx = 0usize;
        while idx < body.len() {
            let HirStmt::If { cond, .. } = &body[idx] else {
                idx += 1;
                continue;
            };
            let Some(trial) = self.try_build_guarded_tail_trial(body, idx, &referenced) else {
                idx += 1;
                continue;
            };
            let trial = match trial {
                Ok(trial) => trial,
                Err(reason) => {
                    self.mark_guarded_tail_execution_rejection(
                        GuardedTailExecutionRejection::Witness(reason),
                    );
                    match reason {
                        GuardedTailWitnessRejection::MissingTerminalJoin => {
                            self.mark_promotion_shape_rejection(
                                PromotionShapeRejection::MissingTerminalJoinTarget,
                            );
                        }
                        GuardedTailWitnessRejection::AmbiguousFollow => {
                            self.mark_promotion_shape_rejection(
                                PromotionShapeRejection::EmptyNonterminalTail,
                            );
                        }
                        GuardedTailWitnessRejection::AliasInterleaveConflict => {}
                        GuardedTailWitnessRejection::NonCanonicalLayout => {}
                        GuardedTailWitnessRejection::SideEntryConflict => {}
                    }
                    idx += 1;
                    continue;
                }
            };
            let legality = trial.witness.region_legality();
            self.region_proof_candidate_count += 1;
            if legality.is_complete_for(RegionKind::GuardedTail) {
                self.region_proof_completed_count += 1;
            }
            let verification = self.verify_guarded_tail_trial(body, idx, &trial);
            if let Some(reason) = verification.rejection_reason {
                self.mark_guarded_tail_execution_rejection(reason);
                idx += 1;
                continue;
            }

            self.guarded_tail_candidate_count += 1;
            self.promotion_candidate_count += 1;
            let plan = match self.build_guarded_tail_execution_plan(body, idx, &trial, &verification) {
                Ok(plan) => plan,
                Err(reason) => {
                    self.mark_guarded_tail_execution_rejection(reason);
                    idx += 1;
                    continue;
                }
            };
            self.execute_guarded_tail_plan(body, idx, trial, plan, cond.clone());
            changed = true;
            idx += 1;
        }
        changed
    }

    pub(crate) fn discover_guarded_tail_candidates(&mut self, body: &[HirStmt]) {
        let (normalized, _) = normalize_guarded_tail_layout(body.to_vec());
        self.discover_guarded_tail_candidates_in_body(&normalized);
    }

    fn discover_guarded_tail_candidates_in_body(&mut self, body: &[HirStmt]) {
        for stmt in body {
            match stmt {
                HirStmt::Block(inner)
                | HirStmt::While { body: inner, .. }
                | HirStmt::DoWhile { body: inner, .. }
                | HirStmt::For { body: inner, .. } => {
                    self.discover_guarded_tail_candidates_in_body(inner);
                }
                HirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.discover_guarded_tail_candidates_in_body(then_body);
                    self.discover_guarded_tail_candidates_in_body(else_body);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        self.discover_guarded_tail_candidates_in_body(&case.body);
                    }
                    self.discover_guarded_tail_candidates_in_body(default);
                }
                HirStmt::Assign { .. }
                | HirStmt::VaStart { .. }
                | HirStmt::Expr(_)
                | HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue => {}
            }
        }

        let referenced = collect_referenced_label_counts(body);
        for idx in 0..body.len() {
            let HirStmt::If { .. } = &body[idx] else {
                continue;
            };
            let Some(trial) = self.try_build_guarded_tail_trial(body, idx, &referenced) else {
                continue;
            };
            self.discovery_seen_guarded_tail_like_shape_count += 1;
            let trial = match trial {
                Ok(trial) => trial,
                Err(reason) => {
                    self.mark_guarded_tail_execution_rejection(
                        GuardedTailExecutionRejection::Witness(reason),
                    );
                    match reason {
                        GuardedTailWitnessRejection::MissingTerminalJoin => {
                            self.mark_guarded_tail_canonicalization_failure(
                                GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                            );
                        }
                        GuardedTailWitnessRejection::AliasInterleaveConflict => {}
                        GuardedTailWitnessRejection::NonCanonicalLayout => {}
                        GuardedTailWitnessRejection::AmbiguousFollow
                        | GuardedTailWitnessRejection::SideEntryConflict => {}
                    }
                    continue;
                }
            };
            let verification = self.verify_guarded_tail_trial(body, idx, &trial);
            if let Some(reason) = verification.rejection_reason {
                self.mark_guarded_tail_execution_rejection(reason);
                continue;
            }

            self.guarded_tail_candidate_count += 1;
            self.promotion_candidate_count += 1;
        }
    }

    pub(crate) fn accept_structured_region(
        &mut self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        self.promotion_candidate_count += 1;
        let accepted = !self.region_has_targeted_internal_entry(start_idx, skip_to, targeted)
            || self
                .is_minimal_structured_promotion_candidate(start_idx, skip_to, targeted)
                .is_ok();
        if !accepted
            && self.region_has_targeted_internal_entry(start_idx, skip_to, targeted)
            && let Err(reason) =
                self.is_minimal_structured_promotion_candidate(start_idx, skip_to, targeted)
        {
            self.mark_promotion_gate_rejection(reason);
        }
        if accepted {
            self.promoted_region_count += 1;
        }
        accepted
    }
}
