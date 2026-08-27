use super::utils::*;
use super::*;
// prelude via parent
use crate::HashSet;
use fission_midend_core::*;
use fission_midend_prehir::*;

fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

fn preserved_temp_binding(name: &str, bits: u32) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn temp_binding(name: &str, bits: u32) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

fn stack_binding(name: &str, offset: i64) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: int(32),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(offset)),
        initializer: None,
    }
}

#[test]
fn recursive_empty_if_cleanup_prunes_nested_pure_empty_guard() {
    let mut stmts = vec![PreHirStmt::Block(
        vec![PreHirStmt::If {
            cond: PreHirExpr::Var("xVar12".to_string()),
            then_body: Vec::new().into(),
            else_body: Vec::new().into(),
        }]
        .into(),
    )];

    assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts, None));
    assert!(stmts.is_empty());
}

#[test]
fn recursive_empty_if_cleanup_preserves_side_effectful_empty_guard() {
    let mut stmts = vec![PreHirStmt::If {
        cond: PreHirExpr::Call {
            target: "unknown_predicate".to_string(),
            args: Vec::new(),
            ty: NirType::Bool,
        },
        then_body: Vec::new().into(),
        else_body: Vec::new().into(),
    }];

    assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts, None));
    assert!(matches!(
        &stmts[..],
        [PreHirStmt::Expr(PreHirExpr::Call { target, .. })] if target == "unknown_predicate"
    ));
}

#[test]
fn collapse_trivial_assign_returns_skips_preserved_temp() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar0".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(PreHirExpr::Var("eax".to_string())),
                rhs: Box::new(PreHirExpr::Var("ecx".to_string())),
                ty: int(32),
            },
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("uVar0".to_string()))),
    ];

    assert!(!collapse_trivial_assign_returns(
        &mut stmts,
        &["uVar0"].into_iter().collect::<HashSet<_>>(),
    ));
    assert!(matches!(stmts[0], PreHirStmt::Assign { .. }));
    assert!(matches!(
        stmts[1],
        PreHirStmt::Return(Some(PreHirExpr::Var(_)))
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rewrites_do_while_exit_copy() {
    let mut stmts = vec![
        PreHirStmt::DoWhile {
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("sum".to_string())),
                        rhs: Box::new(PreHirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("exit_sum".to_string()),
                    rhs: PreHirExpr::Var("sum".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("ptr".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("ptr".to_string())),
                        rhs: Box::new(PreHirExpr::Const(1, int(64))),
                        ty: int(64),
                    },
                },
            ]
            .into(),
            cond: PreHirExpr::Var("keep_going".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let PreHirStmt::DoWhile { body, .. } = &stmts[0] else {
        panic!("expected do/while");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[1],
        PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_rhs_mutated_after_copy() {
    let mut stmts = vec![
        PreHirStmt::DoWhile {
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("exit_sum".to_string()),
                    rhs: PreHirExpr::Var("sum".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
            ]
            .into(),
            cond: PreHirExpr::Var("keep_going".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn eliminate_redundant_var_assigns_removes_adjacent_duplicate_assign() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar84".to_string()),
            rhs: PreHirExpr::Const(0, int(64)),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar84".to_string()),
            rhs: PreHirExpr::Const(0, int(64)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("uVar84".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: PreHirExpr::Const(0, _),
        } if name == "uVar84"
    ));
}

#[test]
fn eliminate_redundant_var_assigns_keeps_self_dependent_duplicate() {
    let rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("sum".to_string())),
        rhs: Box::new(PreHirExpr::Const(1, int(32))),
        ty: int(32),
    };
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("sum".to_string()),
            rhs: rhs.clone(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("sum".to_string()),
            rhs,
        },
    ];

    assert!(!eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
}

#[test]
fn eliminate_redundant_var_assigns_removes_exact_self_assign() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar29".to_string()),
            rhs: PreHirExpr::Var("xVar29".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("xVar29".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(
        stmts,
        vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "xVar29".to_string()
        )))]
    );
}

#[test]
fn eliminate_redundant_var_assigns_recurses_into_nested_block() {
    // Structured O0 bodies often wrap the real statements in a single Block;
    // self-assigns inside that nest must still be removed (measured recursive
    // dual-call / iterative loop noise: `uVar = uVar`).
    let mut stmts = vec![PreHirStmt::Block(
        vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar1".to_string()),
                rhs: PreHirExpr::Var("param_1".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar1".to_string()),
                rhs: PreHirExpr::Var("uVar1".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("uVar1".to_string()))),
        ]
        .into(),
    )];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    match &stmts[0] {
        PreHirStmt::Block(body) => {
            assert_eq!(
                body.len(),
                2,
                "self-assign inside Block must be dropped: {body:?}"
            );
            assert!(
                !body.iter().any(|s| matches!(
                    s,
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(a),
                        rhs: PreHirExpr::Var(b),
                    } if a == b
                )),
                "no pure identity assign should remain: {body:?}"
            );
        }
        other => panic!("expected outer Block, got {other:?}"),
    }
}

