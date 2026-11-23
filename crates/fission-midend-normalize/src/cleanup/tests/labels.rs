use super::*;

#[test]
fn collapse_common_exit_guard_chain_wraps_body_and_preserves_exit_label() {
    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLe,
                lhs: Box::new(PreHirExpr::Var("rows".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Goto("exit".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLe,
                lhs: Box::new(PreHirExpr::Var("cols".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Goto("exit".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::Label("loop".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("ptr".to_string())),
                ty: int(32),
            },
            rhs: PreHirExpr::Var("value".to_string()),
        },
        PreHirStmt::Goto("loop".to_string()),
        PreHirStmt::Label("exit".to_string()),
        PreHirStmt::Return(None),
    ];

    assert!(collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts.len(), 3);
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = &stmts[0]
    else {
        panic!("expected wrapped body");
    };
    assert!(else_body.is_empty());
    assert!(matches!(
        cond,
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            ..
        }
    ));
    assert_eq!(then_body.len(), 3);
    assert!(matches!(&then_body[0], PreHirStmt::Label(label) if label == "loop"));
    assert!(matches!(&stmts[1], PreHirStmt::Label(label) if label == "exit"));
    assert!(matches!(&stmts[2], PreHirStmt::Return(None)));
}

#[test]
fn collapse_common_exit_guard_chain_rejects_mixed_targets() {
    let original = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("a".to_string()),
            then_body: vec![PreHirStmt::Goto("exit_a".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("b".to_string()),
            then_body: vec![PreHirStmt::Goto("exit_b".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
        PreHirStmt::Label("exit_a".to_string()),
        PreHirStmt::Return(None),
        PreHirStmt::Label("exit_b".to_string()),
        PreHirStmt::Return(None),
    ];
    let mut stmts = original.clone();

    assert!(!collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts, original);
}

#[test]
fn rewrite_orphan_loop_gotos_to_continue_rewrites_missing_label() {
    let mut stmts = vec![PreHirStmt::While {
        cond: PreHirExpr::Const(1, int(32)),
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Le,
                    lhs: Box::new(PreHirExpr::Var("n".to_string())),
                    rhs: Box::new(PreHirExpr::Var("i".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Break].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("miss".to_string()),
                then_body: vec![PreHirStmt::Goto("missing_continue".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("v".to_string()))),
        ]
        .into(),
    }];
    let defined: HashSet<String> = collect_defined_labels(&stmts).into_iter().collect();
    assert!(rewrite_orphan_loop_gotos_to_continue(
        &mut stmts,
        Some(&defined)
    ));
    let PreHirStmt::While { body, .. } = &stmts[0] else {
        panic!("expected while");
    };
    // Orphan goto becomes `i++; continue` (search-loop continue edge).
    assert!(
        matches!(
            &body[1],
            PreHirStmt::If {
                then_body,
                ..
            } if matches!(
                then_body.as_slice(),
                [
                    PreHirStmt::Block(inner)
                ] if matches!(
                    inner.as_slice(),
                    [PreHirStmt::Assign { .. }, PreHirStmt::Continue]
                )
            ) || matches!(then_body.as_slice(), [PreHirStmt::Continue])
        ),
        "expected continue (with optional i++), got {:?}",
        body[1]
    );
}

#[test]
fn prune_unreachable_after_return_stops_at_label_boundary() {
    let mut stmts = vec![
        PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("dead".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
        PreHirStmt::Goto("kept".to_string()),
        PreHirStmt::Label("kept".to_string()),
        PreHirStmt::Return(None),
    ];

    assert!(prune_unreachable_after_terminal(&mut stmts, None));
    assert_eq!(
        stmts,
        vec![
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
            PreHirStmt::Label("kept".to_string()),
            PreHirStmt::Return(None),
        ]
    );
}

#[test]
fn prune_unreachable_after_terminal_keeps_protected_lsda_landing_pad() {
    let mut stmts = vec![
        PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        PreHirStmt::Label("landing_pad".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cleanup".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
    ];

    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().insert("landing_pad".to_string());
    });
    let changed = prune_unreachable_after_terminal(&mut stmts, None);
    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().clear();
    });

    assert!(!changed);
    assert_eq!(stmts.len(), 3);
    assert_eq!(stmts[1], PreHirStmt::Label("landing_pad".to_string()));
}

#[test]
fn prune_unreachable_after_terminal_keeps_label_referenced_only_from_outer_scope() {
    // The `Goto` targeting "block_8d88" lives in a sibling scope, not in
    // `stmts` itself -- e.g. a switch case elsewhere in the same function.
    // `stmts`-local `collect_referenced_labels` alone can't see it, so
    // without `global_refs` this pass treated the label as dead code after
    // the preceding `Goto` and deleted it, leaving the other scope's `Goto`
    // dangling with no matching label anywhere in the rendered function.
    // Reproduces the real bug traced in bin_039.elf (DecBench sample set).
    let mut global_refs = HashSet::default();
    global_refs.insert("block_8d88".to_string());

    let mut stmts = vec![
        PreHirStmt::Goto("block_8779".to_string()),
        PreHirStmt::Label("block_8d88".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("dead".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
    ];

    let changed = prune_unreachable_after_terminal(&mut stmts, Some(&global_refs));

    assert!(!changed);
    assert_eq!(stmts.len(), 3);
    assert_eq!(stmts[1], PreHirStmt::Label("block_8d88".to_string()));
}

#[test]
fn single_pred_label_inline_drains_unprotected_dead_zone() {
    let mut stmts = vec![
        PreHirStmt::Goto("a".to_string()),
        PreHirStmt::Label("dead".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("unreached".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
        PreHirStmt::Label("a".to_string()),
        PreHirStmt::Return(None),
    ];

    assert!(single_pred_label_inline(&mut stmts));
    assert_eq!(stmts, vec![PreHirStmt::Return(None)]);
}

#[test]
fn single_pred_label_inline_keeps_protected_lsda_landing_pad_in_dead_zone() {
    let mut stmts = vec![
        PreHirStmt::Goto("a".to_string()),
        PreHirStmt::Label("landing_pad".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cleanup".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
        PreHirStmt::Label("a".to_string()),
        PreHirStmt::Return(None),
    ];

    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().insert("landing_pad".to_string());
    });
    let changed = single_pred_label_inline(&mut stmts);
    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().clear();
    });

    assert!(!changed);
    assert_eq!(
        stmts,
        vec![
            PreHirStmt::Goto("a".to_string()),
            PreHirStmt::Label("landing_pad".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("cleanup".to_string()),
                rhs: PreHirExpr::Const(1, int(32)),
            },
            PreHirStmt::Label("a".to_string()),
            PreHirStmt::Return(None),
        ]
    );
}

#[test]
fn single_pred_label_inline_keeps_dead_zone_with_externally_referenced_nested_label() {
    // goto "a" is textually referenced exactly once, and "a" is found later,
    // so the segment in between looks drainable at a glance -- but that
    // segment is a `While` whose *body* defines "inner", and "inner" is
    // `goto`'d from outside the segment (after label "a"). Draining the
    // segment must not happen: it would delete "inner" along with the While
    // that defines it, leaving the later `Goto("inner")` dangling.
    let mut stmts = vec![
        PreHirStmt::Goto("a".to_string()),
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Label("inner".to_string()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(1, int(32)),
                },
            ]
            .into(),
        },
        PreHirStmt::Label("a".to_string()),
        PreHirStmt::Goto("inner".to_string()),
        PreHirStmt::Return(None),
    ];

    single_pred_label_inline(&mut stmts);

    let defined: std::collections::HashSet<String> =
        collect_defined_labels(&stmts).into_iter().collect();
    let referenced = utils::collect_referenced_labels(&stmts);
    for label in &referenced {
        assert!(
            defined.contains(label),
            "goto target {label:?} must still have a matching label after cleanup"
        );
    }
}

/// `goto L; <dead>; L:` inside one nested list, with a *second* `goto L` in a
/// different list. The label has two references in the function, so it is not
/// single-predecessor, even though only one of them is visible from the list
/// being rewritten. Counting references per list inlined it anyway and left
/// the other `goto` without a target.
fn nested_single_pred_candidate(with_outside_reference: bool) -> Vec<PreHirStmt> {
    let mut stmts = vec![PreHirStmt::If {
        cond: PreHirExpr::Var("c".to_string()),
        then_body: vec![
            PreHirStmt::Goto("latch".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("unreached".to_string()),
                rhs: PreHirExpr::Const(1, int(32)),
            },
            PreHirStmt::Label("latch".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("y".to_string()),
                rhs: PreHirExpr::Const(2, int(32)),
            },
        ]
        .into(),
        else_body: vec![].into(),
    }];
    if with_outside_reference {
        stmts.push(PreHirStmt::Goto("latch".to_string()));
    } else {
        stmts.push(PreHirStmt::Return(None));
    }
    stmts
}

#[test]
fn single_pred_label_inline_keeps_label_referenced_from_another_list() {
    let mut stmts = nested_single_pred_candidate(true);

    single_pred_label_inline(&mut stmts);

    let defined: std::collections::HashSet<String> =
        collect_defined_labels(&stmts).into_iter().collect();
    let referenced = utils::collect_referenced_labels(&stmts);
    for label in &referenced {
        assert!(
            defined.contains(label),
            "goto target {label:?} must still have a matching label after cleanup"
        );
    }
}

#[test]
fn single_pred_label_inline_still_inlines_when_every_reference_is_local() {
    // Positive control for the test above: without the outside `goto` the same
    // nested pair has one reference in the whole function and must still be
    // inlined, so the fix cannot pass by switching the rewrite off.
    let mut stmts = nested_single_pred_candidate(false);

    assert!(single_pred_label_inline(&mut stmts));

    let defined: std::collections::HashSet<String> =
        collect_defined_labels(&stmts).into_iter().collect();
    assert!(
        !defined.contains("latch"),
        "the single-predecessor label should have been inlined away"
    );
}

#[test]
fn fuse_if_goto_allows_returns_in_segment() {
    // if (c) goto L; return 1; L: return 0;
    // → if (!c) { return 1; } L: return 0;
    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("c".to_string()),
            then_body: vec![PreHirStmt::Goto("L".to_string())].into(),
            else_body: vec![].into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Const(1, int(32)))),
        PreHirStmt::Label("L".to_string()),
        PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32)))),
    ];
    assert!(
        fuse_single_predecessor_boundaries(&mut stmts),
        "expected fuse: {stmts:?}"
    );
    assert!(
        matches!(
            &stmts[..],
            [
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                },
                PreHirStmt::Label(l),
                PreHirStmt::Return(Some(PreHirExpr::Const(0, _))),
            ] if else_body.is_empty()
                && matches!(then_body.as_slice(), [PreHirStmt::Return(Some(PreHirExpr::Const(1, _)))])
                && l == "L"
        ),
        "unexpected shape: {stmts:?}"
    );
}

