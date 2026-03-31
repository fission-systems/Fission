use super::*;
use crate::nir::structuring::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};

#[test]
fn structuring_promotes_single_entry_guarded_tail_region() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("flag".to_string()),
            then_body: vec![HirStmt::Expr(HirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    HirStmt::Expr(HirExpr::Var("middle".to_string())),
                    HirStmt::If {
                        cond: HirExpr::Var("flag".to_string()),
                        then_body: vec![HirStmt::Expr(HirExpr::Var("side".to_string()))],
                        else_body: Vec::new(),
                    },
                ],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ]
    );
}

#[test]
fn structuring_promotes_terminal_single_entry_guarded_tail_region() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(
        body,
        vec![HirStmt::If {
            cond: HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("reg".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Expr(HirExpr::Var("middle".to_string()))],
            else_body: Vec::new(),
        },]
    );
}

#[test]
fn structuring_promotes_guarded_tail_when_only_deleted_alias_chain_references_join_label() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Goto("block_join_1".to_string()),
        HirStmt::Label("block_join_1".to_string()),
        HirStmt::Goto("block_join_2".to_string()),
        HirStmt::Label("block_join_2".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Expr(HirExpr::Var("middle".to_string()))],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ]
    );
}

#[test]
fn structuring_candidate_discovery_counts_internal_label_gate_rejection() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_inner".to_string()),
        HirStmt::Expr(HirExpr::Var("inner".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
}

#[test]
fn structuring_candidate_discovery_counts_surviving_middle_ref_gate_rejection() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Var("inner".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert_eq!(stats.rejected_must_emit_label_surviving_middle_ref, 1);
    assert_eq!(stats.rejected_must_emit_label_surviving_external_ref, 1);
    assert_eq!(stats.rejected_must_emit_label_owner_conflict, 0);
}

#[test]
fn structuring_candidate_discovery_relaxes_trailing_middle_goto_to_join_label() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.rejected_must_emit_label_surviving_middle_ref, 0);
}

#[test]
fn structuring_candidate_discovery_counts_owner_conflict_gate_rejection() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        HirStmt::If {
            cond: HirExpr::Var("other_a".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Var("other_b".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert_eq!(stats.rejected_must_emit_label_surviving_middle_ref, 0);
    assert_eq!(stats.rejected_must_emit_label_surviving_external_ref, 0);
    assert_eq!(stats.rejected_must_emit_label_owner_conflict, 1);
}

#[test]
fn structuring_candidate_discovery_relaxes_same_owner_forward_refs_from_owner_conflict() {
    let body = vec![
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.rejected_must_emit_label_owner_conflict, 0);
    assert_eq!(stats.rejected_must_emit_label_surviving_external_ref, 1);
}

#[test]
fn structuring_candidate_discovery_relaxes_single_forward_external_ref() {
    let body = vec![
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.rejected_must_emit_label_surviving_external_ref, 0);
}

#[test]
fn structuring_candidate_discovery_keeps_post_label_external_ref_rejected() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        HirStmt::Goto("block_tail".to_string()),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.rejected_must_emit_label_surviving_external_ref, 1);
}

#[test]
fn structuring_terminal_guarded_tail_promotion_keeps_direct_shape_subtypes_zero() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(
        stats.promotion_rejected_by_shape_missing_terminal_join_target_count,
        0
    );
    assert_eq!(
        stats.promotion_rejected_by_shape_empty_nonterminal_tail_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_allows_leading_label_before_payload() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_leading".to_string()),
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
    assert_eq!(stats.canonicalized_guarded_tail_shape_count, 0);
}

