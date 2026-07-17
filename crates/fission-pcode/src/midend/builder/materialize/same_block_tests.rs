use super::super::test_support::*;
use super::*;

fn test_ptr() -> NirType {
    NirType::Ptr(Box::new(NirType::Unknown))
}

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn stack_addr_proof(
    consumer_kind: DisallowedSingleConsumerConsumerKind,
    downstream_opcode: Option<PcodeOpcode>,
    base_reg: StackAddressBaseReg,
    same_block_use_count: usize,
    crosses_call: bool,
    crosses_store: bool,
    rsp_redefined_before_use: bool,
    frame_relative_candidate: bool,
    reason: StackAddressStabilityReason,
) -> StackAddressStabilityProof {
    StackAddressStabilityProof {
        consumer_kind,
        downstream_opcode,
        base_reg,
        offset: Some(0x20),
        same_block_use_count,
        crosses_call,
        crosses_store,
        rsp_redefined_before_use,
        frame_relative_candidate,
        reason,
    }
}

#[test]
fn output_nonlocal_use_detection_matches_overlapping_register_aliases() {
    let x20 = reg(0x40a0, 8);
    let w20 = reg(0x40a0, 4);
    let x21 = reg(0x40a8, 8);
    let mut def_block = block_at(
        0x1000,
        0,
        vec![op(
            1,
            PcodeOpcode::Copy,
            Some(x20.clone()),
            vec![constant(1)],
        )],
    );
    def_block.successors = vec![1];
    let use_block = block_at(
        0x2000,
        1,
        vec![op(
            2,
            PcodeOpcode::IntAdd,
            Some(x21),
            vec![w20, constant(1)],
        )],
    );
    let pcode = pcode_function(vec![def_block.clone(), use_block]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(builder.output_has_nonlocal_use(&def_block, 0, &x20));
}

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
fn low_cost_builder_inline_accepts_pcodeop_intrinsics() {
    let expr = HirExpr::Call {
        target: "__pcodeop_294".to_string(),
        args: vec![HirExpr::Var("param_1".to_string())],
        ty: int(32),
    };

    assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
        &expr
    ));
}

#[test]
fn single_use_builder_inline_blocks_call_like_consumers() {
    assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::Call));
    assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallInd));
    assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallOther));
    assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CBranch));
    assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::BranchInd));
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
    assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
        PcodeOpcode::IntEqual
    ));
    assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
        PcodeOpcode::BoolOr
    ));
    assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
        PcodeOpcode::PopCount
    ));
    assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
        PcodeOpcode::LzCount
    ));
}

#[test]
fn count_intrinsic_consumers_are_data_consumers_for_single_use_inline() {
    let popcount = op(
        1,
        PcodeOpcode::PopCount,
        Some(varnode(0x20)),
        vec![varnode(0x10)],
    );
    let lzcnt = op(
        2,
        PcodeOpcode::LzCount,
        Some(varnode(0x30)),
        vec![varnode(0x10)],
    );

    assert_eq!(
        PreviewBuilder::classify_disallowed_single_consumer_kind(&popcount, &[0]),
        DisallowedSingleConsumerConsumerKind::OtherData
    );
    assert_eq!(
        PreviewBuilder::classify_disallowed_single_consumer_kind(&lzcnt, &[0]),
        DisallowedSingleConsumerConsumerKind::OtherData
    );
}

#[test]
fn carry_intrinsic_consumers_are_predicate_consumers() {
    for opcode in [
        PcodeOpcode::IntCarry,
        PcodeOpcode::IntSCarry,
        PcodeOpcode::IntSBorrow,
    ] {
        let carry = op(
            1,
            opcode,
            Some(varnode(0x20)),
            vec![varnode(0x10), varnode(0x14)],
        );
        assert_eq!(
            PreviewBuilder::classify_disallowed_single_consumer_kind(&carry, &[0]),
            DisallowedSingleConsumerConsumerKind::Predicate
        );
    }
}