#[test]
fn cast_elision_rewrites_self_widening_assignment_to_self_assign() {
    let mut func = PreHirFunction {
        name: "test_self_widening_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "uVar84".to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar84".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(PreHirExpr::Var("uVar84".to_string())),
                },
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("uVar84".to_string()))),
        ],
        ..Default::default()
    };

    assert!(cast_elision_pass(&mut func));
    assert_eq!(
        func.body[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar84".to_string()),
            rhs: PreHirExpr::Var("uVar84".to_string()),
        }
    );
}

#[test]
fn cast_elision_keeps_self_narrowing_assignment_to_wide_binding() {
    let mut func = PreHirFunction {
        name: "test_self_narrowing_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "xVar29".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar29".to_string()),
            rhs: PreHirExpr::Cast {
                ty: int(32),
                expr: Box::new(PreHirExpr::Var("xVar29".to_string())),
            },
        }],
        ..Default::default()
    };

    assert!(!cast_elision_pass(&mut func));
}

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
fn collapse_loop_exit_alias_return_rewrites_guarded_for_exit_copy() {
    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLe,
                lhs: Box::new(PreHirExpr::Var("len".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Goto("exit_zero".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("sum".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::For {
            init: Some(Box::new(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: PreHirExpr::Const(0, int(64)),
            })),
            cond: Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::SLt,
                lhs: Box::new(PreHirExpr::Var("i".to_string())),
                rhs: Box::new(PreHirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(PreHirExpr::Var("len".to_string())),
                }),
                ty: NirType::Bool,
            }),
            update: Some(Box::new(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("i".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, int(64))),
                    ty: int(64),
                },
            })),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("sum".to_string())),
                        rhs: Box::new(PreHirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("exit_sum".to_string()),
                    rhs: PreHirExpr::Var("sum".to_string()),
                },
            ]
            .into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("exit_sum".to_string()))),
        PreHirStmt::Label("exit_zero".to_string()),
        PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32)))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let PreHirStmt::For { body, .. } = &stmts[2] else {
        panic!("expected for loop");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[3],
        PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_non_alias_expression() {
    let mut stmts = vec![
        PreHirStmt::DoWhile {
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("exit_sum".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("sum".to_string())),
                    rhs: Box::new(PreHirExpr::Var("value".to_string())),
                    ty: int(32),
                },
            }]
            .into(),
            cond: PreHirExpr::Var("keep_going".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
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
fn rewrite_found_path_break_to_return_promotes_match_path() {
    // while { if (done) break; ... result = value; break; } return -1;
    // → match path becomes return result
    let mut stmts = vec![
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("done".to_string()),
                    then_body: vec![PreHirStmt::Break].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("miss".to_string()),
                    then_body: vec![PreHirStmt::Continue].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("result".to_string()),
                    rhs: PreHirExpr::Var("value".to_string()),
                },
                PreHirStmt::Break,
            ]
            .into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Const(-1, int(32)))),
    ];
    assert!(rewrite_found_path_break_to_return(&mut stmts));
    let PreHirStmt::While { body, .. } = &stmts[0] else {
        panic!("expected while");
    };
    assert!(
        matches!(
            body.last(),
            Some(PreHirStmt::Return(Some(PreHirExpr::Var(name)))) if name == "result"
        ),
        "match-path break must become return result, body={body:?}"
    );
    // Exit break must remain break.
    assert!(matches!(
        &body[0],
        PreHirStmt::If {
            then_body,
            ..
        } if matches!(then_body.as_slice(), [PreHirStmt::Break])
    ));
    assert!(matches!(
        &stmts[1],
        PreHirStmt::Return(Some(PreHirExpr::Const(-1, _)))
    ));
}

#[test]
fn recover_guarded_loop_tail_accumulator_return_rewrites_stale_temp_return() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("value".to_string()),
            then_body: vec![PreHirStmt::Goto("loop_body".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("bit".to_string()))),
        PreHirStmt::Label("loop_body".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(PreHirExpr::Var("value".to_string())),
                rhs: Box::new(PreHirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("count".to_string())),
                rhs: Box::new(PreHirExpr::Var("bit".to_string())),
                ty: int(32),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("value".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(PreHirExpr::Var("value".to_string())),
                rhs: Box::new(PreHirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
    ];

    assert!(recover_guarded_loop_tail_accumulator_returns(&mut stmts));
    assert_eq!(stmts.len(), 3);
    assert!(
        matches!(&stmts[1], PreHirStmt::While { cond: PreHirExpr::Var(name), .. } if name == "value")
    );
    let PreHirStmt::While { body, .. } = &stmts[1] else {
        panic!("expected while");
    };
    assert_eq!(body.len(), 3);
    assert!(matches!(
        &stmts[2],
        PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "count"
    ));
}

#[test]
fn recover_guarded_loop_tail_accumulator_return_rejects_without_cond_update() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("value".to_string()),
            then_body: vec![PreHirStmt::Goto("loop_body".to_string())].into(),
            else_body: Vec::new().into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("bit".to_string()))),
        PreHirStmt::Label("loop_body".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(PreHirExpr::Var("value".to_string())),
                rhs: Box::new(PreHirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("count".to_string())),
                rhs: Box::new(PreHirExpr::Var("bit".to_string())),
                ty: int(32),
            },
        },
    ];

    assert!(!recover_guarded_loop_tail_accumulator_returns(&mut stmts));
}

