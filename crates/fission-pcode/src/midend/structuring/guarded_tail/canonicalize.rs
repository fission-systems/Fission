use super::*;

impl<'a> PreviewBuilder<'a> {
    fn top_level_guard_goto_signature(stmt: &HirStmt) -> Option<(&HirExpr, &str)> {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_guard_goto_signature(stmt)
    }

    fn collapse_duplicate_top_level_guard_ladder(stmts: &mut Vec<HirStmt>) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::collapse_duplicate_top_level_guard_ladder(stmts)
    }

    fn top_level_label_definition_count(body: &[HirStmt], label: &str) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_label_definition_count(body, label)
    }

    fn stmt_is_sink_safe_return_goto(stmt: &HirStmt, full_body: &[HirStmt]) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_sink_safe_return_goto(stmt, full_body)
    }

    fn stmt_is_guard_cluster_trivial_gap(stmt: &HirStmt, full_body: &[HirStmt]) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_guard_cluster_trivial_gap(stmt, full_body)
    }

    fn stmt_is_sink_equivalent_after_label_gap(
        stmt: &HirStmt,
        full_body: &[HirStmt],
        sink_return: &Option<HirExpr>,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_sink_equivalent_after_label_gap(stmt, full_body, sink_return)
    }

    fn local_after_label_ref_is_sink_equivalent(
        body: &[HirStmt],
        full_body: &[HirStmt],
        label: &str,
        label_idx: usize,
        after_label_pos: usize,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::local_after_label_ref_is_sink_equivalent(body, full_body, label, label_idx, after_label_pos)
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
        fission_midend_structuring::guarded_tail::pure_hir::count_sink_equivalent_top_level_after_label_refs(body, full_body, label, label_idx, top_level_after_positions, nested_after_label_count, external_ref_count)
    }

    fn top_level_after_label_ref_is_dead_post_return(
        body: &[HirStmt],
        after_label_pos: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::top_level_after_label_ref_is_dead_post_return(body, after_label_pos, label)
    }

    fn factor_duplicate_top_level_guard_cluster_with_trivial_gap(
        stmts: &mut Vec<HirStmt>,
        full_body: &[HirStmt],
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::factor_duplicate_top_level_guard_cluster_with_trivial_gap(stmts, full_body)
    }

    fn stmt_is_guard_prefix_safe(stmt: &HirStmt) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::stmt_is_guard_prefix_safe(stmt)
    }

    fn collapse_top_level_sink_to_return_goto_chain(
        stmts: &mut [HirStmt],
        full_body: &[HirStmt],
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::collapse_top_level_sink_to_return_goto_chain(stmts, full_body)
    }

    pub(super) fn canonicalize_interleaved_local_aliases(
        &mut self,
        body: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        fission_midend_structuring::guarded_tail::canonicalize_interleaved_local_aliases(self, body, full_body, segment_start, referenced)
    }

    pub(super) fn canonicalize_guarded_tail_segment(
        &mut self,
        segment: &[HirStmt],
        full_body: &[HirStmt],
        segment_start: usize,
        referenced: &HashMap<String, usize>,
    ) -> Result<(Vec<HirStmt>, Vec<(String, String)>), GuardedTailCanonicalizationFailure> {
        fission_midend_structuring::guarded_tail::canonicalize_guarded_tail_segment(self, segment, full_body, segment_start, referenced)
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
