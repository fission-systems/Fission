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

#[derive(Debug, Clone, PartialEq, Eq)]
enum SuffixTailRejection {
    SuffixHasSideEffect { stmt_idx: usize },
    SuffixHasNonTerminalGoto { stmt_idx: usize, target: String },
    SuffixHasNestedOrNonlocalRef { stmt_idx: usize },
    SuffixHasLabelCrossing { stmt_idx: usize, label: String },
    SuffixHasExternalEntry { stmt_idx: usize, label: String },
    SuffixHasLoopOrSwitchCrossing { stmt_idx: usize },
    SuffixAliasRedirectUnresolved { stmt_idx: usize, label: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SuffixExternalEntryBudget {
    raw_refs: usize,
    internal_top_level_refs: usize,
    suffix_safe_refs: usize,
    guard_family_internalized_refs: usize,
    effective_external_refs: usize,
    allowed_external_refs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalEntryRefKind {
    TopLevelExternalGoto,
    NestedConditionalGoto,
    AliasRedirectDerived,
    LoopSwitchDerived,
    UnknownExternalEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NestedSuffixShapeKind {
    NestedSingleGotoThen,
    NestedSingleGotoElse,
    NestedBothBranches,
    NestedMultiStmtBranch,
    NestedNonlocalTarget,
    NestedGuardFamilyMismatch,
    NestedCrossesTerminalJoin,
    NestedUnknown,
}

impl SuffixTailRejection {
    fn stmt_idx(&self) -> usize {
        match self {
            Self::SuffixHasSideEffect { stmt_idx }
            | Self::SuffixHasNestedOrNonlocalRef { stmt_idx }
            | Self::SuffixHasLoopOrSwitchCrossing { stmt_idx } => *stmt_idx,
            Self::SuffixHasNonTerminalGoto { stmt_idx, .. }
            | Self::SuffixHasLabelCrossing { stmt_idx, .. }
            | Self::SuffixHasExternalEntry { stmt_idx, .. }
            | Self::SuffixAliasRedirectUnresolved { stmt_idx, .. } => *stmt_idx,
        }
    }
}

impl<'a> PreviewBuilder<'a> {
    fn guarded_tail_diag_enabled() -> bool {
        std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
    }

    fn guarded_tail_function_address(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or(0)
    }

    fn guarded_tail_trace_target_addr() -> Option<u64> {
        let raw = std::env::var("FISSION_PREVIEW_DIAG_ADDR").ok()?;
        let trimmed = raw.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);
        u64::from_str_radix(hex, 16).ok()
    }

    pub(super) fn guarded_tail_trace_enabled_for_current_fn(&self) -> bool {
        let Some(target) = Self::guarded_tail_trace_target_addr() else {
            return false;
        };
        self.guarded_tail_function_address() == target
    }

    pub(super) fn guarded_tail_trace_emit_snapshot(
        prefix: &str,
        stmts: &[HirStmt],
        max_lines: usize,
    ) {
        let take_n = stmts.len().min(max_lines.max(1));
        for (idx, stmt) in stmts.iter().take(take_n).enumerate() {
            eprintln!("{prefix} [{idx:02}] {stmt:?}");
        }
        if stmts.len() > take_n {
            eprintln!(
                "{prefix} ... (truncated {} stmts)",
                stmts.len().saturating_sub(take_n)
            );
        }
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

    fn top_level_label_definition_count_for_owned_tail(body: &[HirStmt], label: &str) -> usize {
        body.iter()
            .filter(|stmt| matches!(stmt, HirStmt::Label(candidate) if candidate == label))
            .count()
    }

    fn stmt_is_sink_safe_return_goto_for_owned_tail(stmt: &HirStmt, body: &[HirStmt]) -> bool {
        let HirStmt::Goto(target) = stmt else {
            return false;
        };
        if Self::top_level_label_definition_count_for_owned_tail(body, target) != 1 {
            return false;
        }
        matches!(
            Self::resolve_terminal_tail_exit_stmt(body, target),
            Some(HirStmt::Return(_))
        )
    }

    fn suffix_stmt_has_nested_or_nonlocal_ref(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::If { .. } => true,
            HirStmt::Block(inner) => !inner.is_empty(),
            _ => false,
        }
    }

    fn classify_nested_suffix_shape(
        stmt: &HirStmt,
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> NestedSuffixShapeKind {
        let Some(HirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return NestedSuffixShapeKind::NestedUnknown;
        };
        match stmt {
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if then_body.len() > 1 || else_body.len() > 1 {
                    return NestedSuffixShapeKind::NestedMultiStmtBranch;
                }
                let then_target = single_goto_target(then_body);
                let else_target = single_goto_target(else_body);
                if then_target.is_some() && else_target.is_some() {
                    return NestedSuffixShapeKind::NestedBothBranches;
                }
                if let Some(target) = then_target {
                    if target == terminal_label {
                        return NestedSuffixShapeKind::NestedCrossesTerminalJoin;
                    }
                    if target != next_label {
                        return NestedSuffixShapeKind::NestedNonlocalTarget;
                    }
                    if !Self::suffix_window_has_terminal_guard_family_match(
                        body,
                        current_label_idx,
                        terminal_label_idx,
                        cond,
                    ) {
                        return NestedSuffixShapeKind::NestedGuardFamilyMismatch;
                    }
                    return NestedSuffixShapeKind::NestedSingleGotoThen;
                }
                if let Some(target) = else_target {
                    if target == terminal_label {
                        return NestedSuffixShapeKind::NestedCrossesTerminalJoin;
                    }
                    if target != next_label {
                        return NestedSuffixShapeKind::NestedNonlocalTarget;
                    }
                    if !Self::suffix_window_has_terminal_guard_family_match(
                        body,
                        current_label_idx,
                        terminal_label_idx,
                        cond,
                    ) {
                        return NestedSuffixShapeKind::NestedGuardFamilyMismatch;
                    }
                    return NestedSuffixShapeKind::NestedSingleGotoElse;
                }
                NestedSuffixShapeKind::NestedUnknown
            }
            HirStmt::Block(inner) if !inner.is_empty() => NestedSuffixShapeKind::NestedUnknown,
            _ => NestedSuffixShapeKind::NestedUnknown,
        }
    }

    fn resolve_suffix_redirect_to_terminal(
        body: &[HirStmt],
        target_label: &str,
        next_label: &str,
    ) -> bool {
        if Self::top_level_label_definition_count_for_owned_tail(body, target_label) != 1 {
            return false;
        }

        let mut current = target_label.to_string();
        let mut seen = HashSet::new();

        loop {
            if !seen.insert(current.clone()) {
                return false;
            }

            let ref_count = body
                .iter()
                .map(|stmt| Self::stmt_contains_goto_label(stmt, &current))
                .sum::<usize>();
            if ref_count != 1 {
                return false;
            }

            let Some(label_idx) = body
                .iter()
                .position(|stmt| matches!(stmt, HirStmt::Label(label) if label == &current))
            else {
                return false;
            };
            let next_label_idx = body[label_idx + 1..]
                .iter()
                .position(|stmt| matches!(stmt, HirStmt::Label(_)))
                .map(|offset| label_idx + 1 + offset)
                .unwrap_or(body.len());

            let mut terminal_return = false;
            let mut terminal_goto: Option<String> = None;

            for stmt in &body[label_idx + 1..next_label_idx] {
                if is_ignorable_discovery_stmt(stmt)
                    || matches!(stmt, HirStmt::Block(inner) if inner.is_empty())
                    || Self::stmt_is_pure_value_expr(stmt)
                    || Self::stmt_is_pure_value_assign(stmt)
                {
                    if terminal_return || terminal_goto.is_some() {
                        return false;
                    }
                    continue;
                }

                match stmt {
                    HirStmt::Goto(target) => {
                        if terminal_return || terminal_goto.is_some() {
                            return false;
                        }
                        terminal_goto = Some(target.clone());
                    }
                    HirStmt::Return(_) => {
                        if terminal_return || terminal_goto.is_some() {
                            return false;
                        }
                        terminal_return = true;
                    }
                    HirStmt::Break
                    | HirStmt::Continue
                    | HirStmt::If { .. }
                    | HirStmt::Switch { .. }
                    | HirStmt::While { .. }
                    | HirStmt::DoWhile { .. }
                    | HirStmt::For { .. }
                    | HirStmt::Block(_)
                    | HirStmt::VaStart { .. }
                    | HirStmt::Assign { .. }
                    | HirStmt::Expr(_)
                    | HirStmt::Label(_) => return false,
                }
            }

            if terminal_return {
                return true;
            }
            let Some(next_target) = terminal_goto else {
                return false;
            };
            if next_target == next_label {
                return true;
            }
            current = next_target;
        }
    }

    fn classify_suffix_stmt(
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        if is_ignorable_discovery_stmt(stmt) || matches!(stmt, HirStmt::Block(inner) if inner.is_empty()) {
            return Ok(());
        }
        if Self::stmt_is_pure_value_expr(stmt) || Self::stmt_is_pure_value_assign(stmt) {
            return Ok(());
        }
        if let HirStmt::Goto(target) = stmt {
            if target == next_label || Self::stmt_is_sink_safe_return_goto_for_owned_tail(stmt, body) {
                return Ok(());
            }
            if Self::top_level_label_definition_count_for_owned_tail(body, &target) != 1 {
                return Err(SuffixTailRejection::SuffixAliasRedirectUnresolved {
                    stmt_idx,
                    label: target.clone(),
                });
            }
            if Self::resolve_suffix_redirect_to_terminal(body, target, next_label) {
                return Ok(());
            }
            return Err(SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx,
                target: target.clone(),
            });
        }
        if matches!(
            stmt,
            HirStmt::Switch { .. }
                | HirStmt::While { .. }
                | HirStmt::DoWhile { .. }
                | HirStmt::For { .. }
                | HirStmt::Break
                | HirStmt::Continue
        ) {
            return Err(SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx });
        }
        if Self::suffix_stmt_has_nested_or_nonlocal_ref(stmt) {
            if Self::guarded_tail_diag_enabled() {
                let kind = Self::classify_nested_suffix_shape(
                    stmt,
                    body,
                    current_label_idx,
                    terminal_label_idx,
                    next_label,
                );
                eprintln!(
                    "[GT-TRACE] nested-suffix-shape stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx,
                    kind,
                    stmt
                );
            }
            return Err(SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx });
        }
        Err(SuffixTailRejection::SuffixHasSideEffect { stmt_idx })
    }

