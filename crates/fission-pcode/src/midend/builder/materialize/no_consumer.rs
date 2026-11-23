use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn no_consumer_suppression_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            matches!(
                std::env::var("FISSION_ENABLE_NO_CONSUMER_SUPPRESSION"),
                Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
            )
        })
    }

    pub(in crate::midend::builder) fn is_x86_status_flag_output(output: &Varnode) -> bool {
        is_register_varnode(output)
            && output.size == 1
            && matches!(
                output.offset,
                0x200 | 0x202 | 0x204 | 0x206 | 0x207 | 0x209 | 0x20a | 0x20b
            )
    }

    /// Whether `output` names one of the `.pspec`-declared hidden registers
    /// (e.g. ARM's `ISAModeSwitch`/`TB` interworking-mode bookkeeping): a
    /// SLEIGH register that only steers how the *next* instruction gets
    /// decoded, never real program-visible state, so a dead write to it is
    /// as safe to drop as a dead write to an x86 status flag -- generalizes
    /// `is_x86_status_flag_output` beyond the one architecture it was
    /// hardcoded for. `hw_name_at` always lowercases, so the comparison
    /// against the pspec's original-case register names is case-insensitive.
    pub(in crate::midend::builder) fn is_hidden_pspec_register_output(
        &self,
        output: &Varnode,
    ) -> bool {
        if !is_register_varnode(output) || self.options.pspec_hidden_registers.is_empty() {
            return false;
        }
        let Some(name) = self.sla_hw_name(output.offset, output.size) else {
            return false;
        };
        self.options
            .pspec_hidden_registers
            .iter()
            .any(|hidden| hidden.eq_ignore_ascii_case(&name))
    }

    fn no_consumer_flag_rhs_is_pure(expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(..) => true,
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
                Self::no_consumer_flag_rhs_is_pure(expr)
            }
            PreHirExpr::FieldAccess { base, .. } => Self::no_consumer_flag_rhs_is_pure(base),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                Self::no_consumer_flag_rhs_is_pure(lhs) && Self::no_consumer_flag_rhs_is_pure(rhs)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::no_consumer_flag_rhs_is_pure(cond)
                    && Self::no_consumer_flag_rhs_is_pure(then_expr)
                    && Self::no_consumer_flag_rhs_is_pure(else_expr)
            }
            PreHirExpr::Call { .. }
            | PreHirExpr::Load { .. }
            | PreHirExpr::PtrOffset { .. }
            | PreHirExpr::Index { .. }
            | PreHirExpr::AggregateCopy { .. } => false,
        }
    }

    pub(super) fn analyze_no_consumer_materialization_profile(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> NoConsumerMaterializationProfile {
        let same_block_consumers =
            Self::collect_output_use_sites_in_block(block, op_idx, output).len();
        let (cross_block_consumers, has_phi_merge_use) =
            self.collect_output_use_sites_outside_block(block.start_address, output);
        NoConsumerMaterializationProfile {
            same_block_consumers,
            cross_block_consumers,
            has_later_block_use: cross_block_consumers > 0,
            has_phi_merge_use,
            has_debug_use: false,
            rhs_side_effectful: Self::expr_is_side_effectful_for_materialization_trace(rhs),
        }
    }

    pub(super) fn classify_no_consumer_materialization_decision(
        output: &Varnode,
        rhs: &PreHirExpr,
        legacy_inline_candidate: bool,
        plan: ReplacementValuePlan,
        hazard: Option<AliasUnsafeHazard>,
        profile: NoConsumerMaterializationProfile,
        is_hidden_pspec_register: bool,
    ) -> NoConsumerMaterializationDecision {
        if plan.rejection_reason() != Some(MaterializationRejectionReason::AliasUnsafe) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound,
            );
        }
        if hazard.map(|hazard| hazard.kind) != Some(AliasUnsafeHazardKind::UnknownNoConsumerFound) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound,
            );
        }
        if profile.same_block_consumers != 0 {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::SameBlockConsumerPresent,
            );
        }
        if profile.cross_block_consumers != 0 {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::CrossBlockConsumerPresent,
            );
        }
        if profile.has_later_block_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::LaterBlockUsePresent,
            );
        }
        if profile.has_phi_merge_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PhiMergeUsePresent,
            );
        }
        if profile.has_debug_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::DebugUsePresent,
            );
        }
        if (Self::is_x86_status_flag_output(output) || is_hidden_pspec_register)
            && Self::no_consumer_flag_rhs_is_pure(rhs)
        {
            return NoConsumerMaterializationDecision::SuppressAlways;
        }
        if legacy_inline_candidate {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::LegacyInlineCandidate,
            );
        }
        if Self::should_preserve_materialized_expr(rhs) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PreserveMaterialization,
            );
        }
        if profile.rhs_side_effectful {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::RhsSideEffectful,
            );
        }
        if output.space_id != UNIQUE_SPACE_ID || output.is_constant {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::StateVisibleOutput,
            );
        }
        NoConsumerMaterializationDecision::Suppress
    }

    pub(super) fn classify_no_consumer_suppression_rhs_kind(
        rhs: &PreHirExpr,
    ) -> NoConsumerSuppressionRhsKind {
        match rhs {
            PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
                NoConsumerSuppressionRhsKind::Var
            }
            PreHirExpr::Const(..) => NoConsumerSuppressionRhsKind::Const,
            PreHirExpr::Cast { .. } => NoConsumerSuppressionRhsKind::Cast,
            PreHirExpr::Unary { .. } => NoConsumerSuppressionRhsKind::Unary,
            PreHirExpr::Binary { .. } => NoConsumerSuppressionRhsKind::Binary,
            PreHirExpr::Load { .. } => NoConsumerSuppressionRhsKind::Load,
            PreHirExpr::Call { .. } => NoConsumerSuppressionRhsKind::Call,
            PreHirExpr::AggregateCopy { .. } => NoConsumerSuppressionRhsKind::Aggregate,
            PreHirExpr::PtrOffset { .. } => NoConsumerSuppressionRhsKind::PtrOffset,
            PreHirExpr::Index { .. } => NoConsumerSuppressionRhsKind::Index,
            PreHirExpr::Select { .. } => NoConsumerSuppressionRhsKind::Select,
            PreHirExpr::FieldAccess { .. } => NoConsumerSuppressionRhsKind::FieldAccess,
        }
    }

    pub(super) fn classify_no_consumer_suppression_output_kind(
        output: &Varnode,
    ) -> NoConsumerSuppressionOutputKind {
        if output.space_id == UNIQUE_SPACE_ID && !output.is_constant {
            NoConsumerSuppressionOutputKind::TempOnly
        } else if output.space_id == REGISTER_SPACE_ID {
            NoConsumerSuppressionOutputKind::RegisterVisible
        } else {
            NoConsumerSuppressionOutputKind::MemoryDerived
        }
    }

    pub(super) fn classify_no_consumer_suppression_block_position(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> NoConsumerSuppressionBlockPosition {
        if let Some(term_idx) = self.block_terminator_index(block) {
            let term = &block.ops[term_idx];
            if op_idx + 1 == term_idx {
                return match term.opcode {
                    PcodeOpcode::CBranch => NoConsumerSuppressionBlockPosition::PredicateAdjacent,
                    PcodeOpcode::Return => NoConsumerSuppressionBlockPosition::ReturnAdjacent,
                    _ => NoConsumerSuppressionBlockPosition::PreBranch,
                };
            }
            if op_idx < term_idx {
                return NoConsumerSuppressionBlockPosition::PreBranch;
            }
        }
        let Some(block_idx) = self.block_position(block) else {
            return NoConsumerSuppressionBlockPosition::Local;
        };
        if self.successors.get(block_idx).is_some_and(|succs| {
            succs.iter().any(|succ| {
                self.predecessors
                    .get(*succ)
                    .is_some_and(|preds| preds.len() > 1)
            })
        }) {
            return NoConsumerSuppressionBlockPosition::MergeAdjacent;
        }
        NoConsumerSuppressionBlockPosition::Local
    }

    pub(super) fn collect_output_use_sites_outside_block(
        &self,
        current_block_addr: u64,
        output: &Varnode,
    ) -> (usize, bool) {
        let key = VarnodeKey::from(output);
        let mut consumer_count = 0usize;
        let mut has_phi_merge_use = false;
        // Only ops that can match the key are visited, in (block, op) order;
        // `varnode_matches_key` below stays the sole judge of a match. A block
        // whose scan ended (it redefined the key, or it is the current block)
        // is skipped for its remaining candidates.
        let mut finished_block = None;
        for &(block_idx, op_idx) in self.space_op_index.candidate_ops(&key).iter() {
            if finished_block == Some(block_idx) {
                continue;
            }
            let block = &self.pcode.blocks[block_idx];
            if block.start_address == current_block_addr {
                finished_block = Some(block_idx);
                continue;
            }
            let candidate = &block.ops[op_idx];
            if candidate
                .output
                .as_ref()
                .is_some_and(|output| Self::varnode_matches_key(output, &key))
            {
                finished_block = Some(block_idx);
                continue;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| Self::varnode_matches_key(input, &key))
            {
                consumer_count += 1;
                has_phi_merge_use |= candidate.opcode == PcodeOpcode::MultiEqual;
            }
        }
        (consumer_count, has_phi_merge_use)
    }
}