#[test]
fn lzcnt_intrinsic_rhs_is_low_cost_builder_inline_candidate() {
    let expr = HirExpr::Call {
        target: "__lzcnt".to_string(),
        args: vec![HirExpr::Var("tmp_1".to_string())],
        ty: int(32),
    };

    assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
        &expr
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
        PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(PcodeOpcode::Copy)
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
        op(1, PcodeOpcode::BranchInd, None, vec![output.clone()]),
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
    assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::BranchInd));
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
fn single_consumer_call_rhs_proof_marks_known_pure_intrinsic() {
    let output = varnode(0x10);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::CallOther,
            Some(output.clone()),
            vec![constant(0x2000), constant(1)],
        ),
        op(
            1,
            PcodeOpcode::IntAnd,
            Some(varnode(0x20)),
            vec![output.clone(), constant(1)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "__popcount".to_string(),
        args: vec![HirExpr::Var("tmp_1".to_string())],
        ty: int(32),
    };

    let proof = builder
        .describe_single_consumer_call_rhs_proof(&block, 0, &output, &rhs)
        .expect("call rhs proof");

    assert_eq!(
        proof.family,
        SingleConsumerCallRhsFamily::KnownPureIntrinsic
    );
    assert_eq!(proof.call_target, "__popcount");
    assert_eq!(proof.call_effect_source, None);
    assert_eq!(proof.downstream_opcode, Some(PcodeOpcode::IntAnd));
    assert!(proof.return_used);
}

#[test]
fn single_consumer_call_rhs_proof_marks_preview_unsafe_internal_call() {
    let output = varnode(0x10);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::Call,
            Some(output.clone()),
            vec![constant(0x140043d30)],
        ),
        op(
            1,
            PcodeOpcode::IntAdd,
            Some(varnode(0x20)),
            vec![output.clone(), constant(1)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let mut type_context = NirTypeContext::default();
    type_context.call_effect_summaries.insert(
        "FUN_0x140043d30".to_string(),
        NirCallEffectSummary {
            writes_memory: Some(true),
            may_call_unknown: Some(true),
            may_exit: Some(true),
            source: Some(CallEffectSummarySource::PreviewCalleeAnalysis),
            ..NirCallEffectSummary::default()
        },
    );
    let builder = PreviewBuilder::new(&pcode, &options, Some(&type_context));
    let rhs = HirExpr::Call {
        target: "FUN_0x140043d30".to_string(),
        args: vec![HirExpr::Var("arg_1".to_string())],
        ty: int(32),
    };

    let proof = builder
        .describe_single_consumer_call_rhs_proof(&block, 0, &output, &rhs)
        .expect("call rhs proof");

    assert_eq!(
        proof.family,
        SingleConsumerCallRhsFamily::PreviewCalleeAnalysisUnsafe
    );
    assert_eq!(
        proof.call_effect_source,
        Some(CallEffectSummarySource::PreviewCalleeAnalysis)
    );
    assert_eq!(proof.writes_memory, Some(true));
    assert_eq!(proof.may_call_unknown, Some(true));
    assert_eq!(proof.may_exit, Some(true));
    assert_eq!(proof.downstream_opcode, Some(PcodeOpcode::IntAdd));
}

#[test]
fn carry_intrinsic_predicate_proof_marks_boolor_branch_chain() {
    let output = varnode(0x10);
    let boolor_output = varnode(0x20);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::CallOther,
            Some(output.clone()),
            vec![constant(0x2000), constant(1), constant(2)],
        ),
        op(
            1,
            PcodeOpcode::BoolOr,
            Some(boolor_output.clone()),
            vec![output.clone(), constant(1)],
        ),
        op(
            2,
            PcodeOpcode::CBranch,
            None,
            vec![constant(0x3000), boolor_output],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "__carry".to_string(),
        args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
        ty: int(1),
    };

    let proof = builder
        .describe_carry_intrinsic_predicate_proof(&block, 0, &output, &rhs)
        .expect("carry predicate proof");

    assert_eq!(
        proof.bool_chain_role,
        CarryIntrinsicPredicateUseFamily::CarryFeedsBoolOr
    );
    assert_eq!(
        proof.boolor_downstream_use,
        Some(BoolOrDownstreamUseFamily::BoolOrFeedsBranch)
    );
    assert_eq!(
        proof.final_predicate_context,
        CarryIntrinsicFinalPredicateContext::BranchPredicate
    );
    assert!(proof.args_side_effect_free);
}

#[test]
fn carry_intrinsic_predicate_proof_marks_compare_zero_chain() {
    let output = varnode(0x10);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::CallOther,
            Some(output.clone()),
            vec![constant(0x2000), constant(1), constant(2)],
        ),
        op(
            1,
            PcodeOpcode::IntEqual,
            Some(varnode(0x20)),
            vec![output.clone(), constant(0)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "__scarry".to_string(),
        args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
        ty: int(1),
    };

    let proof = builder
        .describe_carry_intrinsic_predicate_proof(&block, 0, &output, &rhs)
        .expect("carry predicate proof");

    assert_eq!(
        proof.bool_chain_role,
        CarryIntrinsicPredicateUseFamily::CarryFeedsCompareZero
    );
    assert_eq!(proof.boolor_downstream_use, None);
    assert_eq!(
        proof.final_predicate_context,
        CarryIntrinsicFinalPredicateContext::CompareZero
    );
}