/// The implementation `fuse_single_predecessor_boundaries` replaced: it
/// copied the run up to the next label for every statement before looking at
/// the statement. Kept as the oracle the rewrite must agree with.
fn fuse_single_predecessor_boundaries_reference(stmts: &mut Vec<PreHirStmt>) -> bool {
    fn next_label(stmts: &[PreHirStmt], start: usize) -> Option<(usize, String)> {
        stmts
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(idx, stmt)| match stmt {
                PreHirStmt::Label(label) => Some((idx, label.clone())),
                _ => None,
            })
    }
    fn lone_goto(body: &[PreHirStmt], label: &str) -> bool {
        matches!(body, [PreHirStmt::Goto(target)] if target == label)
    }
    fn linear(stmt: &PreHirStmt) -> bool {
        match stmt {
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Return(_) => true,
            PreHirStmt::Block(body) => body.iter().all(linear),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => then_body.iter().all(linear) && else_body.iter().all(linear),
            _ => false,
        }
    }
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        let Some((label_idx, label_name)) = next_label(stmts, idx + 1) else {
            idx += 1;
            continue;
        };
        let fused_segment = stmts[idx + 1..label_idx].to_vec();
        if fused_segment.is_empty() || !fused_segment.iter().all(linear) {
            idx += 1;
            continue;
        }
        let replacement = match &stmts[idx] {
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } if lone_goto(then_body, &label_name) && else_body.is_empty() => {
                Some(PreHirStmt::If {
                    cond: fission_midend_prehir::util::negate_expr(cond.clone()),
                    then_body: fused_segment.clone().into(),
                    else_body: Vec::new().into(),
                })
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } if then_body.is_empty() && lone_goto(else_body, &label_name) => {
                Some(PreHirStmt::If {
                    cond: cond.clone(),
                    then_body: fused_segment.clone().into(),
                    else_body: Vec::new().into(),
                })
            }
            _ => None,
        };
        let Some(replacement) = replacement else {
            idx += 1;
            continue;
        };
        stmts[idx] = replacement;
        stmts.drain(idx + 1..label_idx);
        changed = true;
        idx += 1;
    }
    changed
}