/// Which ops of a function can read or write a given storage key, derived once
/// from the immutable p-code in `PreviewBuilder::new`.
///
/// A scan for the readers and writers of one key would otherwise visit every
/// op of every block per query. `varnode_matches_key` holds only between
/// varnodes of one address space whose byte ranges coincide or overlap, so
/// ops are filed under the fixed-size buckets of their varnodes' byte ranges
/// and a query visits just the ops sharing a bucket with its key. The result
/// is a superset of the true matches and the caller still applies
/// `varnode_matches_key`, so the index can only skip ops that could not match.
#[derive(Debug, Default)]
pub(in crate::midend::builder) struct SpaceOpIndex {
    /// Every op touching the space (constants included): the exact fallback
    /// for keys the buckets cannot bound.
    by_space: HashMap<u64, Vec<(usize, usize)>>,
    /// Non-constant varnodes, by `(space, offset >> BUCKET_SHIFT)`.
    by_bucket: HashMap<(u64, u64), Vec<(usize, usize)>>,
    /// Calls with no output varnode, in `(block, op)` order.
    call_ops: Vec<(usize, usize)>,
    /// Ops with a non-constant varnode too wide to bucket, per space.
    wide: HashMap<u64, Vec<(usize, usize)>>,
}

impl SpaceOpIndex {
    const BUCKET_SHIFT: u32 = 4;
    const MAX_BUCKET_SPAN: u64 = 64;