    fn suffix_stmt_is_terminal_join_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        next_label_idx: usize,
        terminal_label: &str,
    ) -> bool {
        let HirStmt::Goto(target) = &body[stmt_idx] else {
            return false;
        };
        if target != terminal_label {
            return false;
        }
        if Self::top_level_label_definition_count_for_owned_tail(body, terminal_label) != 1 {
            return false;
        }

        for trailing_stmt in &body[stmt_idx + 1..next_label_idx] {
            if is_ignorable_discovery_stmt(trailing_stmt)
                || matches!(trailing_stmt, HirStmt::Block(inner) if inner.is_empty())
                || Self::stmt_is_pure_value_expr(trailing_stmt)
                || Self::stmt_is_pure_value_assign(trailing_stmt)
            {
                continue;
            }

            match trailing_stmt {
                HirStmt::Goto(target) if target == terminal_label => continue,
                HirStmt::Break
                | HirStmt::Continue
                | HirStmt::Switch { .. }
                | HirStmt::While { .. }
                | HirStmt::DoWhile { .. }
                | HirStmt::For { .. }
                | HirStmt::If { .. }
                | HirStmt::Block(_)
                | HirStmt::VaStart { .. }
                | HirStmt::Assign { .. }
                | HirStmt::Expr(_)
                | HirStmt::Return(_)
                | HirStmt::Label(_) => return false,
                HirStmt::Goto(_) => return false,
            }
        }

        true
    }

    fn count_candidate_internal_top_level_refs_in_suffix_window(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= terminal_label_idx {
            return 0;
        }
        body[anchor_idx + 1..terminal_label_idx]
            .iter()
            .filter(|stmt| matches!(stmt, HirStmt::Goto(target) if target == label))
            .count()
    }

    fn count_suffix_safe_self_terminal_refs_in_suffix_window(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= terminal_label_idx {
            return 0;
        }

        let mut count = 0usize;
        for stmt_idx in anchor_idx + 1..terminal_label_idx {
            if !matches!(body.get(stmt_idx), Some(HirStmt::Goto(target)) if target == label) {
                continue;
            }
            let Some(next_label_idx) =
                (stmt_idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)))
            else {
                continue;
            };
            if next_label_idx > terminal_label_idx {
                continue;
            }
            if Self::suffix_stmt_is_terminal_join_owned_safe(body, stmt_idx, next_label_idx, label) {
                count += 1;
            }
        }
        count
    }

    fn compute_suffix_external_entry_budget(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        raw_refs: usize,
        rewrites: usize,
    ) -> SuffixExternalEntryBudget {
        let internal_candidate_refs = Self::count_candidate_internal_top_level_refs_in_suffix_window(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        );
        let suffix_safe_refs = Self::count_suffix_safe_self_terminal_refs_in_suffix_window(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        )
        .min(internal_candidate_refs);
        let guard_family_internalized_refs =
            Self::count_internalized_guard_family_nested_conditional_entries(
                body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
            );
        let internal_top_level_refs = internal_candidate_refs.saturating_sub(suffix_safe_refs);
        let effective_external_refs = raw_refs
            .saturating_sub(internal_top_level_refs)
            .saturating_sub(suffix_safe_refs);
        let effective_external_refs = effective_external_refs
            .saturating_sub(guard_family_internalized_refs);
        let allowed_external_refs = usize::from(rewrites == 0);

        SuffixExternalEntryBudget {
            raw_refs,
            internal_top_level_refs,
            suffix_safe_refs,
            guard_family_internalized_refs,
            effective_external_refs,
            allowed_external_refs,
        }
    }

    fn stmt_is_single_goto_then_if_to_label<'b>(
        stmt: &'b HirStmt,
        label: &str,
    ) -> Option<&'b HirExpr> {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if !else_body.is_empty() {
            return None;
        }
        matches!(single_goto_target(then_body), Some(target) if target == label).then_some(cond)
    }

    fn stmt_is_single_branch_if_to_label<'b>(
        stmt: &'b HirStmt,
        label: &str,
    ) -> Option<&'b HirExpr> {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if matches!(single_goto_target(then_body), Some(target) if target == label)
            && else_body.is_empty()
        {
            return Some(cond);
        }
        if matches!(single_goto_target(else_body), Some(target) if target == label)
            && then_body.is_empty()
        {
            return Some(cond);
        }
        None
    }

    fn exprs_share_guard_family(lhs: &HirExpr, rhs: &HirExpr) -> bool {
        if lhs == rhs {
            return true;
        }
        if let HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } = lhs
            && expr.as_ref() == rhs
        {
            return true;
        }
        if let HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } = rhs
            && expr.as_ref() == lhs
        {
            return true;
        }
        false
    }

    fn suffix_window_has_terminal_guard_family_match(
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &HirExpr,
    ) -> bool {
        let Some(HirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return false;
        };
        if current_label_idx + 1 >= terminal_label_idx {
            return false;
        }
        body[current_label_idx + 1..terminal_label_idx]
            .iter()
            .filter_map(|stmt| Self::stmt_is_single_branch_if_to_label(stmt, terminal_label))
            .any(|suffix_cond| Self::exprs_share_guard_family(entry_cond, suffix_cond))
    }

    fn nested_conditional_entry_is_guard_family_internal(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        stmt_idx: usize,
    ) -> bool {
        if stmt_idx <= anchor_idx || stmt_idx >= current_label_idx {
            return false;
        }
        let Some(stmt) = body.get(stmt_idx) else {
            return false;
        };
        let Some(entry_cond) = Self::stmt_is_single_goto_then_if_to_label(stmt, label) else {
            return false;
        };
        Self::suffix_window_has_terminal_guard_family_match(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
        )
    }

    fn count_internalized_guard_family_nested_conditional_entries(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> usize {
        if anchor_idx + 1 >= current_label_idx {
            return 0;
        }

        let mut count = 0usize;
        for stmt_idx in anchor_idx + 1..current_label_idx {
            let internalized = Self::nested_conditional_entry_is_guard_family_internal(
                body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
                stmt_idx,
            );
            if Self::guarded_tail_diag_enabled()
                && let Some(cond) = body
                    .get(stmt_idx)
                    .and_then(|stmt| Self::stmt_is_single_goto_then_if_to_label(stmt, label))
            {
                eprintln!(
                    "[GT-TRACE] nested-entry-probe label={} cond={:?} ref_stmt_idx={} internalized={}",
                    label,
                    cond,
                    stmt_idx,
                    internalized
                );
            }
            if !internalized {
                continue;
            }
            count += 1;
            if Self::guarded_tail_diag_enabled()
                && let Some(cond) = body
                    .get(stmt_idx)
                    .and_then(|stmt| Self::stmt_is_single_goto_then_if_to_label(stmt, label))
            {
                eprintln!(
                    "[GT-TRACE] nested-entry-internalized label={} cond={:?} ref_stmt_idx={}",
                    label,
                    cond,
                    stmt_idx
                );
            }
        }
        count
    }

    fn classify_external_entry_ref_kind_for_stmt(
        stmt: &HirStmt,
        label: &str,
    ) -> ExternalEntryRefKind {
        match stmt {
            HirStmt::Goto(target) if target == label => ExternalEntryRefKind::TopLevelExternalGoto,
            HirStmt::If { .. }
                if Self::stmt_contains_goto_label(stmt, label) > 0 =>
            {
                ExternalEntryRefKind::NestedConditionalGoto
            }
            HirStmt::Switch { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
                if Self::stmt_contains_goto_label(stmt, label) > 0 =>
            {
                ExternalEntryRefKind::LoopSwitchDerived
            }
            HirStmt::Block(_)
                if Self::stmt_contains_goto_label(stmt, label) > 0 =>
            {
                ExternalEntryRefKind::AliasRedirectDerived
            }
            _ => ExternalEntryRefKind::UnknownExternalEntry,
        }
    }

    fn classify_external_entry_ref_kind(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
    ) -> Option<(ExternalEntryRefKind, usize)> {
        for (stmt_idx, stmt) in body.iter().enumerate() {
            if Self::stmt_contains_goto_label(stmt, label) == 0 {
                continue;
            }
            if stmt_idx > anchor_idx
                && stmt_idx < terminal_label_idx
                && matches!(stmt, HirStmt::Goto(target) if target == label)
            {
                continue;
            }
            return Some((
                Self::classify_external_entry_ref_kind_for_stmt(stmt, label),
                stmt_idx,
            ));
        }
        None
    }

    fn suffix_is_nonowned_terminal_tail(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label: &str,
        start_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        if start_label_idx >= terminal_label_idx {
            return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: start_label_idx,
                label: start_label.to_string(),
            });
        }

        let mut current_label = start_label.to_string();
        let mut current_label_idx = start_label_idx;
        let mut rewrites = 0usize;
        let mut seen = HashSet::new();

        while current_label_idx < terminal_label_idx {
            if !seen.insert(current_label.clone()) {
                return Err(SuffixTailRejection::SuffixAliasRedirectUnresolved {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            }

            let raw_refs = referenced.get(&current_label).copied().unwrap_or(0);
            let budget = Self::compute_suffix_external_entry_budget(
                body,
                &current_label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
                raw_refs,
                rewrites,
            );
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] suffix-budget label={} raw_refs={} internal_refs={} suffix_safe_refs={} guard_family_internalized_refs={} effective_external={} allowed_external={}",
                    current_label,
                    budget.raw_refs,
                    budget.internal_top_level_refs,
                    budget.suffix_safe_refs,
                    budget.guard_family_internalized_refs,
                    budget.effective_external_refs,
                    budget.allowed_external_refs,
                );
            }
            if budget.effective_external_refs > budget.allowed_external_refs {
                if Self::guarded_tail_diag_enabled()
                    && let Some((kind, ref_stmt_idx)) = Self::classify_external_entry_ref_kind(
                        body,
                        &current_label,
                        anchor_idx,
                        terminal_label_idx,
                    )
                    && let Some(ref_stmt) = body.get(ref_stmt_idx)
                {
                    eprintln!(
                        "[GT-TRACE] suffix-external-entry label={} external_entry_kind={:?} ref_stmt_idx={} ref_stmt={:?}",
                        current_label,
                        kind,
                        ref_stmt_idx,
                        ref_stmt
                    );
                }
                return Err(SuffixTailRejection::SuffixHasExternalEntry {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            }

            let Some(next_label_idx) =
                (current_label_idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)))
            else {
                return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            };
            if next_label_idx > terminal_label_idx {
                return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                    stmt_idx: next_label_idx,
                    label: current_label,
                });
            }
            let HirStmt::Label(terminal_label) = &body[terminal_label_idx] else {
                unreachable!();
            };
            let HirStmt::Label(next_label) = &body[next_label_idx] else {
                unreachable!();
            };
            for (offset, stmt) in body[current_label_idx + 1..next_label_idx]
                .iter()
                .enumerate()
            {
                let stmt_idx = current_label_idx + 1 + offset;
                if matches!(stmt, HirStmt::Goto(target) if target == terminal_label)
                    && Self::suffix_stmt_is_terminal_join_owned_safe(
                        body,
                        stmt_idx,
                        next_label_idx,
                        terminal_label,
                    )
                {
                    continue;
                }
                Self::classify_suffix_stmt(
                    stmt,
                    body,
                    stmt_idx,
                    current_label_idx,
                    terminal_label_idx,
                    next_label,
                )?;
            }

            current_label = next_label.clone();
            current_label_idx = next_label_idx;
            rewrites += 1;
        }

        Ok(())
    }

    fn candidate_window_can_shrink_to_label(
        body: &[HirStmt],
        anchor_idx: usize,
        candidate_label: &str,
        candidate_label_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(), SuffixTailRejection> {
        if candidate_label_idx >= terminal_label_idx {
            return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: candidate_label_idx,
                label: candidate_label.to_string(),
            });
        }
        if !has_non_ignorable_payload(&body[anchor_idx + 1..candidate_label_idx]) {
            return Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: candidate_label_idx,
                label: candidate_label.to_string(),
            });
        }
        Self::suffix_is_nonowned_terminal_tail(
                body,
                anchor_idx,
                candidate_label,
                candidate_label_idx,
                terminal_label_idx,
                referenced,
            )
    }

    fn find_earliest_owned_join_label(
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        if anchor_idx + 1 >= terminal_label_idx {
            return None;
        }

        for candidate_label_idx in anchor_idx + 1..terminal_label_idx {
            let HirStmt::Label(candidate_label) = &body[candidate_label_idx] else {
                continue;
            };
            let has_payload = has_non_ignorable_payload(&body[anchor_idx + 1..candidate_label_idx]);
            let suffix_result = Self::candidate_window_can_shrink_to_label(
                body,
                anchor_idx,
                candidate_label,
                candidate_label_idx,
                terminal_label_idx,
                referenced,
            );
            let suffix_safe = suffix_result.is_ok();
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] owned-join candidate anchor={} label={} label_idx={} terminal_idx={} payload_before={} suffix_safe={}",
                    anchor_idx,
                    candidate_label,
                    candidate_label_idx,
                    terminal_label_idx,
                    has_payload,
                    suffix_safe
                );
            }
            if trace_enabled
                && anchor_idx == 35
                && let Err(reason) = &suffix_result
                && let Some(stmt) = body.get(reason.stmt_idx())
            {
                eprintln!(
                    "[GT-TRACE] candidate={} join_label={} early_label={} first_fail={:?} stmt_idx={} first_fail_stmt={:?}",
                    anchor_idx,
                    match body.get(terminal_label_idx) {
                        Some(HirStmt::Label(label)) => label.as_str(),
                        _ => "<missing-terminal-label>",
                    },
                    candidate_label,
                    reason,
                    reason.stmt_idx(),
                    stmt
                );
            }
            if has_payload && suffix_safe {
                return Some((candidate_label.clone(), candidate_label_idx));
            }
        }

        None
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
        if self.guarded_tail_trace_enabled_for_current_fn() {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} canonicalization_failure={:?}",
                self.guarded_tail_function_address(),
                reason
            );
        }
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

        let (initial_target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
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

        let Some(original_label_idx) =
            Self::find_top_level_label_after(body, idx, &initial_target_label)
        else {
            return None;
        };
        if !has_non_ignorable_payload(&body[idx + 1..original_label_idx]) {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::NonCanonicalLayout
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_label_idx],
                    20,
                );
            }
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
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    original_label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..original_tail_end],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::AmbiguousFollow));
        }

        let Some((resolved_target_label, resolved_label_idx)) =
            self.resolve_terminal_join_target(body, idx, &initial_target_label, referenced)
        else {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    initial_target_label,
                    GuardedTailWitnessRejection::MissingTerminalJoin
                );
                let upper = body.len().min(idx + 1 + 20);
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..upper],
                    20,
                );
            }
            return Some(Err(GuardedTailWitnessRejection::MissingTerminalJoin));
        };

        let (owned_join_label, label_idx) = Self::find_earliest_owned_join_label(
            body,
            idx,
            resolved_label_idx,
            referenced,
            self.guarded_tail_trace_enabled_for_current_fn(),
        )
        .unwrap_or_else(|| (resolved_target_label.clone(), resolved_label_idx));
        let target_label = resolved_target_label.clone();

        if self.guarded_tail_trace_enabled_for_current_fn() && label_idx != resolved_label_idx {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} owned_join_narrowed from={}({}) to={}({})",
                self.guarded_tail_function_address(),
                idx,
                resolved_target_label,
                resolved_label_idx,
                owned_join_label,
                label_idx
            );
        }

        if self.guarded_tail_trace_enabled_for_current_fn() {
            let raw_middle = &body[idx + 1..label_idx];
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} raw_middle_len={}",
                self.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                raw_middle.len()
            );
        }

        let (middle, external_redirects) = match self.canonicalize_guarded_tail_segment(
            &body[idx + 1..label_idx],
            body,
            idx + 1,
            referenced,
        ) {
            Ok(middle) => middle,
            Err(reason) => {
                if self.guarded_tail_trace_enabled_for_current_fn() {
                    eprintln!(
                        "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                        self.guarded_tail_function_address(),
                        idx,
                        target_label,
                        label_idx,
                        reason
                    );
                    Self::guarded_tail_trace_emit_snapshot(
                        "[GT-TRACE] reject_snapshot",
                        &body[idx + 1..label_idx],
                        20,
                    );
                }
                self.mark_guarded_tail_canonicalization_failure(reason);
                return Some(Err(Self::map_guarded_tail_canonicalization_rejection(
                    reason,
                )));
            }
        };
        if middle.is_empty() {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailCanonicalizationFailure::InterleavedJoinUses
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..label_idx],
                    20,
                );
            }
            self.mark_guarded_tail_canonicalization_failure(
                GuardedTailCanonicalizationFailure::InterleavedJoinUses,
            );
            return Some(Err(GuardedTailWitnessRejection::AliasInterleaveConflict));
        }

        if self.guarded_tail_trace_enabled_for_current_fn() {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} canonical_middle_len={} external_redirects={:?}",
                self.guarded_tail_function_address(),
                idx,
                target_label,
                label_idx,
                middle.len(),
                external_redirects
            );
        }

        let tail_end = (label_idx + 1..body.len())
            .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
            .unwrap_or(body.len());
        if body[label_idx + 1..tail_end].is_empty() && label_idx + 1 != body.len() {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    target_label,
                    label_idx,
                    GuardedTailWitnessRejection::AmbiguousFollow
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &body[idx + 1..tail_end],
                    20,
                );
            }
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
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                    self.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    GuardedTailExecutionRejection::Witness(GuardedTailWitnessRejection::NonCanonicalLayout)
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &witness.middle,
                    20,
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

        let rewritten = Self::rewrite_guarded_tail_sequence(
            &witness.middle,
            &witness.target_label,
            &[],
        );
        let (outside_refs, middle_refs) = Self::surviving_label_refs_after_guarded_tail_promotion(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
        );
        let effective_middle_refs = Self::effective_middle_refs_for_promotion(
            &rewritten.stmts,
            &witness.target_label,
            middle_refs,
        );
        let execution_safe =
            Self::guarded_tail_middle_is_execution_safe(&rewritten.stmts, &witness.target_label);
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[DIAG] guarded-tail verify idx={} label={} outside_refs={} middle_refs={} effective_middle_refs={} unresolved_join_refs={} execution_safe={}",
                idx,
                witness.target_label,
                outside_refs,
                middle_refs,
                effective_middle_refs,
                rewritten.unresolved_join_refs,
                execution_safe,
            );
        }
        if let Some(rejection) = Self::classify_must_emit_label_rejection(
            body,
            &rewritten.stmts,
            idx,
            witness.label_idx,
            &witness.target_label,
            outside_refs,
            middle_refs,
        ) {
            self.mark_promotion_gate_rejection(rejection);
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[DIAG] guarded-tail verify idx={} label={} rejected=MustEmitLabelConflict({:?})",
                    idx, witness.target_label, rejection
                );
            }
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject=MustEmitLabelConflict({:?})",
                    self.guarded_tail_function_address(),
                    idx,
                    witness.target_label,
                    witness.label_idx,
                    rejection
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] reject_snapshot",
                    &rewritten.stmts,
                    20,
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
                if self.guarded_tail_trace_enabled_for_current_fn() {
                    eprintln!(
                        "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                        self.guarded_tail_function_address(),
                        idx,
                        witness.target_label,
                        witness.label_idx,
                        reason
                    );
                    Self::guarded_tail_trace_emit_snapshot(
                        "[GT-TRACE] reject_snapshot",
                        &rewritten.stmts,
                        20,
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

        if self.guarded_tail_trace_enabled_for_current_fn() {
            let reason = if !removable_ops_legal {
                GuardedTailExecutionRejection::MustEmitLabelConflict
            } else {
                GuardedTailExecutionRejection::ReplacementIncomplete
            };
            eprintln!(
                "[GT-TRACE] fn=0x{:x} candidate={} join_label={} label_idx={} first_reject={:?}",
                self.guarded_tail_function_address(),
                idx,
                witness.target_label,
                witness.label_idx,
                reason
            );
            Self::guarded_tail_trace_emit_snapshot(
                "[GT-TRACE] reject_snapshot",
                &rewritten.stmts,
                20,
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

#[cfg(test)]
mod owned_join_window_tests {
    use super::*;

    fn test_if_goto(label: &str) -> HirStmt {
        HirStmt::If {
            cond: HirExpr::Var("cond".to_string()),
            then_body: vec![HirStmt::Goto(label.to_string())],
            else_body: Vec::new(),
        }
    }

    fn assert_suffix_rejection(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label_idx: usize,
        terminal_label_idx: usize,
        expected: SuffixTailRejection,
    ) {
        let referenced = collect_referenced_label_counts(body);
        let HirStmt::Label(start_label) = &body[start_label_idx] else {
            panic!("expected start label at index {start_label_idx}");
        };
        let result = PreviewBuilder::suffix_is_nonowned_terminal_tail(
            body,
            anchor_idx,
            start_label,
            start_label_idx,
            terminal_label_idx,
            &referenced,
        );
        assert_eq!(result, Err(expected));
    }

    fn assert_suffix_accepts(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label_idx: usize,
        terminal_label_idx: usize,
    ) {
        let referenced = collect_referenced_label_counts(body);
        let HirStmt::Label(start_label) = &body[start_label_idx] else {
            panic!("expected start label at index {start_label_idx}");
        };
        let result = PreviewBuilder::suffix_is_nonowned_terminal_tail(
            body,
            anchor_idx,
            start_label,
            start_label_idx,
            terminal_label_idx,
            &referenced,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_ok(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) {
        let stmt = &body[stmt_idx];
        let result = PreviewBuilder::classify_suffix_stmt(
            stmt,
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_nested_suffix_shape_kind(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: NestedSuffixShapeKind,
    ) {
        let stmt = &body[stmt_idx];
        let kind = PreviewBuilder::classify_nested_suffix_shape(
            stmt,
            body,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(kind, expected);
    }

    fn assert_suffix_external_budget(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        rewrites: usize,
        expected: SuffixExternalEntryBudget,
    ) {
        let referenced = collect_referenced_label_counts(body);
        let raw_refs = referenced.get(label).copied().unwrap_or(0);
        let budget = PreviewBuilder::compute_suffix_external_entry_budget(
            body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
            rewrites,
        );
        assert_eq!(budget, expected);
    }

    fn assert_external_entry_ref_kind(
        body: &[HirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
        expected: Option<(ExternalEntryRefKind, usize)>,
    ) {
        let classified = PreviewBuilder::classify_external_entry_ref_kind(
            body,
            label,
            anchor_idx,
            terminal_label_idx,
        );
        assert_eq!(classified, expected);
    }

    #[test]
    fn earliest_owned_join_window_accepts_sink_safe_terminal_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join1".to_string()),
            HirStmt::Label("join1".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 6, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_empty_block_alias_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_alias_redirect_only_suffix() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join1".to_string()),
            HirStmt::Label("join1".to_string()),
            HirStmt::Goto("join2".to_string()),
            HirStmt::Label("join2".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 8, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_rejects_side_effectful_suffix() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("not_safe".to_string())),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_external_entry_in_suffix() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 1, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_nested_nonlocal_suffix_ref() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("sink".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 4, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_when_terminal_join_is_already_owned() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed =
            PreviewBuilder::find_earliest_owned_join_label(&body, 0, 2, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn suffix_accepts_ignorable_and_empty_block_only() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 5);
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_next_label() {
        let body = vec![
            HirStmt::Goto("skip".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Label("skip".to_string()),
            HirStmt::Expr(HirExpr::Var("redirect_gap".to_string())),
            HirStmt::Goto("alias".to_string()),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 3, "alias");
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_terminal_return() {
        let body = vec![
            HirStmt::Goto("skip".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Label("skip".to_string()),
            HirStmt::Expr(HirExpr::Var("redirect_gap".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("done".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 6, "alias");
    }

    #[test]
    fn suffix_accepts_self_terminal_join_goto_with_pure_tail() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Expr(HirExpr::Var("pure_gap".to_string())),
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 9);
    }

    #[test]
    fn suffix_budget_counts_candidate_internal_top_level_refs_inside_window() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join0".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 1,
                guard_family_internalized_refs: 0,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_keeps_nested_candidate_ref_external() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_internalizes_same_guard_family_nested_conditional_entry() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 1,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_different_guard_family_nested_entry() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("other_cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_single_goto_then() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("next".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedSingleGotoThen,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_guard_family_mismatch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("other".to_string()),
                then_body: vec![HirStmt::Goto("next".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedGuardFamilyMismatch,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_crosses_terminal_join() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            4,
            "next",
            NestedSuffixShapeKind::NestedCrossesTerminalJoin,
        );
    }

    #[test]
    fn external_entry_kind_classifies_top_level_external_goto() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            5,
            Some((ExternalEntryRefKind::TopLevelExternalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_nested_conditional_goto() {
        let body = vec![
            test_if_goto("anchor"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            0,
            4,
            Some((ExternalEntryRefKind::NestedConditionalGoto, 3)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_loop_switch_derived_goto() {
        let body = vec![
            test_if_goto("anchor"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Switch {
                expr: HirExpr::Var("selector".to_string()),
                cases: vec![HirSwitchCase {
                    values: vec![0],
                    body: vec![HirStmt::Goto("join0".to_string())],
                }],
                default: vec![],
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            0,
            4,
            Some((ExternalEntryRefKind::LoopSwitchDerived, 3)),
        );
    }

    #[test]
    fn external_entry_kind_skips_candidate_internal_top_level_goto() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("external".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join0".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            5,
            Some((ExternalEntryRefKind::NestedConditionalGoto, 0)),
        );
    }

    #[test]
    fn suffix_rejects_self_terminal_join_goto_with_nested_tail_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            7,
            SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx: 3,
                target: "terminal".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_side_effectful_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::VaStart {
                va_list: HirExpr::Var("ap".to_string()),
                last_named_param: "arg".to_string(),
            },
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            5,
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 3 },
        );
    }

    #[test]
    fn suffix_rejects_nonterminal_goto() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("other".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            HirStmt::Label("other".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("late".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("done".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            4,
            SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx: 3,
                target: "other".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_nested_nonlocal_ref() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("nested".to_string()),
                then_body: vec![HirStmt::Goto("sink".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            4,
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 3 },
        );
    }

    #[test]
    fn suffix_rejects_label_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            4,
            4,
            SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: 4,
                label: "sink".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_external_entry() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("alias".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Goto("sink".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            HirStmt::Goto("alias".to_string()),
        ];

        assert_suffix_rejection(
            &body,
            1,
            3,
            7,
            SuffixTailRejection::SuffixHasExternalEntry {
                stmt_idx: 3,
                label: "join0".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_loop_or_switch_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("alias".to_string()),
            HirStmt::Label("alias".to_string()),
            HirStmt::Switch {
                expr: HirExpr::Var("selector".to_string()),
                cases: vec![HirSwitchCase {
                    values: vec![0],
                    body: vec![HirStmt::Goto("sink".to_string())],
                }],
                default: vec![HirStmt::Goto("sink".to_string())],
            },
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            6,
            SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx: 5 },
        );
    }

    #[test]
    fn suffix_rejects_unresolved_alias_redirect() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("missing".to_string()),
            HirStmt::Label("sink".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_rejection(
            &body,
            0,
            2,
            4,
            SuffixTailRejection::SuffixAliasRedirectUnresolved {
                stmt_idx: 3,
                label: "missing".to_string(),
            },
        );
    }
}
