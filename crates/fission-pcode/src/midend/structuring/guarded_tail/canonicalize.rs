use super::*;

impl<'a> PreviewBuilder<'a> {
    fn top_level_guard_goto_signature(stmt: &HirStmt) -> Option<(&HirExpr, &str)> {
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
        match then_body.as_slice() {
            [HirStmt::Goto(label)] => Some((cond, label.as_str())),
            _ => None,
        }
    }

    fn collapse_duplicate_top_level_guard_ladder(stmts: &mut Vec<HirStmt>) -> usize {
        let mut removed = 0usize;
        let mut i = 0usize;

        while i < stmts.len() {
            let Some((cond_i, target_i)) = Self::top_level_guard_goto_signature(&stmts[i]) else {
                i += 1;
                continue;
            };

            // Keep this narrowly scoped: only allow empty blocks between duplicates.
            // Crossing labels can change ownership/fallthrough interpretation.
            let mut j = i + 1;
            while j < stmts.len() {
                match &stmts[j] {
                    HirStmt::Block(body) if body.is_empty() => j += 1,
                    _ => break,
                }
            }
            if j >= stmts.len() {
                i += 1;
                continue;
            }

            let Some((cond_j, target_j)) = Self::top_level_guard_goto_signature(&stmts[j]) else {
                i += 1;
                continue;
            };

            if cond_i == cond_j && target_i == target_j {
                stmts.remove(j);
                removed += 1;
                // Keep `i` to fold guard ladders of length >= 3.
                continue;
            }

            i += 1;
        }

        removed
    }

    fn top_level_label_definition_count(body: &[HirStmt], label: &str) -> usize {
        body.iter()
            .filter(|stmt| matches!(stmt, HirStmt::Label(candidate) if candidate == label))
            .count()
    }

    fn stmt_is_sink_safe_return_goto(stmt: &HirStmt, full_body: &[HirStmt]) -> bool {
        let HirStmt::Goto(target) = stmt else {
            return false;
        };
        if Self::top_level_label_definition_count(full_body, target) != 1 {
            return false;
        }
        matches!(
            Self::resolve_terminal_tail_exit_stmt(full_body, target),
            Some(HirStmt::Return(_))
        )
    }

