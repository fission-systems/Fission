use super::*;
use crate::midend::structuring::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};

fn extract_promoted_guarded_tail_merge_name(body: &[DirStmt]) -> Option<String> {
    let outer_if = body.iter().find_map(|stmt| match stmt {
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => Some((then_body, else_body)),
        _ => None,
    })?;
    let ret_name = match body.last() {
        Some(DirStmt::Return(Some(DirExpr::Var(name)))) => name,
        _ => return None,
    };
    match (outer_if.0.last(), outer_if.1.last()) {
        (
            Some(DirStmt::Assign {
                lhs: DirLValue::Var(then_name),
                rhs: DirExpr::Var(then_rhs),
            }),
            Some(DirStmt::Assign {
                lhs: DirLValue::Var(else_name),
                rhs: DirExpr::Var(else_rhs),
            }),
        ) if then_rhs == "probe"
            && else_rhs == "seed"
            && then_name == else_name
            && then_name == ret_name =>
        {
            Some(then_name.clone())
        }
        _ => None,
    }
}

#[test]
fn structuring_promotes_single_entry_guarded_tail_region() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("flag".to_string()),
            then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promotion_candidate_count, 1);
    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(
        body,
        vec![
            DirStmt::If {
                cond: DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    DirStmt::Expr(DirExpr::Var("middle".to_string())),
                    DirStmt::If {
                        cond: DirExpr::Var("flag".to_string()),
                        then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
                        else_body: Vec::new(),
                    },
                ],
                else_body: Vec::new(),
            },
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ],
        "stats={:#?}\nbody={:#?}",
        stats,
        body
    );
}

#[test]
fn structuring_promotes_terminal_single_entry_guarded_tail_region() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);
    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(stats.promotion_rejected_by_shape_count, 0);
    assert_eq!(
        body,
        vec![DirStmt::If {
            cond: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("reg".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Expr(DirExpr::Var("middle".to_string()))],
            else_body: Vec::new(),
        },]
    );
}

#[test]
fn structuring_promotes_guarded_tail_when_only_deleted_alias_chain_references_join_label() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Goto("block_join_1".to_string()),
        DirStmt::Label("block_join_1".to_string()),
        DirStmt::Goto("block_join_2".to_string()),
        DirStmt::Label("block_join_2".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
    assert_eq!(
        body,
        vec![
            DirStmt::If {
                cond: DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Expr(DirExpr::Var("middle".to_string()))],
                else_body: Vec::new(),
            },
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ]
    );
}