#[test]
fn eliminate_dead_temp_assigns_removes_dead_preserved_temp() {
    let mut stmts = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("uVar0".to_string()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("eax".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, int(32))),
            ty: int(32),
        },
    }];

    assert!(eliminate_dead_temp_assigns(
        &mut stmts,
        &["uVar0"].into_iter().collect::<HashSet<_>>(),
    ));
    assert!(stmts.is_empty());
}

#[test]
fn eliminate_dead_temp_assigns_removes_unused_reg_flag_artifact() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("reg".to_string()),
            rhs: PreHirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![
                    PreHirExpr::Var("rsp".to_string()),
                    PreHirExpr::Const(16, int(64)),
                ],
                ty: NirType::Bool,
            },
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("rsp".to_string()))),
    ];

    assert!(eliminate_dead_temp_assigns(&mut stmts, &HashSet::default()));
    assert_eq!(
        stmts,
        vec![PreHirStmt::Return(Some(PreHirExpr::Var("rsp".to_string())))]
    );
}

#[test]
fn eliminate_dead_temp_assigns_keeps_used_reg_flag_artifact() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("reg".to_string()),
            rhs: PreHirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![
                    PreHirExpr::Var("rsp".to_string()),
                    PreHirExpr::Const(16, int(64)),
                ],
                ty: NirType::Bool,
            },
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("reg".to_string()),
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(1, int(32))))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))].into(),
        },
    ];

    assert!(!eliminate_dead_temp_assigns(
        &mut stmts,
        &HashSet::default()
    ));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } if name == "reg"
    ));
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
    let referenced = super::utils::collect_referenced_labels(&stmts);
    for label in &referenced {
        assert!(
            defined.contains(label),
            "goto target {label:?} must still have a matching label after cleanup"
        );
    }
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp() {
    let mut func = PreHirFunction {
        name: "test_preserved_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("uVar0", 32)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_plain_temp_with_nontrivial_name() {
    let mut func = PreHirFunction {
        name: "test_plain_temp_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp_with_nontrivial_name() {
    let mut func = PreHirFunction {
        name: "test_preserved_named_register_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_keeps_used_plain_temp_with_nontrivial_name() {
    let mut func = PreHirFunction {
        name: "test_plain_temp_used".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("rcx".to_string())))],
        ..Default::default()
    };

    assert!(!prune_unused_temp_bindings(&mut func));
    assert_eq!(func.locals.len(), 1);
    assert_eq!(func.locals[0].name, "rcx");
}

#[test]
fn prune_unused_temp_bindings_keeps_side_effect_assignment_target() {
    let mut func = PreHirFunction {
        name: "test_side_effect_lhs_preserved".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("xVar30", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar30".to_string()),
            rhs: PreHirExpr::Call {
                target: "__pcodeop_294".to_string(),
                args: vec![],
                ty: int(64),
            },
        }],
        ..Default::default()
    };

    assert!(!prune_unused_temp_bindings(&mut func));
    assert_eq!(func.locals.len(), 1);
    assert_eq!(func.locals[0].name, "xVar30");
}

#[test]
fn inline_single_use_temps_does_not_cross_label_boundary() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::Label("loop_head".to_string()),
        PreHirStmt::If {
            cond: PreHirExpr::Var("xVar0".to_string()),
            then_body: vec![PreHirStmt::Goto("loop_head".to_string())].into(),
            else_body: Vec::new().into(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert!(matches!(
        &stmts[2],
        PreHirStmt::If {
            cond: PreHirExpr::Var(name),
            ..
        } if name == "xVar0"
    ));
}

#[test]
fn inline_single_use_temps_keeps_same_linear_segment_inline() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Const(1, int(32)),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar1".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("xVar0".to_string())),
                rhs: Box::new(PreHirExpr::Const(2, int(32))),
                ty: int(32),
            },
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 1);
    let PreHirStmt::Assign { rhs, .. } = &stmts[0] else {
        panic!("expected assignment");
    };
    assert!(!expr_contains_var(rhs, "xVar0"));
}

