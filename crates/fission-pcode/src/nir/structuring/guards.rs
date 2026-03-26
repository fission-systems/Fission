use super::cleanup::{
    collect_referenced_label_counts, has_non_ignorable_payload, has_top_level_label,
    is_ignorable_discovery_stmt, normalize_guarded_tail_layout, single_goto_target,
    trim_ignorable_stmt_bounds,
};
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailCanonicalizationFailure {
    MultiplePayloadEntries,
    InterleavedJoinUses,
    NonterminalJoinLabel,
    NestedTailEscape,
    AliasNotFallthrough,
    AliasHasMultipleInternalPredecessors,
    AliasHasNonlocalRef,
    AliasBodyNotTrivial,
    PayloadCrossesJoin,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromotionGateRejection {
    MustEmitLabel,
    NotSinglePredSucc,
    ExternalEntry,
    LoopOrSwitchTarget,
}

impl<'a> PreviewBuilder<'a> {
    fn find_top_level_label_after(body: &[HirStmt], start_idx: usize, label: &str) -> Option<usize> {
        (start_idx + 1..body.len()).find(
            |pos| matches!(body.get(*pos), Some(HirStmt::Label(candidate)) if candidate == label),
        )
    }

    fn is_nontrivial_internal_target_entry(&self, idx: usize) -> bool {
        let preds = &self.predecessors[idx];
        if preds.len() != 1 {
            return true;
        }
        preds[0] + 1 != idx
    }

    fn mark_promotion_shape_rejection(&mut self) {
        self.promotion_rejected_by_shape_count += 1;
    }

    fn mark_noncanonical_layout_rejection(&mut self) {
        self.discovery_rejected_noncanonical_layout_count += 1;
        self.mark_promotion_shape_rejection();
    }

    fn mark_guarded_tail_canonicalization_failure(
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
            GuardedTailCanonicalizationFailure::AliasBodyNotTrivial => {
                self.canonicalization_failed_alias_body_not_trivial_count += 1;
            }
            GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                self.canonicalization_failed_payload_crosses_join_count += 1;
            }
        }
    }

    fn local_goto_positions_by_label(body: &[HirStmt]) -> HashMap<String, Vec<usize>> {
        let mut refs = HashMap::new();
        for (idx, stmt) in body.iter().enumerate() {
            if let HirStmt::Goto(label) = stmt {
                refs.entry(label.clone()).or_insert_with(Vec::new).push(idx);
            }
        }
        refs
    }

    fn is_local_alias_forward_segment(segment: &[HirStmt], next_label: &str) -> bool {
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

    fn is_trivial_join_forward_segment(segment: &[HirStmt], next_label: &str) -> bool {
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

    fn count_top_level_goto_refs_in_range(
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

    fn resolve_terminal_join_target(
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
            let top_level_window_refs = Self::count_top_level_goto_refs_in_range(
                body,
                &current,
                anchor_idx,
                label_idx,
            );
            let hop_ref_budget = if rewrites == 0 {
                top_level_window_refs + 1
            } else {
                top_level_window_refs
            };
            let no_nonlocal_refs = referenced.get(&current).copied().unwrap_or(0) <= hop_ref_budget;
            if no_nonlocal_refs
                && (Self::is_trivial_join_forward_segment(segment, next_label)
                    || segment.iter().all(is_ignorable_discovery_stmt))
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

    fn resolve_alias_redirect(
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

    fn count_goto_refs_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
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
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
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
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    fn goto_ref_counts(body: &[HirStmt]) -> HashMap<String, usize> {
        let mut out = HashMap::new();
        for stmt in body {
            Self::count_goto_refs_in_stmt(stmt, &mut out);
        }
        out
    }

    fn rewrite_goto_label_in_stmt(stmt: &mut HirStmt, from: &str, to: &str) {
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
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
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
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    fn rewrite_goto_label_in_stmts(stmts: &mut [HirStmt], from: &str, to: &str) {
        for stmt in stmts {
            Self::rewrite_goto_label_in_stmt(stmt, from, to);
        }
    }

    fn terminalizable_join_alias_target(
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
            || segment.iter().all(is_ignorable_discovery_stmt)
        {
            return Some((next_label.clone(), next_label_idx));
        }
        None
    }

    fn canonicalize_interleaved_local_aliases(
        &mut self,
        body: &[HirStmt],
        referenced: &HashMap<String, usize>,
    ) -> Result<Vec<HirStmt>, GuardedTailCanonicalizationFailure> {
        let local_refs = Self::local_goto_positions_by_label(body);
        let mut alias_redirects = HashMap::new();
        let mut canonicalized_local_nonfallthrough = 0usize;

        for (idx, stmt) in body.iter().enumerate() {
            let HirStmt::Label(label) = stmt else {
                continue;
            };
            let Some(goto_positions) = local_refs.get(label) else {
                continue;
            };
            let total_refs = referenced.get(label).copied().unwrap_or(0);
            if total_refs > goto_positions.len() {
                return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
            }
            if goto_positions.iter().any(|pos| *pos >= idx) {
                return Err(GuardedTailCanonicalizationFailure::AliasNotFallthrough);
            }
            let has_non_ignorable_gap = goto_positions.iter().any(|pos| {
                body[pos + 1..idx]
                    .iter()
                    .any(|stmt| !is_ignorable_discovery_stmt(stmt))
            });
            let next_label_idx =
                (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let payload_end = next_label_idx.unwrap_or(body.len());
            let segment = &body[idx + 1..payload_end];
            if let Some(next_label_idx) = next_label_idx
                && let HirStmt::Label(next_label) = &body[next_label_idx]
                && Self::is_local_alias_forward_segment(segment, next_label)
            {
                if has_non_ignorable_gap {
                    if goto_positions.len() != 1 {
                        return Err(
                            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                        );
                    }
                    canonicalized_local_nonfallthrough += 1;
                }
                alias_redirects.insert(label.clone(), Some(next_label.clone()));
                continue;
            }
            if has_non_ignorable_gap {
                if segment.iter().any(|stmt| {
                    matches!(
                        stmt,
                        HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
                    )
                }) {
                    return Err(GuardedTailCanonicalizationFailure::PayloadCrossesJoin);
                }
                return Err(GuardedTailCanonicalizationFailure::AliasBodyNotTrivial);
            }
            if segment.iter().any(|stmt| {
                matches!(
                    stmt,
                    HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
                )
            }) {
                return Err(GuardedTailCanonicalizationFailure::PayloadCrossesJoin);
            }
            alias_redirects.insert(label.clone(), None);
        }

        if alias_redirects.is_empty() {
            return Ok(body.to_vec());
        }

        self.canonicalized_interleaved_join_use_count += alias_redirects.len();
        self.canonicalized_local_nonfallthrough_alias_count += canonicalized_local_nonfallthrough;
        Ok(body
            .iter()
            .filter_map(|stmt| match stmt {
                HirStmt::Goto(label) if alias_redirects.contains_key(label) => {
                    match Self::resolve_alias_redirect(label, &alias_redirects) {
                        Some(resolved) if resolved != *label => Some(HirStmt::Goto(resolved)),
                        Some(_) => Some(stmt.clone()),
                        None => None,
                    }
                }
                HirStmt::Label(label) if alias_redirects.contains_key(label) => None,
                other => Some(other.clone()),
            })
            .collect())
    }

    fn canonicalize_guarded_tail_segment(
        &mut self,
        segment: &[HirStmt],
        referenced: &HashMap<String, usize>,
    ) -> Result<Vec<HirStmt>, GuardedTailCanonicalizationFailure> {
        let mut flattened = Vec::new();
        Self::flatten_guarded_tail_segment(segment, &mut flattened);
        let Some((start, end)) = trim_ignorable_stmt_bounds(&flattened) else {
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        };
        let flattened =
            self.canonicalize_interleaved_local_aliases(&flattened[start..end], referenced)?;

        let mut canonical = Vec::new();
        let mut saw_payload = false;
        let mut saw_gap_after_payload = false;
        let mut removed_any = start > 0 || end < flattened.len() || flattened.len() != end - start;
        let mut payload_entry_count = 0usize;
        let segment_ref_counts = Self::goto_ref_counts(&flattened);
        let mut idx = 0usize;

        while idx < flattened.len() {
            let stmt = &flattened[idx];
            let trailing_has_non_ignorable = flattened[idx + 1..]
                .iter()
                .any(|stmt| !is_ignorable_discovery_stmt(stmt));
            match stmt {
                HirStmt::Label(label) => {
                    if referenced.get(label).copied().unwrap_or(0) > 0 {
                        let local_ref_count = segment_ref_counts.get(label).copied().unwrap_or(0);
                        let total_ref_count = referenced.get(label).copied().unwrap_or(0);
                        if total_ref_count > local_ref_count {
                            return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                        }
                        if let Some((next_label, next_idx)) =
                            Self::terminalizable_join_alias_target(&flattened, idx)
                        {
                            Self::rewrite_goto_label_in_stmts(&mut canonical, label, &next_label);
                            removed_any = true;
                            self.canonicalized_interleaved_join_use_count += 1;
                            idx = next_idx;
                            continue;
                        }
                        return Err(GuardedTailCanonicalizationFailure::InterleavedJoinUses);
                    }
                    removed_any = true;
                    if saw_payload {
                        saw_gap_after_payload = true;
                    }
                }
                HirStmt::Block(body) if body.is_empty() => {
                    removed_any = true;
                    if saw_payload {
                        saw_gap_after_payload = true;
                    }
                }
                HirStmt::Return(_) => {
                    if saw_payload {
                        if trailing_has_non_ignorable {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                    } else {
                        saw_payload = true;
                        payload_entry_count += 1;
                    }
                    canonical.push(stmt.clone());
                }
                HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => {
                    if saw_payload {
                        return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                    }
                    canonical.push(stmt.clone());
                }
                other => {
                    if !saw_payload || saw_gap_after_payload {
                        payload_entry_count += 1;
                        saw_payload = true;
                        saw_gap_after_payload = false;
                    }
                    canonical.push(other.clone());
                }
            }
            idx += 1;
        }

        if payload_entry_count > 1 {
            return Err(GuardedTailCanonicalizationFailure::MultiplePayloadEntries);
        }
        if canonical.is_empty() || !has_non_ignorable_payload(&canonical) {
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        }
        if removed_any {
            self.canonicalized_guarded_tail_shape_count += 1;
        }
        Ok(canonical)
    }

    fn flatten_guarded_tail_segment(segment: &[HirStmt], out: &mut Vec<HirStmt>) {
        for stmt in segment {
            match stmt {
                HirStmt::Block(body) => Self::flatten_guarded_tail_segment(body, out),
                other => out.push(other.clone()),
            }
        }
    }

    fn mark_promotion_gate_rejection(&mut self, reason: PromotionGateRejection) {
        self.promotion_rejected_by_gate_count += 1;
        match reason {
            PromotionGateRejection::MustEmitLabel => self.rejected_must_emit_label += 1,
            PromotionGateRejection::NotSinglePredSucc => self.rejected_not_single_pred_succ += 1,
            PromotionGateRejection::ExternalEntry => self.rejected_external_entry += 1,
            PromotionGateRejection::LoopOrSwitchTarget => self.rejected_loop_or_switch_target += 1,
        }
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
            let HirStmt::If {
                cond,
                then_body,
                else_body,
            } = &body[idx]
            else {
                idx += 1;
                continue;
            };

            let (target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
                let Some(label) = single_goto_target(then_body) else {
                    idx += 1;
                    continue;
                };
                (label.to_string(), false)
            } else if then_body.is_empty() {
                let Some(label) = single_goto_target(else_body) else {
                    idx += 1;
                    continue;
                };
                (label.to_string(), true)
            } else {
                idx += 1;
                continue;
            };

            let Some((target_label, label_idx)) =
                self.resolve_terminal_join_target(body, idx, &target_label, &referenced)
            else {
                if Self::find_top_level_label_after(body, idx, &target_label).is_some() {
                    self.mark_promotion_shape_rejection();
                }
                idx += 1;
                continue;
            };
            if !has_non_ignorable_payload(&body[idx + 1..label_idx]) {
                idx += 1;
                continue;
            }

            let middle = match self
                .canonicalize_guarded_tail_segment(&body[idx + 1..label_idx], &referenced)
            {
                Ok(middle) => middle,
                Err(reason) => {
                    self.mark_guarded_tail_canonicalization_failure(reason);
                    idx += 1;
                    continue;
                }
            };
            if middle.is_empty() || has_top_level_label(&middle) {
                self.mark_guarded_tail_canonicalization_failure(
                    GuardedTailCanonicalizationFailure::InterleavedJoinUses,
                );
                idx += 1;
                continue;
            }

            let tail_end = (label_idx + 1..body.len())
                .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
                .unwrap_or(body.len());
            let tail = body[label_idx + 1..tail_end].to_vec();
            if tail.is_empty() {
                self.mark_promotion_shape_rejection();
                idx += 1;
                continue;
            }

            self.promotion_candidate_count += 1;
            if referenced.get(&target_label).copied().unwrap_or(0) != 1 {
                self.mark_promotion_gate_rejection(PromotionGateRejection::MustEmitLabel);
                idx += 1;
                continue;
            }

            let replacement = HirStmt::If {
                cond: if keep_middle_when_cond_true {
                    cond.clone()
                } else {
                    negate_expr(cond.clone())
                },
                then_body: middle,
                else_body: Vec::new(),
            };

            body[idx] = replacement;
            body.drain(idx + 1..=label_idx);
            self.promoted_region_count += 1;
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
                | HirStmt::DoWhile { body: inner, .. } => {
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
            let HirStmt::If {
                then_body,
                else_body,
                ..
            } = &body[idx]
            else {
                continue;
            };

            let target_label = if else_body.is_empty() {
                single_goto_target(then_body)
            } else if then_body.is_empty() {
                single_goto_target(else_body)
            } else {
                None
            };
            let Some(target_label) = target_label else {
                continue;
            };
            if Self::find_top_level_label_after(body, idx, target_label).is_none() {
                continue;
            }
            self.discovery_seen_guarded_tail_like_shape_count += 1;

            let Some((target_label, label_idx)) =
                self.resolve_terminal_join_target(body, idx, target_label, &referenced)
            else {
                self.mark_guarded_tail_canonicalization_failure(
                    GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                );
                continue;
            };
            if !has_non_ignorable_payload(&body[idx + 1..label_idx]) {
                continue;
            }

            match self.canonicalize_guarded_tail_segment(&body[idx + 1..label_idx], &referenced) {
                Ok(_) => {}
                Err(reason) => {
                    self.mark_guarded_tail_canonicalization_failure(reason);
                    continue;
                }
            }

            self.promotion_candidate_count += 1;

            if referenced.get(&target_label).copied().unwrap_or(0) != 1 {
                self.mark_promotion_gate_rejection(PromotionGateRejection::MustEmitLabel);
                continue;
            }
        }
    }

    pub(crate) fn region_has_external_entry(
        &self,
        region: &HashSet<usize>,
        header_idx: usize,
    ) -> bool {
        region.iter().copied().any(|idx| {
            idx != header_idx
                && self.predecessors[idx]
                    .iter()
                    .any(|pred| !region.contains(pred))
        })
    }

    fn region_has_targeted_internal_entry(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        if skip_to <= start_idx + 1 {
            return false;
        }
        (start_idx + 1..skip_to).any(|idx| {
            targeted.contains(&self.block_target_key(idx))
                && self.is_nontrivial_internal_target_entry(idx)
        })
    }

    fn targeted_internal_entries(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> Vec<usize> {
        if skip_to <= start_idx + 1 {
            return Vec::new();
        }
        (start_idx + 1..skip_to)
            .filter(|idx| {
                targeted.contains(&self.block_target_key(*idx))
                    && self.is_nontrivial_internal_target_entry(*idx)
            })
            .collect()
    }

    fn region_external_exit_nodes(&self, region: &HashSet<usize>) -> Vec<usize> {
        region
            .iter()
            .copied()
            .filter(|idx| {
                self.successors[*idx]
                    .iter()
                    .any(|succ| !region.contains(succ))
            })
            .collect()
    }

    fn ensure_graph_invariant_promotion_region(
        &self,
        start_idx: usize,
        internal_entries: &[usize],
        region: &HashSet<usize>,
    ) -> Result<(), PromotionGateRejection> {
        let scc = self.analyze_cfg_scc();
        if region.iter().copied().any(|idx| scc.is_irreducible_node(idx)) {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let dom = self.analyze_cfg_dominators();
        if !internal_entries
            .iter()
            .copied()
            .all(|idx| dom.dominates(start_idx, idx))
        {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        if let Some(exit_idx) = self.region_external_exit_nodes(region).first().copied() {
            let Some(postdom) = PostDomTree::analyze_window_with_exit(&self.successors, region, exit_idx) else {
                return Err(PromotionGateRejection::NotSinglePredSucc);
            };
            let start_postdom = postdom
                .postdominators()
                .get(&start_idx)
                .is_some_and(|set| set.contains(&exit_idx));
            if !start_postdom {
                return Err(PromotionGateRejection::NotSinglePredSucc);
            }
        }

        Ok(())
    }

    fn is_minimal_structured_promotion_candidate(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> Result<(), PromotionGateRejection> {
        let internal = self.targeted_internal_entries(start_idx, skip_to, targeted);
        if internal.is_empty() {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }
        if internal.len() > 2 {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let region: HashSet<usize> = (start_idx..skip_to).collect();
        if self.region_has_external_entry(&region, start_idx) {
            return Err(PromotionGateRejection::ExternalEntry);
        }

        let single_pred = internal.iter().all(|idx| {
            let preds = &self.predecessors[*idx];
            !preds.is_empty() && preds.iter().all(|pred| region.contains(pred))
        });
        if !single_pred {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let legacy_single_pred_succ = internal.iter().all(|idx| {
            let preds = &self.predecessors[*idx];
            !preds.is_empty()
                && preds
                    .iter()
                    .all(|pred| region.contains(pred) && *pred < *idx)
        });
        if legacy_single_pred_succ {
            return Ok(());
        }

        self.ensure_graph_invariant_promotion_region(start_idx, &internal, &region)
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
mod tests {
    use super::*;
    use crate::PcodeBasicBlock;

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
        }
    }

    fn test_pcode_with_blocks(count: usize) -> PcodeFunction {
        let blocks = (0..count)
            .map(|idx| PcodeBasicBlock {
                index: idx as u32,
                start_address: 0x1000 + (idx as u64) * 0x10,
                ops: Vec::new(),
            })
            .collect();
        PcodeFunction { blocks }
    }

    #[test]
    fn minimal_structured_promotion_accepts_non_monotonic_layout_when_graph_invariants_hold() {
        let pcode = test_pcode_with_blocks(4);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let successors = vec![vec![2], vec![3], vec![1], vec![]];
        builder.successors = successors.clone();
        builder.predecessors = build_predecessor_index_map(&successors);

        let targeted = HashSet::from([builder.block_target_key(1)]);
        let result = builder.is_minimal_structured_promotion_candidate(0, 3, &targeted);
        assert!(result.is_ok());
    }

    #[test]
    fn minimal_structured_promotion_rejects_irreducible_region() {
        let pcode = test_pcode_with_blocks(4);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let successors = vec![vec![1, 2], vec![2], vec![1, 3], vec![]];
        builder.successors = successors.clone();
        builder.predecessors = build_predecessor_index_map(&successors);

        let targeted = HashSet::from([builder.block_target_key(1)]);
        let result = builder.is_minimal_structured_promotion_candidate(0, 3, &targeted);
        assert_eq!(result, Err(PromotionGateRejection::NotSinglePredSucc));
    }
}
