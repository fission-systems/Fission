use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn no_consumer_suppression_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_NO_CONSUMER_SUPPRESSION"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    pub(super) fn analyze_no_consumer_materialization_profile(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> NoConsumerMaterializationProfile {
        let same_block_consumers =
            Self::collect_output_use_sites_in_block(block, op_idx, output).len();
        let (cross_block_consumers, has_phi_merge_use) =
            Self::collect_output_use_sites_outside_block(
                &self.pcode.blocks,
                block.start_address,
                output,
            );
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
        rhs: &HirExpr,
        legacy_inline_candidate: bool,
        plan: ReplacementValuePlan,
        hazard: Option<AliasUnsafeHazard>,
        profile: NoConsumerMaterializationProfile,
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
        rhs: &HirExpr,
    ) -> NoConsumerSuppressionRhsKind {
        match rhs {
            HirExpr::Var(_) => NoConsumerSuppressionRhsKind::Var,
            HirExpr::Const(..) => NoConsumerSuppressionRhsKind::Const,
            HirExpr::Cast { .. } => NoConsumerSuppressionRhsKind::Cast,
            HirExpr::Unary { .. } => NoConsumerSuppressionRhsKind::Unary,
            HirExpr::Binary { .. } => NoConsumerSuppressionRhsKind::Binary,
            HirExpr::Load { .. } => NoConsumerSuppressionRhsKind::Load,
            HirExpr::Call { .. } => NoConsumerSuppressionRhsKind::Call,
            HirExpr::AggregateCopy { .. } => NoConsumerSuppressionRhsKind::Aggregate,
            HirExpr::PtrOffset { .. } => NoConsumerSuppressionRhsKind::PtrOffset,
            HirExpr::Index { .. } => NoConsumerSuppressionRhsKind::Index,
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
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
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
                if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                    break;
                }
                if candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
                {
                    consumer_count += 1;
                    has_phi_merge_use |= candidate.opcode == PcodeOpcode::MultiEqual;
                }
            }
        }
        (consumer_count, has_phi_merge_use)
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
            &HirExpr::Const(1, int(32)),
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
            &HirExpr::Const(1, int(32)),
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
            &HirExpr::Const(1, int(32)),
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
        );

        assert_eq!(decision, NoConsumerMaterializationDecision::Suppress);
    }

    #[test]
    fn no_consumer_materialization_decision_keeps_preserved_rhs() {
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &varnode(0x10),
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("x".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
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
            &HirExpr::Const(1, int(32)),
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
        );

        assert_eq!(
            decision,
            NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::StateVisibleOutput
            )
        );
    }
}