#[test]
fn fold_signed_eq_zero_or_slt_zero_to_sle() {
    use crate::arith::normalize_boolean_logic;
    let signed = NirType::Int {
        bits: 32,
        signed: true,
    };
    let x = PreHirExpr::Var("param_18".into());
    let expr = PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalOr,
        lhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs: Box::new(x.clone()),
            rhs: Box::new(PreHirExpr::Const(0, signed.clone())),
            ty: NirType::Bool,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(x.clone()),
            rhs: Box::new(PreHirExpr::Const(0, signed.clone())),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    };
    let out = normalize_boolean_logic(&expr).expect("fold");
    match out {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => {
            assert!(matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == "param_18"));
            assert!(matches!(rhs.as_ref(), PreHirExpr::Const(0, _)));
        }
        other => panic!("expected SLe, got {other:?}"),
    }
}

/// power-class: pure copy into predicate with two uses of the temp.
#[test]
fn inline_pure_copy_into_multi_use_predicate() {
    let signed = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("iVar36".to_string()),
            rhs: PreHirExpr::Var("param_18".to_string()),
        },
        PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(PreHirExpr::Var("iVar36".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0, signed.clone())),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
                    lhs: Box::new(PreHirExpr::Var("iVar36".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0, signed.clone())),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Break].into(),
            else_body: vec![].into(),
        },
    ];
    assert!(
        inline_single_use_temps(&mut stmts, &HashSet::default()),
        "pure copy into multi-use predicate should inline: {stmts:?}"
    );
    assert_eq!(stmts.len(), 1, "temp assign removed: {stmts:?}");
    match &stmts[0] {
        PreHirStmt::If { cond, .. } => {
            assert!(
                !expr_contains_var(cond, "iVar36"),
                "iVar36 should be substituted: {cond:?}"
            );
            assert!(
                expr_contains_var(cond, "param_18"),
                "param_18 should appear: {cond:?}"
            );
        }
        other => panic!("expected If, got {other:?}"),
    }
}

/// power-class: t = a; t = t * t; a = t  →  a = a * a
#[test]
fn collapse_temp_self_square_into_inplace() {
    let ty = NirType::Int {
        bits: 64,
        signed: true,
    };
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar23".to_string()),
            rhs: PreHirExpr::Var("param_10".to_string()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar23".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(PreHirExpr::Var("xVar23".to_string())),
                rhs: Box::new(PreHirExpr::Var("xVar23".to_string())),
                ty: ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("param_10".to_string()),
            rhs: PreHirExpr::Var("xVar23".to_string()),
        },
    ];
    assert!(
        collapse_temp_self_square_assigns(&mut stmts),
        "expected square collapse: {stmts:?}"
    );
    assert_eq!(stmts.len(), 1, "{stmts:?}");
    match &stmts[0] {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst),
            rhs:
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs,
                    rhs,
                    ..
                },
        } => {
            assert_eq!(dst, "param_10");
            assert!(matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == "param_10"));
            assert!(matches!(rhs.as_ref(), PreHirExpr::Var(n) if n == "param_10"));
        }
        other => panic!("expected param_10 = param_10 * param_10, got {other:?}"),
    }
}

#[test]
fn inline_single_use_temps_preserves_rhs_across_dependency_redefinition() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(PreHirExpr::Var("address_reg".to_string())),
                rhs: Box::new(PreHirExpr::Const(4, int(64))),
                ty: int(64),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("address_reg".to_string()),
            rhs: PreHirExpr::Var("base".to_string()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("address_reg".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("address_reg".to_string())),
                rhs: Box::new(PreHirExpr::Var("xVar0".to_string())),
                ty: int(64),
            },
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 3);
    let PreHirStmt::Assign { rhs, .. } = &stmts[2] else {
        panic!("expected address assignment");
    };
    assert!(expr_contains_var(rhs, "xVar0"));
}

#[test]
fn inline_single_use_temps_inlines_flag_intrinsic_into_predicate() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![
                    PreHirExpr::Var("param_1".to_string()),
                    PreHirExpr::Const(1, int(32)),
                ],
                ty: NirType::Bool,
            },
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("xVar0".to_string()),
            then_body: Vec::new().into(),
            else_body: Vec::new().into(),
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 1);
    let PreHirStmt::If { cond, .. } = &stmts[0] else {
        panic!("expected if");
    };
    assert!(matches!(cond, PreHirExpr::Call { target, .. } if target == "__sborrow"));
}

#[test]
fn inline_loop_condition_trailing_temps_substitutes_condition_chain() {
    let mut func = PreHirFunction {
        name: "test_loop_cond_inline".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![PreHirStmt::DoWhile {
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("sum".to_string())),
                        rhs: Box::new(PreHirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar38".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(PreHirExpr::Var("ptr".to_string())),
                        rhs: Box::new(PreHirExpr::Var("end".to_string())),
                        ty: int(64),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar39".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("xVar38".to_string())),
                        rhs: Box::new(PreHirExpr::Const(0, int(64))),
                        ty: NirType::Bool,
                    },
                },
            ]
            .into(),
            cond: PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                expr: Box::new(PreHirExpr::Var("xVar39".to_string())),
                ty: NirType::Bool,
            },
        }],
        ..Default::default()
    };

    assert!(inline_loop_condition_trailing_temps(&mut func,));
    let PreHirStmt::DoWhile { body, cond } = &func.body[0] else {
        panic!("expected do-while");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(
        cond,
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } if matches!(
            expr.as_ref(),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs,
                ..
            } if matches!(
                lhs.as_ref(),
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    ..
                }
            )
        )
    ));
}