    fn stmt_is_guard_cluster_trivial_gap(stmt: &HirStmt, full_body: &[HirStmt]) -> bool {
        if matches!(stmt, HirStmt::Label(_)) {
            return false;
        }
        is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, HirStmt::Block(body) if body.is_empty())
            || Self::stmt_is_sink_safe_return_goto(stmt, full_body)
    }

    fn stmt_is_sink_equivalent_after_label_gap(
        stmt: &HirStmt,
        full_body: &[HirStmt],
        sink_return: &Option<HirExpr>,
    ) -> bool {
        if is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, HirStmt::Block(body) if body.is_empty())
        {
            return true;
        }
        let HirStmt::Goto(target) = stmt else {
            return false;
        };
        if Self::top_level_label_definition_count(full_body, target) != 1 {
            return false;
        }
        matches!(
            Self::resolve_terminal_tail_exit_stmt(full_body, target),
            Some(HirStmt::Return(ret)) if ret == *sink_return
        )
    }

    fn local_after_label_ref_is_sink_equivalent(
        body: &[HirStmt],
        full_body: &[HirStmt],
        label: &str,
        label_idx: usize,
        after_label_pos: usize,
    ) -> bool {
        let Some(HirStmt::Goto(target)) = body.get(after_label_pos) else {
            return false;
        };
        if after_label_pos <= label_idx || target != label {
            return false;
        }
        if Self::top_level_label_definition_count(full_body, label) != 1 {
            return false;
        }

        let Some(HirStmt::Return(sink_return)) =
            Self::resolve_terminal_tail_exit_stmt(full_body, label)
        else {
            return false;
        };

        let next_label_idx = (after_label_pos + 1..body.len())
            .find(|pos| matches!(body[*pos], HirStmt::Label(_)))
            .unwrap_or(body.len());

        body[after_label_pos + 1..next_label_idx]
            .iter()
            .all(|stmt| {
                Self::stmt_is_sink_equivalent_after_label_gap(stmt, full_body, &sink_return)
            })
    }

    fn count_sink_equivalent_top_level_after_label_refs(
        body: &[HirStmt],
        full_body: &[HirStmt],
        label: &str,
        label_idx: usize,
        top_level_after_positions: &[usize],
        nested_after_label_count: usize,
        external_ref_count: usize,
    ) -> usize {
        if nested_after_label_count > 0 || external_ref_count > 0 {
            return 0;
        }
        top_level_after_positions
            .iter()
            .copied()
            .filter(|pos| {
                Self::local_after_label_ref_is_sink_equivalent(
                    body, full_body, label, label_idx, *pos,
                )
            })
            .count()
    }

    fn top_level_after_label_ref_is_dead_post_return(
        body: &[HirStmt],
        after_label_pos: usize,
        label: &str,
    ) -> bool {
        let Some(HirStmt::Goto(target)) = body.get(after_label_pos) else {
            return false;
        };
        if target != label {
            return false;
        }

        let mut saw_terminal_return = false;
        for stmt in &body[..after_label_pos] {
            if is_ignorable_discovery_stmt(stmt)
                || matches!(stmt, HirStmt::Block(inner) if inner.is_empty())
            {
                continue;
            }
            match stmt {
                HirStmt::Return(_) => saw_terminal_return = true,
                _ => saw_terminal_return = false,
            }
        }

        saw_terminal_return
    }

    fn factor_duplicate_top_level_guard_cluster_with_trivial_gap(
        stmts: &mut Vec<HirStmt>,
        full_body: &[HirStmt],
    ) -> usize {
        let mut removed = 0usize;
        let mut i = 0usize;

        while i < stmts.len() {
            let Some((cond_i, target_i)) = Self::top_level_guard_goto_signature(&stmts[i]) else {
                i += 1;
                continue;
            };

            let mut j = i + 1;
            let mut duplicate_at = None;
            while j < stmts.len() {
                if let Some((cond_j, target_j)) = Self::top_level_guard_goto_signature(&stmts[j]) {
                    if cond_i == cond_j && target_i == target_j {
                        duplicate_at = Some(j);
                    }
                    break;
                }
                if Self::stmt_is_guard_cluster_trivial_gap(&stmts[j], full_body) {
                    j += 1;
                    continue;
                }
                break;
            }

            if let Some(j) = duplicate_at {
                stmts.remove(j);
                removed += 1;
                // Keep `i` for chains with >= 3 same-family guards.
                continue;
            }

            i += 1;
        }

        removed
    }

    fn stmt_is_guard_prefix_safe(stmt: &HirStmt) -> bool {
        is_ignorable_discovery_stmt(stmt)
            || matches!(stmt, HirStmt::Label(_))
            || matches!(stmt, HirStmt::Block(body) if body.is_empty())
            || Self::top_level_guard_goto_signature(stmt).is_some()
    }

    fn collapse_top_level_sink_to_return_goto_chain(
        stmts: &mut [HirStmt],
        full_body: &[HirStmt],
    ) -> usize {
        let mut rewritten = 0usize;

        for idx in 0..stmts.len() {
            let target = match &stmts[idx] {
                HirStmt::Goto(target) => target.clone(),
                _ => continue,
            };

            // Restrict to guard-only prefixes so we don't consume payload-tail
            // exits that are already handled by canonical tail logic.
            if !stmts[..idx].iter().all(Self::stmt_is_guard_prefix_safe) {
                continue;
            }

            // Keep this narrow: collapse only when the target label is unique
            // and the existing terminal-safe resolver proves a return sink.
            if Self::top_level_label_definition_count(full_body, &target) != 1 {
                continue;
            }

            let Some(HirStmt::Return(ret)) =
                Self::resolve_terminal_tail_exit_stmt(full_body, &target)
            else {
                continue;
            };

            stmts[idx] = HirStmt::Return(ret);
            rewritten += 1;
        }

        rewritten
    }

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
            let sink_equivalent_top_level_after_label_count =
                Self::count_sink_equivalent_top_level_after_label_refs(
                    body,
                    full_body,
                    label,
                    idx,
                    &top_level_after_positions,
                    nested_after_label_count,
                    external_ref_count,
                );
            let effective_top_level_after_label_count = top_level_after_label_count
                .saturating_sub(sink_equivalent_top_level_after_label_count);
            let blocking_top_level_after_positions: Vec<usize> = top_level_after_positions
                .iter()
                .copied()
                .filter(|pos| {
                    !Self::local_after_label_ref_is_sink_equivalent(
                        body, full_body, label, idx, *pos,
                    ) && !Self::top_level_after_label_ref_is_dead_post_return(body, *pos, label)
                })
                .collect();
            if nested_before > 0 {
                return Err(
                    GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                );
            }
            let has_non_ignorable_gap =
                goto_positions.iter().filter(|pos| **pos < idx).any(|pos| {
                    body[pos + 1..idx]
                        .iter()
                        .any(|stmt| !is_ignorable_discovery_stmt(stmt))
                });
            let next_label_idx =
                (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
            let payload_end = next_label_idx.unwrap_or(body.len());
            let segment = &body[idx + 1..payload_end];
            let allow_top_level_after_label_redirect = if let Some(next_label_idx) = next_label_idx
            {
                if let HirStmt::Label(next_label) = &body[next_label_idx] {
                    nested_after_label_count == 0
                        && !blocking_top_level_after_positions.is_empty()
                        && blocking_top_level_after_positions
                            .iter()
                            .all(|pos| *pos < next_label_idx)
                        && Self::is_local_alias_forward_segment_with_after_label_refs(
                            segment, label, next_label,
                        )
                } else {
                    false
                }
            } else {
                nested_after_label_count == 0
                    && !blocking_top_level_after_positions.is_empty()
                    && Self::inferred_alias_forward_target_with_after_label_refs(segment, label)
                        .is_some()
            };

            if self.guarded_tail_trace_enabled_for_current_fn()
                && sink_equivalent_top_level_after_label_count > 0
            {
                eprintln!(
                    "[GT-TRACE] candidate={} alias_after_sink_equiv label={} raw_after={} sink_equiv={} effective_after={}",
                    segment_start.saturating_sub(1),
                    label,
                    top_level_after_label_count,
                    sink_equivalent_top_level_after_label_count,
                    effective_top_level_after_label_count
                );
            }

            if nested_after_label_count > 0
                || (effective_top_level_after_label_count > 0
                    && !allow_top_level_after_label_redirect)
            {
                self.telemetry
                    .structuring
                    .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
                    effective_top_level_after_label_count;
                self.telemetry
                    .structuring
                    .canonicalization_failed_alias_not_fallthrough_nested_after_label_count +=
                    nested_after_label_count;
                return Err(GuardedTailCanonicalizationFailure::AliasNotFallthrough);
            }

            // Priority 1: If we have external refs with top-level-after-label + all top-level goto,
            // try forward-chain resolution first (allow reaching beyond immediate next label)
            let forward_chain_redirect = if allow_top_level_after_label_redirect
                && external_ref_count > 0
                && Self::are_all_external_refs_top_level_goto(
                    full_body,
                    segment_start,
                    segment_end,
                    label,
                ) {
                let resolved = if next_label_idx.is_some() {
                    self.resolve_terminal_join_target(body, idx, label, referenced)
                        .map(|(resolved_label, _)| resolved_label)
                } else {
                    Self::inferred_alias_forward_target_with_after_label_refs(segment, label)
                };
                resolved.and_then(|resolved_label| {
                    // Prefer forward-chain resolution if it goes beyond immediate next
                    if let Some(next_label_idx) = next_label_idx {
                        if let HirStmt::Label(next_label) = &body[next_label_idx] {
                            if resolved_label != *label && resolved_label != next_label.as_str() {
                                return Some(resolved_label);
                            }
                        }
                    } else if resolved_label != *label {
                        return Some(resolved_label);
                    }
                    None
                })
            } else {
                None
            };

            // Priority 2: Try immediate next-label redirect (only if forward-chain didn't apply)
            let immediate_next_redirect = if forward_chain_redirect.is_none() {
                if let Some(next_label_idx) = next_label_idx {
                    if let HirStmt::Label(next_label) = &body[next_label_idx] {
                        if Self::is_local_alias_forward_segment(segment, next_label)
                            || allow_top_level_after_label_redirect
                        {
                            Some(next_label.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let next_redirect_label = forward_chain_redirect.or(immediate_next_redirect);

            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] candidate={} alias_redirect label={} local_refs={} external_refs={} resolved={}",
                    segment_start.saturating_sub(1),
                    label,
                    local_ref_count,
                    external_ref_count,
                    next_redirect_label.as_deref().unwrap_or("<none>")
                );
            }

            if let Some(next_label) = next_redirect_label {
                if external_ref_count > 0 {
                    let (
                        external_top_level_before,
                        external_nested_before,
                        external_top_level_after,
                        external_nested_after,
                    ) = Self::classify_external_alias_ref_sites_detailed(
                        full_body,
                        segment_start,
                        segment_end,
                        label,
                    );
                    let nested_before_proof = if external_nested_before > 0 {
                        Some(Self::build_nested_before_alias_ownership_proof(
                            full_body,
                            segment_start,
                            segment_end,
                            label,
                            external_nested_before,
                        ))
                    } else {
                        None
                    };
                    let effective_nested_before = nested_before_proof
                        .as_ref()
                        .map(|proof| proof.effective_nested_before())
                        .unwrap_or(external_nested_before);
                    let internalized_nested_before =
                        external_nested_before.saturating_sub(effective_nested_before);
                    let external_refs_after = external_top_level_after + external_nested_after;
                    if external_nested_after > 0 {
                        self.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    if self.guarded_tail_trace_enabled_for_current_fn() {
                        if let Some(proof) = nested_before_proof.as_ref() {
                            eprintln!(
                                "[GT-TRACE] candidate={} alias_ownership label={} raw_nested_before={} internalized_nested_before={} class={:?} legality={:?} witnesses={:?}",
                                segment_start.saturating_sub(1),
                                proof.label,
                                proof.raw_nested_before,
                                proof.internalized_nested_before,
                                proof.class,
                                proof.legality_reason,
                                proof
                                    .witnesses
                                    .iter()
                                    .map(|w| (w.stmt_idx, &w.class, &w.cond))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    if effective_nested_before > 0 {
                        self.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    let effective_external_ref_count =
                        external_ref_count.saturating_sub(internalized_nested_before);
                    if external_top_level_before + external_top_level_after
                        != effective_external_ref_count
                    {
                        self.mark_alias_nonlocal_external_before();
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    external_safe_redirect_labels.push(label.clone());
                }
                if has_non_ignorable_gap {
                    if goto_positions.len() != 1
                        && !Self::is_pure_multi_goto_gap_to_label(body, goto_positions, idx, label)
                    {
                        return Err(
                            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors,
                        );
                    }
                    canonicalized_local_nonfallthrough += 1;
                } else if effective_top_level_after_label_count > 0 {
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
                alias_redirects.insert(label.clone(), None);
                continue;
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

        self.telemetry
            .structuring
            .canonicalized_interleaved_join_use_count += alias_redirects.len();
        self.telemetry
            .structuring
            .canonicalized_local_nonfallthrough_alias_count += canonicalized_local_nonfallthrough;
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
        let flatten_before_len = flattened.len();
        let collapsed_guards = Self::collapse_duplicate_top_level_guard_ladder(&mut flattened);
        let factored_guard_clusters =
            Self::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
                &mut flattened,
                full_body,
            );
        let collapsed_sink_returns =
            Self::collapse_top_level_sink_to_return_goto_chain(&mut flattened, full_body);
        let Some((start, end)) = trim_ignorable_stmt_bounds(&flattened) else {
            if self.guarded_tail_trace_enabled_for_current_fn() {
                eprintln!(
                    "[GT-TRACE] candidate={} canonicalize flatten_before={} trim=<none> collapse_dup={} cluster={} sink={} first_reject={:?}",
                    segment_start.saturating_sub(1),
                    flatten_before_len,
                    collapsed_guards,
                    factored_guard_clusters,
                    collapsed_sink_returns,
                    GuardedTailCanonicalizationFailure::NonterminalJoinLabel
                );
                Self::guarded_tail_trace_emit_snapshot(
                    "[GT-TRACE] canonicalize_snapshot",
                    &flattened,
                    20,
                );
            }
            return Err(GuardedTailCanonicalizationFailure::NonterminalJoinLabel);
        };
        let (flattened, external_redirects) = self.canonicalize_interleaved_local_aliases(
            &flattened[start..end],
            full_body,
            segment_start,
            referenced,
        )?;

        if self.guarded_tail_trace_enabled_for_current_fn() {
            eprintln!(
                "[GT-TRACE] candidate={} canonicalize flatten_before={} trim=[{}, {}) flatten_after={} collapse_dup={} cluster={} sink={} redirects={:?}",
                segment_start.saturating_sub(1),
                flatten_before_len,
                start,
                end,
                flattened.len(),
                collapsed_guards,
                factored_guard_clusters,
                collapsed_sink_returns,
                external_redirects
            );
        }

        let mut canonical = Vec::new();
        let mut saw_payload = false;
        let mut saw_gap_after_payload = false;
        let mut removed_any = start > 0
            || end < flattened.len()
            || flattened.len() != end - start
            || collapsed_guards > 0
            || factored_guard_clusters > 0
            || collapsed_sink_returns > 0;
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
                        let terminalizable_target =
                            Self::terminalizable_join_alias_target(&flattened, idx);
                        if total_ref_count > local_ref_count {
                            let (
                                external_top_level_before,
                                external_nested_before,
                                external_top_level_after,
                                external_nested_after,
                            ) = Self::classify_external_alias_ref_sites_detailed(
                                full_body,
                                segment_start,
                                segment_start + flattened.len(),
                                label,
                            );
                            let nested_before_proof = if external_nested_before > 0 {
                                Some(Self::build_nested_before_alias_ownership_proof(
                                    full_body,
                                    segment_start,
                                    segment_start + flattened.len(),
                                    label,
                                    external_nested_before,
                                ))
                            } else {
                                None
                            };
                            let effective_nested_before = nested_before_proof
                                .as_ref()
                                .map(|proof| proof.effective_nested_before())
                                .unwrap_or(external_nested_before);
                            let external_refs_after =
                                external_top_level_after + external_nested_after;
                            let only_top_level_external_refs =
                                effective_nested_before == 0 && external_nested_after == 0;
                            if self.guarded_tail_trace_enabled_for_current_fn() {
                                if let Some(proof) = nested_before_proof.as_ref() {
                                    eprintln!(
                                        "[GT-TRACE] candidate={} terminalizable_alias label={} raw_nested_before={} internalized_nested_before={} class={:?} legality={:?}",
                                        segment_start.saturating_sub(1),
                                        proof.label,
                                        proof.raw_nested_before,
                                        proof.internalized_nested_before,
                                        proof.class,
                                        proof.legality_reason,
                                    );
                                }
                            }
                            if !only_top_level_external_refs || terminalizable_target.is_none() {
                                self.mark_alias_nonlocal_from_external_sites(
                                    external_top_level_before,
                                    external_nested_before,
                                    external_refs_after,
                                );
                                return Err(
                                    GuardedTailCanonicalizationFailure::AliasHasNonlocalRef,
                                );
                            }
                        }
                        if let Some((next_label, next_idx)) = terminalizable_target {
                            Self::rewrite_goto_label_in_stmts(&mut canonical, label, &next_label);
                            removed_any = true;
                            self.telemetry
                                .structuring
                                .canonicalized_interleaved_join_use_count += 1;
                            idx = next_idx;
                            continue;
                        }
                        canonical.push(stmt.clone());
                        idx += 1;
                        continue;
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
                        let HirStmt::Goto(target) = stmt else {
                            unreachable!();
                        };
                        if let Some(return_stmt) =
                            Self::resolve_terminal_tail_exit_stmt(full_body, target)
                        {
                            canonical.push(return_stmt);
                            idx += 1;
                            continue;
                        }
                        if trailing_has_non_ignorable {
                            return Err(GuardedTailCanonicalizationFailure::NestedTailEscape);
                        }
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
            self.telemetry
                .structuring
                .canonicalized_guarded_tail_shape_count += 1;
        }
        Ok((canonical, external_redirects))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_duplicate_guard_ladder_identical_cond_target() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_identical_deref_cond_target() {
        let cond = HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("p".to_string())),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            ty: NirType::Bool,
        };
        let mut body = vec![
            HirStmt::If {
                cond: cond.clone(),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond,
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_allows_empty_block_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Block(Vec::new()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_rejects_different_cond() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c1".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Var("c2".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 0);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_rejects_different_target() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L1".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L2".to_string())],
                else_body: Vec::new(),
            },
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 0);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_rejects_non_ignorable_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("x".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("p".to_string())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 0);
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_does_not_touch_nested_loop_body() {
        let mut body = vec![
            HirStmt::While {
                cond: HirExpr::Var("loop_c".to_string()),
                body: vec![
                    HirStmt::If {
                        cond: HirExpr::Var("c".to_string()),
                        then_body: vec![HirStmt::Goto("L".to_string())],
                        else_body: Vec::new(),
                    },
                    HirStmt::If {
                        cond: HirExpr::Var("c".to_string()),
                        then_body: vec![HirStmt::Goto("L".to_string())],
                        else_body: Vec::new(),
                    },
                ],
            },
            HirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 0);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_sink_to_return_chain_top_level_goto_to_return() {
        let mut body = vec![
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 1);
        assert!(matches!(&body[0], HirStmt::Return(None)));
    }

    #[test]
    fn collapse_sink_to_return_chain_allows_pure_gap_hop() {
        let mut body = vec![
            HirStmt::Goto("Lhop".to_string()),
            HirStmt::Label("Lhop".to_string()),
            HirStmt::Expr(HirExpr::Var("tmp".to_string())),
            HirStmt::Assign {
                lhs: HirLValue::Var("x".to_string()),
                rhs: HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 1);
        assert!(matches!(&body[0], HirStmt::Return(None)));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_reentry() {
        let mut body = vec![
            HirStmt::Goto("Lret".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("Lret".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], HirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_ambiguous_target() {
        let mut body = vec![
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], HirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_side_effectful_gap() {
        let mut body = vec![
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "FUN_0x140001000".to_string(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], HirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_loop_crossing() {
        let mut body = vec![
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::While {
                cond: HirExpr::Var("loop_c".to_string()),
                body: vec![HirStmt::Break],
            },
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], HirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_guard_cluster_allows_sink_safe_trivial_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("Lret".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 1);
        assert_eq!(
            body.iter()
                .filter(|stmt| matches!(stmt, HirStmt::If { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn collapse_guard_cluster_allows_empty_block_and_sink_safe_gaps() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 1);
        assert_eq!(
            body.iter()
                .filter(|stmt| matches!(stmt, HirStmt::If { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn collapse_guard_cluster_rejects_side_effectful_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Expr(HirExpr::Call {
                target: "FUN_0x140001000".to_string(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 0);
    }

    #[test]
    fn collapse_guard_cluster_rejects_ambiguous_sink_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("Lret".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 0);
    }

    #[test]
    fn collapse_guard_cluster_rejects_label_crossing_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("mid".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("A".to_string()),
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 0);
    }

    #[test]
    fn collapse_guard_cluster_rejects_loop_crossing_sink_gap() {
        let mut body = vec![
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Goto("Lloop".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("c".to_string()),
                then_body: vec![HirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("Lloop".to_string()),
            HirStmt::While {
                cond: HirExpr::Var("loop_c".to_string()),
                body: vec![HirStmt::Break],
            },
            HirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_accepts_same_return_sink() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Label("Lafter".to_string()),
            HirStmt::Goto("L".to_string()),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[5],
            0,
            0,
        );

        assert_eq!(count, 1);
    }

    #[test]
    fn sink_equivalent_after_label_ref_accepts_empty_and_sink_safe_gap() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Label("Lafter".to_string()),
            HirStmt::Goto("L".to_string()),
            HirStmt::Block(Vec::new()),
            HirStmt::Goto("Lhop".to_string()),
            HirStmt::Label("Lhop".to_string()),
            HirStmt::Return(None),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[5],
            0,
            0,
        );

        assert_eq!(count, 1);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_nested_after_ref() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            1,
            0,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_side_effectful_gap() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
            HirStmt::Expr(HirExpr::Call {
                target: "FUN_0x140002000".to_string(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            0,
            0,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_ambiguous_sink_target() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
            HirStmt::Goto("Lamb".to_string()),
            HirStmt::Label("Lamb".to_string()),
            HirStmt::Return(None),
            HirStmt::Label("Lamb".to_string()),
            HirStmt::Return(None),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            0,
            0,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_nonlocal_reentry() {
        let body = vec![
            HirStmt::Goto("L".to_string()),
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            1,
            &[5],
            0,
            0,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_label_crossing_to_non_sink_join() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
            HirStmt::Goto("Lother".to_string()),
            HirStmt::Label("Lother".to_string()),
            HirStmt::Goto("Ltail".to_string()),
            HirStmt::Label("Ltail".to_string()),
            HirStmt::Return(Some(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            0,
            0,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_external_ref_ownership_change() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            0,
            1,
        );

        assert_eq!(count, 0);
    }

    #[test]
    fn sink_equivalent_after_label_ref_rejects_different_terminal_sink() {
        let body = vec![
            HirStmt::Label("L".to_string()),
            HirStmt::Goto("Lret".to_string()),
            HirStmt::Label("Lret".to_string()),
            HirStmt::Return(None),
            HirStmt::Goto("L".to_string()),
            HirStmt::Goto("Lother".to_string()),
            HirStmt::Label("Lother".to_string()),
            HirStmt::Return(Some(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
        ];

        let count = PreviewBuilder::count_sink_equivalent_top_level_after_label_refs(
            &body,
            &body,
            "L",
            0,
            &[4],
            0,
            0,
        );

        assert_eq!(count, 0);
    }
}