    /// The inclusive bucket range a varnode's bytes fall in, or `None` when it
    /// spans too many buckets to enumerate. A zero-size varnode is placed at
    /// its offset: it can only match by exact equality.
    fn bucket_span(offset: u64, size: u32) -> Option<(u64, u64)> {
        let last = offset.saturating_add(u64::from(size.max(1)) - 1);
        let (low, high) = (offset >> Self::BUCKET_SHIFT, last >> Self::BUCKET_SHIFT);
        (high - low < Self::MAX_BUCKET_SPAN).then_some((low, high))
    }

    pub(in crate::midend::builder) fn build(blocks: &[crate::pcode::PcodeBasicBlock]) -> Self {
        fn push_op(list: &mut Vec<(usize, usize)>, site: (usize, usize)) {
            if list.last() != Some(&site) {
                list.push(site);
            }
        }
        let mut index = Self::default();
        for (block_idx, block) in blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                let site = (block_idx, op_idx);
                if op.output.is_none()
                    && matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                {
                    index.call_ops.push(site);
                }
                for vn in op.output.iter().chain(op.inputs.iter()) {
                    push_op(index.by_space.entry(vn.space_id).or_default(), site);
                    if vn.is_constant {
                        continue;
                    }
                    match Self::bucket_span(vn.offset, vn.size) {
                        Some((low, high)) => {
                            for bucket in low..=high {
                                push_op(
                                    index.by_bucket.entry((vn.space_id, bucket)).or_default(),
                                    site,
                                );
                            }
                        }
                        None => push_op(index.wide.entry(vn.space_id).or_default(), site),
                    }
                }
            }
        }
        index
    }

    /// The entries of `candidates` (as returned by `candidate_ops`) that lie in
    /// `block_idx`, in op order.
    pub(in crate::midend::builder) fn candidates_in_block(
        candidates: &[(usize, usize)],
        block_idx: usize,
    ) -> &[(usize, usize)] {
        let start = candidates.partition_point(|&(block, _)| block < block_idx);
        let len = candidates[start..].partition_point(|&(block, _)| block == block_idx);
        &candidates[start..start + len]
    }

    /// `candidate_ops` plus every output-less call, for a scan whose
    /// redefinition test is not only a varnode match: such a call clobbers a
    /// return register without naming it.
    pub(in crate::midend::builder) fn candidate_ops_with_calls(
        &self,
        key: &VarnodeKey,
    ) -> std::borrow::Cow<'_, [(usize, usize)]> {
        let candidates = self.candidate_ops(key);
        if self.call_ops.is_empty() {
            return candidates;
        }
        let mut merged = candidates.into_owned();
        merged.extend_from_slice(&self.call_ops);
        merged.sort_unstable();
        merged.dedup();
        std::borrow::Cow::Owned(merged)
    }

    /// The ascending, repeat-free op lists that together hold every
    /// `(block, op)` that may carry a varnode matching `key`.
    fn candidate_lists<'s>(
        &'s self,
        key: &VarnodeKey,
    ) -> impl Iterator<Item = &'s [(usize, usize)]> {
        let span = (!key.is_constant)
            .then(|| Self::bucket_span(key.offset, key.size))
            .flatten();
        let (buckets, wide, whole_space) = match span {
            Some((low, high)) => (low..=high, self.wide.get(&key.space_id), None),
            None => (1..=0, None, self.by_space.get(&key.space_id)),
        };
        buckets
            .filter_map(move |bucket| self.by_bucket.get(&(key.space_id, bucket)))
            .chain(wide)
            .chain(whole_space)
            .map(Vec::as_slice)
            .filter(|ops| !ops.is_empty())
    }

    /// Every `(block, op)` that may hold a varnode matching `key`, in
    /// ascending order without repeats.
    ///
    /// Each stored list is already ascending and repeat-free, so when only one
    /// list applies (the common case: a key inside one bucket) it is borrowed
    /// as is; several lists are merged into a fresh sorted vector.
    pub(in crate::midend::builder) fn candidate_ops(
        &self,
        key: &VarnodeKey,
    ) -> std::borrow::Cow<'_, [(usize, usize)]> {
        let mut lists = self.candidate_lists(key);
        let Some(first) = lists.next() else {
            return std::borrow::Cow::Borrowed(&[]);
        };
        let Some(second) = lists.next() else {
            return std::borrow::Cow::Borrowed(first);
        };
        let mut merged = Vec::with_capacity(first.len() + second.len());
        merged.extend_from_slice(first);
        merged.extend_from_slice(second);
        for list in lists {
            merged.extend_from_slice(list);
        }
        merged.sort_unstable();
        merged.dedup();
        std::borrow::Cow::Owned(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    #[test]
    fn no_consumer_materialization_profile_marks_deadish_local_output() {
        let output = varnode(0x10);
        let blocks = vec![block(vec![op(
            0,
            PcodeOpcode::Copy,
            Some(output.clone()),
            vec![constant(1)],
        )])];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let profile = builder.analyze_no_consumer_materialization_profile(
            &blocks[0],
            0,
            &output,
            &PreHirExpr::Const(1, int(32)),
        );

        assert_eq!(profile.same_block_consumers, 0);
        assert_eq!(profile.cross_block_consumers, 0);
        assert!(!profile.has_later_block_use);
        assert!(!profile.has_phi_merge_use);
        assert!(!profile.has_debug_use);
        assert!(!profile.rhs_side_effectful);
    }

    #[test]
    fn no_consumer_materialization_profile_detects_cross_block_multiequal_use() {
        let output = varnode(0x10);
        let blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x2000,
                1,
                vec![op(
                    0,
                    PcodeOpcode::MultiEqual,
                    Some(varnode(0x20)),
                    vec![output.clone(), constant(2)],
                )],
            ),
        ];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let profile = builder.analyze_no_consumer_materialization_profile(
            &blocks[0],
            0,
            &output,
            &PreHirExpr::Const(1, int(32)),
        );

        assert_eq!(profile.same_block_consumers, 0);
        assert_eq!(profile.cross_block_consumers, 1);
        assert!(profile.has_later_block_use);
        assert!(profile.has_phi_merge_use);
        assert!(!profile.has_debug_use);
    }

    #[test]
    fn no_consumer_materialization_decision_suppresses_dead_unique_const() {
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &varnode(0x10),
            &PreHirExpr::Const(1, int(32)),
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
            false,
        );

        assert_eq!(decision, NoConsumerMaterializationDecision::Suppress);
    }

    #[test]
    fn no_consumer_materialization_decision_keeps_preserved_rhs() {
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &varnode(0x10),
            &PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
            false,
        );

        assert_eq!(
            decision,
            NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PreserveMaterialization
            )
        );
    }

    #[test]
    fn no_consumer_materialization_decision_keeps_non_unique_outputs() {
        let mut output = varnode(0x10);
        output.space_id = REGISTER_SPACE_ID;
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &output,
            &PreHirExpr::Const(1, int(32)),
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
            false,
        );

        assert_eq!(
            decision,
            NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::StateVisibleOutput
            )
        );
    }

    #[test]
    fn no_consumer_materialization_decision_suppresses_unused_x86_flags() {
        let mut output = varnode(0x206);
        output.space_id = REGISTER_SPACE_ID;
        output.size = 1;
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &output,
            &PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
            false,
        );

        assert_eq!(decision, NoConsumerMaterializationDecision::SuppressAlways);
    }

    /// The scan the space index replaced: every op of every other block.
    fn reference_outside_block_uses(
        blocks: &[crate::pcode::PcodeBasicBlock],
        current_block_addr: u64,
        output: &Varnode,
    ) -> (usize, bool) {
        let key = VarnodeKey::from(output);
        let mut consumer_count = 0usize;
        let mut has_phi_merge_use = false;
        for block in blocks {
            if block.start_address == current_block_addr {
                continue;
            }
            for candidate in &block.ops {
                if candidate
                    .output
                    .as_ref()
                    .is_some_and(|output| PreviewBuilder::varnode_matches_key(output, &key))
                {
                    break;
                }
                if candidate
                    .inputs
                    .iter()
                    .any(|input| PreviewBuilder::varnode_matches_key(input, &key))
                {
                    consumer_count += 1;
                    has_phi_merge_use |= candidate.opcode == PcodeOpcode::MultiEqual;
                }
            }
        }
        (consumer_count, has_phi_merge_use)
    }

    #[test]
    fn outside_block_use_scan_matches_exhaustive_scan() {
        let reg = |space, offset, size| Varnode {
            space_id: space,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        };
        let wide = reg(REGISTER_SPACE_ID, 0x10, 8);
        let low = reg(REGISTER_SPACE_ID, 0x10, 4);
        let high = reg(REGISTER_SPACE_ID, 0x14, 4);
        let far = reg(REGISTER_SPACE_ID, 0x40, 8);
        let other_space = reg(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
        let temp = varnode(0x30);
        // straddles the 16-byte bucket boundary at 0x20, sits inside one
        // bucket, spans more buckets than the index enumerates, and has no size
        let straddle = reg(REGISTER_SPACE_ID, 0x1e, 4);
        let inside = reg(REGISTER_SPACE_ID, 0x21, 1);
        let huge = reg(REGISTER_SPACE_ID, 0x100, 4096);
        let huge_inner = reg(REGISTER_SPACE_ID, 0x800, 8);
        let empty = reg(REGISTER_SPACE_ID, 0x10, 0);
        let blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(0, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(1)]),
                    op(1, PcodeOpcode::Copy, Some(temp.clone()), vec![wide.clone()]),
                ],
            ),
            // reads an overlapping half, then redefines the whole register:
            // the read counts, the later read does not.
            block_at(
                0x2000,
                1,
                vec![
                    op(0, PcodeOpcode::Copy, Some(far.clone()), vec![low.clone()]),
                    op(1, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(2)]),
                    op(2, PcodeOpcode::Copy, Some(far.clone()), vec![wide.clone()]),
                ],
            ),
            block_at(
                0x3000,
                2,
                vec![
                    op(
                        0,
                        PcodeOpcode::MultiEqual,
                        Some(far.clone()),
                        vec![high.clone(), wide.clone()],
                    ),
                    op(1, PcodeOpcode::Copy, Some(far.clone()), vec![temp.clone()]),
                ],
            ),
            // a same-space-id read of a different register, and a different space
            block_at(
                0x4000,
                3,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(far.clone()),
                        vec![other_space.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntAdd,
                        Some(far.clone()),
                        vec![far.clone(), constant(3)],
                    ),
                ],
            ),
            block_at(
                0x5000,
                4,
                vec![op(0, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
            block_at(
                0x6000,
                5,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(far.clone()),
                        vec![straddle.clone()],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(huge.clone()),
                        vec![inside.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(far.clone()),
                        vec![huge_inner.clone()],
                    ),
                    op(
                        3,
                        PcodeOpcode::Copy,
                        Some(empty.clone()),
                        vec![empty.clone()],
                    ),
                ],
            ),
            block_at(
                0x7000,
                6,
                vec![
                    op(0, PcodeOpcode::Copy, Some(far.clone()), vec![huge.clone()]),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(inside.clone()),
                        vec![empty.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(far.clone()),
                        vec![straddle.clone()],
                    ),
                ],
            ),
        ];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let probes = [
            wide,
            low,
            high,
            far,
            other_space,
            temp,
            constant(2),
            straddle,
            inside,
            huge,
            huge_inner,
            empty,
        ];
        for block in &blocks {
            for probe in &probes {
                assert_eq!(
                    builder.collect_output_use_sites_outside_block(block.start_address, probe),
                    reference_outside_block_uses(&blocks, block.start_address, probe),
                    "block {:#x} probe {probe:?}",
                    block.start_address
                );
            }
        }
        assert_ne!(
            builder.collect_output_use_sites_outside_block(0x1000, &probes[0]),
            (0, false),
            "fixture must exercise a real consumer"
        );
    }
}
