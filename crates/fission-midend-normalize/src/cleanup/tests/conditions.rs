use super::*;

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
            origin: None,
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
fn iblock_phi_elimination_rejects_side_effecting_condition() {
    use crate::cleanup::apply_iblock_phi_elimination;

    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Call {
                origin: None,
                target: "has_flag".to_string(),
                args: Vec::new(),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(1, int(32)),
            }]
            .into(),
            else_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(2, int(32)),
            }]
            .into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string()))),
    ];

    // Side-effecting condition must not be duplicated into select phi
    assert!(!apply_iblock_phi_elimination(&mut stmts));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(&stmts[0], PreHirStmt::If { .. }));
}

#[test]
fn iblock_phi_elimination_stops_substitution_at_reassignment() {
    use crate::cleanup::apply_iblock_phi_elimination;

    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(1, int(32)),
            }]
            .into(),
            else_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(2, int(32)),
            }]
            .into(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("y".to_string()),
            rhs: PreHirExpr::Var("x".to_string()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_string()),
            rhs: PreHirExpr::Const(100, int(32)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string()))),
    ];

    assert!(apply_iblock_phi_elimination(&mut stmts));
    // Diamond if is eliminated; stmts.len() becomes 3: y = select, x = 100, return x
    assert_eq!(stmts.len(), 3);
    // y should receive the select expression
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(y),
            rhs: PreHirExpr::Select { .. },
        } if y == "y"
    ));
    // x = 100 remains
    assert!(matches!(
        &stmts[1],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(x),
            rhs: PreHirExpr::Const(100, _),
        } if x == "x"
    ));
    // return x must NOT be corrupted with the select expression!
    assert!(matches!(
        &stmts[2],
        PreHirStmt::Return(Some(PreHirExpr::Var(x))) if x == "x"
    ));
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

#[test]
fn condexe_folding_merges_sequential_siblings() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = PreHirFunction {
        variadic_fixed_arity: None,
        name: "test_condexe_siblings".to_string(),
        int_param_offsets: Vec::new(),
        float_param_offsets: Vec::new(),
        float_shares_int_slots: false,
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
        is_big_endian: false,
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
        variadic_fixed_arity: None,
        name: "test_condexe_nested".to_string(),
        int_param_offsets: Vec::new(),
        float_param_offsets: Vec::new(),
        float_shares_int_slots: false,
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
        is_big_endian: false,
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
        variadic_fixed_arity: None,
        name: "test_condexe_safety".to_string(),
        int_param_offsets: Vec::new(),
        float_param_offsets: Vec::new(),
        float_shares_int_slots: false,
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
        is_big_endian: false,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    // Should not fold/merge because "a" is modified in the first then_body
    assert!(!apply_condexe_folding_pass(&mut func.body));
    assert_eq!(func.body.len(), 2);
}

fn if_then(cond: PreHirExpr, then_body: Vec<PreHirStmt>) -> PreHirStmt {
    PreHirStmt::If {
        cond,
        then_body: then_body.into(),
        else_body: Vec::new().into(),
    }
}

fn assign_var(name: &str, value: i64) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name.to_string()),
        rhs: PreHirExpr::Const(value, int(32)),
    }
}

#[test]
fn condexe_folding_keeps_two_evaluations_of_a_condition_that_calls() {
    use crate::cleanup::apply_condexe_folding_pass;

    // `if (f()) x = 1; if (f()) y = 2;` calls `f` twice, and may take a
    // different branch the second time. Merged, it calls `f` once.
    let call = || PreHirExpr::Call {
        origin: None,
        target: "f".to_string(),
        args: Vec::new(),
        ty: int(32),
    };
    let mut body = vec![
        if_then(call(), vec![assign_var("x", 1)]),
        if_then(call(), vec![assign_var("y", 2)]),
    ];

    assert!(!apply_condexe_folding_pass(&mut body));
    assert_eq!(body.len(), 2);
}

#[test]
fn condexe_folding_keeps_a_condition_that_reads_memory_the_first_body_writes() {
    use crate::cleanup::apply_condexe_folding_pass;

    // `if (*p) *p = 0; if (*p) y = 2;` -- the second test sees the store.
    let load = || PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Var("p".to_string())),
        ty: int(32),
    };
    let store = PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(PreHirExpr::Var("p".to_string())),
            ty: int(32),
        },
        rhs: PreHirExpr::Const(0, int(32)),
    };
    let mut body = vec![
        if_then(load(), vec![store]),
        if_then(load(), vec![assign_var("y", 2)]),
    ];

    assert!(!apply_condexe_folding_pass(&mut body));
    assert_eq!(body.len(), 2);
}