#[test]
fn structuring_candidate_discovery_counts_missing_target_as_noncanonical_shape() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 0);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_skips_backward_target_without_nonterminal_failure() {
    let body = vec![
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Expr(HirExpr::Var("seed".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 0);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_terminalizes_nonterminal_join_label_forwarder() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_terminalizes_multihop_nonterminal_join_forwarder() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Goto("block_join_1".to_string()),
        HirStmt::Label("block_join_1".to_string()),
        HirStmt::Goto("block_join_2".to_string()),
        HirStmt::Label("block_join_2".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_safe_interleaved_alias_stub() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_alias".to_string()),
        HirStmt::Label("block_alias".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("payload".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_interleaved_join_stub_with_multiple_forward_gotos()
{
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_alias".to_string()),
        HirStmt::Label("block_alias".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("payload".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 0);
}

#[test]
fn structuring_candidate_discovery_counts_nested_tail_escape() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("after_escape".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 1);
}

#[test]
fn structuring_candidate_discovery_allows_tail_terminal_return_after_payload() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Return(Some(HirExpr::Var("ret_mid".to_string()))),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
}

#[test]
fn structuring_candidate_discovery_allows_tail_terminal_goto_after_payload() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_exit".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 0);
}

#[test]
fn structuring_candidate_discovery_counts_interleaved_referenced_label_use() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 1);
    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 1);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_interleaved_join_use_with_nontrivial_segment() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Block(vec![
            HirStmt::If {
                cond: HirExpr::Var("inner".to_string()),
                then_body: vec![HirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("block_alias".to_string()),
            HirStmt::Expr(HirExpr::Var("payload".to_string())),
            HirStmt::Label("block_join".to_string()),
            HirStmt::Expr(HirExpr::Var("after_join".to_string())),
        ]),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 0);
    assert_eq!(
        stats.canonicalization_failed_interleaved_join_uses_no_next_label_count,
        0
    );
    assert_eq!(
        stats.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_keeps_interleaved_join_use_with_side_effectful_segment_rejected()
{
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Block(vec![
            HirStmt::If {
                cond: HirExpr::Var("inner".to_string()),
                then_body: vec![HirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("block_alias".to_string()),
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp".to_string()),
                rhs: HirExpr::Var("payload".to_string()),
            },
            HirStmt::Label("block_join".to_string()),
            HirStmt::Expr(HirExpr::Var("after_join".to_string())),
        ]),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 1);
    assert_eq!(
        stats.canonicalization_failed_interleaved_join_uses_no_next_label_count,
        0
    );
    assert_eq!(
        stats.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
        1
    );
}

#[test]
fn structuring_candidate_discovery_counts_nested_after_label_alias_not_fallthrough() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("late".to_string()),
            then_body: vec![HirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 1);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        0
    );
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
        1
    );
}

#[test]
fn structuring_candidate_discovery_canonicalizes_local_fallthrough_alias_label() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.canonicalized_interleaved_join_use_count, 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_alias_forward_chain() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid_1".to_string()),
        HirStmt::Label("block_mid_1".to_string()),
        HirStmt::Block(vec![]),
        HirStmt::Goto("block_mid_2".to_string()),
        HirStmt::Label("block_mid_2".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 1);
    assert!(stats.canonicalized_interleaved_join_use_count >= 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_pure_multi_goto_alias_chain() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("skip_l".to_string())),
            rhs: Box::new(HirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        0
    );
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_local_nonfallthrough_alias() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("skipped".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_safe_top_level_after_label_alias() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        0
    );
    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_pure_value_top_level_after_label_alias() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("skip_l".to_string())),
            rhs: Box::new(HirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_keeps_side_effectful_top_level_after_label_alias_rejected() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Assign {
            lhs: HirLValue::Var("tmp".to_string()),
            rhs: HirExpr::Var("skip".to_string()),
        },
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 1);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        1
    );
}

#[test]
fn structuring_candidate_discovery_counts_join_external_ref_inside_nested_if() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("other".to_string()),
            then_body: vec![HirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Assign {
            lhs: HirLValue::Var("tmp".to_string()),
            rhs: HirExpr::Var("more".to_string()),
        },
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert!(stats.promotion_rejected_by_shape_count >= 1);
    assert!(stats.discovery_rejected_noncanonical_layout_count >= 1);
    assert_eq!(
        stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_true_nonlocal_alias_ref() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Assign {
            lhs: HirLValue::Var("tmp".to_string()),
            rhs: HirExpr::Var("more".to_string()),
        },
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];
    body.insert(
        0,
        HirStmt::If {
            cond: HirExpr::Var("outer".to_string()),
            then_body: vec![HirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
    );

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
        0
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_external_before_alias_nonlocal_ref() {
    let body = vec![
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
        1
    );
}

#[test]
fn structuring_candidate_discovery_counts_post_segment_alias_nonlocal_ref() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_inner".to_string()),
        HirStmt::Expr(HirExpr::Var("inner_work".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        HirStmt::If {
            cond: HirExpr::Var("late".to_string()),
            then_body: vec![HirStmt::Goto("block_inner".to_string())],
            else_body: Vec::new(),
        },
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
        1
    );
}

#[test]
fn structuring_candidate_discovery_rewrites_safe_external_alias_ref() {
    let body = vec![
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_rewrites_safe_nested_before_alias_ref() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_tail".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];
    body.insert(
        0,
        HirStmt::If {
            cond: HirExpr::Var("outer".to_string()),
            then_body: vec![HirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
    );

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_alias_multiple_internal_predecessors() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("skip_1".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("skip_2".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_alias_body_not_trivial() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("skip".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("local_work".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_body_not_trivial_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_accepts_alias_body_with_pure_value_exprs() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("skip_l".to_string())),
            rhs: Box::new(HirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("local_work".to_string())),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_body_not_trivial_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_alias_nonlocal_ref() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("other".to_string()),
            then_body: vec![HirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("skip".to_string())),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Goto("block_join".to_string()),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Assign {
            lhs: HirLValue::Var("tmp".to_string()),
            rhs: HirExpr::Var("more".to_string()),
        },
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert!(stats.promotion_rejected_by_shape_count >= 1);
    assert!(stats.discovery_rejected_noncanonical_layout_count >= 1);
    assert_eq!(
        stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        1
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_counts_alias_payload_crossing_join() {
    let body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("middle".to_string())),
        HirStmt::Goto("block_mid".to_string()),
        HirStmt::Label("block_mid".to_string()),
        HirStmt::Expr(HirExpr::Var("more".to_string())),
        HirStmt::Goto("block_next".to_string()),
        HirStmt::Label("block_tail".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        HirStmt::Label("block_next".to_string()),
        HirStmt::Return(Some(HirExpr::Var("ret2".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 1);
    assert_eq!(stats.canonicalization_failed_payload_crosses_join_count, 1);
}