#[test]
fn inline_loop_header_temp_promotes_single_use_load_into_condition() {
    let load = PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Var("table_entry".to_string())),
        ty: int(16),
    };
    let mut func = PreHirFunction {
        name: "test_loop_header_temp".to_string(),
        body: vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar90".to_string()),
                    rhs: load.clone(),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("xVar90".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1, int(16))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Break].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Expr(PreHirExpr::Var("body_value".to_string())),
            ]
            .into(),
        }],
        ..Default::default()
    };

    assert!(inline_loop_condition_trailing_temps(&mut func));
    let PreHirStmt::While { cond, body } = &func.body[0] else {
        panic!("expected while");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(
        cond,
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } if matches!(
            expr.as_ref(),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs,
                ..
            } if lhs.as_ref() == &load
        )
    ));
}

#[test]
fn inline_loop_header_temp_resolves_pcode_assignment_chain() {
    let mut func = PreHirFunction {
        name: "test_loop_header_chain".to_string(),
        body: vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("iVar55".to_string()),
                    rhs: PreHirExpr::Var("index".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("iVar55".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Shl,
                        lhs: Box::new(PreHirExpr::Var("iVar55".to_string())),
                        rhs: Box::new(PreHirExpr::Const(4, int(32))),
                        ty: int(64),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar86".to_string()),
                    rhs: PreHirExpr::Const(300192, int(64)),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar86".to_string()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(PreHirExpr::Var("iVar55".to_string())),
                            rhs: Box::new(PreHirExpr::Const(300192, int(64))),
                            ty: int(64),
                        }),
                        ty: int(16),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar90".to_string()),
                    rhs: PreHirExpr::Var("xVar86".to_string()),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("xVar90".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1, int(16))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Break].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Expr(PreHirExpr::Var("body_value".to_string())),
            ]
            .into(),
        }],
        ..Default::default()
    };

    assert!(inline_loop_condition_trailing_temps(&mut func));
    let PreHirStmt::While { cond, body } = &func.body[0] else {
        panic!("expected while");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(
        cond,
        PreHirExpr::Unary { expr, .. } if matches!(
            expr.as_ref(),
            PreHirExpr::Binary { lhs, .. }
                if matches!(lhs.as_ref(), PreHirExpr::Load { .. })
        )
    ));
    assert!(!expr_mentions_var(cond, "iVar55"));
    assert!(!expr_mentions_var(cond, "xVar86"));
    assert!(!expr_mentions_var(cond, "xVar90"));
}

#[test]
fn inline_loop_header_temp_keeps_value_read_after_guard() {
    let mut func = PreHirFunction {
        name: "test_loop_header_live_temp".to_string(),
        body: vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar90".to_string()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("table_entry".to_string())),
                        ty: int(16),
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("xVar90".to_string()),
                    then_body: vec![PreHirStmt::Break].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Expr(PreHirExpr::Var("xVar90".to_string())),
            ]
            .into(),
        }],
        ..Default::default()
    };

    assert!(!inline_loop_condition_trailing_temps(&mut func));
    let PreHirStmt::While { cond, body } = &func.body[0] else {
        panic!("expected while");
    };
    assert!(matches!(cond, PreHirExpr::Const(1, _)));
    assert_eq!(body.len(), 3);
}

#[test]
fn inline_loop_header_temp_keeps_side_effecting_call() {
    let mut func = PreHirFunction {
        name: "test_loop_header_call".to_string(),
        body: vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar90".to_string()),
                    rhs: PreHirExpr::Call {
                        target: "next_item".to_string(),
                        args: Vec::new(),
                        ty: int(64),
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("xVar90".to_string()),
                    then_body: vec![PreHirStmt::Break].into(),
                    else_body: Vec::new().into(),
                },
            ]
            .into(),
        }],
        ..Default::default()
    };

    assert!(!inline_loop_condition_trailing_temps(&mut func));
    let PreHirStmt::While { cond, body } = &func.body[0] else {
        panic!("expected while");
    };
    assert!(matches!(cond, PreHirExpr::Const(1, _)));
    assert_eq!(body.len(), 2);
}

#[test]
fn inline_single_use_temps_keeps_unknown_call_out_of_predicate() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Call {
                target: "unknown_helper".to_string(),
                args: vec![PreHirExpr::Var("param_1".to_string())],
                ty: int(32),
            },
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("xVar0".to_string()),
            then_body: Vec::new().into(),
            else_body: Vec::new().into(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 2);
}