#[test]
fn condexe_folding_still_merges_a_pure_condition_with_an_unrelated_assignment() {
    use crate::cleanup::apply_condexe_folding_pass;

    // Positive control for the two tests above: a side-effect-free condition
    // whose variable the first body never touches is still merged, so the
    // guard is not just "never fold".
    let unrelated_store = PreHirStmt::Assign {
        lhs: PreHirLValue::Var("x".to_string()),
        rhs: PreHirExpr::Const(1, int(32)),
    };
    let mut body = vec![
        if_then(PreHirExpr::Var("a".to_string()), vec![unrelated_store]),
        if_then(PreHirExpr::Var("a".to_string()), vec![assign_var("y", 2)]),
    ];

    assert!(apply_condexe_folding_pass(&mut body));
    assert_eq!(body.len(), 1);
}

fn if_else(cond: PreHirExpr, then_body: Vec<PreHirStmt>, else_body: Vec<PreHirStmt>) -> PreHirStmt {
    PreHirStmt::If {
        cond,
        then_body: then_body.into(),
        else_body: else_body.into(),
    }
}

fn load_through(pointer: &str) -> PreHirExpr {
    PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Var(pointer.to_string())),
        ty: int(32),
    }
}

fn store_through(pointer: &str, value: i64) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(PreHirExpr::Var(pointer.to_string())),
            ty: int(32),
        },
        rhs: PreHirExpr::Const(value, int(32)),
    }
}

fn call_to(target: &str) -> PreHirExpr {
    PreHirExpr::Call {
        origin: None,
        target: target.to_string(),
        args: Vec::new(),
        ty: int(32),
    }
}

/// `if (cond) { stop(); return; } else { <between>; if (cond) y = 2; }`, and
/// whether the pass left the inner test where it was.
fn inner_test_survives(cond: impl Fn() -> PreHirExpr, between: Vec<PreHirStmt>) -> bool {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut else_body = between;
    let inner_at = else_body.len();
    else_body.push(if_then(cond(), vec![assign_var("y", 2)]));
    let mut body = vec![if_else(
        cond(),
        vec![PreHirStmt::Expr(call_to("stop")), PreHirStmt::Return(None)],
        else_body,
    )];
    apply_condexe_folding_pass(&mut body);
    let PreHirStmt::If { else_body, .. } = &body[0] else {
        panic!("the outer test is gone");
    };
    matches!(else_body.get(inner_at), Some(PreHirStmt::If { .. }))
}

#[test]
fn condexe_folding_drops_a_test_already_decided_on_the_way_in() {
    // Positive control for the tests below: with nothing in between that
    // could change `a`, the inner `if (a)` sits where `a` is known false.
    assert!(!inner_test_survives(
        || PreHirExpr::Var("a".to_string()),
        vec![assign_var("x", 1)]
    ));
    // The same for a condition that reads memory nothing has written since.
    assert!(!inner_test_survives(
        || load_through("p"),
        vec![assign_var("x", 1)]
    ));
}

#[test]
fn condexe_folding_keeps_a_test_of_memory_written_since_the_first() {
    // `if (*p) { stop(); return; } *p = 0; if (*p) y = 2;` -- one expression,
    // two values. The shape is `argc--; if (argc == 0) usage();` behind an
    // earlier `if (argc == 0) usage();` on a frame slot.
    assert!(inner_test_survives(
        || load_through("p"),
        vec![store_through("p", 0)]
    ));
}

#[test]
fn condexe_folding_keeps_a_test_of_memory_behind_a_call() {
    // The callee may have written what the condition reads.
    assert!(inner_test_survives(
        || load_through("p"),
        vec![PreHirStmt::Expr(call_to("g"))]
    ));
}

