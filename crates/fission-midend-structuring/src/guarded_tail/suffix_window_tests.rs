//! Suffix-window pure/unit tests (moved from fission-pcode).
#![cfg(test)]

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::cleanup::*;
    use crate::guarded_tail::pure_hir;
    use crate::guarded_tail::types::*;
    use fission_midend_core::ir::*;
    use fission_midend_prehir::ir::*;

    fn test_if_goto(label: &str) -> PreHirStmt {
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![PreHirStmt::Goto(label.to_string())].into(),
            else_body: std::rc::Rc::new(Vec::new()),
        }
    }

    fn assert_suffix_accepts(
        body: &[PreHirStmt],
        anchor_idx: usize,
        start_label_idx: usize,
        terminal_label_idx: usize,
    ) {
        let PreHirStmt::Label(start_label) = &body[start_label_idx] else {
            panic!("start label missing at {start_label_idx}");
        };
        let referenced = collect_referenced_label_counts(body);
        let result = pure_hir::suffix_is_nonowned_terminal_tail(
            body,
            anchor_idx,
            start_label,
            start_label_idx,
            terminal_label_idx,
            &referenced,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_ok(
        body: &[PreHirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
    ) {
        let result = pure_hir::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Ok(()));
    }

    fn assert_classify_suffix_stmt_rejection(
        body: &[PreHirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: SuffixTailRejection,
    ) {
        let result = pure_hir::classify_suffix_stmt(
            &body[stmt_idx],
            body,
            stmt_idx,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(result, Err(expected));
    }

    fn assert_nested_suffix_shape_kind(
        body: &[PreHirStmt],
        stmt_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        next_label: &str,
        expected: NestedSuffixShapeKind,
    ) {
        let stmt = &body[stmt_idx];
        let kind = pure_hir::classify_nested_suffix_shape(
            stmt,
            body,
            current_label_idx,
            terminal_label_idx,
            next_label,
        );
        assert_eq!(kind, expected);
    }

    fn assert_suffix_side_effect_shape_kind(stmt: PreHirStmt, expected: SuffixSideEffectShapeKind) {
        let kind = pure_hir::classify_suffix_side_effect_shape(&stmt);
        assert_eq!(kind, expected);
    }

    fn assert_suffix_call_effect_shape_kind(stmt: PreHirStmt, expected: SuffixCallEffectShapeKind) {
        let kind = pure_hir::classify_suffix_call_effect_shape(&stmt);
        assert_eq!(kind, expected);
    }

    fn assert_suffix_external_budget(
        body: &[PreHirStmt],
        label: &str,
        anchor_idx: usize,
        current_label_idx: usize,
        terminal_label_idx: usize,
        rewrites: usize,
        expected: SuffixExternalEntryBudget,
    ) {
        let referenced = collect_referenced_label_counts(body);
        let raw_refs = referenced.get(label).copied().unwrap_or(0);
        let budget = pure_hir::compute_suffix_external_entry_budget(
            body,
            label,
            anchor_idx,
            current_label_idx,
            terminal_label_idx,
            raw_refs,
            rewrites,
        );
        assert_eq!(budget, expected);
    }

    fn assert_external_entry_ref_kind(
        body: &[PreHirStmt],
        label: &str,
        anchor_idx: usize,
        terminal_label_idx: usize,
        expected: Option<(ExternalEntryRefKind, usize)>,
    ) {
        let classified =
            pure_hir::classify_external_entry_ref_kind(body, label, anchor_idx, terminal_label_idx);
        assert_eq!(classified, expected);
    }

    #[test]
    fn earliest_owned_join_window_accepts_sink_safe_terminal_tail() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("join1".to_string()),
            PreHirStmt::Label("join1".to_string()),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 6, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_empty_block_alias_tail() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Block(std::rc::Rc::new(Vec::new())),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_accepts_alias_redirect_only_suffix() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("join1".to_string()),
            PreHirStmt::Label("join1".to_string()),
            PreHirStmt::Goto("join2".to_string()),
            PreHirStmt::Label("join2".to_string()),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 8, &referenced, false);

        assert_eq!(narrowed, Some(("join0".to_string(), 2)));
    }

    #[test]
    fn earliest_owned_join_window_rejects_side_effectful_suffix() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("not_safe".to_string())),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_external_entry_in_suffix() {
        let body = vec![
            PreHirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 1, 5, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_nested_nonlocal_suffix_ref() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("nested".to_string()),
                then_body: vec![PreHirStmt::Goto("sink".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 4, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn earliest_owned_join_window_rejects_when_terminal_join_is_already_owned() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];
        let referenced = collect_referenced_label_counts(&body);

        let narrowed = pure_hir::find_earliest_owned_join_label(&body, 0, 2, &referenced, false);

        assert_eq!(narrowed, None);
    }

    #[test]
    fn suffix_accepts_ignorable_and_empty_block_only() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Block(std::rc::Rc::new(Vec::new())),
            PreHirStmt::Goto("sink".to_string()),
            PreHirStmt::Label("sink".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 5);
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_next_label() {
        let body = vec![
            PreHirStmt::Goto("skip".to_string()),
            PreHirStmt::Label("alias".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("pure_gap".to_string())),
            PreHirStmt::Label("skip".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("redirect_gap".to_string())),
            PreHirStmt::Goto("alias".to_string()),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 3, "alias");
    }

    #[test]
    fn suffix_accepts_trivial_redirect_chain_to_terminal_return() {
        let body = vec![
            PreHirStmt::Goto("skip".to_string()),
            PreHirStmt::Label("alias".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("pure_gap".to_string())),
            PreHirStmt::Label("skip".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("redirect_gap".to_string())),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("done".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 0, 0, 6, "alias");
    }

    #[test]
    fn suffix_accepts_self_terminal_join_goto_with_pure_tail() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("pure_gap".to_string())),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("tmp".to_string()),
                rhs: PreHirExpr::Var("value".to_string()),
            },
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_accepts(&body, 0, 2, 9);
    }

    #[test]
    fn suffix_budget_counts_candidate_internal_top_level_refs_inside_window() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("join0".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 1,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_keeps_nested_candidate_ref_external() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("nested".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            2,
            4,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_internalizes_same_guard_family_nested_conditional_entry() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 1,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 1,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_different_guard_family_nested_entry() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("other_cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_internalizes_paired_same_guard_nested_boundary() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("body".to_string())),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 2,
                effective_external_refs: 0,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_paired_nested_boundary_with_guard_mismatch() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("other".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("body".to_string())),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_budget_does_not_internalize_paired_boundary_when_ref_kind_is_not_nested() {
        let body = vec![
            PreHirStmt::Goto("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("body".to_string())),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_suffix_external_budget(
            &body,
            "join0",
            0,
            3,
            6,
            0,
            SuffixExternalEntryBudget {
                raw_refs: 2,
                internal_top_level_refs: 0,
                suffix_safe_refs: 0,
                guard_family_internalized_refs: 0,
                paired_nested_boundary_refs: 0,
                effective_external_refs: 2,
                allowed_external_refs: 1,
            },
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_single_goto_then() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("next".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedSingleGotoThen,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_guard_family_mismatch() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("other".to_string()),
                then_body: vec![PreHirStmt::Goto("next".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            5,
            "next",
            NestedSuffixShapeKind::NestedGuardFamilyMismatch,
        );
    }

    #[test]
    fn suffix_nested_shape_classifies_crosses_terminal_join() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_nested_suffix_shape_kind(
            &body,
            1,
            0,
            4,
            "next",
            NestedSuffixShapeKind::NestedCrossesTerminalJoin,
        );
    }

    #[test]
    fn suffix_accepts_nested_terminal_join_tail_same_guard_family_then_branch() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_accepts_nested_terminal_join_tail_negated_guard_match_else_branch() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: std::rc::Rc::new(Vec::new()),
                else_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
            },
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 4, "next");
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_different_guard_family() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Var("other".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_nonterminal_target() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("next".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            4,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_nonempty_else_payload() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: vec![PreHirStmt::Expr(PreHirExpr::Var("payload".to_string()))].into(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_nested_terminal_join_tail_with_side_effectful_branch() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![
                    PreHirStmt::Expr(PreHirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Unknown,
                    }),
                    PreHirStmt::Goto("terminal".to_string()),
                ]
                .into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_read_only_assign() {
        assert_suffix_side_effect_shape_kind(
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar116".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("xVar43".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixSideEffectShapeKind::MemoryReadOnlyAssign,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_call_expr_side_effect() {
        assert_suffix_side_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixSideEffectShapeKind::CallExprSideEffect,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_void_unknown_call() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::VoidUnknownCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_ignored_call() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            SuffixCallEffectShapeKind::ReturnValueIgnoredCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_return_value_assigned_local() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("tmp".to_string()),
                rhs: PreHirExpr::Call {
                    target: "helper".to_string(),
                    args: vec![PreHirExpr::Var("arg".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::ReturnValueAssignedLocal,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_pure_known_helper_call() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("tmp".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::PureKnownHelperCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_flag_intrinsics_as_pure_helpers() {
        for target in ["__carry", "__scarry", "__sborrow"] {
            assert_suffix_call_effect_shape_kind(
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("tmp".to_string()),
                    rhs: PreHirExpr::Call {
                        target: target.to_string(),
                        args: vec![
                            PreHirExpr::Var("lhs".to_string()),
                            PreHirExpr::Const(
                                1,
                                NirType::Int {
                                    bits: 32,
                                    signed: false,
                                },
                            ),
                        ],
                        ty: NirType::Bool,
                    },
                },
                SuffixCallEffectShapeKind::PureKnownHelperCall,
            );
        }
    }

    #[test]
    fn guarded_tail_pure_value_accepts_flag_intrinsic_exprs() {
        for target in ["__carry", "__scarry", "__sborrow"] {
            assert!(pure_hir::expr_is_pure_value(&PreHirExpr::Call {
                target: target.to_string(),
                args: vec![
                    PreHirExpr::Var("lhs".to_string()),
                    PreHirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    ),
                ],
                ty: NirType::Bool,
            }));
        }
    }

    #[test]
    fn suffix_call_effect_shape_classifies_memory_mutating_call() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "memcpy".to_string(),
                args: vec![
                    PreHirExpr::Var("dst".to_string()),
                    PreHirExpr::Var("src".to_string()),
                ],
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            }),
            SuffixCallEffectShapeKind::MemoryMutatingCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_control_effect_call() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "abort".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            SuffixCallEffectShapeKind::ControlEffectCall,
        );
    }

    #[test]
    fn suffix_call_effect_shape_classifies_nested_call_as_unknown_effect() {
        assert_suffix_call_effect_shape_kind(
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("tmp".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Call {
                        target: "helper".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }),
                    rhs: Box::new(PreHirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            SuffixCallEffectShapeKind::UnknownCallEffect,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_memory_write() {
        assert_suffix_side_effect_shape_kind(
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Var("value".to_string()),
            },
            SuffixSideEffectShapeKind::MemoryWrite,
        );
    }

    #[test]
    fn suffix_side_effect_shape_classifies_volatile_or_unknown_load() {
        assert_suffix_side_effect_shape_kind(
            PreHirStmt::Expr(PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Call {
                    target: "addr_provider".to_string(),
                    args: vec![],
                    ty: NirType::Ptr(Box::new(NirType::Int {
                        bits: 8,
                        signed: false,
                    })),
                }),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            SuffixSideEffectShapeKind::VolatileOrUnknownLoad,
        );
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_condition_use() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("loaded".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_memory_read_only_assign_with_pure_ptroffset() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("base".to_string())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Expr(PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                expr: Box::new(PreHirExpr::Var("loaded".to_string())),
                ty: NirType::Bool,
            }),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_unknown_load_type() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                    ty: NirType::Unknown,
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("loaded".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_reused_in_return() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("loaded".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_when_ptr_contains_call() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Call {
                        target: "ptr_source".to_string(),
                        args: vec![],
                        ty: NirType::Ptr(Box::new(NirType::Int {
                            bits: 8,
                            signed: false,
                        })),
                    }),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("loaded".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_memory_read_only_assign_with_memory_visible_alias_risk() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("loaded".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("loaded".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Var("value".to_string()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_condition_use() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("count".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_accepts_known_pure_helper_call_with_pure_expr_use() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Expr(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("count".to_string())),
                rhs: Box::new(PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_ok(&body, 1, 0, 3, "next");
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_unknown_target() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount64".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("count".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_call_arg() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Call {
                        target: "value_provider".to_string(),
                        args: vec![],
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("count".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_reused_in_return() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("count".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_memory_visible_alias_risk() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("count".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__popcount".to_string(),
                    args: vec![PreHirExpr::Var("value".to_string())],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("count".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Var("value".to_string()),
            },
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            3,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn suffix_rejects_known_pure_helper_call_with_ignored_result() {
        let body = vec![
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "__popcount".to_string(),
                args: vec![PreHirExpr::Var("value".to_string())],
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            1,
            0,
            2,
            "next",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 1 },
        );
    }

    #[test]
    fn external_entry_kind_classifies_top_level_external_goto() {
        let body = vec![
            PreHirStmt::Goto("join0".to_string()),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::TopLevelExternalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_nested_conditional_goto() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join0".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::NestedConditionalGoto, 0)),
        );
    }

    #[test]
    fn external_entry_kind_classifies_loop_switch_derived_goto() {
        let body = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Var("cond".to_string()),
                body: vec![PreHirStmt::Goto("join0".to_string())].into(),
            },
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(
            &body,
            "join0",
            1,
            3,
            Some((ExternalEntryRefKind::LoopSwitchDerived, 0)),
        );
    }

    #[test]
    fn external_entry_kind_skips_candidate_internal_top_level_goto() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_external_entry_ref_kind(&body, "join0", 0, 4, None);
    }

    #[test]
    fn suffix_rejects_self_terminal_join_goto_with_nested_tail_stmt() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("terminal".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_side_effectful_stmt() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Call {
                target: "helper".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            PreHirStmt::Label("terminal".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasSideEffect { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_nonterminal_goto() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("other".to_string()),
            PreHirStmt::Label("next".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("after".to_string())),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            5,
            "next",
            SuffixTailRejection::SuffixHasNonTerminalGoto {
                stmt_idx: 2,
                target: "other".to_string(),
            },
        );
    }

    #[test]
    fn suffix_rejects_nested_nonlocal_ref() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("other".to_string())].into(),
                else_body: std::rc::Rc::new(Vec::new()),
            },
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasNestedOrNonlocalRef { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_label_crossing() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("payload".to_string())),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result =
            pure_hir::candidate_window_can_shrink_to_label(&body, 0, "join0", 1, 1, &referenced);
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasLabelCrossing {
                stmt_idx: 1,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_external_entry() {
        let body = vec![
            PreHirStmt::Goto("join0".to_string()),
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("terminal".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];
        let referenced = collect_referenced_label_counts(&body);
        let result =
            pure_hir::candidate_window_can_shrink_to_label(&body, 1, "join0", 2, 4, &referenced);
        assert_eq!(
            result,
            Err(SuffixTailRejection::SuffixHasExternalEntry {
                stmt_idx: 2,
                label: "join0".to_string(),
            })
        );
    }

    #[test]
    fn suffix_rejects_loop_or_switch_crossing() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::While {
                cond: PreHirExpr::Var("cond".to_string()),
                body: vec![].into(),
            },
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixHasLoopOrSwitchCrossing { stmt_idx: 2 },
        );
    }

    #[test]
    fn suffix_rejects_unresolved_alias_redirect() {
        let body = vec![
            test_if_goto("join0"),
            PreHirStmt::Label("join0".to_string()),
            PreHirStmt::Goto("unknown".to_string()),
            PreHirStmt::Label("terminal".to_string()),
        ];

        assert_classify_suffix_stmt_rejection(
            &body,
            2,
            1,
            3,
            "terminal",
            SuffixTailRejection::SuffixAliasRedirectUnresolved {
                stmt_idx: 2,
                label: "unknown".to_string(),
            },
        );
    }
}
