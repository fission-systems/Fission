use super::*;

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

fn exit_alias_loop(prefix: Vec<PreHirStmt>, suffix: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    // do { <prefix>; exit_sum = sum; <suffix> } while (keep_going); return exit_sum;
    let mut body = prefix;
    body.push(PreHirStmt::Assign {
        lhs: PreHirLValue::Var("exit_sum".to_string()),
        rhs: PreHirExpr::Var("sum".to_string()),
    });
    body.extend(suffix);
    vec![
        PreHirStmt::DoWhile {
            body: body.into(),
            cond: PreHirExpr::Var("keep_going".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("exit_sum".to_string()))),
    ]
}

fn bump_sum() -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Var("sum".to_string()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("sum".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, int(32))),
            ty: int(32),
        },
    }
}

fn early_exit_before_copy(exit: PreHirStmt) -> PreHirStmt {
    // if (done) <exit>;
    PreHirStmt::If {
        cond: PreHirExpr::Var("done".to_string()),
        then_body: vec![exit].into(),
        else_body: vec![].into(),
    }
}

#[test]
fn collapse_loop_exit_alias_return_rejects_an_exit_before_the_copy() {
    // `sum` changes, then `break` leaves before `exit_sum = sum` ran: exit_sum
    // still holds the previous iteration's `sum`, so `return sum` is wrong.
    for exit in [
        PreHirStmt::Break,
        PreHirStmt::Continue,
        PreHirStmt::Goto("elsewhere".to_string()),
    ] {
        let mut stmts = exit_alias_loop(vec![bump_sum(), early_exit_before_copy(exit)], vec![]);
        let before = stmts.clone();
        assert!(!collapse_loop_exit_alias_returns(&mut stmts));
        assert_eq!(stmts, before);
    }
}

#[test]
fn collapse_loop_exit_alias_return_rejects_a_label_that_can_skip_the_copy() {
    // A `goto` from outside to `entry` lands after the copy.
    let mut stmts = exit_alias_loop(vec![], vec![PreHirStmt::Label("entry".to_string())]);
    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn collapse_loop_exit_alias_return_allows_a_return_before_the_copy() {
    // `return` leaves the function; it never reaches the `return exit_sum`.
    let mut stmts = exit_alias_loop(
        vec![
            bump_sum(),
            early_exit_before_copy(PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))),
        ],
        vec![],
    );
    assert!(collapse_loop_exit_alias_returns(&mut stmts));
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
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
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
    assert_eq!(stmts.len(), 4);
    assert!(
        matches!(&stmts[2], PreHirStmt::While { cond: PreHirExpr::Var(name), .. } if name == "value")
    );
    let PreHirStmt::While { body, .. } = &stmts[2] else {
        panic!("expected while");
    };
    assert_eq!(body.len(), 3);
    assert!(matches!(
        &stmts[3],
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
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
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
fn rewrite_found_path_break_preserves_switch_and_inner_loop_breaks() {
    let mut stmts = vec![
        PreHirStmt::While {
            cond: PreHirExpr::Const(1, int(32)),
            body: vec![
                PreHirStmt::Switch {
                    expr: PreHirExpr::Var("tag".to_string()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: vec![
                            PreHirStmt::Assign {
                                lhs: PreHirLValue::Var("result".to_string()),
                                rhs: PreHirExpr::Var("val".to_string()),
                            },
                            PreHirStmt::Break,
                        ]
                        .into(),
                    }],
                    default: Vec::new().into(),
                },
                PreHirStmt::While {
                    cond: PreHirExpr::Var("inner_cond".to_string()),
                    body: vec![
                        PreHirStmt::Assign {
                            lhs: PreHirLValue::Var("inner_res".to_string()),
                            rhs: PreHirExpr::Var("inner_val".to_string()),
                        },
                        PreHirStmt::Break,
                    ]
                    .into(),
                },
            ]
            .into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Const(-1, int(32)))),
    ];

    assert!(!rewrite_found_path_break_to_return(&mut stmts));
    let PreHirStmt::While { body, .. } = &stmts[0] else {
        panic!("expected while");
    };
    let PreHirStmt::Switch { cases, .. } = &body[0] else {
        panic!("expected switch");
    };
    assert!(matches!(cases[0].body.last(), Some(PreHirStmt::Break)));
    let PreHirStmt::While { body: inner, .. } = &body[1] else {
        panic!("expected inner while");
    };
    assert!(matches!(inner.last(), Some(PreHirStmt::Break)));
}

#[test]
fn recover_guarded_loop_tail_preserves_label_with_other_incoming_jumps() {
    let mut stmts = vec![
        PreHirStmt::Goto("loop_body".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
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
    assert!(
        stmts
            .iter()
            .any(|s| matches!(s, PreHirStmt::Label(l) if l == "loop_body"))
    );
}

#[test]
fn recover_guarded_loop_tail_rejects_mismatched_stale_temp() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("count".to_string()),
            rhs: PreHirExpr::Const(0, int(32)),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("bit".to_string()),
            rhs: PreHirExpr::Const(999, int(32)),
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

    assert!(!recover_guarded_loop_tail_accumulator_returns(&mut stmts));
}