#[test]
fn intrinsic_compare_only_proof_marks_sborrow_compare_zero() {
    let output = varnode(0x10);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::CallOther,
            Some(output.clone()),
            vec![constant(0x2000), constant(1), constant(2)],
        ),
        op(
            1,
            PcodeOpcode::IntEqual,
            Some(varnode(0x20)),
            vec![output.clone(), constant(0)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "__sborrow".to_string(),
        args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
        ty: int(1),
    };

    let proof = builder
        .describe_intrinsic_compare_only_proof(&block, 0, &output, &rhs)
        .expect("intrinsic compare-only proof");

    assert_eq!(proof.family, IntrinsicCompareOnlyFamily::BorrowCompareZero);
    assert_eq!(proof.compare_const, Some(0));
    assert_eq!(
        proof.final_predicate_context,
        IntrinsicCompareFinalPredicateContext::CompareZero
    );
    assert!(proof.args_side_effect_free);
}

#[test]
fn intrinsic_compare_only_proof_marks_carry_compare_nonzero() {
    let output = varnode(0x10);
    let block = block(vec![
        op(
            0,
            PcodeOpcode::CallOther,
            Some(output.clone()),
            vec![constant(0x2000), constant(1), constant(2)],
        ),
        op(
            1,
            PcodeOpcode::IntNotEqual,
            Some(varnode(0x20)),
            vec![output.clone(), constant(0)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "__carry".to_string(),
        args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
        ty: int(1),
    };

    let proof = builder
        .describe_intrinsic_compare_only_proof(&block, 0, &output, &rhs)
        .expect("intrinsic compare-only proof");

    assert_eq!(proof.family, IntrinsicCompareOnlyFamily::CarryCompareZero);
    assert_eq!(proof.compare_const, Some(0));
    assert_eq!(
        proof.final_predicate_context,
        IntrinsicCompareFinalPredicateContext::CompareNonZero
    );
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

#[test]
fn alias_stable_required_family_prefers_load_addr() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        DisallowedSingleConsumerRhsKind::Arithmetic,
        Some(PcodeOpcode::Copy),
    );
    assert_eq!(family, AliasStableRequiredFamily::LoadAddrStableRequired);
}

#[test]
fn alias_stable_required_family_prefers_store_addr() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::StoreAddr,
        DisallowedSingleConsumerRhsKind::VarOrConst,
        Some(PcodeOpcode::Store),
    );
    assert_eq!(family, AliasStableRequiredFamily::StoreAddrStableRequired);
}

#[test]
fn alias_stable_required_family_prefers_branch_ind() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::OtherData,
        DisallowedSingleConsumerRhsKind::VarOrConst,
        Some(PcodeOpcode::BranchInd),
    );
    assert_eq!(family, AliasStableRequiredFamily::BranchIndStableRequired);
}

#[test]
fn alias_stable_required_family_prefers_otherdata_loadlike() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::OtherData,
        DisallowedSingleConsumerRhsKind::LoadLike,
        Some(PcodeOpcode::Copy),
    );
    assert_eq!(family, AliasStableRequiredFamily::OtherDataLoadLikeStable);
}

#[test]
fn alias_stable_required_family_prefers_otherdata_copy() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::OtherData,
        DisallowedSingleConsumerRhsKind::VarOrConst,
        Some(PcodeOpcode::Copy),
    );
    assert_eq!(family, AliasStableRequiredFamily::OtherDataCopyStable);
}

#[test]
fn alias_stable_required_family_prefers_arithmetic() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::OtherData,
        DisallowedSingleConsumerRhsKind::Arithmetic,
        Some(PcodeOpcode::IntAdd),
    );
    assert_eq!(family, AliasStableRequiredFamily::ArithmeticStableRequired);
}

#[test]
fn alias_stable_required_family_falls_back_to_unknown() {
    let family = PreviewBuilder::classify_alias_stable_required_family(
        DisallowedSingleConsumerConsumerKind::CallArg,
        DisallowedSingleConsumerRhsKind::Other,
        None,
    );
    assert_eq!(family, AliasStableRequiredFamily::UnknownAliasStable);
}

#[test]
fn address_stable_required_family_prefers_expr_has_load() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::StackRelative,
        AddressStableRequiredExprKind::HasLoad,
        false,
        false,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprHasLoad);
}

