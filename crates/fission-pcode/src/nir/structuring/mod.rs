use super::*;
use std::time::Instant;

mod conditionals;
mod linear;
mod loops;
mod switch;

pub(crate) fn cleanup_redundant_labels(body: Vec<HirStmt>) -> Vec<HirStmt> {
    let aliases = adjacent_label_aliases(&body);
    let body = rewrite_stmt_labels(body, &aliases);
    let referenced = collect_referenced_labels(&body);
    let mut cleaned = Vec::with_capacity(body.len());
    let mut seen_labels = HashSet::new();

    for stmt in body {
        match stmt {
            HirStmt::Label(label) => {
                if !seen_labels.insert(label.clone()) {
                    continue;
                }
                if cleaned.is_empty() || referenced.contains(&label) {
                    cleaned.push(HirStmt::Label(label));
                }
            }
            other => cleaned.push(other),
        }
    }

    cleaned
}

fn adjacent_label_aliases(body: &[HirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let HirStmt::Label(_) = &body[idx] else {
            idx += 1;
            continue;
        };
        let start = idx;
        while idx + 1 < body.len() && matches!(body[idx + 1], HirStmt::Label(_)) {
            idx += 1;
        }
        if idx > start {
            let HirStmt::Label(canonical) = &body[idx] else {
                unreachable!();
            };
            for alias_idx in start..idx {
                let HirStmt::Label(alias) = &body[alias_idx] else {
                    unreachable!();
                };
                aliases.insert(alias.clone(), canonical.clone());
            }
        }
        idx += 1;
    }
    aliases
}

fn canonicalize_label(label: &str, aliases: &HashMap<String, String>) -> String {
    let mut current = label.to_string();
    let mut seen = HashSet::new();
    while let Some(next) = aliases.get(&current) {
        if !seen.insert(current.clone()) {
            break;
        }
        current = next.clone();
    }
    current
}

fn rewrite_stmt_labels(body: Vec<HirStmt>, aliases: &HashMap<String, String>) -> Vec<HirStmt> {
    body.into_iter()
        .map(|stmt| rewrite_stmt_label(stmt, aliases))
        .collect()
}

fn rewrite_stmt_label(stmt: HirStmt, aliases: &HashMap<String, String>) -> HirStmt {
    match stmt {
        HirStmt::Block(body) => HirStmt::Block(rewrite_stmt_labels(body, aliases)),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => HirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|case| HirSwitchCase {
                    values: case.values,
                    body: rewrite_stmt_labels(case.body, aliases),
                })
                .collect(),
            default: rewrite_stmt_labels(default, aliases),
        },
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => HirStmt::If {
            cond,
            then_body: rewrite_stmt_labels(then_body, aliases),
            else_body: rewrite_stmt_labels(else_body, aliases),
        },
        HirStmt::While { cond, body } => HirStmt::While {
            cond,
            body: rewrite_stmt_labels(body, aliases),
        },
        HirStmt::DoWhile { body, cond } => HirStmt::DoWhile {
            body: rewrite_stmt_labels(body, aliases),
            cond,
        },
        HirStmt::Label(label) => HirStmt::Label(canonicalize_label(&label, aliases)),
        HirStmt::Goto(label) => HirStmt::Goto(canonicalize_label(&label, aliases)),
        other => other,
    }
}

