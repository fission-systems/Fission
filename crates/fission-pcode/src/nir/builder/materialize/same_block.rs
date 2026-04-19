use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn should_preserve_materialized_expr(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(..) => false,
            HirExpr::Cast { expr, .. } => Self::should_preserve_materialized_expr(expr),
            HirExpr::Unary { .. }
            | HirExpr::Binary { .. }
            | HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => true,
        }
    }

    pub(super) fn expr_is_side_effectful_for_materialization_trace(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(lhs)
                    || Self::expr_is_side_effectful_for_materialization_trace(rhs)
            }
            HirExpr::Load { ptr, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(ptr)
            }
            HirExpr::PtrOffset { base, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(base)
            }
            HirExpr::Index { base, index, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(base)
                    || Self::expr_is_side_effectful_for_materialization_trace(index)
            }
            HirExpr::AggregateCopy { src, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(src)
            }
            HirExpr::Var(_) | HirExpr::Const(..) => false,
        }
    }

    pub(super) fn classify_terminator_sensitive_output_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<ReplacementReadClass> {
        let Some(terminator_index) = terminator_index else {
            return None;
        };
        let use_sites = self.output_use_sites_in_block(block, op_idx, output);
        if use_sites.len() != 1 || use_sites[0].0 != terminator_index {
            return None;
        }
        let terminator = &block.ops[terminator_index];
        Some(match terminator.opcode {
            PcodeOpcode::CBranch => ReplacementReadClass::PredicateSensitive,
            PcodeOpcode::BranchInd => ReplacementReadClass::SelectorSensitive,
            PcodeOpcode::Return => ReplacementReadClass::ReturnPath,
            _ => ReplacementReadClass::SameBlockData,
        })
    }

    pub(super) fn replacement_read_requires_stable_representative(
        read_class: ReplacementReadClass,
        rhs: &HirExpr,
    ) -> bool {
        matches!(
            read_class,
            ReplacementReadClass::PredicateSensitive
                | ReplacementReadClass::SelectorSensitive
                | ReplacementReadClass::ReturnPath
        ) && (Self::should_preserve_materialized_expr(rhs)
            || !Self::expr_is_low_cost_builder_inline_candidate(rhs))
    }

    pub(super) fn same_block_replacement_requires_stable_representative(rhs: &HirExpr) -> bool {
        Self::should_preserve_materialized_expr(rhs)
    }

    pub(super) fn output_has_nonlocal_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let block_idx = self
            .address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(usize::MAX);
        for (candidate_block_idx, candidate_block) in self.pcode.blocks.iter().enumerate() {
            if candidate_block_idx == block_idx {
                continue;
            }
            for candidate in &candidate_block.ops {
                if candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
                {
                    return true;
                }
                if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                    break;
                }
            }
        }
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| VarnodeKey::from(input) == key)
            {
                return false;
            }
        }
        false
    }

    pub(super) fn classify_alias_unsafe_hazard(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> AliasUnsafeHazard {
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        if let Some(hazard) = Self::first_intervening_alias_unsafe_hazard(block, op_idx, &uses) {
            return hazard;
        }
        if uses.len() > 1 {
            let second_use = uses.get(1).or_else(|| uses.first());
            return AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::MultipleSameBlockConsumers,
                second_use.map(|(idx, _)| *idx),
                second_use.map(|(idx, _)| *idx),
                second_use.map(|(_, op)| op.opcode),
            );
        }
        if let Some((use_idx, use_op)) = uses.first().copied() {
            let passthrough_required = Self::expr_requires_passthrough_single_use_inline(rhs);
            let consumer_allows_inline = if passthrough_required {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(use_op.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(use_op.opcode)
            };
            if !Self::expr_is_low_cost_builder_inline_candidate(rhs) || !consumer_allows_inline {
                return AliasUnsafeHazard::new(
                    if terminator_index.is_some_and(|term_idx| use_idx > term_idx) {
                        AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
                    } else if consumer_allows_inline {
                        AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
                    } else {
                        AliasUnsafeHazardKind::DisallowedSingleConsumer
                    },
                    Some(use_idx),
                    Some(use_idx),
                    Some(use_op.opcode),
                );
            }
        }
        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block(block, op_idx, output)
        {
            return AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownMalformedDefUseWindow,
                None,
                Some(redef_idx),
                Some(redef_op.opcode),
            );
        }
        AliasUnsafeHazard::new(
            AliasUnsafeHazardKind::UnknownNoConsumerFound,
            None,
            None,
            None,
        )
    }

    fn materialize_expr_contains_load(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Load { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::materialize_expr_contains_load(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::materialize_expr_contains_load(lhs)
                    || Self::materialize_expr_contains_load(rhs)
            }
            HirExpr::PtrOffset { base, .. } => Self::materialize_expr_contains_load(base),
            HirExpr::Index { base, index, .. } => {
                Self::materialize_expr_contains_load(base)
                    || Self::materialize_expr_contains_load(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::materialize_expr_contains_load(src),
            HirExpr::Call { .. } | HirExpr::Var(_) | HirExpr::Const(_, _) => false,
        }
    }

    fn materialize_expr_contains_call(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::materialize_expr_contains_call(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::materialize_expr_contains_call(lhs)
                    || Self::materialize_expr_contains_call(rhs)
            }
            HirExpr::Load { ptr, .. } => Self::materialize_expr_contains_call(ptr),
            HirExpr::PtrOffset { base, .. } => Self::materialize_expr_contains_call(base),
            HirExpr::Index { base, index, .. } => {
                Self::materialize_expr_contains_call(base)
                    || Self::materialize_expr_contains_call(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::materialize_expr_contains_call(src),
            HirExpr::Var(_) | HirExpr::Const(_, _) => false,
        }
    }

    fn classify_disallowed_single_consumer_rhs_kind(
        rhs: &HirExpr,
    ) -> DisallowedSingleConsumerRhsKind {
        match rhs {
            HirExpr::Var(_) | HirExpr::Const(_, _) => DisallowedSingleConsumerRhsKind::VarOrConst,
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => DisallowedSingleConsumerRhsKind::UnaryBoolean,
            HirExpr::Binary { op, .. }
                if matches!(
                    op,
                    HirBinaryOp::LogicalAnd
                        | HirBinaryOp::LogicalOr
                        | HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                ) =>
            {
                DisallowedSingleConsumerRhsKind::BinaryBoolean
            }
            HirExpr::Binary { .. } => DisallowedSingleConsumerRhsKind::Arithmetic,
            HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => DisallowedSingleConsumerRhsKind::LoadLike,
            HirExpr::Call { .. } => DisallowedSingleConsumerRhsKind::CallLike,
            HirExpr::Cast { expr, .. } => Self::classify_disallowed_single_consumer_rhs_kind(expr),
            HirExpr::Unary { .. } => DisallowedSingleConsumerRhsKind::Other,
        }
    }

    fn classify_disallowed_single_consumer_kind(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> DisallowedSingleConsumerConsumerKind {
        match use_op.opcode {
            PcodeOpcode::CBranch | PcodeOpcode::BranchInd => {
                DisallowedSingleConsumerConsumerKind::BranchCondition
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                if matched_inputs.iter().any(|idx| *idx >= 1) {
                    DisallowedSingleConsumerConsumerKind::CallArg
                } else {
                    DisallowedSingleConsumerConsumerKind::UnknownConsumerKind
                }
            }
            PcodeOpcode::Store => {
                if matched_inputs.contains(&1) {
                    DisallowedSingleConsumerConsumerKind::StoreAddr
                } else if matched_inputs.contains(&2) {
                    DisallowedSingleConsumerConsumerKind::StoreValue
                } else {
                    DisallowedSingleConsumerConsumerKind::UnknownConsumerKind
                }
            }
            PcodeOpcode::Load => DisallowedSingleConsumerConsumerKind::LoadAddr,
            PcodeOpcode::MultiEqual => DisallowedSingleConsumerConsumerKind::PhiMerge,
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => DisallowedSingleConsumerConsumerKind::Predicate,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub => DisallowedSingleConsumerConsumerKind::OtherData,
            _ => DisallowedSingleConsumerConsumerKind::UnknownConsumerKind,
        }
    }

    fn classify_unknown_consumer_kind_reason(
        opcode: PcodeOpcode,
        matched_inputs: &[usize],
    ) -> UnknownConsumerKindReason {
        if matched_inputs.len() > 1 {
            return UnknownConsumerKindReason::ConsumerHasMultipleMatchedInputs;
        }
        match opcode {
            PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                UnknownConsumerKindReason::ConsumerIsIndirectUse
            }
            PcodeOpcode::Branch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::CBranch
            | PcodeOpcode::Return => UnknownConsumerKindReason::ConsumerIsControlLike,
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => {
                UnknownConsumerKindReason::ConsumerIsAddressComputation
            }
            PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => UnknownConsumerKindReason::ConsumerIsSubpieceOrCast,
            PcodeOpcode::Call | PcodeOpcode::Store => {
                UnknownConsumerKindReason::ConsumerInputRoleUnknown
            }
            PcodeOpcode::Copy
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::Load
            | PcodeOpcode::MultiEqual => UnknownConsumerKindReason::ConsumerOpcodeUnhandled,
            _ => UnknownConsumerKindReason::Unknown,
        }
    }

    fn classify_popcount_result_use_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> PopCountResultUseFamily {
        match use_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return PopCountResultUseFamily::UnknownPopCountUse;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                let other_input = if lhs_matches && !rhs_matches {
                    use_op.inputs.get(1)
                } else if rhs_matches && !lhs_matches {
                    use_op.inputs.first()
                } else {
                    None
                };
                match other_input {
                    Some(input) if input.is_constant && input.constant_val == 0 => {
                        PopCountResultUseFamily::PopCountFeedsCompareZero
                    }
                    Some(input) if input.is_constant => {
                        PopCountResultUseFamily::PopCountFeedsCompareConst
                    }
                    Some(_) => PopCountResultUseFamily::PopCountFeedsPredicate,
                    None => PopCountResultUseFamily::UnknownPopCountUse,
                }
            }
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => PopCountResultUseFamily::PopCountFeedsPredicate,
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Store
            | PcodeOpcode::Load => PopCountResultUseFamily::PopCountFeedsStoreOrCall,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::PopCount => PopCountResultUseFamily::PopCountFeedsArithmetic,
            _ => PopCountResultUseFamily::UnknownPopCountUse,
        }
    }

    fn classify_popcount_intand_mask_kind(mask: Option<u64>) -> PopCountIntAndMaskKind {
        match mask {
            Some(1) => PopCountIntAndMaskKind::AndOne,
            Some(0xff) => PopCountIntAndMaskKind::AndByteMask,
            Some(value) if value > 1 && (value + 1).is_power_of_two() => {
                PopCountIntAndMaskKind::AndPowerOfTwoMinusOne
            }
            Some(_) => PopCountIntAndMaskKind::AndNonPowerOfTwoMask,
            None => PopCountIntAndMaskKind::UnknownMask,
        }
    }

    fn classify_popcount_intand_downstream_use_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> PopCountIntAndDownstreamUseFamily {
        match use_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return PopCountIntAndDownstreamUseFamily::FeedsUnknown;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                let other_input = if lhs_matches && !rhs_matches {
                    use_op.inputs.get(1)
                } else if rhs_matches && !lhs_matches {
                    use_op.inputs.first()
                } else {
                    None
                };
                match other_input {
                    Some(input) if input.is_constant && input.constant_val == 0 => {
                        PopCountIntAndDownstreamUseFamily::FeedsCompareZero
                    }
                    Some(input) if input.is_constant => {
                        PopCountIntAndDownstreamUseFamily::FeedsCompareConst
                    }
                    Some(_) => PopCountIntAndDownstreamUseFamily::FeedsPredicate,
                    None => PopCountIntAndDownstreamUseFamily::FeedsUnknown,
                }
            }
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => PopCountIntAndDownstreamUseFamily::FeedsPredicate,
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Store
            | PcodeOpcode::Load => PopCountIntAndDownstreamUseFamily::FeedsStoreOrCall,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::PopCount
            | PcodeOpcode::MultiEqual => PopCountIntAndDownstreamUseFamily::FeedsArithmetic,
            _ => PopCountIntAndDownstreamUseFamily::FeedsUnknown,
        }
    }

    pub(super) fn parity_chain_materialization_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    fn classify_single_consumer_predicate_family(expr: &HirExpr) -> SingleConsumerPredicateFamily {
        match expr {
            HirExpr::Var(_) => SingleConsumerPredicateFamily::DirectFlag,
            HirExpr::Cast { expr, .. } => Self::classify_single_consumer_predicate_family(expr),
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => SingleConsumerPredicateFamily::NegatedFlag,
            HirExpr::Unary { .. } => SingleConsumerPredicateFamily::UnknownPredicate,
            HirExpr::Binary { op, lhs, rhs, .. } => match op {
                HirBinaryOp::Eq => {
                    if matches!(&**lhs, HirExpr::Const(0, _))
                        || matches!(&**rhs, HirExpr::Const(0, _))
                    {
                        SingleConsumerPredicateFamily::CompareZero
                    } else if matches!(&**lhs, HirExpr::Const(_, _))
                        || matches!(&**rhs, HirExpr::Const(_, _))
                    {
                        SingleConsumerPredicateFamily::CompareConst
                    } else {
                        SingleConsumerPredicateFamily::CompareOtherVar
                    }
                }
                HirBinaryOp::Ne => {
                    if matches!(&**lhs, HirExpr::Const(0, _))
                        || matches!(&**rhs, HirExpr::Const(0, _))
                    {
                        SingleConsumerPredicateFamily::CompareNonZero
                    } else if matches!(&**lhs, HirExpr::Const(_, _))
                        || matches!(&**rhs, HirExpr::Const(_, _))
                    {
                        SingleConsumerPredicateFamily::CompareConst
                    } else {
                        SingleConsumerPredicateFamily::CompareOtherVar
                    }
                }
                HirBinaryOp::LogicalAnd
                | HirBinaryOp::LogicalOr
                | HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe => SingleConsumerPredicateFamily::ComposedPredicate,
                _ => SingleConsumerPredicateFamily::UnknownPredicate,
            },
            HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. }
            | HirExpr::Const(_, _) => SingleConsumerPredicateFamily::UnknownPredicate,
        }
    }

    fn classify_single_consumer_guard_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> SingleConsumerPredicateFamily {
        match use_op.opcode {
            PcodeOpcode::BoolNegate if matched_inputs == [0] => {
                SingleConsumerPredicateFamily::NegatedFlag
            }
            PcodeOpcode::IntEqual => {
                if use_op.inputs.len() != 2 {
                    return SingleConsumerPredicateFamily::UnknownPredicate;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                if lhs_matches && use_op.inputs[1].is_constant && use_op.inputs[1].constant_val == 0
                    || rhs_matches
                        && use_op.inputs[0].is_constant
                        && use_op.inputs[0].constant_val == 0
                {
                    SingleConsumerPredicateFamily::CompareZero
                } else if lhs_matches && use_op.inputs[1].is_constant
                    || rhs_matches && use_op.inputs[0].is_constant
                {
                    SingleConsumerPredicateFamily::CompareConst
                } else if lhs_matches || rhs_matches {
                    SingleConsumerPredicateFamily::CompareOtherVar
                } else {
                    SingleConsumerPredicateFamily::UnknownPredicate
                }
            }
            PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return SingleConsumerPredicateFamily::UnknownPredicate;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                if lhs_matches && use_op.inputs[1].is_constant && use_op.inputs[1].constant_val == 0
                    || rhs_matches
                        && use_op.inputs[0].is_constant
                        && use_op.inputs[0].constant_val == 0
                {
                    SingleConsumerPredicateFamily::CompareNonZero
                } else if lhs_matches && use_op.inputs[1].is_constant
                    || rhs_matches && use_op.inputs[0].is_constant
                {
                    SingleConsumerPredicateFamily::CompareConst
                } else if lhs_matches || rhs_matches {
                    SingleConsumerPredicateFamily::CompareOtherVar
                } else {
                    SingleConsumerPredicateFamily::UnknownPredicate
                }
            }
            PcodeOpcode::BoolAnd | PcodeOpcode::BoolOr | PcodeOpcode::BoolXor => matched_inputs
                .iter()
                .any(|idx| *idx < use_op.inputs.len())
                .then_some(SingleConsumerPredicateFamily::ComposedPredicate)
                .unwrap_or(SingleConsumerPredicateFamily::UnknownPredicate),
            PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => matched_inputs
                .iter()
                .any(|idx| *idx < use_op.inputs.len())
                .then_some(SingleConsumerPredicateFamily::ComposedPredicate)
                .unwrap_or(SingleConsumerPredicateFamily::UnknownPredicate),
            _ => SingleConsumerPredicateFamily::UnknownPredicate,
        }
    }

    fn predicate_families_match(
        predicate_family: SingleConsumerPredicateFamily,
        guard_family: SingleConsumerPredicateFamily,
    ) -> bool {
        predicate_family == guard_family
    }

    fn classify_arithmetic_predicate_shape(
        expr: &HirExpr,
    ) -> (ArithmeticPredicateShape, Option<u64>) {
        match expr {
            HirExpr::Cast { expr, .. } => Self::classify_arithmetic_predicate_shape(expr),
            HirExpr::Binary { op, lhs, rhs, .. } if matches!(op, HirBinaryOp::And) => {
                let (value_expr, mask_value) = match (&**lhs, &**rhs) {
                    (HirExpr::Const(value, _), other) if *value >= 0 => {
                        (other, Some(*value as u64))
                    }
                    (other, HirExpr::Const(value, _)) if *value >= 0 => {
                        (other, Some(*value as u64))
                    }
                    _ => return (ArithmeticPredicateShape::UnknownArithmetic, None),
                };
                let Some(mask_value) = mask_value else {
                    return (ArithmeticPredicateShape::UnknownArithmetic, None);
                };
                let shape = if matches!(
                    value_expr,
                    HirExpr::Binary {
                        op: HirBinaryOp::Shr | HirBinaryOp::Sar | HirBinaryOp::Shl,
                        ..
                    }
                ) {
                    ArithmeticPredicateShape::ShiftAndMask
                } else if mask_value == 1 {
                    ArithmeticPredicateShape::LowBitAndOne
                } else if mask_value.is_power_of_two() {
                    ArithmeticPredicateShape::PowerOfTwoMask
                } else {
                    ArithmeticPredicateShape::NonPowerOfTwoMask
                };
                (shape, Some(mask_value))
            }
            _ => (ArithmeticPredicateShape::UnknownArithmetic, None),
        }
    }

    fn classify_arithmetic_predicate_stable_reason(
        proof: &SingleConsumerPredicateProof,
        shape: ArithmeticPredicateShape,
    ) -> Option<ArithmeticPredicateStableReason> {
        if !proof.requires_stable_representative {
            return None;
        }
        if shape != ArithmeticPredicateShape::UnknownArithmetic {
            return Some(ArithmeticPredicateStableReason::ArithmeticMask);
        }
        if !proof.same_guard_as_consumer {
            return Some(ArithmeticPredicateStableReason::NonCanonicalPredicate);
        }
        if matches!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
                | SingleConsumerPredicateFamily::CompareNonZero
                | SingleConsumerPredicateFamily::CompareConst
                | SingleConsumerPredicateFamily::CompareOtherVar
        ) {
            return Some(ArithmeticPredicateStableReason::ConsumerCompare);
        }
        Some(ArithmeticPredicateStableReason::PredicateSensitive)
    }

    fn low_bit_mask_input_expr<'b>(expr: &'b HirExpr) -> Option<&'b HirExpr> {
        match expr {
            HirExpr::Cast { expr, .. } => Self::low_bit_mask_input_expr(expr),
            HirExpr::Binary { op, lhs, rhs, .. } if matches!(op, HirBinaryOp::And) => {
                match (&**lhs, &**rhs) {
                    (HirExpr::Const(1, _), other) | (other, HirExpr::Const(1, _)) => Some(other),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn expr_boolean_like(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Const(_, ty) => {
                matches!(ty, NirType::Bool) || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::Cast { ty, expr } => match ty {
                NirType::Bool => true,
                NirType::Int { bits, .. } if *bits == 1 => true,
                _ => Self::expr_boolean_like(expr),
            },
            HirExpr::Unary { op, ty, expr } => match op {
                HirUnaryOp::Not => true,
                _ => {
                    matches!(ty, NirType::Bool)
                        || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
                        || Self::expr_boolean_like(expr)
                }
            },
            HirExpr::Binary { op, ty, .. } => {
                matches!(
                    op,
                    HirBinaryOp::LogicalAnd
                        | HirBinaryOp::LogicalOr
                        | HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                ) || matches!(ty, NirType::Bool)
                    || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::Call { ty, .. } | HirExpr::Load { ty, .. } => {
                matches!(ty, NirType::Bool) || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. }
            | HirExpr::Var(_) => false,
        }
    }

    fn classify_low_bit_mask_input_origin_kind(expr: &HirExpr) -> LowBitMaskInputOriginKind {
        match expr {
            HirExpr::Cast { expr, .. } => Self::classify_low_bit_mask_input_origin_kind(expr),
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => LowBitMaskInputOriginKind::BoolOp,
            HirExpr::Unary { .. } => LowBitMaskInputOriginKind::Unknown,
            HirExpr::Binary { op, .. } => match op {
                HirBinaryOp::Eq
                | HirBinaryOp::Ne
                | HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe => LowBitMaskInputOriginKind::Compare,
                HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr => {
                    LowBitMaskInputOriginKind::BoolOp
                }
                HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Div
                | HirBinaryOp::Mod
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar => LowBitMaskInputOriginKind::Arithmetic,
            },
            HirExpr::Load { .. } | HirExpr::PtrOffset { .. } | HirExpr::Index { .. } => {
                LowBitMaskInputOriginKind::Load
            }
            HirExpr::Call { .. } => LowBitMaskInputOriginKind::Call,
            HirExpr::AggregateCopy { .. } | HirExpr::Var(_) | HirExpr::Const(_, _) => {
                LowBitMaskInputOriginKind::Unknown
            }
        }
    }

    fn classify_low_bit_mask_predicate_family(
        input_origin_kind: LowBitMaskInputOriginKind,
        input_is_boolean_like: bool,
    ) -> LowBitMaskPredicateFamily {
        match (input_origin_kind, input_is_boolean_like) {
            (LowBitMaskInputOriginKind::Compare, _) => {
                LowBitMaskPredicateFamily::MaskFromCompareResult
            }
            (LowBitMaskInputOriginKind::BoolOp, true) => LowBitMaskPredicateFamily::BooleanFlagMask,
            (LowBitMaskInputOriginKind::Arithmetic, _) => {
                LowBitMaskPredicateFamily::MaskFromArithmeticValue
            }
            (LowBitMaskInputOriginKind::Load, false)
            | (LowBitMaskInputOriginKind::Call, false)
            | (LowBitMaskInputOriginKind::Unknown, false) => {
                LowBitMaskPredicateFamily::IntegerBitTest
            }
            _ => LowBitMaskPredicateFamily::UnknownLowBitMask,
        }
    }

    pub(super) fn describe_disallowed_single_consumer_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<DisallowedSingleConsumerProof> {
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (_, use_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = use_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        let consumer_kind = Self::classify_disallowed_single_consumer_kind(use_op, &matched_inputs);
        let rhs_kind = Self::classify_disallowed_single_consumer_rhs_kind(rhs);
        let rhs_has_call = Self::materialize_expr_contains_call(rhs);
        let rhs_has_load = Self::materialize_expr_contains_load(rhs);
        let rhs_low_cost = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let reason = if rhs_has_call {
            DisallowedSingleConsumerReason::RhsHasCall
        } else if rhs_has_load {
            DisallowedSingleConsumerReason::RhsHasLoad
        } else if !rhs_low_cost {
            DisallowedSingleConsumerReason::RhsNotLowCost
        } else {
            match consumer_kind {
                DisallowedSingleConsumerConsumerKind::BranchCondition => {
                    DisallowedSingleConsumerReason::ConsumerIsBranchCondition
                }
                DisallowedSingleConsumerConsumerKind::Predicate => {
                    DisallowedSingleConsumerReason::ConsumerIsPredicate
                }
                DisallowedSingleConsumerConsumerKind::CallArg => {
                    DisallowedSingleConsumerReason::ConsumerIsCallArg
                }
                DisallowedSingleConsumerConsumerKind::StoreAddr => {
                    DisallowedSingleConsumerReason::ConsumerIsStoreAddr
                }
                DisallowedSingleConsumerConsumerKind::StoreValue => {
                    DisallowedSingleConsumerReason::ConsumerIsStoreValue
                }
                DisallowedSingleConsumerConsumerKind::LoadAddr => {
                    DisallowedSingleConsumerReason::ConsumerIsLoadAddr
                }
                DisallowedSingleConsumerConsumerKind::PhiMerge => {
                    DisallowedSingleConsumerReason::ConsumerIsPhiMerge
                }
                DisallowedSingleConsumerConsumerKind::OtherData
                | DisallowedSingleConsumerConsumerKind::UnknownConsumerKind => {
                    DisallowedSingleConsumerReason::UnknownConsumerKind
                }
            }
        };
        Some(DisallowedSingleConsumerProof {
            consumer_block_addr: block.start_address,
            consumer_op_seq: use_op.seq_num,
            consumer_opcode: use_op.opcode,
            matched_input_indices: matched_inputs,
            consumer_kind,
            rhs_kind,
            rhs_low_cost,
            rhs_has_load,
            rhs_has_call,
            reason,
        })
    }

    pub(super) fn describe_unknown_consumer_kind_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<UnknownConsumerKindProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::UnknownConsumerKind {
            return None;
        }
        Some(UnknownConsumerKindProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            matched_input_indices: base.matched_input_indices.clone(),
            rhs_kind: base.rhs_kind,
            reason: Self::classify_unknown_consumer_kind_reason(
                base.consumer_opcode,
                &base.matched_input_indices,
            ),
        })
    }

    pub(super) fn describe_popcount_consumer_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<PopCountConsumerProof> {
        let base = Self::describe_unknown_consumer_kind_proof(block, op_idx, output, rhs)?;
        if base.consumer_opcode != PcodeOpcode::PopCount {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (consumer_idx, consumer_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let matched_input = *base.matched_input_indices.first()?;
        let input_width = consumer_op
            .inputs
            .get(matched_input)
            .map(|input| input.size)
            .unwrap_or(output.size);
        let output_width = consumer_op.output.as_ref().map(|vn| vn.size);
        let (popcount_result_used_by, downstream_consumer_opcode) =
            if let Some(popcount_output) = consumer_op.output.as_ref() {
                let downstream_uses =
                    Self::collect_output_use_sites_in_block(block, consumer_idx, popcount_output);
                let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                    &self.pcode.blocks,
                    block.start_address,
                    popcount_output,
                );
                if downstream_uses.is_empty() && cross_block_consumers == 0 {
                    (PopCountResultUseFamily::PopCountResultUnused, None)
                } else if downstream_uses.len() == 1 && cross_block_consumers == 0 {
                    let (_, downstream_op) = downstream_uses[0];
                    let output_key = VarnodeKey::from(popcount_output);
                    let matched_inputs = downstream_op
                        .inputs
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, input)| {
                            (VarnodeKey::from(input) == output_key).then_some(idx)
                        })
                        .collect::<Vec<_>>();
                    (
                        Self::classify_popcount_result_use_family(downstream_op, &matched_inputs),
                        Some(downstream_op.opcode),
                    )
                } else {
                    (
                        PopCountResultUseFamily::UnknownPopCountUse,
                        downstream_uses.first().map(|(_, op)| op.opcode),
                    )
                }
            } else {
                (PopCountResultUseFamily::PopCountResultUnused, None)
            };
        Some(PopCountConsumerProof {
            consumer_op_seq: consumer_op.seq_num,
            input_width,
            output_width,
            rhs_kind: base.rhs_kind,
            rhs_low_cost: Self::expr_is_low_cost_builder_inline_candidate(rhs),
            rhs_has_call: Self::materialize_expr_contains_call(rhs),
            rhs_has_load: Self::materialize_expr_contains_load(rhs),
            popcount_result_used_by,
            downstream_consumer_opcode,
        })
    }

    pub(super) fn describe_popcount_intand_chain_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<PopCountIntAndChainProof> {
        let popcount = self.describe_popcount_consumer_proof(block, op_idx, output, rhs)?;
        if popcount.downstream_consumer_opcode != Some(PcodeOpcode::IntAnd) {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (consumer_idx, consumer_op) = *uses.first()?;
        let popcount_output = consumer_op.output.as_ref()?;
        let downstream_uses =
            Self::collect_output_use_sites_in_block(block, consumer_idx, popcount_output);
        let (intand_idx, intand_op) = *downstream_uses.first()?;
        if intand_op.opcode != PcodeOpcode::IntAnd {
            return None;
        }
        let popcount_output_key = VarnodeKey::from(popcount_output);
        let matched_inputs = intand_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| {
                (VarnodeKey::from(input) == popcount_output_key).then_some(idx)
            })
            .collect::<Vec<_>>();
        let intand_mask = if matched_inputs == [0] {
            intand_op
                .inputs
                .get(1)
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else if matched_inputs == [1] {
            intand_op
                .inputs
                .first()
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else {
            None
        };
        let intand_mask_kind = Self::classify_popcount_intand_mask_kind(intand_mask);
        let (intand_result_consumer, downstream_consumer_opcode) = if let Some(intand_output) =
            intand_op.output.as_ref()
        {
            let intand_output_uses =
                Self::collect_output_use_sites_in_block(block, intand_idx, intand_output);
            let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                &self.pcode.blocks,
                block.start_address,
                intand_output,
            );
            if intand_output_uses.len() == 1 && cross_block_consumers == 0 {
                let (_, use_op) = intand_output_uses[0];
                let intand_output_key = VarnodeKey::from(intand_output);
                let matched_inputs = use_op
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, input)| {
                        (VarnodeKey::from(input) == intand_output_key).then_some(idx)
                    })
                    .collect::<Vec<_>>();
                (
                    Self::classify_popcount_intand_downstream_use_family(use_op, &matched_inputs),
                    Some(use_op.opcode),
                )
            } else {
                (
                    PopCountIntAndDownstreamUseFamily::FeedsUnknown,
                    intand_output_uses.first().map(|(_, op)| op.opcode),
                )
            }
        } else {
            (PopCountIntAndDownstreamUseFamily::FeedsUnknown, None)
        };
        Some(PopCountIntAndChainProof {
            popcount_consumer_op_seq: popcount.consumer_op_seq,
            intand_op_seq: intand_op.seq_num,
            popcount_result: format!(
                "space:{} off:0x{:x} size:{}",
                popcount_output.space_id, popcount_output.offset, popcount_output.size
            ),
            intand_mask,
            intand_mask_kind,
            intand_result_consumer,
            downstream_consumer_opcode,
            chain_low_cost: popcount.rhs_low_cost,
            chain_side_effect_free: !popcount.rhs_has_call && !popcount.rhs_has_load,
        })
    }

    fn extract_popcount_input_expr<'b>(rhs: &'b HirExpr) -> Option<&'b HirExpr> {
        let HirExpr::Call { target, args, .. } = rhs else {
            return None;
        };
        (target == "__popcount" && args.len() == 1).then_some(&args[0])
    }

    fn extract_intand_popcount_operand<'b>(rhs: &'b HirExpr) -> Option<(&'b HirExpr, Option<u64>)> {
        match rhs {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                let lhs_popcount = Self::extract_popcount_input_expr(lhs);
                let rhs_popcount = Self::extract_popcount_input_expr(rhs);
                let lhs_const = match rhs.as_ref() {
                    HirExpr::Const(value, _) => Some(*value as u64),
                    _ => None,
                };
                let rhs_const = match lhs.as_ref() {
                    HirExpr::Const(value, _) => Some(*value as u64),
                    _ => None,
                };
                if let Some(popcount_input) = lhs_popcount {
                    Some((popcount_input, lhs_const))
                } else if let Some(popcount_input) = rhs_popcount {
                    Some((popcount_input, rhs_const))
                } else {
                    None
                }
            }
            _ => Self::extract_popcount_input_expr(rhs).map(|expr| (expr, None)),
        }
    }

    fn range_has_parity_chain_side_effect(
        block: &crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        end_idx: usize,
    ) -> bool {
        if end_idx <= start_idx + 1 {
            return false;
        }
        block.ops[start_idx + 1..end_idx].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Store
                    | PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    fn classify_parity_chain_compare<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        producer_idx: usize,
        output: &Varnode,
    ) -> Result<(usize, &'b PcodeOp, u64), ParityChainKeepReason> {
        let uses = Self::collect_output_use_sites_in_block(block, producer_idx, output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            output,
        );
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Err(ParityChainKeepReason::IntAndHasMultipleConsumers);
        }
        let (compare_idx, compare_op) = uses[0];
        if !matches!(
            compare_op.opcode,
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual
        ) {
            return Err(ParityChainKeepReason::FinalConsumerNotCompare);
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = compare_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        if matched_inputs.len() != 1 {
            return Err(ParityChainKeepReason::FinalConsumerNotCompare);
        }
        let compare_const = if matched_inputs[0] == 0 {
            compare_op
                .inputs
                .get(1)
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else {
            compare_op
                .inputs
                .first()
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        };
        let Some(compare_const) = compare_const else {
            return Err(ParityChainKeepReason::CompareConstUnsupported);
        };
        if !matches!(compare_const, 0 | 1) {
            return Err(ParityChainKeepReason::CompareConstUnsupported);
        }
        Ok((compare_idx, compare_op, compare_const))
    }

    pub(super) fn describe_parity_chain_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<Result<ParityChainProof, ParityChainKeepReason>> {
        let current_op = block.ops.get(op_idx)?;
        match current_op.opcode {
            PcodeOpcode::PopCount => {
                let input_expr = Self::extract_popcount_input_expr(rhs)?;
                if !Self::expr_is_low_cost_builder_inline_candidate(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsNotLowCost));
                }
                if Self::materialize_expr_contains_load(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasLoad));
                }
                if Self::materialize_expr_contains_call(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasCall));
                }
                let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
                let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                    &self.pcode.blocks,
                    block.start_address,
                    output,
                );
                if uses.len() != 1 || cross_block_consumers != 0 {
                    return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
                }
                let (intand_idx, intand_op) = uses[0];
                let output_key = VarnodeKey::from(output);
                let matched_inputs = intand_op
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, input)| {
                        (VarnodeKey::from(input) == output_key).then_some(idx)
                    })
                    .collect::<Vec<_>>();
                let intand_mask =
                    if matched_inputs == [0] {
                        intand_op.inputs.get(1).and_then(|input| {
                            input.is_constant.then_some(input.constant_val as u64)
                        })
                    } else if matched_inputs == [1] {
                        intand_op.inputs.first().and_then(|input| {
                            input.is_constant.then_some(input.constant_val as u64)
                        })
                    } else {
                        None
                    };
                if intand_mask != Some(1) {
                    return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
                }
                if Self::range_has_parity_chain_side_effect(block, op_idx, intand_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                let intand_output = intand_op.output.as_ref()?;
                let (compare_idx, compare_op, compare_const) =
                    match self.classify_parity_chain_compare(block, intand_idx, intand_output) {
                        Ok(proof) => proof,
                        Err(reason) => return Some(Err(reason)),
                    };
                if Self::range_has_parity_chain_side_effect(block, intand_idx, compare_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                return Some(Ok(ParityChainProof {
                    role: ParityChainRole::PopCountResult,
                    popcount_op_seq: current_op.seq_num,
                    intand_op_seq: block.ops.get(intand_idx)?.seq_num,
                    compare_op_seq: compare_op.seq_num,
                    compare_opcode: compare_op.opcode,
                    compare_const,
                    chain_low_cost: true,
                    chain_side_effect_free: true,
                }));
            }
            PcodeOpcode::IntAnd => {
                let Some((input_expr, mask_value)) = Self::extract_intand_popcount_operand(rhs)
                else {
                    return None;
                };
                if mask_value != Some(1) {
                    return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
                }
                if !Self::expr_is_low_cost_builder_inline_candidate(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsNotLowCost));
                }
                if Self::materialize_expr_contains_load(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasLoad));
                }
                if Self::materialize_expr_contains_call(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasCall));
                }
                let (compare_idx, compare_op, compare_const) =
                    match self.classify_parity_chain_compare(block, op_idx, output) {
                        Ok(proof) => proof,
                        Err(reason) => return Some(Err(reason)),
                    };
                if Self::range_has_parity_chain_side_effect(block, op_idx, compare_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                let intand_op = block.ops.get(op_idx)?;
                let popcount_input = if intand_op
                    .inputs
                    .first()
                    .is_some_and(|input| input.is_constant)
                {
                    intand_op.inputs.get(1)?
                } else {
                    intand_op.inputs.first()?
                };
                let popcount_output_key = VarnodeKey::from(popcount_input);
                let popcount_idx = block
                    .ops
                    .iter()
                    .enumerate()
                    .take(op_idx)
                    .rev()
                    .find(|(_, candidate)| {
                        candidate.output.as_ref().map(VarnodeKey::from)
                            == Some(popcount_output_key.clone())
                    })
                    .map(|(idx, _)| idx)?;
                return Some(Ok(ParityChainProof {
                    role: ParityChainRole::IntAndResult,
                    popcount_op_seq: block.ops.get(popcount_idx)?.seq_num,
                    intand_op_seq: current_op.seq_num,
                    compare_op_seq: compare_op.seq_num,
                    compare_opcode: compare_op.opcode,
                    compare_const,
                    chain_low_cost: true,
                    chain_side_effect_free: true,
                }));
            }
            _ => {}
        }

        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            output,
        );
        let Some((popcount_idx, popcount_op)) = uses.first().copied() else {
            return None;
        };
        if popcount_op.opcode != PcodeOpcode::PopCount {
            return None;
        }
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
        }
        if !Self::expr_is_low_cost_builder_inline_candidate(rhs) {
            return Some(Err(ParityChainKeepReason::RhsNotLowCost));
        }
        if Self::materialize_expr_contains_load(rhs) {
            return Some(Err(ParityChainKeepReason::RhsHasLoad));
        }
        if Self::materialize_expr_contains_call(rhs) {
            return Some(Err(ParityChainKeepReason::RhsHasCall));
        }
        if Self::range_has_parity_chain_side_effect(block, op_idx, popcount_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        let chain = self.describe_popcount_intand_chain_proof(block, op_idx, output, rhs)?;
        if chain.intand_mask_kind != PopCountIntAndMaskKind::AndOne {
            return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
        }
        let popcount_output = popcount_op.output.as_ref()?;
        let uses = Self::collect_output_use_sites_in_block(block, popcount_idx, popcount_output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            popcount_output,
        );
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
        }
        let (intand_idx, _) = uses[0];
        if Self::range_has_parity_chain_side_effect(block, popcount_idx, intand_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        let intand_output = block.ops.get(intand_idx)?.output.as_ref()?;
        let (compare_idx, compare_op, compare_const) =
            match self.classify_parity_chain_compare(block, intand_idx, intand_output) {
                Ok(proof) => proof,
                Err(reason) => return Some(Err(reason)),
            };
        if Self::range_has_parity_chain_side_effect(block, intand_idx, compare_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        Some(Ok(ParityChainProof {
            role: ParityChainRole::PopCountInput,
            popcount_op_seq: popcount_op.seq_num,
            intand_op_seq: block.ops.get(intand_idx)?.seq_num,
            compare_op_seq: compare_op.seq_num,
            compare_opcode: compare_op.opcode,
            compare_const,
            chain_low_cost: true,
            chain_side_effect_free: true,
        }))
    }

    pub(super) fn describe_parity_chain_final_hir_expr(
        rhs: &HirExpr,
        proof: &ParityChainProof,
    ) -> Option<String> {
        let input_expr = match proof.role {
            ParityChainRole::PopCountInput => rhs,
            ParityChainRole::PopCountResult => Self::extract_popcount_input_expr(rhs)?,
            ParityChainRole::IntAndResult => Self::extract_intand_popcount_operand(rhs)?.0,
        };
        let compare_op = match proof.compare_opcode {
            PcodeOpcode::IntEqual => "==",
            PcodeOpcode::IntNotEqual => "!=",
            _ => return None,
        };
        Some(format!(
            "((__popcount({input_expr:?}) & 1) {compare_op} {})",
            proof.compare_const
        ))
    }

    pub(super) fn describe_single_consumer_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<SingleConsumerPredicateProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::ConsumerIsPredicate {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (_, use_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = use_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        let predicate_family = Self::classify_single_consumer_predicate_family(rhs);
        let guard_family = Self::classify_single_consumer_guard_family(use_op, &matched_inputs);
        let low_cost_if_predicate = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let requires_stable_representative = Self::replacement_read_requires_stable_representative(
            ReplacementReadClass::PredicateSensitive,
            rhs,
        );
        Some(SingleConsumerPredicateProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            rhs_kind: base.rhs_kind,
            predicate_family,
            guard_family,
            same_guard_as_consumer: Self::predicate_families_match(predicate_family, guard_family),
            requires_stable_representative,
            low_cost_if_predicate,
            has_call: base.rhs_has_call,
            has_load: base.rhs_has_load,
        })
    }

    pub(super) fn describe_arithmetic_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<ArithmeticPredicateProof> {
        let predicate = Self::describe_single_consumer_predicate_proof(block, op_idx, output, rhs)?;
        if predicate.predicate_family != SingleConsumerPredicateFamily::UnknownPredicate {
            return None;
        }
        let (mask_kind, mask_value) = Self::classify_arithmetic_predicate_shape(rhs);
        let boolean_width = mask_value == Some(1) || output.size == 1;
        let stable_required_reason =
            Self::classify_arithmetic_predicate_stable_reason(&predicate, mask_kind);
        Some(ArithmeticPredicateProof {
            consumer_guard: predicate.guard_family,
            mask_kind,
            mask_value,
            boolean_width,
            low_cost: predicate.low_cost_if_predicate,
            stable_required: predicate.requires_stable_representative,
            stable_required_reason,
        })
    }

    pub(super) fn describe_low_bit_mask_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<LowBitMaskPredicateProof> {
        let arithmetic = Self::describe_arithmetic_predicate_proof(block, op_idx, output, rhs)?;
        if arithmetic.mask_kind != ArithmeticPredicateShape::LowBitAndOne {
            return None;
        }
        let mask_input = Self::low_bit_mask_input_expr(rhs)?;
        let input_origin_kind = Self::classify_low_bit_mask_input_origin_kind(mask_input);
        let input_is_boolean_like = Self::expr_boolean_like(mask_input);
        let family =
            Self::classify_low_bit_mask_predicate_family(input_origin_kind, input_is_boolean_like);
        Some(LowBitMaskPredicateProof {
            family,
            mask_input: format!("{mask_input:?}"),
            consumer_guard: arithmetic.consumer_guard,
            feeds_only_predicate: true,
            input_is_boolean_like,
            input_origin_kind,
            stable_required_reason: arithmetic.stable_required_reason,
        })
    }

    pub(super) fn first_intervening_alias_unsafe_hazard(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        uses: &[(usize, &PcodeOp)],
    ) -> Option<AliasUnsafeHazard> {
        let first_use_idx = uses.first().map(|(idx, _)| *idx)?;
        let mut first_store: Option<(usize, PcodeOpcode)> = None;
        for (candidate_idx, candidate) in block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .take(first_use_idx.saturating_sub(op_idx + 1))
        {
            match candidate.opcode {
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                    return Some(AliasUnsafeHazard::new(
                        AliasUnsafeHazardKind::CallBetweenDefUse,
                        Some(first_use_idx),
                        Some(candidate_idx),
                        Some(candidate.opcode),
                    ));
                }
                PcodeOpcode::Load if first_store.is_some() => {
                    return Some(AliasUnsafeHazard::new(
                        AliasUnsafeHazardKind::LoadAfterStore,
                        Some(first_use_idx),
                        Some(candidate_idx),
                        Some(candidate.opcode),
                    ));
                }
                PcodeOpcode::Store => {
                    first_store.get_or_insert((candidate_idx, candidate.opcode));
                }
                _ => {}
            }
        }
        first_store.map(|(store_idx, store_opcode)| {
            AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::SameBlockStore,
                Some(first_use_idx),
                Some(store_idx),
                Some(store_opcode),
            )
        })
    }

    pub(super) fn classify_same_block_overwrite_rhs_kind(
        opcode: PcodeOpcode,
    ) -> SameBlockOverwriteRhsKind {
        match opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => SameBlockOverwriteRhsKind::CopyLike,
            PcodeOpcode::Load => SameBlockOverwriteRhsKind::Load,
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                SameBlockOverwriteRhsKind::Call
            }
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor => SameBlockOverwriteRhsKind::Predicate,
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => SameBlockOverwriteRhsKind::Arithmetic,
            _ => SameBlockOverwriteRhsKind::Unknown,
        }
    }

    pub(super) fn classify_same_block_overwrite_shape(
        consumer_relation: CrossBlockConsumerRelation,
        redef_idx: usize,
        redef_opcode: PcodeOpcode,
        terminator_idx: Option<usize>,
    ) -> SameBlockOverwriteShapeKind {
        if consumer_relation == CrossBlockConsumerRelation::LoopBackedge {
            return SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate;
        }
        match redef_opcode {
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                return SameBlockOverwriteShapeKind::OverwriteAtCallResult;
            }
            PcodeOpcode::Load => return SameBlockOverwriteShapeKind::OverwriteAtLoadResult,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => return SameBlockOverwriteShapeKind::OverwriteAtCopy,
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor => {
                return SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer;
            }
            _ => {}
        }
        if terminator_idx.is_some_and(|term_idx| redef_idx < term_idx) {
            SameBlockOverwriteShapeKind::OverwriteBeforeBranch
        } else {
            SameBlockOverwriteShapeKind::OverwriteUnknown
        }
    }

    pub(super) fn expr_is_low_cost_builder_inline_candidate(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(_, _) => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(expr)
            }
            HirExpr::Load { ptr, .. }
            | HirExpr::PtrOffset { base: ptr, .. }
            | HirExpr::AggregateCopy { src: ptr, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(ptr)
            }
            HirExpr::Index { base, index, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(base)
                    && Self::expr_is_low_cost_builder_inline_candidate(index)
            }
            HirExpr::Binary { op, lhs, rhs, .. } => {
                matches!(
                    op,
                    HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                        | HirBinaryOp::And
                        | HirBinaryOp::Or
                        | HirBinaryOp::Xor
                        | HirBinaryOp::Add
                        | HirBinaryOp::Sub
                        | HirBinaryOp::Shl
                        | HirBinaryOp::Shr
                        | HirBinaryOp::Sar
                        | HirBinaryOp::Mul
                ) && Self::expr_is_low_cost_builder_inline_candidate(lhs)
                    && Self::expr_is_low_cost_builder_inline_candidate(rhs)
            }
            HirExpr::Call { .. } => false,
        }
    }

    pub(super) fn use_opcode_allows_single_use_builder_inline(opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Load
                | PcodeOpcode::Store
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
                | PcodeOpcode::IntMult
                | PcodeOpcode::Piece
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Cast
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
        )
    }

    pub(super) fn use_opcode_allows_passthrough_single_use_builder_inline(
        opcode: PcodeOpcode,
    ) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::Piece
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Cast
        )
    }

    pub(super) fn expr_requires_passthrough_single_use_inline(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(_, _) => false,
            HirExpr::Cast { expr, .. } => Self::expr_requires_passthrough_single_use_inline(expr),
            HirExpr::Unary { op, expr, .. } => {
                matches!(op, HirUnaryOp::Not)
                    || Self::expr_requires_passthrough_single_use_inline(expr)
            }
            HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => true,
            HirExpr::Binary { op, .. } => matches!(
                op,
                HirBinaryOp::LogicalAnd
                    | HirBinaryOp::LogicalOr
                    | HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
            ),
            HirExpr::Call { .. } => true,
        }
    }

    pub(super) fn output_used_only_by_block_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let mut use_sites = block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter(|(_, candidate)| {
                candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
            })
            .map(|(idx, _)| idx);

        let Some(first_use) = use_sites.next() else {
            return false;
        };
        if use_sites.next().is_some() {
            return false;
        }
        Some(first_use) == terminator_index
    }

    pub(super) fn output_use_sites_in_block<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        Self::collect_output_use_sites_in_block(block, op_idx, output)
    }

    pub(super) fn output_used_only_by_single_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1 && uses[0].1.opcode == PcodeOpcode::Store
    }

    pub(super) fn output_used_only_by_passthrough_chain(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        !uses.is_empty()
            && uses.iter().all(|(_, op)| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::SubPiece
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    #[test]
    fn low_cost_builder_inline_accepts_single_use_load_chain() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::PtrOffset {
                base: Box::new(HirExpr::Var("param_1".to_string())),
                offset: 0x20,
            }),
            ty: int(64),
        };

        assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
            &expr
        ));
    }

    #[test]
    fn low_cost_builder_inline_rejects_calls() {
        let expr = HirExpr::Call {
            target: "helper".to_string(),
            args: vec![HirExpr::Var("param_1".to_string())],
            ty: int(32),
        };

        assert!(!PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
            &expr
        ));
    }

    #[test]
    fn single_use_builder_inline_blocks_call_like_consumers() {
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::Call));
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallInd));
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallOther)
        );
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CBranch));
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::BranchInd)
        );
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::IntEqual)
        );
    }

    #[test]
    fn single_use_builder_inline_keeps_dataflow_consumers() {
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::Copy
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::Load
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::PtrAdd
        ));
    }

    #[test]
    fn memory_backed_single_use_inline_requires_passthrough_consumer() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: int(64),
        };

        assert!(PreviewBuilder::expr_requires_passthrough_single_use_inline(
            &expr
        ));
        assert!(
            PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::Copy
            )
        );
        assert!(
            !PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::IntAdd
            )
        );
    }

    #[test]
    fn plain_leaf_single_use_inline_can_flow_into_data_consumer() {
        let expr = HirExpr::Var("tmp_1".to_string());
        assert!(!PreviewBuilder::expr_requires_passthrough_single_use_inline(&expr));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
    }

    #[test]
    fn arithmetic_single_use_inline_can_flow_into_data_consumer() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        assert!(!PreviewBuilder::expr_requires_passthrough_single_use_inline(&expr));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
    }

    #[test]
    fn predicate_single_use_inline_requires_passthrough_consumer() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: NirType::Bool,
        };

        assert!(PreviewBuilder::expr_requires_passthrough_single_use_inline(
            &expr
        ));
        assert!(
            !PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::IntAdd
            )
        );
    }

    #[test]
    fn predicate_sensitive_reads_require_stable_representative_for_nontrivial_rhs() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: int(64),
        };
        assert!(
            PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::PredicateSensitive,
                &expr
            )
        );
        assert!(
            PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::SelectorSensitive,
                &expr
            )
        );
    }

    #[test]
    fn predicate_sensitive_reads_allow_direct_leaf_replacement() {
        let expr = HirExpr::Var("tmp_1".to_string());
        assert!(
            !PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::PredicateSensitive,
                &expr
            )
        );
        assert!(
            !PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::ReturnPath,
                &expr
            )
        );
    }

    #[test]
    fn same_block_replacement_keeps_nonleaf_representatives() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        assert!(PreviewBuilder::same_block_replacement_requires_stable_representative(&expr));
        assert!(
            !PreviewBuilder::same_block_replacement_requires_stable_representative(&HirExpr::Var(
                "tmp_1".to_string()
            ))
        );
    }

    #[test]
    fn alias_unsafe_hazard_prefers_call_between_def_and_use() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::CallBetweenDefUse);
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Call));
    }

    #[test]
    fn alias_unsafe_hazard_detects_load_after_store_chain() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Store,
                None,
                vec![varnode(0x30), varnode(0x31), constant(0)],
            ),
            op(
                2,
                PcodeOpcode::Load,
                Some(varnode(0x40)),
                vec![varnode(0x30)],
            ),
            op(
                3,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::LoadAfterStore);
        assert_eq!(hazard.use_stmt_idx, Some(3));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Load));
    }

    #[test]
    fn alias_unsafe_hazard_falls_back_to_multiple_consumers() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(varnode(0x30)),
                vec![output.clone(), constant(1)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::MultipleSameBlockConsumers
        );
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntAdd));
    }

    #[test]
    fn alias_unsafe_hazard_marks_disallowed_single_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::DisallowedSingleConsumer);
        assert_eq!(hazard.use_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntEqual));
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_predicate_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("disallowed single consumer proof");

        assert_eq!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::Predicate
        );
        assert_eq!(proof.rhs_kind, DisallowedSingleConsumerRhsKind::VarOrConst);
        assert_eq!(
            proof.reason,
            DisallowedSingleConsumerReason::ConsumerIsPredicate
        );
        assert!(proof.rhs_low_cost);
        assert!(!proof.rhs_has_load);
        assert!(!proof.rhs_has_call);
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_load_rhs_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Load {
                ptr: Box::new(HirExpr::Var("ptr".to_string())),
                ty: int(32),
            },
        )
        .expect("disallowed single consumer proof");

        assert_eq!(proof.reason, DisallowedSingleConsumerReason::RhsHasLoad);
        assert_eq!(proof.rhs_kind, DisallowedSingleConsumerRhsKind::LoadLike);
        assert!(proof.rhs_has_load);
        assert!(!proof.rhs_has_call);
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_call_arg_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Call,
                None,
                vec![constant(0x2000), output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("disallowed single consumer proof");

        assert_eq!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::CallArg
        );
        assert_eq!(
            proof.reason,
            DisallowedSingleConsumerReason::ConsumerIsCallArg
        );
        assert_eq!(proof.consumer_opcode, PcodeOpcode::Call);
    }

    #[test]
    fn unknown_consumer_kind_proof_marks_address_computation() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PtrAdd,
                Some(varnode(0x20)),
                vec![output.clone(), constant(4)],
            ),
        ]);

        let proof = PreviewBuilder::describe_unknown_consumer_kind_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("unknown consumer proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::PtrAdd);
        assert_eq!(proof.matched_input_indices, vec![0]);
        assert_eq!(
            proof.reason,
            UnknownConsumerKindReason::ConsumerIsAddressComputation
        );
    }

    #[test]
    fn unknown_consumer_kind_proof_marks_multiple_matched_inputs() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(varnode(0x20)),
                vec![output.clone(), output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_unknown_consumer_kind_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("unknown consumer proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::IntAdd);
        assert_eq!(proof.matched_input_indices, vec![0, 1]);
        assert_eq!(
            proof.reason,
            UnknownConsumerKindReason::ConsumerHasMultipleMatchedInputs
        );
    }

    #[test]
    fn popcount_consumer_proof_marks_compare_zero_downstream_use() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntEqual,
                Some(varnode(0x30)),
                vec![popcount_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_consumer_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount proof");

        assert_eq!(proof.input_width, 8);
        assert_eq!(proof.output_width, Some(8));
        assert_eq!(
            proof.popcount_result_used_by,
            PopCountResultUseFamily::PopCountFeedsCompareZero
        );
        assert_eq!(
            proof.downstream_consumer_opcode,
            Some(PcodeOpcode::IntEqual)
        );
        assert!(proof.rhs_low_cost);
        assert!(!proof.rhs_has_call);
        assert!(!proof.rhs_has_load);
    }

    #[test]
    fn popcount_consumer_proof_marks_unused_result() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output),
                vec![output.clone()],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_consumer_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount proof");

        assert_eq!(
            proof.popcount_result_used_by,
            PopCountResultUseFamily::PopCountResultUnused
        );
        assert_eq!(proof.downstream_consumer_opcode, None);
    }

    #[test]
    fn popcount_intand_chain_proof_marks_and_one_compare_zero() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_intand_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount intand proof");

        assert_eq!(proof.popcount_consumer_op_seq, 1);
        assert_eq!(proof.intand_op_seq, 2);
        assert_eq!(proof.intand_mask, Some(1));
        assert_eq!(proof.intand_mask_kind, PopCountIntAndMaskKind::AndOne);
        assert_eq!(
            proof.intand_result_consumer,
            PopCountIntAndDownstreamUseFamily::FeedsCompareZero
        );
        assert_eq!(
            proof.downstream_consumer_opcode,
            Some(PcodeOpcode::IntEqual)
        );
        assert!(proof.chain_low_cost);
        assert!(proof.chain_side_effect_free);
    }

    #[test]
    fn popcount_intand_chain_proof_marks_byte_mask_arithmetic() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(0xff)],
            ),
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_intand_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount intand proof");

        assert_eq!(proof.intand_mask, Some(0xff));
        assert_eq!(proof.intand_mask_kind, PopCountIntAndMaskKind::AndByteMask);
        assert_eq!(
            proof.intand_result_consumer,
            PopCountIntAndDownstreamUseFamily::FeedsArithmetic
        );
        assert_eq!(proof.downstream_consumer_opcode, Some(PcodeOpcode::IntAdd));
    }

    #[test]
    fn parity_chain_proof_marks_popcount_input_role() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_parity_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::PopCountInput);
        assert_eq!(proof.compare_opcode, PcodeOpcode::IntEqual);
        assert_eq!(proof.compare_const, 0);
        assert!(proof.chain_low_cost);
        assert!(proof.chain_side_effect_free);
    }

    #[test]
    fn parity_chain_proof_marks_popcount_result_role() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntNotEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__popcount".into(),
            args: vec![HirExpr::Var("tmp_1".into())],
            ty: int(32),
        };

        let proof = builder
            .describe_parity_chain_proof(&block, 1, &popcount_output, &rhs)
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::PopCountResult);
        assert_eq!(proof.compare_opcode, PcodeOpcode::IntNotEqual);
        assert_eq!(proof.compare_const, 1);
    }

    #[test]
    fn parity_chain_proof_marks_intand_result_role() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Call {
                target: "__popcount".into(),
                args: vec![HirExpr::Var("tmp_1".into())],
                ty: int(32),
            }),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        let proof = builder
            .describe_parity_chain_proof(&block, 2, &and_output, &rhs)
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::IntAndResult);
        assert_eq!(proof.compare_const, 0);
    }

    #[test]
    fn parity_chain_proof_rejects_non_one_mask() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(3)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let result = builder
            .describe_parity_chain_proof(&block, 0, &input, &HirExpr::Var("tmp_1".into()))
            .expect("parity chain result");

        assert_eq!(result, Err(ParityChainKeepReason::IntAndMaskNotOne));
    }

    #[test]
    fn single_consumer_predicate_proof_marks_compare_zero_same_guard() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("tmp_1".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert!(proof.same_guard_as_consumer);
        assert!(proof.requires_stable_representative);
    }

    #[test]
    fn single_consumer_predicate_proof_marks_negated_flag_same_guard() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::BoolNegate,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("tmp_1".to_string())),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::NegatedFlag
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::NegatedFlag
        );
        assert!(proof.same_guard_as_consumer);
    }

    #[test]
    fn single_consumer_predicate_proof_marks_compare_other_var_guard_mismatch() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("lhs".to_string())),
                rhs: Box::new(HirExpr::Var("rhs".to_string())),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::CompareOtherVar
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert!(!proof.same_guard_as_consumer);
        assert!(proof.low_cost_if_predicate);
    }

    #[test]
    fn arithmetic_predicate_proof_marks_low_bit_and_one() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_arithmetic_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Var("flag_bits".to_string())),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("arithmetic predicate proof");

        assert_eq!(proof.mask_kind, ArithmeticPredicateShape::LowBitAndOne);
        assert_eq!(proof.mask_value, Some(1));
        assert!(proof.boolean_width);
        assert!(proof.stable_required);
        assert_eq!(
            proof.stable_required_reason,
            Some(ArithmeticPredicateStableReason::ArithmeticMask)
        );
    }

    #[test]
    fn arithmetic_predicate_proof_marks_shift_and_mask() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_arithmetic_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(HirExpr::Var("flags".to_string())),
                    rhs: Box::new(HirExpr::Const(3, int(32))),
                    ty: int(32),
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("arithmetic predicate proof");

        assert_eq!(proof.mask_kind, ArithmeticPredicateShape::ShiftAndMask);
        assert_eq!(proof.mask_value, Some(1));
        assert!(proof.boolean_width);
    }

    #[test]
    fn low_bit_mask_predicate_proof_marks_compare_origin() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_low_bit_mask_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(0, int(32))),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("low-bit mask predicate proof");

        assert_eq!(
            proof.family,
            LowBitMaskPredicateFamily::MaskFromCompareResult
        );
        assert_eq!(proof.input_origin_kind, LowBitMaskInputOriginKind::Compare);
        assert!(proof.input_is_boolean_like);
        assert!(proof.feeds_only_predicate);
    }

    #[test]
    fn low_bit_mask_predicate_proof_marks_arithmetic_origin() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_low_bit_mask_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(1, int(32))),
                    ty: int(32),
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("low-bit mask predicate proof");

        assert_eq!(
            proof.family,
            LowBitMaskPredicateFamily::MaskFromArithmeticValue
        );
        assert_eq!(
            proof.input_origin_kind,
            LowBitMaskInputOriginKind::Arithmetic
        );
        assert!(!proof.input_is_boolean_like);
        assert_eq!(
            proof.stable_required_reason,
            Some(ArithmeticPredicateStableReason::ArithmeticMask)
        );
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_no_consumer_found() {
        let output = varnode(0x10);
        let block = block(vec![op(
            0,
            PcodeOpcode::Copy,
            Some(output.clone()),
            vec![constant(1)],
        )]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::UnknownNoConsumerFound);
        assert_eq!(hazard.use_stmt_idx, None);
        assert_eq!(hazard.hazard_stmt_idx, None);
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_redefinition_before_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(2)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownMalformedDefUseWindow
        );
        assert_eq!(hazard.use_stmt_idx, None);
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Copy));
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_allowed_consumer_but_non_low_cost_rhs() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Call {
                target: "helper".to_string(),
                args: vec![HirExpr::Var("tmp_1".to_string())],
                ty: int(32),
            },
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
        );
        assert_eq!(hazard.use_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Copy));
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_after_terminator_single_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            Some(1),
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
        );
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntEqual));
    }
}