#[test]
fn address_stable_required_family_prefers_expr_has_call() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::GlobalRelative,
        AddressStableRequiredExprKind::HasCall,
        false,
        false,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprHasCall);
}

#[test]
fn address_stable_required_family_prefers_crosses_call_over_base_kind() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::StackRelative,
        AddressStableRequiredExprKind::PureArithmetic,
        false,
        true,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprCrossesCall);
}

#[test]
fn address_stable_required_family_prefers_crosses_store_over_base_kind() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::GlobalRelative,
        AddressStableRequiredExprKind::PureArithmetic,
        true,
        false,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprCrossesStore);
}

#[test]
fn address_stable_required_family_classifies_stack_relative() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::StackRelative,
        AddressStableRequiredExprKind::PureArithmetic,
        false,
        false,
    );
    assert_eq!(
        family,
        AddressStableRequiredFamily::AddressExprStackRelative
    );
}

#[test]
fn address_stable_required_family_classifies_global_relative() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::GlobalRelative,
        AddressStableRequiredExprKind::PureArithmetic,
        false,
        false,
    );
    assert_eq!(
        family,
        AddressStableRequiredFamily::AddressExprGlobalRelative
    );
}

#[test]
fn address_stable_required_family_classifies_register_base() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::RegisterBase,
        AddressStableRequiredExprKind::PureArithmetic,
        false,
        false,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprRegisterBase);
}

#[test]
fn address_stable_required_family_falls_back_to_unknown_base() {
    let family = PreviewBuilder::classify_address_stable_required_family(
        AddressStableRequiredBaseKind::UnknownBase,
        AddressStableRequiredExprKind::UnknownAddressExpr,
        false,
        false,
    );
    assert_eq!(family, AddressStableRequiredFamily::AddressExprUnknownBase);
}

#[test]
fn address_stable_required_base_kind_recognizes_stack_like_names() {
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
            "home_20".to_string()
        )),
        AddressStableRequiredBaseKind::StackRelative
    );
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
            "rsp".to_string()
        )),
        AddressStableRequiredBaseKind::StackRelative
    );
}

#[test]
fn address_stable_required_base_kind_recognizes_dat_global() {
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
            "DAT_140008000".to_string()
        )),
        AddressStableRequiredBaseKind::GlobalRelative
    );
}

#[test]
fn address_stable_required_expr_kind_recognizes_nested_load() {
    let expr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: NirType::Unknown,
        }),
        offset: 8,
    };
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
        AddressStableRequiredExprKind::HasLoad
    );
}

#[test]
fn address_stable_required_expr_kind_recognizes_nested_call() {
    let expr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Call {
            target: "sub_140001000".to_string(),
            args: vec![HirExpr::Var("param_1".to_string())],
            ty: NirType::Unknown,
        }),
        rhs: Box::new(HirExpr::Const(4, NirType::Unknown)),
        ty: NirType::Unknown,
    };
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
        AddressStableRequiredExprKind::HasCall
    );
}

#[test]
fn address_stable_required_expr_kind_recognizes_pure_ptr_arithmetic() {
    let expr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("local_10".to_string())),
            rhs: Box::new(HirExpr::Const(8, NirType::Unknown)),
            ty: NirType::Unknown,
        }),
        offset: 4,
    };
    assert_eq!(
        PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
        AddressStableRequiredExprKind::PureArithmetic
    );
}

#[test]
fn stack_address_stability_reason_prefers_crosses_call() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        false,
        true,
        true,
        true,
        true,
        StackAddressBaseReg::Rsp,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrCrossesCall);
}

#[test]
fn stack_address_stability_reason_prefers_crosses_store() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        false,
        false,
        true,
        true,
        true,
        StackAddressBaseReg::Rsp,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrCrossesStore);
}

#[test]
fn stack_address_stability_reason_prefers_rsp_mutated_before_use() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        false,
        false,
        false,
        true,
        true,
        StackAddressBaseReg::Rsp,
    );
    assert_eq!(
        reason,
        StackAddressStabilityReason::StackAddrRspMutatedBeforeUse
    );
}

#[test]
fn stack_address_stability_reason_prefers_escapes_over_single_use() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        true,
        false,
        false,
        false,
        true,
        StackAddressBaseReg::Rsp,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrEscapes);
}

#[test]
fn stack_address_stability_reason_prefers_multiple_use() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        2,
        false,
        false,
        false,
        false,
        true,
        StackAddressBaseReg::Rsp,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrMultipleUse);
}