#[test]
fn switch_norm_folds_range_check_guard() {
    let mut func = PreHirFunction {
        name: "test_switch_norm".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::Lt,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(5, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Switch {
                expr: PreHirExpr::Var("x".to_string()),
                cases: vec![PreHirSwitchCase {
                    values: vec![1],
                    body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(10, int(32))))].into(),
                }],
                default: Vec::new().into(),
            }]
            .into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(20, int(32))))].into(),
        }],
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_switch_norm_pass(&mut func));
    assert_eq!(func.body.len(), 1);
    let PreHirStmt::Switch {
        expr,
        cases,
        default,
    } = &func.body[0]
    else {
        panic!("expected switch statement");
    };
    assert_eq!(expr, &PreHirExpr::Var("x".to_string()));
    assert_eq!(cases.len(), 1);
    assert_eq!(default.len(), 1);
    assert!(matches!(
        &default[0],
        PreHirStmt::Return(Some(PreHirExpr::Const(20, _)))
    ));
}

#[test]
fn constant_ptr_recovery_recovers_symbolic_addresses() {
    use crate::memory::apply_constant_ptr_recovery_pass;
    use crate::pipeline::{GLOBAL_SYMBOL_CONTEXT, GlobalSymbolContext};
    use std::collections::HashMap;

    let mut names = HashMap::default();
    names.insert(0x140003000, "g_data".to_string());
    names.insert(0x140004000, "g_exact".to_string());

    let mut sizes = HashMap::default();
    sizes.insert(0x140003000, 100);
    sizes.insert(0x140004000, 10);

    let context = GlobalSymbolContext { names, sizes };
    GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(context);
    });

    let mut func = PreHirFunction {
        name: "test_constant_ptr".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            // Exact match
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p1".to_string()),
                rhs: PreHirExpr::Const(0x140004000, int(64)),
            },
            // Inside bounds (offset 8)
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p2".to_string()),
                rhs: PreHirExpr::Const(0x140003008, int(64)),
            },
            // Outside any bounds / no match
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p3".to_string()),
                rhs: PreHirExpr::Const(0x140005000, int(64)),
            },
        ],
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_constant_ptr_recovery_pass(&mut func));

    GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });

    assert_eq!(func.body.len(), 3);

    // Assert exact match: AddressOfGlobal("g_exact")
    let PreHirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    assert_eq!(rhs1, &PreHirExpr::AddressOfGlobal("g_exact".to_string()));

    // Assert offset match: PtrOffset { base: AddressOfGlobal("g_data"), offset: 8 }
    let PreHirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    assert!(
        matches!(rhs2, PreHirExpr::PtrOffset { base, offset: 8 } if matches!(base.as_ref(), PreHirExpr::AddressOfGlobal(name) if name == "g_data"))
    );

    // Assert no match: stays Const(0x140005000)
    let PreHirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    assert_eq!(rhs3, &PreHirExpr::Const(0x140005000, int(64)));
}

#[test]
fn condexe_folding_merges_sequential_siblings() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = PreHirFunction {
        name: "test_condexe_siblings".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("a".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(1, int(32)),
                }]
                .into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("a".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("y".to_string()),
                    rhs: PreHirExpr::Const(2, int(32)),
                }]
                .into(),
                else_body: Vec::new().into(),
            },
        ],
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_condexe_folding_pass(&mut func.body));
    assert_eq!(func.body.len(), 1);
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = &func.body[0]
    else {
        panic!();
    };
    assert_eq!(cond, &PreHirExpr::Var("a".to_string()));
    assert_eq!(then_body.len(), 2);
    assert!(else_body.is_empty());
}

#[test]
fn condexe_folding_merges_nested_ifs() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = PreHirFunction {
        name: "test_condexe_nested".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::If {
            cond: PreHirExpr::Var("a".to_string()),
            then_body: vec![PreHirStmt::If {
                cond: PreHirExpr::Var("a".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(1, int(32)),
                }]
                .into(),
                else_body: Vec::new().into(),
            }]
            .into(),
            else_body: Vec::new().into(),
        }],
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_condexe_folding_pass(&mut func.body));
    assert_eq!(func.body.len(), 1);
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = &func.body[0]
    else {
        panic!();
    };
    assert_eq!(cond, &PreHirExpr::Var("a".to_string()));
    assert_eq!(then_body.len(), 1);
    assert!(else_body.is_empty());

    let PreHirStmt::Assign { lhs, .. } = &then_body[0] else {
        panic!();
    };
    assert_eq!(lhs, &PreHirLValue::Var("x".to_string()));
}

#[test]
fn condexe_folding_preserves_safety_on_assignment() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = PreHirFunction {
        name: "test_condexe_safety".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("a".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("a".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                }]
                .into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("a".to_string()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(1, int(32)),
                }]
                .into(),
                else_body: Vec::new().into(),
            },
        ],
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    // Should not fold/merge because "a" is modified in the first then_body
    assert!(!apply_condexe_folding_pass(&mut func.body));
    assert_eq!(func.body.len(), 2);
}