fn collect_referenced_labels(body: &[HirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

fn collect_referenced_label_counts(body: &[HirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

fn collect_stmt_referenced_labels(stmt: &HirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
            for stmt in else_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        HirStmt::Assign { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_stmt_referenced_label_counts(stmt: &HirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
            for stmt in else_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        HirStmt::Assign { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn single_goto_target(body: &[HirStmt]) -> Option<&str> {
    match body {
        [HirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

fn has_top_level_label(body: &[HirStmt]) -> bool {
    body.iter().any(|stmt| matches!(stmt, HirStmt::Label(_)))
}

fn is_ignorable_discovery_stmt(stmt: &HirStmt) -> bool {
    matches!(stmt, HirStmt::Label(_)) || matches!(stmt, HirStmt::Block(body) if body.is_empty())
}

fn trim_ignorable_stmt_bounds(body: &[HirStmt]) -> Option<(usize, usize)> {
    let start = body
        .iter()
        .position(|stmt| !is_ignorable_discovery_stmt(stmt))?;
    let end = body
        .iter()
        .rposition(|stmt| !is_ignorable_discovery_stmt(stmt))
        .unwrap_or(start);
    Some((start, end + 1))
}

fn has_non_ignorable_payload(body: &[HirStmt]) -> bool {
    trim_ignorable_stmt_bounds(body).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuardedTailCanonicalizationFailure {
    MultiplePayloadEntries,
    InterleavedJoinUses,
    NonterminalJoinLabel,
    NestedTailEscape,
    AliasNotFallthrough,
    JoinHasExternalRef,
    PayloadCrossesJoin,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromotionGateRejection {
    MustEmitLabel,
    NotSinglePredSucc,
    ExternalEntry,
    LoopOrSwitchTarget,
}

impl<'a> PreviewBuilder<'a> {
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
            GuardedTailCanonicalizationFailure::JoinHasExternalRef => {
                self.canonicalization_failed_join_has_external_ref_count += 1;
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

    fn canonicalize_interleaved_local_aliases(
        &mut self,
        body: &[HirStmt],
        referenced: &HashMap<String, usize>,
    ) -> Result<Vec<HirStmt>, GuardedTailCanonicalizationFailure> {
        let local_refs = Self::local_goto_positions_by_label(body);
        let mut alias_labels = HashSet::new();

        for (idx, stmt) in body.iter().enumerate() {
            let HirStmt::Label(label) = stmt else {
                continue;
            };
            let Some(goto_positions) = local_refs.get(label) else {
                continue;
            };
            let total_refs = referenced.get(label).copied().unwrap_or(0);
            if total_refs > goto_positions.len() {
                return Err(GuardedTailCanonicalizationFailure::JoinHasExternalRef);
            }
            if goto_positions.iter().any(|pos| {
                *pos >= idx
                    || body[pos + 1..idx]
                        .iter()
                        .any(|stmt| !is_ignorable_discovery_stmt(stmt))
            }) {
                return Err(GuardedTailCanonicalizationFailure::AliasNotFallthrough);
            }
            let next_label_idx =
                (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let payload_end = next_label_idx.unwrap_or(body.len());
            if body[idx + 1..payload_end].iter().any(|stmt| {
                matches!(
                    stmt,
                    HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
                )
            }) {
                return Err(GuardedTailCanonicalizationFailure::PayloadCrossesJoin);
            }
            alias_labels.insert(label.clone());
        }

        if alias_labels.is_empty() {
            return Ok(body.to_vec());
        }

        self.canonicalized_interleaved_join_use_count += alias_labels.len();
        Ok(body
            .iter()
            .filter_map(|stmt| match stmt {
                HirStmt::Goto(label) if alias_labels.contains(label) => None,
                HirStmt::Label(label) if alias_labels.contains(label) => None,
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

        for stmt in &flattened {
            match stmt {
                HirStmt::Label(label) => {
                    if referenced.get(label).copied().unwrap_or(0) > 0 {
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
                HirStmt::Goto(_) | HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue => {
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

    fn promote_single_entry_guarded_tail_regions(&mut self, body: &mut Vec<HirStmt>) -> bool {
        let referenced = collect_referenced_label_counts(body);
        let mut changed = false;
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

            let Some(label_idx) = (idx + 1..body.len()).find(|pos| {
                matches!(body.get(*pos), Some(HirStmt::Label(label)) if label == &target_label)
            }) else {
                self.mark_promotion_shape_rejection();
                idx += 1;
                continue;
            };

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

    fn discover_guarded_tail_candidates(&mut self, body: &[HirStmt]) {
        self.discover_guarded_tail_candidates_in_body(body);
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
            self.discovery_seen_guarded_tail_like_shape_count += 1;

            let Some(label_idx) = (idx + 1..body.len()).find(|pos| {
                matches!(body.get(*pos), Some(HirStmt::Label(label)) if label == target_label)
            }) else {
                self.mark_guarded_tail_canonicalization_failure(
                    GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                );
                continue;
            };

            match self.canonicalize_guarded_tail_segment(&body[idx + 1..label_idx], &referenced) {
                Ok(_) => {}
                Err(reason) => {
                    self.mark_guarded_tail_canonicalization_failure(reason);
                    continue;
                }
            }

            self.promotion_candidate_count += 1;

            if referenced.get(target_label).copied().unwrap_or(0) != 1 {
                self.mark_promotion_gate_rejection(PromotionGateRejection::MustEmitLabel);
                continue;
            }
        }
    }

    pub(super) fn region_has_external_entry(
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
        (start_idx + 1..skip_to).any(|idx| targeted.contains(&self.pcode.blocks[idx].start_address))
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
            .filter(|idx| targeted.contains(&self.pcode.blocks[*idx].start_address))
            .collect()
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
            !preds.is_empty()
                && preds
                    .iter()
                    .all(|pred| region.contains(pred) && *pred < *idx)
        });
        if single_pred {
            Ok(())
        } else {
            Err(PromotionGateRejection::NotSinglePredSucc)
        }
    }

    fn accept_structured_region(
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

    pub(super) fn build_multiblock_body(&mut self) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let diag = structuring_diag_enabled();
        let total_start = Instant::now();
        if diag {
            eprintln!(
                "[DIAG] structuring start: blocks={} edges={} force_linear={}",
                self.pcode.blocks.len(),
                self.successors.iter().map(Vec::len).sum::<usize>(),
                self.should_force_linear_structuring()
            );
        }
        if self.should_force_linear_structuring() {
            let result = self.build_linear_multiblock_body();
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success={}",
                    total_start.elapsed().as_secs_f64(),
                    result.is_ok()
                );
            }
            return result;
        }

        let mut body = Vec::new();
        let targeted = self.collect_jump_targets()?;
        let mut emitted_labels = HashSet::new();
        let mut idx = 0usize;
        while idx < self.pcode.blocks.len() {
            if diag && idx > 0 && idx % 32 == 0 {
                eprintln!(
                    "[DIAG] structuring progress: idx={} elapsed={:.3}s",
                    idx,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=switch elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_switch(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=dowhile elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_dowhile(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=while elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_while(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=short_if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) =
                Self::ignore_unsupported(self.try_lower_short_circuit_if(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if_else elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_if_else(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} attempt=if elapsed={:.3}s",
                    idx,
                    self.pcode.blocks[idx].start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            if let Some((stmt, skip_to)) = Self::ignore_unsupported(self.try_lower_if(idx))?
                && self.accept_structured_region(idx, skip_to, &targeted)
            {
                body.push(stmt);
                idx = skip_to;
                continue;
            }

            let block = &self.pcode.blocks[idx];
            if (idx == 0 || targeted.contains(&block.start_address))
                && emitted_labels.insert(block.start_address)
            {
                body.push(HirStmt::Label(block_label(block.start_address)));
            }
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_stmts elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            body.extend(self.lower_block_stmts(block)?);
            if diag {
                eprintln!(
                    "[DIAG] structuring idx={} block=0x{:x} fallback=lower_block_terminator elapsed={:.3}s",
                    idx,
                    block.start_address,
                    total_start.elapsed().as_secs_f64()
                );
            }
            match self.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if self.next_block_address(idx) != Some(target) {
                        body.push(HirStmt::Goto(block_label(target)));
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = self.next_block_address(idx);
                    let then_body = if next_addr == Some(true_target) {
                        Vec::new()
                    } else {
                        vec![HirStmt::Goto(block_label(true_target))]
                    };
                    let else_body = match false_target {
                        Some(false_target) if Some(false_target) != next_addr => {
                            vec![HirStmt::Goto(block_label(false_target))]
                        }
                        _ => Vec::new(),
                    };
                    body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                }
                LoweredTerminator::Fallthrough(_) => {}
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
            idx += 1;
        }
        while self.promote_single_entry_guarded_tail_regions(&mut body) {}
        self.discover_guarded_tail_candidates(&body);
        if diag {
            eprintln!(
                "[DIAG] structuring done: elapsed={:.3}s stmts={}",
                total_start.elapsed().as_secs_f64(),
                body.len()
            );
            eprintln!(
                "[DIAG] structuring promotions: candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        } else if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage=structuring promotions candidates={} promoted={}",
                self.promotion_candidate_count, self.promoted_region_count
            );
        }
        Ok(cleanup_redundant_labels(body))
    }

    fn should_force_linear_structuring(&self) -> bool {
        let total_ops: usize = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        if self.pcode.blocks.len() > 80 {
            return true;
        }

        if self.options.is_64bit && self.pcode.blocks.len() >= 28 && total_ops >= 350 {
            return true;
        }

        let edge_count: usize = self.successors.iter().map(Vec::len).sum();
        let multi_pred_blocks = self
            .predecessors
            .iter()
            .filter(|preds| preds.len() > 1)
            .count();
        let max_predecessors = self.predecessors.iter().map(Vec::len).max().unwrap_or(0);

        self.pcode.blocks.len() > 32
            && (edge_count > self.pcode.blocks.len().saturating_mul(2)
                || multi_pred_blocks > 8
                || max_predecessors >= 4)
    }

    fn ignore_unsupported<T>(
        result: Result<Option<T>, MlilPreviewError>,
    ) -> Result<Option<T>, MlilPreviewError> {
        match result {
            Ok(result) => Ok(result),
            Err(MlilPreviewError::UnsupportedControlFlow)
            | Err(MlilPreviewError::UnsupportedCfgRegionShape)
            | Err(MlilPreviewError::UnsupportedCfgPhiJoin)
            | Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion)
            | Err(MlilPreviewError::UnsupportedCfgBranchTarget) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

fn structuring_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}

#[cfg(test)]
pub(super) fn promote_single_entry_guarded_tail_regions_for_test(
    body: &mut Vec<HirStmt>,
) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    while builder.promote_single_entry_guarded_tail_regions(body) {}
    builder.preview_build_stats()
}

#[cfg(test)]
pub(super) fn discover_guarded_tail_candidates_for_test(body: &[HirStmt]) -> PreviewBuildStats {
    discover_guarded_tail_candidates_for_stats(body)
}

pub(super) fn discover_guarded_tail_candidates_for_stats(body: &[HirStmt]) -> PreviewBuildStats {
    let dummy = PcodeFunction { blocks: Vec::new() };
    let options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0,
        sections: Vec::new(),
    };
    let mut builder = PreviewBuilder::new(&dummy, &options, None);
    builder.discover_guarded_tail_candidates(body);
    builder.preview_build_stats()
}