#[test]
fn stack_address_stability_reason_classifies_frame_stable() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        false,
        false,
        false,
        false,
        true,
        StackAddressBaseReg::Rbp,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrFrameStable);
}

#[test]
fn stack_address_stability_reason_classifies_single_use() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        1,
        false,
        false,
        false,
        false,
        false,
        StackAddressBaseReg::Unknown,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrSingleUse);
}

#[test]
fn stack_address_stability_reason_falls_back_to_unknown() {
    let reason = PreviewBuilder::classify_stack_address_stability_reason(
        0,
        false,
        false,
        false,
        false,
        false,
        StackAddressBaseReg::Unknown,
    );
    assert_eq!(reason, StackAddressStabilityReason::StackAddrUnknown);
}

#[test]
fn stack_address_base_reg_recognizes_rsp() {
    assert_eq!(
        PreviewBuilder::classify_stack_address_base_reg(&HirExpr::Var("rsp".to_string())),
        StackAddressBaseReg::Rsp
    );
}

#[test]
fn stack_address_base_reg_recognizes_rbp() {
    let expr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Cast {
            ty: test_ptr(),
            expr: Box::new(HirExpr::Var("rbp".to_string())),
        }),
        offset: 0x20,
    };
    assert_eq!(
        PreviewBuilder::classify_stack_address_base_reg(&expr),
        StackAddressBaseReg::Rbp
    );
}

#[test]
fn stack_address_offset_extracts_add_const() {
    let expr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("rsp".to_string())),
        rhs: Box::new(HirExpr::Const(0x28, int(64))),
        ty: test_ptr(),
    };
    assert_eq!(
        PreviewBuilder::extract_stack_address_offset(&expr),
        Some(0x28)
    );
}

#[test]
fn stack_address_offset_extracts_sub_const() {
    let expr = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(HirExpr::Var("rbp".to_string())),
        rhs: Box::new(HirExpr::Const(0x18, int(64))),
        ty: test_ptr(),
    };
    assert_eq!(
        PreviewBuilder::extract_stack_address_offset(&expr),
        Some(-0x18)
    );
}

#[test]
fn stack_address_frame_relative_candidate_accepts_ptr_offset() {
    let expr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Cast {
            ty: test_ptr(),
            expr: Box::new(HirExpr::Var("rsp".to_string())),
        }),
        offset: 0x30,
    };
    assert!(PreviewBuilder::stack_address_frame_relative_candidate(
        &expr
    ));
}

#[test]
fn stack_address_frame_relative_candidate_rejects_complex_binary() {
    let expr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("rsp".to_string())),
        rhs: Box::new(HirExpr::Var("xVar31".to_string())),
        ty: test_ptr(),
    };
    assert!(!PreviewBuilder::stack_address_frame_relative_candidate(
        &expr
    ));
}

#[test]
fn stack_addr_frame_stable_trial_accepts_rsp_frame_stable_load_addr() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrFrameStable,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Ok(())
    );
}

#[test]
fn stack_addr_frame_stable_trial_accepts_rsp_frame_stable_store_addr() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::StoreAddr,
        Some(PcodeOpcode::Store),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrFrameStable,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Ok(())
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_non_frame_stable() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        false,
        false,
        StackAddressStabilityReason::StackAddrSingleUse,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedNonFrameStable)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_multiple_use() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        2,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrMultipleUse,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedMultipleUse)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_escape() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrEscapes,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, true),
        Err(StackAddrFrameStableTrialReason::RejectedEscapes)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_base_mutation() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        true,
        true,
        StackAddressStabilityReason::StackAddrRspMutatedBeforeUse,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedBaseMutation)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_crosses_call() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        true,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrCrossesCall,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedCrossesCallOrStore)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_crosses_store() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rsp,
        1,
        false,
        true,
        false,
        true,
        StackAddressStabilityReason::StackAddrCrossesStore,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedCrossesCallOrStore)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_non_rsp_base() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::LoadAddr,
        Some(PcodeOpcode::Load),
        StackAddressBaseReg::Rbp,
        1,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrFrameStable,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedNonFrameStable)
    );
}

#[test]
fn stack_addr_frame_stable_trial_rejects_non_memory_consumer() {
    let proof = stack_addr_proof(
        DisallowedSingleConsumerConsumerKind::OtherData,
        Some(PcodeOpcode::Copy),
        StackAddressBaseReg::Rsp,
        1,
        false,
        false,
        false,
        true,
        StackAddressStabilityReason::StackAddrFrameStable,
    );
    assert_eq!(
        PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
        Err(StackAddrFrameStableTrialReason::RejectedConsumerKind)
    );
}