#[test]
fn deindirect_resolves_const_address_to_symbol() {
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "target_func".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(0x401000),
                symbol: "target_func".to_string(),
                provenance: CallTargetProvenance::Reference,
                edge_kind: CallEdgeKind::Reference,
                confidence: 128,
            },
            prototype: PrototypeSummary {
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: None,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown],
                param_surface_type_names: vec![None],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 160,
            },
        },
    );

    let mut func = PreHirFunction {
        name: "test_deindirect_const".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![
                PreHirExpr::Const(0x401000, int(64)),
                PreHirExpr::Var("param_1".to_string()),
            ],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression");
    };
    assert_eq!(target, "target_func");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], PreHirExpr::Var("param_1".to_string()));
}

#[test]
fn deindirect_resolves_var_initializer_to_symbol() {
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "target_func".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(0x401000),
                symbol: "target_func".to_string(),
                provenance: CallTargetProvenance::Reference,
                edge_kind: CallEdgeKind::Reference,
                confidence: 128,
            },
            prototype: PrototypeSummary {
                min_arity: 0,
                max_arity: 0,
                locked_exact_arity: None,
                return_lattice: NirType::Unknown,
                param_lattices: vec![],
                param_surface_type_names: vec![],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 160,
            },
        },
    );

    let mut func = PreHirFunction {
        name: "test_deindirect_var".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "fn_ptr".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: Some(PreHirExpr::Const(0x401000, int(64))),
        }],
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![PreHirExpr::Var("fn_ptr".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression");
    };
    assert_eq!(target, "target_func");
    assert!(args.is_empty());
}

#[test]
fn deindirect_resolves_iat_load_to_symbol() {
    // Regression test: x86-64 Sleigh emits `CALL qword ptr [IAT_addr]` as
    //   Copy unique <- [ram:IAT_addr]   (Load from IAT slot)
    //   CallInd unique
    // which the builder lowers to:
    //   __fission_callind_opaque(Load { ptr: Const(IAT_addr) }, ...)
    // The deindirect pass must resolve this to a named direct call.
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    const IAT_SLOT_ADDR: u64 = 0x1400082c8;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "InitializeCriticalSection".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(IAT_SLOT_ADDR),
                symbol: "InitializeCriticalSection".to_string(),
                provenance: CallTargetProvenance::Import,
                edge_kind: CallEdgeKind::Import,
                confidence: 255,
            },
            prototype: PrototypeSummary {
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: None,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown],
                param_surface_type_names: vec![None],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 255,
            },
        },
    );

    let iat_load = PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Const(IAT_SLOT_ADDR as i64, int(64))),
        ty: NirType::Unknown,
    };

    let mut func = PreHirFunction {
        name: "test_deindirect_iat_load".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![iat_load, PreHirExpr::Var("param_1".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(
        apply_deindirect_pass(&mut func),
        "deindirect pass must rewrite IAT-load indirect call"
    );
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression after deindirect");
    };
    assert_eq!(
        target, "InitializeCriticalSection",
        "call target must be resolved from IAT slot"
    );
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], PreHirExpr::Var("param_1".to_string()));
}

// Dual-layer C rendering of CALLIND opaque targets lives in fission-pcode render
// (ADR 0011). midend-core::format_expr_key is a deterministic key/diagnostic printer only.
#[test]
fn diagnostic_print_expr_renders_callind_opaque_call_shape() {
    use fission_midend_prehir::util::format_expr_key;

    let expr = PreHirExpr::Call {
        target: "__fission_callind_opaque".to_string(),
        args: vec![
            PreHirExpr::Var("fn_ptr".to_string()),
            PreHirExpr::Var("arg1".to_string()),
            PreHirExpr::Var("arg2".to_string()),
        ],
        ty: NirType::Unknown,
    };

    let rendered = format_expr_key(&expr);
    assert_eq!(rendered, "__fission_callind_opaque(fn_ptr, arg1, arg2)");
}

