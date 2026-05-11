use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn mark_alias_nonlocal_external_before(&mut self) {
        self.canonicalization_failed_alias_has_nonlocal_ref_external_before_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_nested_before(&mut self) {
        self.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_post_segment_ref(&mut self) {
        self.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count += 1;
    }

    pub(super) fn mark_alias_nonlocal_from_external_sites(
        &mut self,
        external_top_level_before: usize,
        external_nested_before: usize,
        external_refs_after: usize,
    ) {
        if external_nested_before > 0 {
            self.mark_alias_nonlocal_nested_before();
        } else if external_refs_after > 0 {
            self.mark_alias_nonlocal_post_segment_ref();
        } else if external_top_level_before > 0 {
            self.mark_alias_nonlocal_external_before();
        }
    }

    pub(super) fn expr_is_pure_value(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
            HirExpr::Cast { expr, .. } => Self::expr_is_pure_value(expr),
            HirExpr::Unary { expr, .. } => Self::expr_is_pure_value(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_is_pure_value(lhs) && Self::expr_is_pure_value(rhs)
            }
            HirExpr::PtrOffset { base, .. } => Self::expr_is_pure_value(base),
            HirExpr::Index { base, index, .. } => {
                Self::expr_is_pure_value(base) && Self::expr_is_pure_value(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::expr_is_pure_value(src),
            HirExpr::Call { target, args, .. } => {
                guarded_tail_call_target_is_known_pure_helper(target)
                    && args.iter().all(Self::expr_is_pure_value)
            }
            HirExpr::Load { .. } => false,
        }
    }

    pub(super) fn stmt_is_pure_value_expr(stmt: &HirStmt) -> bool {
        matches!(
            stmt,
            HirStmt::Expr(expr)
                if Self::expr_is_pure_value(expr) && !Self::suffix_expr_contains_call(expr)
        )
    }

    pub(super) fn stmt_is_pure_value_assign(stmt: &HirStmt) -> bool {
        matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } if Self::expr_is_pure_value(rhs) && !Self::suffix_expr_contains_call(rhs)
        )
    }

    #[cfg(test)]
    pub(super) fn test_expr_is_pure_value(expr: &HirExpr) -> bool {
        Self::expr_is_pure_value(expr)
    }

    fn stmt_is_alias_forward_safe(stmt: &HirStmt, label: &str, next_label: &str) -> bool {
        if is_ignorable_discovery_stmt(stmt)
            || Self::stmt_is_pure_value_expr(stmt)
            || Self::stmt_is_pure_value_assign(stmt)
        {
            return true;
        }

        match stmt {
            HirStmt::Goto(target) => target == next_label || target == label,
            HirStmt::Block(body) => body
                .iter()
                .all(|stmt| Self::stmt_is_alias_forward_safe(stmt, label, next_label)),
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                Self::expr_is_pure_value(cond)
                    && then_body
                        .iter()
                        .all(|stmt| Self::stmt_is_alias_forward_safe(stmt, label, next_label))
                    && else_body
                        .iter()
                        .all(|stmt| Self::stmt_is_alias_forward_safe(stmt, label, next_label))
            }
            _ => false,
        }
    }

    pub(super) fn classify_external_alias_ref_sites(
        full_body: &[HirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        let (top_level_before, nested_before, top_level_after, nested_after) =
            Self::classify_external_alias_ref_sites_detailed(
                full_body,
                segment_start,
                segment_end,
                label,
            );

        (
            top_level_before,
            nested_before,
            top_level_after + nested_after,
        )
    }

    pub(super) fn classify_external_alias_ref_sites_detailed(
        full_body: &[HirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> (usize, usize, usize, usize) {
        let mut top_level_before = 0usize;
        let mut nested_before = 0usize;
        let mut top_level_after = 0usize;
        let mut nested_after = 0usize;

        for (idx, stmt) in full_body.iter().enumerate() {
            if idx >= segment_start && idx < segment_end {
                continue;
            }
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            if idx < segment_start {
                match stmt {
                    HirStmt::Goto(target) if target == label => top_level_before += 1,
                    _ => nested_before += ref_count,
                }
            } else {
                match stmt {
                    HirStmt::Goto(target) if target == label => top_level_after += 1,
                    _ => nested_after += ref_count,
                }
            }
        }

        (
            top_level_before,
            nested_before,
            top_level_after,
            nested_after,
        )
    }

    pub(super) fn stmt_contains_goto_label(stmt: &HirStmt, label: &str) -> usize {
        match stmt {
            HirStmt::Goto(target) => usize::from(target == label),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body
                    .iter()
                    .map(|stmt| Self::stmt_contains_goto_label(stmt, label))
                    .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| Self::stmt_contains_goto_label(stmt, label))
                        .sum::<usize>()
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => body
                .iter()
                .map(|stmt| Self::stmt_contains_goto_label(stmt, label))
                .sum(),
            HirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| Self::stmt_contains_goto_label(stmt, label))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| Self::stmt_contains_goto_label(stmt, label))
                        .sum::<usize>()
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    pub(super) fn are_all_external_refs_top_level_goto(
        full_body: &[HirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
    ) -> bool {
        let (_, external_nested_before, _, external_nested_after) =
            Self::classify_external_alias_ref_sites_detailed(
                full_body,
                segment_start,
                segment_end,
                label,
            );
        external_nested_before == 0 && external_nested_after == 0
    }

    pub(super) fn classify_alias_ref_sites(
        body: &[HirStmt],
        label_idx: usize,
        label: &str,
    ) -> (usize, usize, usize) {
        let mut top_level_before = 0usize;
        let mut nested_before = 0usize;
        let mut refs_after = 0usize;

        for (idx, stmt) in body.iter().enumerate() {
            let ref_count = Self::stmt_contains_goto_label(stmt, label);
            if ref_count == 0 {
                continue;
            }
            if idx >= label_idx {
                refs_after += ref_count;
                continue;
            }
            match stmt {
                HirStmt::Goto(target) if target == label => top_level_before += 1,
                _ => nested_before += ref_count,
            }
        }

        (top_level_before, nested_before, refs_after)
    }

    fn stmt_is_pure_nested_single_branch_goto_to_label(stmt: &HirStmt, label: &str) -> bool {
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = stmt
        else {
            return false;
        };

        let then_target = single_goto_target(then_body);
        let else_target = single_goto_target(else_body);
        matches!(then_target, Some(target) if target == label) && else_body.is_empty()
            || matches!(else_target, Some(target) if target == label) && then_body.is_empty()
    }

    fn classify_nested_before_nonlocal_payload(stmt: &HirStmt, label: &str) -> bool {
        let HirStmt::If {
            then_body,
            else_body,
            ..
        } = stmt
        else {
            return false;
        };

        let then_target = single_goto_target(then_body);
        let else_target = single_goto_target(else_body);
        if matches!(then_target, Some(target) if target == label) && else_body.is_empty() {
            return false;
        }
        if matches!(else_target, Some(target) if target == label) && then_body.is_empty() {
            return false;
        }
        Self::stmt_contains_goto_label(stmt, label) > 0
    }

    fn classify_nested_before_alias_witnesses(
        full_body: &[HirStmt],
        segment_start: usize,
        label: &str,
    ) -> Vec<NestedBeforeAliasWitness> {
        let mut witnesses = Vec::new();
        for (stmt_idx, stmt) in full_body.iter().enumerate() {
            if stmt_idx >= segment_start {
                break;
            }
            if Self::stmt_contains_goto_label(stmt, label) == 0 {
                continue;
            }
            if matches!(stmt, HirStmt::Goto(target) if target == label) {
                continue;
            }

            let class = if Self::classify_nested_before_nonlocal_payload(stmt, label) {
                NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
            } else if Self::stmt_is_single_branch_if_to_label(stmt, label).is_some() {
                NestedBeforeOwnershipClass::NestedBeforeExternalOwner
            } else {
                NestedBeforeOwnershipClass::NestedBeforeUnknown
            };
            witnesses.push(NestedBeforeAliasWitness {
                stmt_idx,
                cond: Self::stmt_is_single_branch_if_to_label(stmt, label).cloned(),
                class,
            });
        }
        witnesses
    }

    pub(super) fn build_nested_before_alias_ownership_proof(
        full_body: &[HirStmt],
        segment_start: usize,
        segment_end: usize,
        label: &str,
        raw_nested_before: usize,
    ) -> AliasOwnershipProof {
        let witnesses =
            Self::classify_nested_before_alias_witnesses(full_body, segment_start, label);
        if raw_nested_before == 0 {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        }

        let anchor_idx = segment_start.saturating_sub(1);
        let current_label_idx = full_body.iter().enumerate().find_map(|(idx, stmt)| {
            (idx >= segment_start
                && idx < segment_end
                && matches!(stmt, HirStmt::Label(candidate) if candidate == label))
            .then_some(idx)
        });
        let Some(current_label_idx) = current_label_idx else {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        };
        let terminal_label_idx = (current_label_idx + 1..full_body.len())
            .find(|idx| matches!(full_body[*idx], HirStmt::Label(_)));
        let Some(terminal_label_idx) = terminal_label_idx else {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: 0,
                class: NestedBeforeOwnershipClass::NestedBeforeUnknown,
                legality_reason: AliasOwnershipLegalityReason::Unknown,
                witnesses,
            };
        };

        let raw_refs = collect_referenced_label_counts(full_body)
            .get(label)
            .copied()
            .unwrap_or(0);
        let guard_family_internalized =
            Self::count_internalized_guard_family_nested_conditional_entries(
                full_body,
                label,
                anchor_idx,
                current_label_idx,
                terminal_label_idx,
            );
        if guard_family_internalized >= raw_nested_before {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: raw_nested_before,
                class: NestedBeforeOwnershipClass::GuardFamilyInternalizable,
                legality_reason: AliasOwnershipLegalityReason::Complete,
                witnesses,
            };
        }

        let paired_boundary_internalized = Self::count_internalized_paired_nested_boundary_refs(
            full_body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
        );
        if paired_boundary_internalized >= raw_nested_before {
            return AliasOwnershipProof {
                label: label.to_string(),
                raw_nested_before,
                internalized_nested_before: raw_nested_before,
                class: NestedBeforeOwnershipClass::PairedBoundaryInternalizable,
                legality_reason: AliasOwnershipLegalityReason::Complete,
                witnesses,
            };
        }

        let class = if witnesses.iter().any(|w| {
            matches!(
                w.class,
                NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
            )
        }) {
            NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload
        } else if witnesses.iter().all(|w| {
            matches!(
                w.class,
                NestedBeforeOwnershipClass::NestedBeforeExternalOwner
            )
        }) {
            NestedBeforeOwnershipClass::NestedBeforeExternalOwner
        } else {
            NestedBeforeOwnershipClass::NestedBeforeUnknown
        };
        let legality_reason = match class {
            NestedBeforeOwnershipClass::NestedBeforeNonlocalPayload => {
                AliasOwnershipLegalityReason::NonlocalPayload
            }
            NestedBeforeOwnershipClass::NestedBeforeCrossesTerminalJoin => {
                AliasOwnershipLegalityReason::CrossesTerminalJoin
            }
            NestedBeforeOwnershipClass::NestedBeforeExternalOwner => {
                AliasOwnershipLegalityReason::ExternalOwner
            }
            _ => AliasOwnershipLegalityReason::Unknown,
        };

        AliasOwnershipProof {
            label: label.to_string(),
            raw_nested_before,
            internalized_nested_before: 0,
            class,
            legality_reason,
            witnesses,
        }
    }

    pub(super) fn local_goto_positions_by_label(body: &[HirStmt]) -> HashMap<String, Vec<usize>> {
        let mut refs = HashMap::new();
        for (idx, stmt) in body.iter().enumerate() {
            if let HirStmt::Goto(label) = stmt {
                refs.entry(label.clone()).or_insert_with(Vec::new).push(idx);
            }
        }
        refs
    }

    pub(super) fn is_local_alias_forward_segment(segment: &[HirStmt], next_label: &str) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) {
                continue;
            }
            match stmt {
                HirStmt::Goto(label) if !saw_forward_goto && label == next_label => {
                    saw_forward_goto = true;
                }
                _ => return false,
            }
        }
        saw_forward_goto
    }

    pub(super) fn is_local_alias_forward_segment_with_after_label_refs(
        segment: &[HirStmt],
        label: &str,
        next_label: &str,
    ) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if matches!(stmt, HirStmt::Goto(target) if target == next_label) {
                saw_forward_goto = true;
            }
            if !Self::stmt_is_alias_forward_safe(stmt, label, next_label) {
                return false;
            }
        }
        saw_forward_goto
    }

    pub(super) fn inferred_alias_forward_target_with_after_label_refs(
        segment: &[HirStmt],
        label: &str,
    ) -> Option<String> {
        let mut inferred_target = None::<String>;
        let mut saw_forward_goto = false;

        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt)
                || Self::stmt_is_pure_value_expr(stmt)
                || Self::stmt_is_pure_value_assign(stmt)
            {
                continue;
            }

            let HirStmt::Goto(target) = stmt else {
                return None;
            };
            if target == label {
                continue;
            }

            match inferred_target.as_deref() {
                Some(existing) if existing != target => return None,
                Some(_) => {}
                None => inferred_target = Some(target.clone()),
            }
            saw_forward_goto = true;
        }

        let target = inferred_target?;
        if !saw_forward_goto {
            return None;
        }
        segment
            .iter()
            .all(|stmt| Self::stmt_is_alias_forward_safe(stmt, label, &target))
            .then_some(target)
    }

    pub(super) fn is_trivial_join_forward_segment(segment: &[HirStmt], next_label: &str) -> bool {
        let mut saw_forward_goto = false;
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) {
                continue;
            }
            match stmt {
                HirStmt::Goto(label) if label == next_label => {
                    saw_forward_goto = true;
                }
                _ => return false,
            }
        }
        saw_forward_goto
    }

    pub(super) fn is_trivial_join_forward_or_pure_segment(
        segment: &[HirStmt],
        next_label: &str,
    ) -> bool {
        for stmt in segment {
            if is_ignorable_discovery_stmt(stmt) || Self::stmt_is_pure_value_expr(stmt) {
                continue;
            }
            match stmt {
                HirStmt::Goto(label) if label == next_label => {}
                _ => return false,
            }
        }
        true
    }

    pub(super) fn is_pure_multi_goto_gap_to_label(
        body: &[HirStmt],
        goto_positions: &[usize],
        label_idx: usize,
        label: &str,
    ) -> bool {
        let Some(start) = goto_positions.iter().copied().min() else {
            return false;
        };
        if goto_positions.iter().any(|pos| *pos >= label_idx) {
            return false;
        }
        body[start + 1..label_idx].iter().all(|stmt| {
            is_ignorable_discovery_stmt(stmt)
                || Self::stmt_is_pure_value_expr(stmt)
                || matches!(stmt, HirStmt::Goto(target) if target == label)
        })
    }

    pub(super) fn count_top_level_goto_refs_in_range(
        body: &[HirStmt],
        label: &str,
        start_exclusive: usize,
        end_exclusive: usize,
    ) -> usize {
        if start_exclusive + 1 >= end_exclusive {
            return 0;
        }
        body[start_exclusive + 1..end_exclusive]
            .iter()
            .filter(|stmt| matches!(stmt, HirStmt::Goto(target) if target == label))
            .count()
    }

    pub(super) fn resolve_terminal_join_target(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        target_label: &str,
        referenced: &HashMap<String, usize>,
    ) -> Option<(String, usize)> {
        let mut current = target_label.to_string();
        let mut seen = HashSet::new();
        let mut rewrites = 0usize;

        loop {
            if !seen.insert(current.clone()) {
                return None;
            }

            let label_idx = (anchor_idx + 1..body.len()).find(
                |pos| matches!(body.get(*pos), Some(HirStmt::Label(label)) if label == &current),
            )?;
            let next_label_idx =
                (label_idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let Some(next_label_idx) = next_label_idx else {
                if rewrites > 0 {
                    self.canonicalized_guarded_tail_shape_count += rewrites;
                }
                return Some((current, label_idx));
            };
            let HirStmt::Label(next_label) = &body[next_label_idx] else {
                unreachable!();
            };
            let segment = &body[label_idx + 1..next_label_idx];
            let top_level_window_refs =
                Self::count_top_level_goto_refs_in_range(body, &current, anchor_idx, label_idx);
            let hop_ref_budget = if rewrites == 0 {
                top_level_window_refs + 1
            } else {
                top_level_window_refs
            };
            let no_nonlocal_refs = referenced.get(&current).copied().unwrap_or(0) <= hop_ref_budget;
            if no_nonlocal_refs
                && (Self::is_trivial_join_forward_segment(segment, next_label)
                    || Self::is_trivial_join_forward_or_pure_segment(segment, next_label))
            {
                current = next_label.clone();
                rewrites += 1;
                continue;
            }

            if rewrites > 0 {
                self.canonicalized_guarded_tail_shape_count += rewrites;
            }
            return Some((current, label_idx));
        }
    }

    pub(super) fn resolve_alias_redirect(
        label: &str,
        redirects: &HashMap<String, Option<String>>,
    ) -> Option<String> {
        let mut current = label.to_string();
        let mut seen = HashSet::new();
        while let Some(next) = redirects.get(&current) {
            if !seen.insert(current.clone()) {
                return Some(current);
            }
            match next {
                Some(next_label) => current = next_label.clone(),
                None => return None,
            }
        }
        Some(current)
    }

    pub(super) fn count_goto_refs_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
        match stmt {
            HirStmt::Goto(label) => {
                *out.entry(label.clone()).or_insert(0) += 1;
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for nested in then_body {
                    Self::count_goto_refs_in_stmt(nested, out);
                }
                for nested in else_body {
                    Self::count_goto_refs_in_stmt(nested, out);
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                for nested in body {
                    Self::count_goto_refs_in_stmt(nested, out);
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for nested in &case.body {
                        Self::count_goto_refs_in_stmt(nested, out);
                    }
                }
                for nested in default {
                    Self::count_goto_refs_in_stmt(nested, out);
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    pub(super) fn goto_ref_counts(body: &[HirStmt]) -> HashMap<String, usize> {
        let mut out = HashMap::new();
        for stmt in body {
            Self::count_goto_refs_in_stmt(stmt, &mut out);
        }
        out
    }

    pub(super) fn rewrite_goto_label_in_stmt(stmt: &mut HirStmt, from: &str, to: &str) {
        match stmt {
            HirStmt::Goto(label) => {
                if label == from {
                    *label = to.to_string();
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for nested in then_body {
                    Self::rewrite_goto_label_in_stmt(nested, from, to);
                }
                for nested in else_body {
                    Self::rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                for nested in body {
                    Self::rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for nested in &mut case.body {
                        Self::rewrite_goto_label_in_stmt(nested, from, to);
                    }
                }
                for nested in default {
                    Self::rewrite_goto_label_in_stmt(nested, from, to);
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    pub(super) fn rewrite_goto_label_in_stmts(stmts: &mut [HirStmt], from: &str, to: &str) {
        for stmt in stmts {
            Self::rewrite_goto_label_in_stmt(stmt, from, to);
        }
    }

    pub(super) fn terminalizable_join_alias_target(
        body: &[HirStmt],
        label_idx: usize,
    ) -> Option<(String, usize)> {
        let HirStmt::Label(_) = &body[label_idx] else {
            return None;
        };
        let next_label_idx =
            (label_idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)))?;
        let HirStmt::Label(next_label) = &body[next_label_idx] else {
            return None;
        };
        let segment = &body[label_idx + 1..next_label_idx];
        if Self::is_trivial_join_forward_segment(segment, next_label)
            || Self::is_trivial_join_forward_or_pure_segment(segment, next_label)
            || segment.iter().all(is_ignorable_discovery_stmt)
        {
            return Some((next_label.clone(), next_label_idx));
        }
        None
    }

    pub(super) fn resolve_terminal_tail_exit_stmt(
        body: &[HirStmt],
        target_label: &str,
    ) -> Option<HirStmt> {
        let mut current = target_label.to_string();
        let mut seen = HashSet::new();

        loop {
            if !seen.insert(current.clone()) {
                return None;
            }

            // Safe subcase guard: no external re-entry into any hop label.
            // The only allowed predecessor is the unique previous hop goto.
            let ref_count = body
                .iter()
                .map(|stmt| Self::stmt_contains_goto_label(stmt, &current))
                .sum::<usize>();
            if ref_count != 1 {
                return None;
            }

            let label_idx = body
                .iter()
                .position(|stmt| matches!(stmt, HirStmt::Label(label) if label == &current))?;
            let next_label_idx = body[label_idx + 1..]
                .iter()
                .position(|stmt| matches!(stmt, HirStmt::Label(_)))
                .map(|offset| label_idx + 1 + offset)
                .unwrap_or(body.len());

            let mut terminal_return: Option<Option<HirExpr>> = None;
            let mut terminal_goto: Option<String> = None;

            for stmt in &body[label_idx + 1..next_label_idx] {
                if is_ignorable_discovery_stmt(stmt)
                    || Self::stmt_is_pure_value_expr(stmt)
                    || Self::stmt_is_pure_value_assign(stmt)
                {
                    // Terminal exit must be the last meaningful statement in the hop.
                    if terminal_return.is_some() || terminal_goto.is_some() {
                        return None;
                    }
                    continue;
                }

                match stmt {
                    HirStmt::Return(ret) => {
                        if terminal_return.is_some() || terminal_goto.is_some() {
                            return None;
                        }
                        terminal_return = Some(ret.clone());
                    }
                    HirStmt::Goto(next) => {
                        if terminal_return.is_some() || terminal_goto.is_some() {
                            return None;
                        }
                        terminal_goto = Some(next.clone());
                    }
                    // Keep nested/nonlocal control-flow crossing forbidden.
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
                    | HirStmt::Label(_) => return None,
                }
            }

            if let Some(ret) = terminal_return {
                return Some(HirStmt::Return(ret));
            }
            if let Some(next) = terminal_goto {
                current = next;
                continue;
            }
            return None;
        }
    }

    pub(super) fn flatten_guarded_tail_segment(segment: &[HirStmt], out: &mut Vec<HirStmt>) {
        for stmt in segment {
            match stmt {
                HirStmt::Block(body) => Self::flatten_guarded_tail_segment(body, out),
                other => out.push(other.clone()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_before_alias_ownership_internalizes_same_guard_family_ref() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("reg".to_string()),
                then_body: vec![HirStmt::Goto("block_tail".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("middle".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("block_mid".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("block_mid".to_string()),
            HirStmt::Label("block_mid".to_string()),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("block_tail".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("block_tail".to_string()),
            HirStmt::Label("block_tail".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let proof =
            PreviewBuilder::build_nested_before_alias_ownership_proof(&body, 1, 8, "block_mid", 1);

        assert_eq!(
            proof.class,
            NestedBeforeOwnershipClass::GuardFamilyInternalizable
        );
        assert_eq!(
            proof.legality_reason,
            AliasOwnershipLegalityReason::Complete
        );
        assert_eq!(proof.internalized_nested_before, 1);
        assert!(proof.is_complete());
    }

    #[test]
    fn nested_before_alias_ownership_internalizes_paired_boundary_refs() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("join0".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("join0".to_string()),
            HirStmt::Expr(HirExpr::Var("body".to_string())),
            HirStmt::Goto("terminal".to_string()),
            HirStmt::Label("terminal".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let proof =
            PreviewBuilder::build_nested_before_alias_ownership_proof(&body, 1, 6, "join0", 2);

        assert_eq!(
            proof.class,
            NestedBeforeOwnershipClass::PairedBoundaryInternalizable
        );
        assert_eq!(
            proof.legality_reason,
            AliasOwnershipLegalityReason::Complete
        );
        assert_eq!(proof.internalized_nested_before, 2);
        assert!(proof.is_complete());
    }
}