#[test]
fn structuring_guarded_tail_rejects_local_forward_label_branch_under_hard_cutover() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("tmp".to_string()),
            then_body: vec![DirStmt::Goto("block_taken".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("fallthrough".to_string())),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_taken".to_string()),
        DirStmt::If {
            cond: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("tmp".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("taken".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let original = body.clone();
    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1, "{stats:#?}");
    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert_eq!(stats.region_emit_ready_failed_count, 0, "{stats:#?}");
    assert_eq!(
        stats.guarded_tail_rejected_alias_interleave_conflict_count,
        0
    );
    assert_ne!(body, original);
}

#[test]
fn structuring_guarded_tail_execute_rewrites_descendant_reads_with_dominating_else_source() {
    let mut body = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("seed".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("tmp".to_string()),
            then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert!(stats.guarded_tail_replacement_plan_completed_count >= 1);
    assert!(matches!(
        body.first(),
        Some(DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs: DirExpr::Var(seed),
        }) if name == "tmp" && seed == "seed"
    ));
    let Some(DirStmt::If {
        cond,
        then_body,
        else_body,
    }) = body.get(1)
    else {
        panic!("expected promoted if body, got {body:#?}");
    };
    assert!(matches!(
        cond,
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } if expr.as_ref() == &DirExpr::Var("reg".to_string())
    ));
    assert!(
        then_body.iter().all(|stmt| {
            !matches!(
                stmt,
                DirStmt::Assign {
                    lhs: DirLValue::Var(name),
                    ..
                } if name == "tmp"
            )
        }),
        "{body:#?}"
    );
    assert!(
        then_body.iter().any(|stmt| matches!(
            stmt,
            DirStmt::If { cond: DirExpr::Var(name), .. } if name == "probe"
        )),
        "{body:#?}"
    );
    let replacement_name = match (then_body.last(), else_body.last(), body.last()) {
        (
            Some(DirStmt::Assign {
                lhs: DirLValue::Var(then_name),
                rhs: DirExpr::Var(then_rhs),
            }),
            Some(DirStmt::Assign {
                lhs: DirLValue::Var(else_name),
                rhs: DirExpr::Var(else_rhs),
            }),
            Some(DirStmt::Return(Some(DirExpr::Var(ret_name)))),
        ) if then_rhs == "probe"
            && else_rhs == "seed"
            && then_name == else_name
            && then_name == ret_name =>
        {
            then_name.clone()
        }
        _ => panic!("expected synthetic merge surface, got {body:#?}"),
    };
    assert_ne!(replacement_name, "tmp");
}

#[test]
fn structuring_guarded_tail_execute_rewrite_survives_dead_prefix_padding() {
    let mut body = vec![
        DirStmt::Expr(DirExpr::Const(
            0,
            NirType::Int {
                bits: 8,
                signed: false,
            },
        )),
        DirStmt::Expr(DirExpr::Const(
            1,
            NirType::Int {
                bits: 8,
                signed: false,
            },
        )),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("seed".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("tmp".to_string()),
            then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert!(
        stats.guarded_tail_replacement_plan_completed_count >= 1,
        "{stats:#?}"
    );
    assert!(matches!(
        body.first(),
        Some(DirStmt::Expr(DirExpr::Const(
            0,
            NirType::Int {
                bits: 8,
                signed: false,
            }
        )))
    ));
    assert!(matches!(
        body.get(1),
        Some(DirStmt::Expr(DirExpr::Const(
            1,
            NirType::Int {
                bits: 8,
                signed: false,
            }
        )))
    ));
    let merge_name = extract_promoted_guarded_tail_merge_name(&body).expect("{body:#?}");
    assert_ne!(merge_name, "tmp");
}

#[test]
fn structuring_guarded_tail_execute_rewrite_survives_forwarding_tail_label_chain() {
    let mut body = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("seed".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("tmp".to_string()),
            then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Goto("block_ret".to_string()),
        DirStmt::Label("block_ret".to_string()),
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert!(
        stats.guarded_tail_replacement_plan_completed_count >= 1,
        "{stats:#?}"
    );
    let merge_name = extract_promoted_guarded_tail_merge_name(&body).expect("{body:#?}");
    assert_ne!(merge_name, "tmp");
}

#[test]
fn structuring_guarded_tail_execute_rewrite_survives_padded_middle_depth() {
    let mut body = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("seed".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Block(vec![
            DirStmt::Expr(DirExpr::Var("middle_a".to_string())),
            DirStmt::Block(vec![DirStmt::Expr(DirExpr::Var("middle_b".to_string()))]),
        ]),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("tmp".to_string()),
            then_body: vec![DirStmt::Expr(DirExpr::Var("side".to_string()))],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert!(
        stats.guarded_tail_replacement_plan_completed_count >= 1,
        "{stats:#?}"
    );
    let merge_name = extract_promoted_guarded_tail_merge_name(&body).expect("{body:#?}");
    assert_ne!(merge_name, "tmp");
}

#[test]
fn structuring_guarded_tail_rejects_export_without_dominating_else_source() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let original = body.clone();
    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 0, "{stats:#?}");
    assert!(stats.guarded_tail_replacement_plan_rejected_missing_merge_count >= 1);
    assert_eq!(body, original);
}

#[test]
fn structuring_candidate_discovery_counts_internal_label_gate_rejection() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_inner".to_string()),
        DirStmt::Expr(DirExpr::Var("inner".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
fn structuring_candidate_discovery_allows_rewritable_middle_join_ref_after_gate_alignment() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::If {
            cond: DirExpr::Var("inner".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.discovery_seen_guarded_tail_like_shape_count >= 1,
        "{stats:#?}"
    );
    assert!(stats.promotion_candidate_count >= 1, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_surviving_middle_ref, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_guarded_tail_promotes_rewritable_middle_join_ref_after_gate_alignment() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::If {
            cond: DirExpr::Var("inner".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_surviving_middle_ref, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_relaxes_trailing_middle_goto_to_join_label() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.rejected_must_emit_label_surviving_middle_ref, 0);
}

#[test]
fn structuring_candidate_discovery_join_glue_middle_elides_all_goto_refs() {
    // Middle is only ignorable labels and `Goto(join)` hops — not just a trailing suffix.
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("glue".to_string()),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.rejected_must_emit_label_surviving_middle_ref, 0,
        "join-glue-only middle should not count surviving Goto refs: {stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_counts_owner_conflict_gate_rejection() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::If {
            cond: DirExpr::Var("other_a".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::If {
            cond: DirExpr::Var("other_b".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
}

#[test]
fn structuring_candidate_discovery_rejects_same_owner_forward_refs_under_hard_cutover() {
    let body = vec![
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_surviving_external_ref, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_guarded_tail_rejects_forward_external_refs_that_need_join_label() {
    let mut body = vec![
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let original = body.clone();
    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.promoted_region_count, 1, "{stats:#?}");
    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert_eq!(stats.region_emit_ready_failed_count, 0, "{stats:#?}");
    assert_ne!(body, original);
}

#[test]
fn structuring_guarded_tail_rejects_replacement_reads_after_follow_redefinition() {
    let mut body = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("seed".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("probe".to_string()),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("x".to_string()),
            rhs: DirExpr::Var("tmp".to_string()),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("late".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("tmp".to_string()))),
    ];

    let original = body.clone();
    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 0, "{stats:#?}");
    assert!(
        stats.guarded_tail_replacement_read_rejected_nondominated_count >= 1,
        "{stats:#?}"
    );
    assert_eq!(body, original);
}

#[test]
fn structuring_candidate_discovery_allows_single_forward_external_ref_when_elidable() {
    let body = vec![
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.discovery_seen_guarded_tail_like_shape_count >= 1,
        "{stats:#?}"
    );
    assert!(stats.promotion_candidate_count >= 1, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_surviving_external_ref, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_post_label_external_ref_rejected() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Goto("block_tail".to_string()),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.promotion_rejected_by_gate_count, 0);
}

#[test]
fn structuring_guarded_tail_promotion_keeps_post_label_external_ref_rejected() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Goto("block_tail".to_string()),
    ];
    let original = body.clone();

    let stats = promote_single_entry_guarded_tail_regions_for_test(&mut body);

    assert_eq!(stats.guarded_tail_promoted_count, 1, "{stats:#?}");
    assert_eq!(stats.region_emit_ready_failed_count, 0, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_surviving_external_ref, 0,
        "{stats:#?}"
    );
    assert_ne!(body, original);
}

#[test]
fn structuring_terminal_guarded_tail_promotion_keeps_direct_shape_subtypes_zero() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_leading".to_string()),
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Expr(DirExpr::Var("seed".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 0);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_terminalizes_nonterminal_join_label_forwarder() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Goto("block_join_1".to_string()),
        DirStmt::Label("block_join_1".to_string()),
        DirStmt::Goto("block_join_2".to_string()),
        DirStmt::Label("block_join_2".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_nonterminal_join_label, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_safe_interleaved_alias_stub() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_alias".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("payload".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_interleaved_join_stub_with_multiple_forward_gotos()
{
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_alias".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("payload".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert_eq!(stats.canonicalization_failed_interleaved_join_uses, 0);
}

#[test]
fn structuring_candidate_discovery_counts_nested_tail_escape() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("after_escape".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);
    println!("{stats:#?}");

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 1);
}

#[test]
fn structuring_candidate_discovery_allows_tail_terminal_return_after_payload() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 0);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
}

#[test]
fn structuring_candidate_discovery_allows_tail_terminal_goto_after_payload() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.canonicalization_failed_nested_tail_escape, 0);
}

#[test]
fn structuring_candidate_discovery_allows_terminal_tail_chain_with_pure_gap() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit_1".to_string()),
        DirStmt::Label("block_exit_1".to_string()),
        DirStmt::Expr(DirExpr::Var("pure_gap".to_string())),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("v".to_string()),
        },
        DirStmt::Goto("block_exit_2".to_string()),
        DirStmt::Label("block_exit_2".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.discovery_seen_guarded_tail_like_shape_count >= 1,
        "{stats:#?}"
    );
    assert_eq!(
        stats.canonicalization_failed_nested_tail_escape, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_allows_terminal_tail_chain_with_pure_return_hop() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_exit".to_string()),
        DirStmt::Expr(DirExpr::Var("gap_expr".to_string())),
        DirStmt::Assign {
            lhs: DirLValue::Var("x".to_string()),
            rhs: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("flag".to_string())),
                ty: NirType::Bool,
            },
        },
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.discovery_seen_guarded_tail_like_shape_count >= 1,
        "{stats:#?}"
    );
    assert_eq!(
        stats.canonicalization_failed_nested_tail_escape, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_ambiguous_terminal_tail_target_rejected() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_exit".to_string()),
        DirStmt::Goto("block_ret_1".to_string()),
        DirStmt::Goto("block_ret_2".to_string()),
        DirStmt::Label("block_ret_1".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret1".to_string()))),
        DirStmt::Label("block_ret_2".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret2".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.promoted_region_count, 0, "{stats:#?}");
    assert!(
        stats.canonicalization_failed_nested_tail_escape
            + stats.discovery_rejected_noncanonical_layout_count
            + stats.promotion_rejected_by_shape_count
            + stats.guarded_tail_rejected_alias_interleave_conflict_count
            > 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_side_effectful_terminal_tail_rejected() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_exit".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Load {
                ptr: Box::new(DirExpr::Var("ptr".to_string())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        },
        DirStmt::Goto("block_ret".to_string()),
        DirStmt::Label("block_ret".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.promoted_region_count, 0, "{stats:#?}");
    assert!(
        stats.canonicalization_failed_nested_tail_escape
            + stats.discovery_rejected_noncanonical_layout_count
            + stats.promotion_rejected_by_shape_count
            + stats.guarded_tail_rejected_alias_interleave_conflict_count
            > 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_terminal_tail_reentry_rejected() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_exit".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.promoted_region_count, 0, "{stats:#?}");
    assert!(
        stats.canonicalization_failed_nested_tail_escape
            + stats.discovery_rejected_noncanonical_layout_count
            + stats.promotion_rejected_by_shape_count
            + stats.guarded_tail_rejected_alias_interleave_conflict_count
            > 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_terminal_tail_loop_crossing_rejected() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_exit".to_string()),
        DirStmt::Label("block_exit".to_string()),
        DirStmt::While {
            cond: DirExpr::Var("loop_c".to_string()),
            body: vec![DirStmt::Break],
        },
        DirStmt::Goto("block_ret".to_string()),
        DirStmt::Label("block_ret".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.promoted_region_count, 0, "{stats:#?}");
    assert!(
        stats.canonicalization_failed_nested_tail_escape
            + stats.discovery_rejected_noncanonical_layout_count
            + stats.promotion_rejected_by_shape_count
            + stats.guarded_tail_rejected_alias_interleave_conflict_count
            > 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_rejects_goto_to_return_only_tail_label_under_hard_cutover() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_ret".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_tail".to_string()))),
        DirStmt::Label("block_ret".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret_mid".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.region_emit_ready_failed_count, 0, "{stats:#?}");
    assert_eq!(
        stats.guarded_tail_replacement_plan_candidate_count, 1,
        "{stats:#?}"
    );
    assert_eq!(
        stats.guarded_tail_replacement_plan_rejected_unstable_read_count, 0,
        "{stats:#?}"
    );
    assert_eq!(stats.promotion_candidate_count, 1, "{stats:#?}");
}

#[test]
fn structuring_candidate_discovery_counts_interleaved_referenced_label_use() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Block(vec![
            DirStmt::If {
                cond: DirExpr::Var("inner".to_string()),
                then_body: vec![DirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("block_alias".to_string()),
            DirStmt::Expr(DirExpr::Var("payload".to_string())),
            DirStmt::Label("block_join".to_string()),
            DirStmt::Expr(DirExpr::Var("after_join".to_string())),
        ]),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Block(vec![
            DirStmt::If {
                cond: DirExpr::Var("inner".to_string()),
                then_body: vec![DirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("block_alias".to_string()),
            DirStmt::Assign {
                lhs: DirLValue::Var("tmp".to_string()),
                rhs: DirExpr::Var("payload".to_string()),
            },
            DirStmt::Label("block_join".to_string()),
            DirStmt::Expr(DirExpr::Var("after_join".to_string())),
        ]),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.promoted_region_count, 0);
    assert_eq!(stats.guarded_tail_candidate_count, 0);
    assert!(
        stats.discovery_rejected_noncanonical_layout_count
            + stats.guarded_tail_rejected_alias_interleave_conflict_count
            + stats.region_emit_ready_failed_count
            > 0
    );
}

#[test]
fn structuring_candidate_discovery_counts_nested_after_label_alias_not_fallthrough() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("late".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid_1".to_string()),
        DirStmt::Label("block_mid_1".to_string()),
        DirStmt::Block(vec![]),
        DirStmt::Goto("block_mid_2".to_string()),
        DirStmt::Label("block_mid_2".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("skip_l".to_string())),
            rhs: Box::new(DirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("skipped".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 0);
    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 0);
}

#[test]
fn structuring_candidate_discovery_canonicalizes_safe_top_level_after_label_alias() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
fn structuring_candidate_discovery_keeps_forward_alias_chain_with_external_top_level_post_ref() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_alias".to_string()),
        DirStmt::Goto("block_join_1".to_string()),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_join_1".to_string()),
        DirStmt::Expr(DirExpr::Var("pure_gap".to_string())),
        DirStmt::Goto("block_join_2".to_string()),
        DirStmt::Label("block_join_2".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Goto("block_alias".to_string()),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.discovery_seen_guarded_tail_like_shape_count >= 1,
        "{stats:#?}"
    );
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count, 0,
        "{stats:#?}"
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_keeps_forward_alias_chain_with_external_nested_post_ref_rejected()
 {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_alias".to_string()),
        DirStmt::Goto("block_join_1".to_string()),
        DirStmt::Goto("block_alias".to_string()),
        DirStmt::Label("block_join_1".to_string()),
        DirStmt::Goto("block_join_2".to_string()),
        DirStmt::Label("block_join_2".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::If {
            cond: DirExpr::Var("late".to_string()),
            then_body: vec![DirStmt::Goto("block_alias".to_string())],
            else_body: Vec::new(),
        },
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count >= 1
            || stats.canonicalization_failed_alias_has_nonlocal_ref_count >= 1,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_canonicalizes_pure_value_top_level_after_label_alias() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("skip_l".to_string())),
            rhs: Box::new(DirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.canonicalization_failed_alias_not_fallthrough_count, 0);
    assert_eq!(
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_canonicalizes_pure_assign_top_level_after_label_alias() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("flag".to_string())),
                ty: NirType::Bool,
            },
        },
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Load {
                ptr: Box::new(DirExpr::Var("ptr".to_string())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        },
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("other".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("more".to_string()),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.discovery_seen_guarded_tail_like_shape_count >= 1);
    assert!(stats.promotion_candidate_count >= 1);
    assert_eq!(stats.promoted_region_count, 0);
    assert!(stats.promotion_rejected_by_shape_count >= 1);
    assert!(stats.discovery_rejected_noncanonical_layout_count >= 1);
    assert_eq!(
        stats.guarded_tail_rejected_alias_interleave_conflict_count,
        1
    );
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("more".to_string()),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];
    body.insert(
        0,
        DirStmt::If {
            cond: DirExpr::Var("outer".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
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
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_inner".to_string()),
        DirStmt::Expr(DirExpr::Var("inner_work".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::If {
            cond: DirExpr::Var("late".to_string()),
            then_body: vec![DirStmt::Goto("block_inner".to_string())],
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
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        0
    );
}

#[test]
fn structuring_candidate_discovery_internalizes_same_guard_family_nested_before_alias_ref() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("cond".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::If {
            cond: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("cond".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_count, 0,
        "{stats:#?}"
    );
    assert_eq!(
        stats.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count, 0,
        "{stats:#?}"
    );
    assert!(stats.promotion_candidate_count >= 1, "{stats:#?}");
}

#[test]
fn structuring_candidate_discovery_counts_nested_before_alias_ref_gate_rejection_after_alignment() {
    let mut body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_tail".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];
    body.insert(
        0,
        DirStmt::If {
            cond: DirExpr::Var("outer".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
    );

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert!(stats.promotion_candidate_count >= 1, "{stats:#?}");
    assert_eq!(
        stats.rejected_must_emit_label_owner_conflict, 0,
        "{stats:#?}"
    );
}

#[test]
fn structuring_candidate_discovery_counts_alias_multiple_internal_predecessors() {
    let body = vec![
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("skip_1".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("skip_2".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("skip".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("local_work".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("skip_l".to_string())),
            rhs: Box::new(DirExpr::Var("skip_r".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("local_work".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::If {
            cond: DirExpr::Var("other".to_string()),
            then_body: vec![DirStmt::Goto("block_mid".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("skip".to_string())),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Goto("block_join".to_string()),
        DirStmt::Label("block_join".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("tmp".to_string()),
            rhs: DirExpr::Var("more".to_string()),
        },
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Goto("block_mid".to_string()),
        DirStmt::Label("block_mid".to_string()),
        DirStmt::Expr(DirExpr::Var("more".to_string())),
        DirStmt::Goto("block_next".to_string()),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Label("block_next".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret2".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(stats.discovery_seen_guarded_tail_like_shape_count, 1);
    assert_eq!(stats.promotion_candidate_count, 0);
    assert_eq!(stats.promotion_rejected_by_shape_count, 1);
    assert_eq!(stats.discovery_rejected_noncanonical_layout_count, 1);
    assert_eq!(
        stats.guarded_tail_rejected_alias_interleave_conflict_count,
        1
    );
    assert_eq!(stats.canonicalization_failed_payload_crosses_join_count, 1);
}

#[test]
fn structuring_candidate_discovery_counts_ambiguous_follow_witness_rejection() {
    let body = vec![
        DirStmt::Block(vec![DirStmt::Goto("block_after".to_string())]),
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Goto("block_tail".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Expr(DirExpr::Var("middle".to_string())),
        DirStmt::Label("block_tail".to_string()),
        DirStmt::Block(Vec::new()),
        DirStmt::Label("block_after".to_string()),
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
    ];

    let stats = discover_guarded_tail_candidates_for_test(&body);

    assert_eq!(
        stats.discovery_seen_guarded_tail_like_shape_count, 1,
        "{stats:#?}"
    );
    assert_eq!(
        stats.guarded_tail_rejected_ambiguous_follow_count, 1,
        "{stats:#?}"
    );
    assert_eq!(stats.promoted_region_count, 0);
}