#[test]
fn condexe_folding_keeps_a_test_behind_a_label() {
    // A `goto` from outside the arm reaches the label without having taken
    // the branch, so nothing is known about `a` there.
    assert!(inner_test_survives(
        || PreHirExpr::Var("a".to_string()),
        vec![PreHirStmt::Label("L".to_string())]
    ));
    // A label nested in a statement is one too: control leaves that
    // statement having come in at the label.
    assert!(inner_test_survives(
        || PreHirExpr::Var("a".to_string()),
        vec![if_then(
            PreHirExpr::Var("b".to_string()),
            vec![PreHirStmt::Label("L".to_string())]
        )]
    ));
}

#[test]
fn condexe_folding_keeps_a_second_call_in_a_nested_condition() {
    use crate::cleanup::apply_condexe_folding_pass;

    // `if (f()) { if (f()) x = 1; }` calls `f` twice.
    let mut body = vec![if_then(
        call_to("f"),
        vec![if_then(call_to("f"), vec![assign_var("x", 1)])],
    )];

    assert!(!apply_condexe_folding_pass(&mut body));
}

#[test]
fn condexe_folding_keeps_a_test_of_memory_the_loop_around_it_writes() {
    use crate::cleanup::apply_condexe_folding_pass;

    let looped = |after_the_test: PreHirStmt| {
        vec![if_then(
            load_through("p"),
            vec![PreHirStmt::While {
                cond: PreHirExpr::Var("c".to_string()),
                body: vec![
                    if_then(load_through("p"), vec![assign_var("y", 2)]),
                    after_the_test,
                ]
                .into(),
            }],
        )]
    };

    // `if (*p) while (c) { if (*p) y = 2; *p = 0; }` -- the second time
    // round, the store has run.
    let mut body = looped(store_through("p", 0));
    assert!(!apply_condexe_folding_pass(&mut body));

    // Positive control: a body that writes no memory leaves `*p` as it was.
    let mut body = looped(assign_var("x", 1));
    assert!(apply_condexe_folding_pass(&mut body));
}

#[test]
fn iblock_phi_elimination_blocks_clobber_memory_write_and_multi_use() {
    use crate::cleanup::apply_iblock_phi_elimination;

    // 1. Intervening write to condition variable `cond` must block hoisting.
    let mut stmts_clobber_cond = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![assign_var("x", 1)].into(),
            else_body: vec![assign_var("x", 2)].into(),
        },
        assign_var("cond", 0),
        PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string()))),
    ];
    assert!(!apply_iblock_phi_elimination(&mut stmts_clobber_cond));

    // 2. Intervening store when branch RHS reads memory must block hoisting.
    let mut stmts_clobber_mem = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: load_through("p"),
            }]
            .into(),
            else_body: vec![assign_var("x", 0)].into(),
        },
        store_through("p", 99),
        PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string()))),
    ];
    assert!(!apply_iblock_phi_elimination(&mut stmts_clobber_mem));

    // 3. Multiple uses (`x + x` in one statement and across two statements) must block hoisting.
    let mut stmts_same_stmt_multi_use = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![assign_var("x", 1)].into(),
            else_body: vec![assign_var("x", 2)].into(),
        },
        PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("x".to_string())),
            rhs: Box::new(PreHirExpr::Var("x".to_string())),
            ty: int(32),
        })),
    ];
    assert!(!apply_iblock_phi_elimination(
        &mut stmts_same_stmt_multi_use
    ));

    let mut stmts_two_stmt_multi_use = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![assign_var("x", 1)].into(),
            else_body: vec![assign_var("x", 2)].into(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("y".to_string()),
            rhs: PreHirExpr::Var("x".to_string()),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string()))),
    ];
    assert!(!apply_iblock_phi_elimination(&mut stmts_two_stmt_multi_use));
}

#[test]
fn iblock_phi_elimination_tracks_lvalue_base_uses() {
    use crate::cleanup::apply_iblock_phi_elimination;

    // Single use inside `Assign.lhs` (*x = 7) followed by return: must substitute into Deref ptr.
    let mut stmts = vec![
        PreHirStmt::If {
            cond: PreHirExpr::Var("cond".to_string()),
            then_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Var("p1".to_string()),
            }]
            .into(),
            else_body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Var("p2".to_string()),
            }]
            .into(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("x".to_string())),
                ty: int(32),
            },
            rhs: PreHirExpr::Const(7, int(32)),
        },
        PreHirStmt::Return(None),
    ];

    assert!(apply_iblock_phi_elimination(&mut stmts));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { ptr, .. },
            ..
        } if matches!(ptr.as_ref(), PreHirExpr::Select { .. })
    ));
}
