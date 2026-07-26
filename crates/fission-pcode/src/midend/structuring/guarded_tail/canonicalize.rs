use super::*;

impl<'a> PreviewBuilder<'a> {
    fn top_level_guard_goto_signature(stmt: &PreHirStmt) -> Option<(&PreHirExpr, &str)> {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_guard_goto_signature(stmt)
    }

    fn collapse_duplicate_top_level_guard_ladder(stmts: &mut Vec<PreHirStmt>) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::collapse_duplicate_top_level_guard_ladder(stmts)
    }

    fn top_level_label_definition_count(body: &[PreHirStmt], label: &str) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_label_definition_count(
            body, label,
        )
    }

    fn stmt_is_sink_safe_return_goto(stmt: &PreHirStmt, full_body: &[PreHirStmt]) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_sink_safe_return_goto(
            stmt, full_body,
        )
    }

    fn stmt_is_guard_cluster_trivial_gap(stmt: &PreHirStmt, full_body: &[PreHirStmt]) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_guard_cluster_trivial_gap(
            stmt, full_body,
        )
    }

    fn stmt_is_sink_equivalent_after_label_gap(
        stmt: &PreHirStmt,
        full_body: &[PreHirStmt],
        sink_return: &Option<PreHirExpr>,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_sink_equivalent_after_label_gap(
            stmt,
            full_body,
            sink_return,
        )
    }

    fn local_after_label_ref_is_sink_equivalent(
        body: &[PreHirStmt],
        full_body: &[PreHirStmt],
        label: &str,
        label_idx: usize,
        after_label_pos: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::local_after_label_ref_is_sink_equivalent(
            body,
            full_body,
            label,
            label_idx,
            after_label_pos,
        )
    }

    fn count_sink_equivalent_top_level_after_label_refs(
        body: &[PreHirStmt],
        full_body: &[PreHirStmt],
        label: &str,
        label_idx: usize,
        top_level_after_positions: &[usize],
        nested_after_label_count: usize,
        external_ref_count: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::count_sink_equivalent_top_level_after_label_refs(body, full_body, label, label_idx, top_level_after_positions, nested_after_label_count, external_ref_count)
    }

    fn top_level_after_label_ref_is_dead_post_return(
        body: &[PreHirStmt],
        after_label_pos: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_after_label_ref_is_dead_post_return(body, after_label_pos, label)
    }

    fn factor_duplicate_top_level_guard_cluster_with_trivial_gap(
        stmts: &mut Vec<PreHirStmt>,
        full_body: &[PreHirStmt],
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::factor_duplicate_top_level_guard_cluster_with_trivial_gap(stmts, full_body)
    }

    fn stmt_is_guard_prefix_safe(stmt: &PreHirStmt) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_guard_prefix_safe(stmt)
    }

    fn collapse_top_level_sink_to_return_goto_chain(
        stmts: &mut [PreHirStmt],
        full_body: &[PreHirStmt],
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::collapse_top_level_sink_to_return_goto_chain(stmts, full_body)
    }

    pub(super) fn canonicalize_interleaved_local_aliases(
        &mut self,
        body: &[PreHirStmt],
        full_body: &[PreHirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<PreHirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        fission_midend_structuring::guarded_tail::canonicalize_interleaved_local_aliases(
            self,
            body,
            full_body,
            segment_start,
            referenced,
        )
    }

    pub(super) fn canonicalize_guarded_tail_segment(
        &mut self,
        segment: &[PreHirStmt],
        full_body: &[PreHirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<PreHirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        fission_midend_structuring::guarded_tail::canonicalize_guarded_tail_segment(
            self,
            segment,
            full_body,
            segment_start,
            referenced,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_duplicate_guard_ladder_identical_cond_target() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_identical_deref_cond_target() {
        let cond = PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: Box::new(PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("p".to_string())),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            ty: NirType::Bool,
        };
        let mut body = vec![
            PreHirStmt::If {
                cond: cond.clone(),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::If {
                cond,
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_allows_empty_block_gap() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Block(Vec::new()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 1);
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn collapse_duplicate_guard_ladder_rejects_different_cond() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c1".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("c2".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
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
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L1".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L2".to_string())],
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
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("p".to_string())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("L".to_string())],
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
            PreHirStmt::While {
                cond: PreHirExpr::Var("loop_c".to_string()),
                body: vec![
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("c".to_string()),
                        then_body: vec![PreHirStmt::Goto("L".to_string())],
                        else_body: Vec::new(),
                    },
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("c".to_string()),
                        then_body: vec![PreHirStmt::Goto("L".to_string())],
                        else_body: Vec::new(),
                    },
                ],
            },
            PreHirStmt::Return(None),
        ];

        let removed = PreviewBuilder::collapse_duplicate_top_level_guard_ladder(&mut body);

        assert_eq!(removed, 0);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn collapse_sink_to_return_chain_top_level_goto_to_return() {
        let mut body = vec![
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 1);
        assert!(matches!(&body[0], PreHirStmt::Return(None)));
    }

    #[test]
    fn collapse_sink_to_return_chain_allows_pure_gap_hop() {
        let mut body = vec![
            PreHirStmt::Goto("Lhop".to_string()),
            PreHirStmt::Label("Lhop".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("tmp".to_string())),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 1);
        assert!(matches!(&body[0], PreHirStmt::Return(None)));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_reentry() {
        let mut body = vec![
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("Lret".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], PreHirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_ambiguous_target() {
        let mut body = vec![
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], PreHirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_side_effectful_gap() {
        let mut body = vec![
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "FUN_0x140001000".to_string(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], PreHirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_sink_to_return_chain_rejects_loop_crossing() {
        let mut body = vec![
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::While {
                cond: PreHirExpr::Var("loop_c".to_string()),
                body: vec![PreHirStmt::Break],
            },
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let rewritten =
            PreviewBuilder::collapse_top_level_sink_to_return_goto_chain(&mut body, &full_body);

        assert_eq!(rewritten, 0);
        assert!(matches!(&body[0], PreHirStmt::Goto(label) if label == "Lret"));
    }

    #[test]
    fn collapse_guard_cluster_allows_sink_safe_trivial_gap() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 1);
        assert_eq!(
            body.iter()
                .filter(|stmt| matches!(stmt, PreHirStmt::If { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn collapse_guard_cluster_allows_empty_block_and_sink_safe_gaps() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Block(Vec::new()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Block(Vec::new()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
        ];
        let full_body = body.clone();

        let removed = PreviewBuilder::factor_duplicate_top_level_guard_cluster_with_trivial_gap(
            &mut body, &full_body,
        );

        assert_eq!(removed, 1);
        assert_eq!(
            body.iter()
                .filter(|stmt| matches!(stmt, PreHirStmt::If { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn collapse_guard_cluster_rejects_side_effectful_gap() {
        let mut body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "FUN_0x140001000".to_string(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
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
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
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
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("mid".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("A".to_string()),
            PreHirStmt::Return(None),
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
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Goto("Lloop".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: vec![PreHirStmt::Goto("A".to_string())],
                else_body: Vec::new(),
            },
            PreHirStmt::Label("Lloop".to_string()),
            PreHirStmt::While {
                cond: PreHirExpr::Var("loop_c".to_string()),
                body: vec![PreHirStmt::Break],
            },
            PreHirStmt::Return(None),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Label("Lafter".to_string()),
            PreHirStmt::Goto("L".to_string()),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Label("Lafter".to_string()),
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Block(Vec::new()),
            PreHirStmt::Goto("Lhop".to_string()),
            PreHirStmt::Label("Lhop".to_string()),
            PreHirStmt::Return(None),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Expr(PreHirExpr::Call {
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Goto("Lamb".to_string()),
            PreHirStmt::Label("Lamb".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Label("Lamb".to_string()),
            PreHirStmt::Return(None),
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
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Goto("Lother".to_string()),
            PreHirStmt::Label("Lother".to_string()),
            PreHirStmt::Goto("Ltail".to_string()),
            PreHirStmt::Label("Ltail".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Const(
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
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
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Goto("Lret".to_string()),
            PreHirStmt::Label("Lret".to_string()),
            PreHirStmt::Return(None),
            PreHirStmt::Goto("L".to_string()),
            PreHirStmt::Goto("Lother".to_string()),
            PreHirStmt::Label("Lother".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Const(
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