#[test]
fn subvar_trim_eliminates_redundant_casts() {
    use crate::cleanup::apply_subvar_trim_pass;

    // We want to test two cases:
    // Case 1: (u8)(u32)b  where b: u8
    // Case 2: (u8)(y & 0xff)
    let u8_ty = int(8);
    let u32_ty = int(32);

    let mut func = PreHirFunction {
        name: "test_subvar_trim".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![
            PreHirBinding {
                name: "b".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            PreHirBinding {
                name: "y".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "z".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            PreHirBinding {
                name: "res1".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "res2".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // x = (u32)b
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("b".to_string())),
                },
            },
            // res1 = (u8)x
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("res1".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("x".to_string())),
                },
            },
            // z = y & 0xff
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("z".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("y".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0xff, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            },
            // res2 = (u8)z
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("res2".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("z".to_string())),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_subvar_trim_pass(&mut func));

    // Expected:
    // res1 = b (cast elided and replaced with original byte variable b)
    // res2 = (u8)y (bitwise AND elided, cast moved directly to y)
    let PreHirStmt::Assign { rhs: rhs1, .. } = &func.body[1] else {
        panic!();
    };
    assert_eq!(rhs1, &PreHirExpr::Var("b".to_string()));

    let PreHirStmt::Assign { rhs: rhs2, .. } = &func.body[3] else {
        panic!();
    };
    if let PreHirExpr::Cast { ty, expr } = rhs2 {
        assert_eq!(ty, &u8_ty);
        assert_eq!(expr.as_ref(), &PreHirExpr::Var("y".to_string()));
    } else {
        panic!("expected cast of y, got {:?}", rhs2);
    }
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

#[test]
fn collapse_trivial_assign_returns_folds_eax_const() {
    use super::utils::is_abi_return_register_name;
    assert!(is_abi_return_register_name("eax"));
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("eax".to_string()),
            rhs: PreHirExpr::Const(7, int(32)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Return(Some(PreHirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_eax_const_even_if_preserved() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("eax".to_string()),
            rhs: PreHirExpr::Const(7, int(32)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &["eax"].into_iter().collect::<HashSet<_>>(),
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Return(Some(PreHirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_rax_param_add() {
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt};
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("rax".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("param_1".to_string())),
                rhs: Box::new(PreHirExpr::Const(
                    5,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            },
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("rax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &crate::HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            ..
        })) => {}
        other => panic!("expected folded return add, got {other:?}"),
    }
}

fn rescue_undeclared_bindings_declares_stack_local_names() {
    let mut func = PreHirFunction {
        name: "f".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_0".to_string()),
                rhs: PreHirExpr::Var("param_2".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("local_0".to_string()))),
        ],
        ..Default::default()
    };
    assert!(rescue_undeclared_bindings(&mut func));
    assert!(
        func.locals.iter().any(|b| b.name == "local_0"),
        "local_0 must be declared: {:?}",
        func.locals
    );
}

#[test]
fn prune_unused_dead_local_bindings_then_rescue_redeclares_write_only_stack_homes() {
    // `local_18 = param_3` with no RHS uses: `prune_unused_dead_local_bindings`
    // alone drops the (now-dead) binding -- keeping it declared indefinitely
    // instead regressed real structuring tests (a stack-resident shadow of a
    // variable heritage had already promoted to a register stuck around long
    // enough to confuse loop-condition folding elsewhere). The matrix_multiply
    // undeclared-identifier bug this was meant to fix is instead closed by
    // `rescue_undeclared_bindings` re-declaring any still-referenced-but-
    // undeclared name at pipeline end (`run_stage_cleanup`'s final pass) --
    // by which point the fold/prune passes that this exception broke have
    // already run, so it can't interfere with them.
    let mut func = PreHirFunction {
        name: "f".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_3".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(2)),
            initializer: None,
        }],
        locals: vec![PreHirBinding {
            name: "local_18".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x18)),
            initializer: None,
        }],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_18".to_string()),
                rhs: PreHirExpr::Var("param_3".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ))),
        ],
        ..Default::default()
    };
    prune_unused_dead_local_bindings(&mut func);
    assert!(
        !func.locals.iter().any(|b| b.name == "local_18"),
        "dead write-only local_18 should be pruned here: {:?}",
        func.locals
    );
    assert!(
        matches!(
            func.body.first(),
            Some(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            }) if name == "local_18"
        ),
        "the (now-orphaned) assign itself must survive pruning -- rescue \
         re-declares it below: {:?}",
        func.body
    );

    rescue_undeclared_bindings(&mut func);
    assert!(
        func.locals.iter().any(|b| b.name == "local_18"),
        "rescue must re-declare local_18 so it's not an undeclared identifier: {:?}",
        func.locals
    );
}

#[test]
fn canonicalize_orphaned_stack_slot_name_renames_binding_and_uses() {
    let mut claimant = stack_binding("local_8_7", -8);
    claimant.initializer = Some(PreHirExpr::Var("local_8_7".to_string()));
    let mut func = PreHirFunction {
        locals: vec![claimant],
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "local_8_7".to_string(),
        )))],
        ..Default::default()
    };

    assert!(canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(func.locals[0].name, "local_8");
    assert!(matches!(
        func.locals[0].initializer.as_ref(),
        Some(PreHirExpr::Var(name)) if name == "local_8"
    ));
    assert!(matches!(
        func.body.as_slice(),
        [PreHirStmt::Return(Some(PreHirExpr::Var(name)))] if name == "local_8"
    ));
}

#[test]
fn canonicalize_orphaned_stack_slot_name_preserves_live_base_collision() {
    let mut func = PreHirFunction {
        locals: vec![stack_binding("local_8", -8), stack_binding("local_8_7", -8)],
        ..Default::default()
    };

    assert!(!canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["local_8", "local_8_7"]
    );
}

#[test]
fn canonicalize_orphaned_stack_slot_name_preserves_ambiguous_claimants() {
    let mut func = PreHirFunction {
        locals: vec![
            stack_binding("local_8_1", -8),
            stack_binding("local_8_2", -8),
        ],
        ..Default::default()
    };

    assert!(!canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["local_8_1", "local_8_2"]
    );
}
