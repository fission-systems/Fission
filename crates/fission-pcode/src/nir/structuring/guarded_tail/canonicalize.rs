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
                        && !top_level_after_positions.is_empty()
                        && top_level_after_positions
                            .iter()
                            .all(|pos| *pos < next_label_idx)
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
            
            // Priority 1: If we have external refs with top-level-after-label + all top-level goto,
            // try forward-chain resolution first (allow reaching beyond immediate next label)
            let forward_chain_redirect = if allow_top_level_after_label_redirect
                && external_ref_count > 0
                && Self::are_all_external_refs_top_level_goto(full_body, segment_start, segment_end, label)
            {
                self.resolve_terminal_join_target(body, idx, label, referenced)
                    .and_then(|(resolved_label, _)| {
                        // Prefer forward-chain resolution if it goes beyond immediate next
                        if let Some(next_label_idx) = next_label_idx {
                            if let HirStmt::Label(next_label) = &body[next_label_idx] {
                                if resolved_label != *label
                                    && resolved_label != next_label.as_str()
                                {
                                    return Some(resolved_label);
                                }
                            }
                        }
                        None
                    })
            } else {
                None
            };
            
            // Priority 2: Try immediate next-label redirect (only if forward-chain didn't apply)
            let immediate_next_redirect = if forward_chain_redirect.is_none() {
                if let Some(next_label_idx) = next_label_idx
                    && let HirStmt::Label(next_label) = &body[next_label_idx]
                    && (Self::is_local_alias_forward_segment(segment, next_label)
                        || allow_top_level_after_label_redirect)
                {
                    Some(next_label.clone())
                } else {
                    None
                }
            } else {
                None
            };
            
            let next_redirect_label = forward_chain_redirect.or(immediate_next_redirect);

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
                    let external_refs_after = external_top_level_after + external_nested_after;
                    if external_nested_after > 0 {
                        self.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    if external_nested_before > 0 {
                        self.mark_alias_nonlocal_from_external_sites(
                            external_top_level_before,
                            external_nested_before,
                            external_refs_after,
                        );
                        return Err(GuardedTailCanonicalizationFailure::AliasHasNonlocalRef);
                    }
                    if external_top_level_before + external_top_level_after != external_ref_count {
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
        let collapsed_guards = Self::collapse_duplicate_top_level_guard_ladder(&mut flattened);
        let collapsed_sink_returns =
            Self::collapse_top_level_sink_to_return_goto_chain(&mut flattened, full_body);
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
        let mut removed_any = start > 0
            || end < flattened.len()
            || flattened.len() != end - start
            || collapsed_guards > 0
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
                        if total_ref_count > local_ref_count {
                            let (
                                external_top_level_before,
                                external_nested_before,
                                external_refs_after,
                            ) = Self::classify_external_alias_ref_sites(
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
            self.canonicalized_guarded_tail_shape_count += 1;
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
}
