use super::*;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuffixSideEffectShapeKind {
    PureRegisterAssign,
    PureTempAssign,
    MemoryReadOnlyAssign,
    CallExprSideEffect,
    MemoryWrite,
    VolatileOrUnknownLoad,
    CompoundAssignOrPhiLike,
    UnknownSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuffixCallEffectShapeKind {
    VoidUnknownCall,
    ReturnValueIgnoredCall,
    ReturnValueAssignedLocal,
    PureKnownHelperCall,
    MemoryMutatingCall,
    ControlEffectCall,
    UnknownCallEffect,
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
                let then_target = single_goto_target(then_body);
                let else_target = single_goto_target(else_body);
                if then_body.len() > 1 || else_body.len() > 1 {
                    return NestedSuffixShapeKind::NestedMultiStmtBranch;
                }
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

    fn expr_contains_load(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Load { .. } => true,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::expr_contains_load(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_contains_load(lhs) || Self::expr_contains_load(rhs)
            }
            HirExpr::Call { args, .. } => args.iter().any(Self::expr_contains_load),
            HirExpr::PtrOffset { base, .. } => Self::expr_contains_load(base),
            HirExpr::Index { base, index, .. } => {
                Self::expr_contains_load(base) || Self::expr_contains_load(index)
            }
            HirExpr::Var(_) | HirExpr::Const(_, _) => false,
        }
    }

    fn suffix_expr_contains_call(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::suffix_expr_contains_call(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::suffix_expr_contains_call(lhs) || Self::suffix_expr_contains_call(rhs)
            }
            HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => {
                Self::suffix_expr_contains_call(ptr)
            }
            HirExpr::Index { base, index, .. } => {
                Self::suffix_expr_contains_call(base) || Self::suffix_expr_contains_call(index)
            }
            HirExpr::Var(_) | HirExpr::Const(_, _) => false,
        }
    }

    fn classify_suffix_side_effect_shape(stmt: &HirStmt) -> SuffixSideEffectShapeKind {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Deref { .. } | HirLValue::Index { .. },
                ..
            } => SuffixSideEffectShapeKind::MemoryWrite,
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } if Self::expr_is_pure_value(rhs) => match rhs {
                HirExpr::Var(_) => SuffixSideEffectShapeKind::PureTempAssign,
                _ => SuffixSideEffectShapeKind::PureRegisterAssign,
            },
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs: HirExpr::Load { ptr, .. },
            } if Self::expr_is_pure_value(ptr) => SuffixSideEffectShapeKind::MemoryReadOnlyAssign,
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } if Self::suffix_expr_contains_call(rhs) => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } if Self::expr_contains_load(rhs) => SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if Self::expr_contains_var(rhs, name)
                || matches!(rhs, HirExpr::AggregateCopy { .. }) =>
            {
                SuffixSideEffectShapeKind::CompoundAssignOrPhiLike
            }
            HirStmt::Expr(HirExpr::Call { .. }) | HirStmt::VaStart { .. } => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            HirStmt::Expr(HirExpr::Load { .. }) => SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
            HirStmt::Expr(expr) if Self::suffix_expr_contains_call(expr) => {
                SuffixSideEffectShapeKind::CallExprSideEffect
            }
            HirStmt::Expr(expr) if Self::expr_contains_load(expr) => {
                SuffixSideEffectShapeKind::VolatileOrUnknownLoad
            }
            HirStmt::Assign { .. } => SuffixSideEffectShapeKind::UnknownSideEffect,
            _ => SuffixSideEffectShapeKind::UnknownSideEffect,
        }
    }

    fn call_target_is_known_pure_helper(target: &str) -> bool {
        matches!(target, "__popcount")
    }

    fn call_target_is_memory_mutating(target: &str) -> bool {
        let lowered = target.to_ascii_lowercase();
        matches!(
            lowered.as_str(),
            "memcpy"
                | "memmove"
                | "memset"
                | "strcpy"
                | "strncpy"
                | "strcat"
                | "strncat"
                | "wcscpy"
                | "wcsncpy"
                | "wmemcpy"
                | "wmemmove"
                | "wmemset"
        )
    }

    fn call_target_is_control_effect(target: &str) -> bool {
        let lowered = target.to_ascii_lowercase();
        matches!(
            lowered.as_str(),
            "abort"
                | "exit"
                | "_exit"
                | "panic"
                | "__assert_fail"
                | "longjmp"
                | "_longjmp"
                | "raiseexception"
                | "__cxa_throw"
        )
    }

    fn classify_suffix_call_effect_shape(stmt: &HirStmt) -> SuffixCallEffectShapeKind {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs: HirExpr::Call { target, args, .. },
            }
            | HirStmt::Expr(HirExpr::Call { target, args, .. }) => {
                if Self::call_target_is_control_effect(target) {
                    return SuffixCallEffectShapeKind::ControlEffectCall;
                }
                if Self::call_target_is_memory_mutating(target) {
                    return SuffixCallEffectShapeKind::MemoryMutatingCall;
                }
                if Self::call_target_is_known_pure_helper(target)
                    && args.iter().all(Self::expr_is_pure_value)
                {
                    return SuffixCallEffectShapeKind::PureKnownHelperCall;
                }
                match stmt {
                    HirStmt::Assign { .. } => SuffixCallEffectShapeKind::ReturnValueAssignedLocal,
                    HirStmt::Expr(HirExpr::Call { ty, .. }) if matches!(ty, NirType::Unknown) => {
                        SuffixCallEffectShapeKind::VoidUnknownCall
                    }
                    HirStmt::Expr(HirExpr::Call { .. }) => {
                        SuffixCallEffectShapeKind::ReturnValueIgnoredCall
                    }
                    _ => SuffixCallEffectShapeKind::UnknownCallEffect,
                }
            }
            HirStmt::Assign { .. } | HirStmt::Expr(_) | HirStmt::VaStart { .. } => {
                SuffixCallEffectShapeKind::UnknownCallEffect
            }
            _ => SuffixCallEffectShapeKind::UnknownCallEffect,
        }
    }

    fn stmt_reads_binding_only_in_owned_safe_context(stmt: &HirStmt, name: &str) -> bool {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if Self::lvalue_contains_var(lhs, name) {
                    return false;
                }
                !Self::expr_contains_var(rhs, name) || Self::expr_is_pure_value(rhs)
            }
            HirStmt::Expr(expr) => {
                !Self::expr_contains_var(expr, name) || Self::expr_is_pure_value(expr)
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                (!Self::expr_contains_var(cond, name) || Self::expr_is_pure_value(cond))
                    && then_body
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && else_body
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            HirStmt::Block(stmts) => stmts
                .iter()
                .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name)),
            HirStmt::VaStart { va_list, .. } => !Self::expr_contains_var(va_list, name),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                !Self::expr_contains_var(expr, name)
                    && cases.iter().all(|case| {
                        case.body.iter().all(|stmt| {
                            Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name)
                        })
                    })
                    && default
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
                !Self::expr_contains_var(cond, name)
                    && body
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.iter()
                    .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && cond
                        .as_ref()
                        .is_none_or(|expr| !Self::expr_contains_var(expr, name))
                    && update
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
                    && body
                        .iter()
                        .all(|stmt| Self::stmt_reads_binding_only_in_owned_safe_context(stmt, name))
            }
            HirStmt::Return(Some(expr)) => !Self::expr_contains_var(expr, name),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => true,
        }
    }

    fn suffix_memory_read_only_assign_is_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(HirStmt::Assign {
            lhs: HirLValue::Var(binding_name),
            rhs: HirExpr::Load { ptr, ty },
        }) = body.get(stmt_idx)
        else {
            return false;
        };

        if !Self::expr_is_pure_value(ptr) || matches!(ty, NirType::Unknown) {
            return false;
        }

        if body[stmt_idx + 1..]
            .iter()
            .map(|stmt| Self::count_var_defs_stmt(stmt, binding_name))
            .sum::<usize>()
            > 0
        {
            return false;
        }

        if body[stmt_idx + 1..terminal_label_idx]
            .iter()
            .any(|stmt| !Self::stmt_reads_binding_only_in_owned_safe_context(stmt, binding_name))
        {
            return false;
        }

        body[terminal_label_idx..]
            .iter()
            .all(|stmt| Self::count_var_reads_stmt(stmt, binding_name) == 0)
    }

    fn suffix_known_pure_helper_call_is_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(HirStmt::Assign {
            lhs: HirLValue::Var(binding_name),
            rhs: HirExpr::Call { target, args, .. },
        }) = body.get(stmt_idx)
        else {
            return false;
        };

        if !Self::call_target_is_known_pure_helper(target)
            || !args.iter().all(Self::expr_is_pure_value)
        {
            return false;
        }

        if body[stmt_idx + 1..]
            .iter()
            .map(|stmt| Self::count_var_defs_stmt(stmt, binding_name))
            .sum::<usize>()
            > 0
        {
            return false;
        }

        if body[stmt_idx + 1..terminal_label_idx]
            .iter()
            .any(|stmt| !Self::stmt_reads_binding_only_in_owned_safe_context(stmt, binding_name))
        {
            return false;
        }

        body[terminal_label_idx..]
            .iter()
            .all(|stmt| Self::count_var_reads_stmt(stmt, binding_name) == 0)
    }

    fn resolve_suffix_redirect_to_terminal(
        body: &[HirStmt],
        target_label: &str,
        next_label: &str,
    ) -> bool {
        if target_label == next_label {
            return true;
        }
        if Self::top_level_label_definition_count_for_owned_tail(body, target_label) != 1 {
            return false;
        }
        let Some(mut current_idx) = body
            .iter()
            .position(|stmt| matches!(stmt, HirStmt::Label(label) if label == target_label))
        else {
            return false;
        };
        let mut current = target_label.to_string();
        let mut seen = HashSet::new();

        while current != next_label {
            if !seen.insert(current.clone()) {
                return false;
            }

            let Some(next_label_idx) =
                (current_idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)))
            else {
                return false;
            };

            let mut terminal_return = false;
            let mut terminal_goto = None::<String>;
            for stmt in &body[current_idx + 1..next_label_idx] {
                match stmt {
                    HirStmt::Goto(target) => terminal_goto = Some(target.clone()),
                    HirStmt::Return(_) => terminal_return = true,
                    stmt if is_ignorable_discovery_stmt(stmt) => {}
                    HirStmt::Block(inner) if inner.is_empty() => {}
                    _ => return false,
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
            let Some(next_idx) = body
                .iter()
                .position(|stmt| matches!(stmt, HirStmt::Label(label) if label == &next_target))
            else {
                return false;
            };
            current = next_target;
            current_idx = next_idx;
        }

        true
    }

    fn classify_suffix_stmt(
        stmt: &HirStmt,
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) -> Result<(), SuffixTailRejection> {
        if is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, HirStmt::Block(inner) if inner.is_empty())
        {
            return Ok(());
        }
        if Self::stmt_is_pure_value_expr(stmt) || Self::stmt_is_pure_value_assign(stmt) {
            return Ok(());
        }
        if let HirStmt::Goto(target) = stmt {
            if target == next_label
                || Self::stmt_is_sink_safe_return_goto_for_owned_tail(stmt, body)
            {
                return Ok(());
            }
            if Self::top_level_label_definition_count_for_owned_tail(body, target) != 1 {
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
            let kind = Self::classify_nested_suffix_shape(
                stmt,
                body,
                current_label_idx,
                terminal_label_idx,
                next_label,
            );
            if kind == NestedSuffixShapeKind::NestedCrossesTerminalJoin
                && Self::nested_terminal_join_tail_is_guard_family_owned_safe(
                    body,
                    stmt_idx,
                    current_label_idx,
                    terminal_label_idx,
                )
            {
                if Self::guarded_tail_diag_enabled() {
                    eprintln!(
                        "[GT-TRACE] nested-terminal-join-tail-internalized stmt_idx={} kind={:?} stmt={:?}",
                        stmt_idx, kind, stmt
                    );
                }
                return Ok(());
            }
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] nested-suffix-shape stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, kind, stmt
                );
            }
            return Err(SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx });
        }
        let side_effect_kind = Self::classify_suffix_side_effect_shape(stmt);
        if side_effect_kind == SuffixSideEffectShapeKind::MemoryReadOnlyAssign
            && Self::suffix_memory_read_only_assign_is_owned_safe(
                body,
                stmt_idx,
                terminal_label_idx,
            )
        {
            if Self::guarded_tail_diag_enabled() {
                eprintln!(
                    "[GT-TRACE] suffix-memory-readonly-assign-internalized stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, side_effect_kind, stmt
                );
            }
            return Ok(());
        }
        if side_effect_kind == SuffixSideEffectShapeKind::CallExprSideEffect {
            let call_kind = Self::classify_suffix_call_effect_shape(stmt);
            if call_kind == SuffixCallEffectShapeKind::PureKnownHelperCall
                && Self::suffix_known_pure_helper_call_is_owned_safe(
                    body,
                    stmt_idx,
                    terminal_label_idx,
                )
            {
                if Self::guarded_tail_diag_enabled() {
                    eprintln!(
                        "[GT-TRACE] suffix-known-pure-helper-call-internalized stmt_idx={} kind={:?} stmt={:?}",
                        stmt_idx, call_kind, stmt
                    );
                }
                return Ok(());
            }
        }
        if Self::guarded_tail_diag_enabled() {
            if side_effect_kind == SuffixSideEffectShapeKind::CallExprSideEffect {
                let call_kind = Self::classify_suffix_call_effect_shape(stmt);
                eprintln!(
                    "[GT-TRACE] suffix-call-effect-shape stmt_idx={} kind={:?} stmt={:?}",
                    stmt_idx, call_kind, stmt
                );
            }
            eprintln!(
                "[GT-TRACE] suffix-side-effect-shape stmt_idx={} kind={:?} stmt={:?}",
                stmt_idx, side_effect_kind, stmt
            );
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
            if Self::suffix_stmt_is_terminal_join_owned_safe(body, stmt_idx, next_label_idx, label)
            {
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
        let internal_candidate_refs =
            Self::count_candidate_internal_top_level_refs_in_suffix_window(
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
        let effective_external_refs =
            effective_external_refs.saturating_sub(guard_family_internalized_refs);
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

    fn find_terminal_guard_family_match_excluding(
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &HirExpr,
        excluded_stmt_idx: Option<usize>,
    ) -> Option<HirExpr> {
        let Some(HirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return None;
        };
        if current_label_idx + 1 >= terminal_label_idx {
            return None;
        }
        body[current_label_idx + 1..terminal_label_idx]
            .iter()
            .enumerate()
            .filter(|(offset, _)| {
                let absolute_idx = current_label_idx + 1 + offset;
                excluded_stmt_idx != Some(absolute_idx)
            })
            .filter_map(|(_, stmt)| Self::stmt_is_single_branch_if_to_label(stmt, terminal_label))
            .find(|suffix_cond| Self::exprs_share_guard_family(entry_cond, suffix_cond))
            .cloned()
    }

    fn suffix_window_has_terminal_guard_family_match(
        body: &[HirStmt],
        current_label_idx: usize,
        terminal_label_idx: usize,
        entry_cond: &HirExpr,
    ) -> bool {
        Self::find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            None,
        )
        .is_some()
    }

    fn nested_terminal_join_tail_is_guard_family_owned_safe(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
    ) -> bool {
        let Some(HirStmt::Label(terminal_label)) = body.get(terminal_label_idx) else {
            return false;
        };
        let Some(stmt) = body.get(stmt_idx) else {
            return false;
        };
        let Some(entry_cond) = Self::stmt_is_single_branch_if_to_label(stmt, terminal_label) else {
            return false;
        };
        let matched_cond = Self::find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            Some(stmt_idx),
        );
        let result = matched_cond.is_some();
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] nested-terminal-join-proof stmt_idx={} terminal_label={} entry_cond={:?} matched_cond={:?} result={}",
                stmt_idx,
                terminal_label,
                entry_cond,
                matched_cond,
                result
            );
        }
        result
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
        let matched_cond = Self::find_terminal_guard_family_match_excluding(
            body,
            current_label_idx,
            terminal_label_idx,
            entry_cond,
            None,
        );
        let result = matched_cond.is_some();
        if Self::guarded_tail_diag_enabled() {
            eprintln!(
                "[GT-TRACE] nested-entry-guard-family-proof label={} ref_stmt_idx={} entry_cond={:?} matched_cond={:?} result={}",
                label,
                stmt_idx,
                entry_cond,
                matched_cond,
                result
            );
        }
        result
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
                    label, cond, stmt_idx, internalized
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
                    label, cond, stmt_idx
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
            HirStmt::If { .. } if Self::stmt_contains_goto_label(stmt, label) > 0 => {
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
            HirStmt::Block(_) if Self::stmt_contains_goto_label(stmt, label) > 0 => {
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
                        current_label, kind, ref_stmt_idx, ref_stmt
                    );
                }
                return Err(SuffixTailRejection::SuffixHasExternalEntry {
                    stmt_idx: current_label_idx,
                    label: current_label,
                });
            }

            let Some(next_label_idx) = (current_label_idx + 1..body.len())
                .find(|pos| matches!(body[*pos], HirStmt::Label(_)))
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

    pub(super) fn find_earliest_owned_join_label(
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_if_goto(label: &str) -> HirStmt {
        HirStmt::If {
            cond: HirExpr::Var("cond".to_string()),
            then_body: vec![HirStmt::Goto(label.to_string())],
            else_body: Vec::new(),
        }
    }

    fn assert_suffix_accepts(
        body: &[HirStmt],
        anchor_idx: usize,
        start_label_idx: usize,
        terminal_label_idx: usize,
    ) {
        let HirStmt::Label(start_label) = &body[start_label_idx] else {
            panic!("start label missing at {start_label_idx}");
        };
        let referenced = collect_referenced_label_counts(body);
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
        let result = PreviewBuilder::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_rejection(
        body: &[HirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: SuffixTailRejection,
    ) {
        let result = PreviewBuilder::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Err(expected));
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

    fn assert_suffix_side_effect_shape_kind(stmt: HirStmt, expected: SuffixSideEffectShapeKind) {
        let kind = PreviewBuilder::classify_suffix_side_effect_shape(&stmt);
        assert_eq!(kind, expected);
    }

    fn assert_suffix_call_effect_shape_kind(stmt: HirStmt, expected: SuffixCallEffectShapeKind) {
        let kind = PreviewBuilder::classify_suffix_call_effect_shape(&stmt);
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
    fn suffix_accepts_nested_terminal_join_tail_same_guard_family_then_branch() {
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
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_accepts_nested_terminal_join_tail_negated_guard_match_else_branch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: Vec::new(),
                else_body: vec![HirStmt::Goto("terminal".to_string())],
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_different_guard_family() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("other".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_nonterminal_target() {
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

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_nonempty_else_payload() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: vec![HirStmt::Expr(HirExpr::Var("payload".to_string()))],
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
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_side_effectful_branch() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![
                    HirStmt::Expr(HirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Unknown,
                    }),
                    HirStmt::Goto("terminal".to_string()),
                ],
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
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_read_only_assign() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar116".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("xVar43".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixSideEffectShapeKind::MemoryReadOnlyAssign,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_call_expr_side_effect() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixSideEffectShapeKind::CallExprSideEffect,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_void_unknown_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::VoidUnknownCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_ignored_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            SuffixCallEffectShapeKind::ReturnValueIgnoredCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_assigned_local() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Call {
                    target: "helper".to_string(),
                    args: vec![HirExpr::Var("arg".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::ReturnValueAssignedLocal,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_pure_known_helper_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::PureKnownHelperCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_memory_mutating_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "memcpy".to_string(),
                args: vec![
                    HirExpr::Var("dst".to_string()),
                    HirExpr::Var("src".to_string()),
                ],
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            }),
            SuffixCallEffectShapeKind::MemoryMutatingCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_control_effect_call() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Expr(HirExpr::Call {
                target: "abort".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::ControlEffectCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_nested_call_as_unknown_effect() {
        assert_suffix_call_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }),
                    rhs: Box::new(HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::UnknownCallEffect,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_write() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            SuffixSideEffectShapeKind::MemoryWrite,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_volatile_or_unknown_load() {
        assert_suffix_side_effect_shape_kind(
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Call {
                    target: "addr_provider".to_string(),
                    args: vec![],
                    ty: NirType::Ptr(Box::new(NirType::Int {
                        bits: 8,
                        signed: false,
                    })),
                }),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
        );
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_condition_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_pure_ptroffset() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("base".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Expr(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("loaded".to_string())),
                ty: NirType::Bool,
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_unknown_load_type() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Unknown,
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_reused_in_return() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("loaded".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_when_ptr_contains_call() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Call {
                        target: "ptr_source".to_string(),
                        args: vec![],
                        ty: NirType::Ptr(Box::new(NirType::Int {
                            bits: 8,
                            signed: false,
                        })),
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("loaded".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_memory_visible_alias_risk() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("loaded".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_condition_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_pure_expr_use() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Expr(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("count".to_string())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_unknown_target() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount64".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_call_arg() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Call {
                        target: "value_provider".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("count".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_reused_in_return() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("count".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_memory_visible_alias_risk() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("count".to_string()),
                rhs: HirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![HirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("count".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: HirExpr::Var("value".to_string()),
            },
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_ignored_result() {
        let body = vec![
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "__popcount".to_string(),
                args: vec![HirExpr::Var("value".to_string())],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn external_entry_kind_classifies_top_level_external_goto() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::TopLevelExternalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_nested_conditional_goto() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::NestedConditionalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_loop_switch_derived_goto() {
        let body = vec![
            HirStmt::While {
                cond: HirExpr::Var("cond".to_string()),
                body: vec![HirStmt::Goto("join0".to_string())],
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::LoopSwitchDerived, 0)),
        );
    }

    #[test]
    fn external_entry_kind_skips_candidate_internal_top_level_goto() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(&body, "join0", 0, 4, None);
    }

    #[test]
    fn suffix_rejects_self_terminal_join_goto_with_nested_tail_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("terminal".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("next".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_side_effectful_stmt() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_nonterminal_goto() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("other".to_string()),
            HirStmt::Label("next".to_string()),
            HirStmt::Expr(HirExpr::Var("after".to_string())),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx: 2,
                target: "other".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_nested_nonlocal_ref() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("other".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_label_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result = PreviewBuilder::candidate_window_can_shrink_to_label(
            &body,
            0,
            "join0",
            1,
            1,
            &referenced,
        );
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: 1,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_external_entry() {
        let body = vec![
            HirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result = PreviewBuilder::candidate_window_can_shrink_to_label(
            &body,
            1,
            "join0",
            2,
            4,
            &referenced,
        );
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasExternalEntry {
                stmt_idx: 2,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_loop_or_switch_crossing() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::While {
                cond: HirExpr::Var("cond".to_string()),
                body: vec![],
            },
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_unresolved_alias_redirect() {
        let body = vec![
            test_if_goto("join0"),
            HirStmt::Label("join0".to_string()),
            HirStmt::Goto("unknown".to_string()),
            HirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixAliasRedirectUnresolved {
                stmt_idx: 2,
                label: "unknown".to_string(),
            },
        );
    }
}
