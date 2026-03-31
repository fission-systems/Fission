use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn canonicalize_interleaved_local_aliases(
        &mut self,
        body: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        let local_refs = Self::local_goto_positions_by_label(body);
        let mut alias_redirects = HashMap::new();
        let mut canonicalized_local_nonfallthrough = 0usize;
        let mut external_safe_redirect_labels = Vec::new();
        let segment_end = segment_start + body.len();

        for (idx, stmt) in body.iter().enumerate() {
            let HirStmt::Label(label) = stmt else {
                continue;
            };
            let Some(goto_positions) = local_refs.get(label) else {
                continue;
            };
            let total_refs = referenced.get(label).copied().unwrap_or(0);
            let (top_level_before, nested_before, refs_after) =
                Self::classify_alias_ref_sites(body, idx, label);
            let local_ref_count = top_level_before + nested_before + refs_after;
            let external_ref_count = total_refs.saturating_sub(local_ref_count);
            let top_level_after_positions: Vec<usize> = goto_positions
                .iter()
                .copied()
                .filter(|pos| *pos > idx)
                .collect();
            let top_level_after_label_count = top_level_after_positions.len();
            let nested_after_label_count = refs_after.saturating_sub(top_level_after_label_count);
            if nested_before > 0 {
                return Err(GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors);
            }
            let has_non_ignorable_gap = goto_positions.iter().filter(|pos| **pos < idx).any(|pos| {
                body[pos + 1..idx]
                    .iter()
                    .any(|stmt| !is_ignorable_discovery_stmt(stmt))
            });
            let next_label_idx =
                (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let payload_end = next_label_idx.unwrap_or(body.len());
            let segment = &body[idx + 1..payload_end];
            let allow_top_level_after_label_redirect = if let Some(next_label_idx) = next_label_idx {
                if let HirStmt::Label(next_label) = &body[next_label_idx] {
                    nested_after_label_count == 0
                        && !top_level_after_positions.is_empty()
                        && top_level_after_positions.iter().all(|pos| *pos < next_label_idx)
                        && Self::is_local_alias_forward_segment_with_after_label_refs(
                            segment, label, next_label,
                        )
                } else {
                    false
                }
            } else {
                false
            };
            if nested_after_label_count > 0
                || (top_level_after_label_count > 0 && !allow_top_level_after_label_redirect)
            {
                self.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
                    top_level_after_label_count;
                self.canonicalization_failed_alias_not_fallthrough_nested_after_label_count +=
                    nested_after_label_count;
                return Err(GuardedTailCanonicalizationFailure::AliasNotFallthrough);
            }
            let next_redirect_label = if let Some(next_label_idx) = next_label_idx
                && let HirStmt::Label(next_label) = &body[next_label_idx]
                && (Self::is_local_alias_forward_segment(segment, next_label)
                    || allow_top_level_after_label_redirect)
            {
                Some(next_label.clone())
            } else {
                None
            };

            if let Some(next_label) = next_redirect_label {
                if external_ref_count > 0 {
                    let (external_top_level_before, external_nested_before, external_refs_after) =
                        Self::classify_external_alias_ref_sites(
                            full_body,
                            segment_start,
                            segment_end,
                            label,
                        );
                    if external_refs_after > 0 {
                        self.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    if external_top_level_before + external_nested_before != external_ref_count {
                        self.mark_alias_nonlocal_external_before();
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    external_safe_redirect_labels.push(label.clone());
                }
                if has_non_ignorable_gap {
                    if goto_positions.len() != 1 {
                        return Err(
                            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                        );
                    }
                    canonicalized_local_nonfallthrough += 1;
                } else if top_level_after_label_count > 0 {
                    canonicalized_local_nonfallthrough += 1;
                }
                alias_redirects.insert(label.clone(), Some(next_label.clone()));
                continue;
            }
            if external_ref_count > 0 {
                let (external_top_level_before, external_nested_before, external_refs_after) =
                    Self::classify_external_alias_ref_sites(
                        full_body,
                        segment_start,
                        segment_end,
                        label,
                    );
                self.mark_alias_nonlocal_from_external_sites(
                    external_top_level_before,
                    external_nested_before,
                    external_refs_after,
                );
                return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
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
                if segment.iter().all(|stmt| Self::stmt_is_pure_value_expr(stmt)) {
                    alias_redirects.insert(label.clone(), None);
                    continue;
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
            return Ok((body.to_vec(), Vec::new()));
        }

        self.canonicalized_interleaved_join_use_count += alias_redirects.len();
        self.canonicalized_local_nonfallthrough_alias_count += canonicalized_local_nonfallthrough;
        let external_redirects = external_safe_redirect_labels
            .into_iter()
            .filter_map(|label| {
                Self::resolve_alias_redirect(&label, &alias_redirects)
                    .filter(|resolved| resolved != &label)
                    .map(|resolved| (label, resolved))
            })
            .collect();
        Ok((
            body.iter()
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
                .collect(),
            external_redirects,
        ))
    }

    pub(super) fn canonicalize_guarded_tail_segment(
        &mut self,
        segment: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        let mut flattened = Vec::new();
        Self::flatten_guarded_tail_segment(segment, &mut flattened);
        let Some((start, end)) = trim_ignorable_stmt_bounds(&flattened) else {
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        };
        let (flattened, external_redirects) = self.canonicalize_interleaved_local_aliases(
            &flattened[start..end],
            full_body,
            segment_start,
            referenced,
        )?;

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
                            let (external_top_level_before, external_nested_before, external_refs_after) =
                                Self::classify_external_alias_ref_sites(
                                    full_body,
                                    segment_start,
                                    segment_start + flattened.len(),
                                    label,
                                );
                            self.mark_alias_nonlocal_from_external_sites(
                                external_top_level_before,
                                external_nested_before,
                                external_refs_after,
                            );
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
                        if (idx + 1..flattened.len()).any(|pos| matches!(flattened[pos], HirStmt::Label(_))) {
                            self.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count += 1;
                        } else {
                            self.canonicalization_failed_interleaved_join_uses_no_next_label_count += 1;
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
                HirStmt::Goto(_) => {
                    if saw_payload {
                        if trailing_has_non_ignorable {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        let HirStmt::Goto(target) = stmt else {
                            unreachable!();
                        };
                        if flattened[..idx]
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(_)))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        if full_body
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == target))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                        if flattened
                            .iter()
                            .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == target))
                        {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
                    }
                    canonical.push(stmt.clone());
                }
                HirStmt::Break | HirStmt::Continue => {
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
        Ok((canonical, external_redirects))
    }
}