#[test]
fn fuse_if_goto_agrees_with_the_implementation_it_replaced() {
    // Statement lists generated from a fixed seed: guards jumping to the
    // next label, to a later one, or to none; in either arm; over runs that
    // are linear, empty, or interrupted by a loop.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let guard = |label: String, in_then: bool| {
        let goto: Vec<PreHirStmt> = vec![PreHirStmt::Goto(label)];
        PreHirStmt::If {
            cond: PreHirExpr::Var("c".to_string()),
            then_body: if in_then { goto.clone() } else { Vec::new() }.into(),
            else_body: if in_then { Vec::new() } else { goto }.into(),
        }
    };
    let mut fused_cases = 0;
    for case in 0..4000 {
        let len = 2 + (next() % 9) as usize;
        let mut stmts: Vec<PreHirStmt> = (0..len)
            .map(|_| match next() % 9 {
                0 | 1 => guard(format!("L{}", next() % 3), next() % 2 == 0),
                2 | 3 => PreHirStmt::Label(format!("L{}", next() % 3)),
                4 => PreHirStmt::Return(Some(PreHirExpr::Const(1, int(32)))),
                5 => PreHirStmt::While {
                    cond: PreHirExpr::Var("w".to_string()),
                    body: Vec::new().into(),
                },
                6 => PreHirStmt::If {
                    cond: PreHirExpr::Var("d".to_string()),
                    then_body: vec![PreHirStmt::Expr(PreHirExpr::Var("x".to_string()))].into(),
                    else_body: Vec::new().into(),
                },
                _ => PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(format!("v{}", next() % 4)),
                    rhs: PreHirExpr::Const((next() % 7) as i64, int(32)),
                },
            })
            .collect();
        let mut expected = stmts.clone();
        let expected_changed = fuse_single_predecessor_boundaries_reference(&mut expected);
        let changed = fuse_single_predecessor_boundaries(&mut stmts);
        assert_eq!(changed, expected_changed, "case {case}");
        assert_eq!(stmts, expected, "case {case}");
        fused_cases += usize::from(changed);
    }
    assert!(
        fused_cases > 50,
        "the generator must exercise the rewrite: {fused_cases}"
    );
}
