use super::*;

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

/// `t = <value>; if (rax) { <arm_prefix>; rax = f(t); }` -- the shape a
/// path-joining `if` takes around a call whose argument was read before it.
fn copy_used_in_an_if_arm(value: PreHirExpr, arm_prefix: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    let mut arm = arm_prefix;
    arm.push(PreHirStmt::Assign {
        lhs: PreHirLValue::Var("rax".to_string()),
        rhs: PreHirExpr::Call {
            origin: None,
            target: "f".to_string(),
            args: vec![PreHirExpr::Var("xVar0".to_string())],
            ty: int(64),
        },
    });
    vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: value,
        },
        PreHirStmt::If {
            cond: PreHirExpr::Var("rax".to_string()),
            then_body: arm.into(),
            else_body: Vec::new().into(),
        },
    ]
}

/// The argument `f` receives, after cleanup.
fn argument_of_f(stmts: &[PreHirStmt]) -> PreHirExpr {
    let PreHirStmt::If { then_body, .. } = stmts.last().expect("an if") else {
        panic!("expected the if last: {stmts:?}");
    };
    let Some(PreHirStmt::Assign {
        rhs: PreHirExpr::Call { args, .. },
        ..
    }) = then_body.last()
    else {
        panic!("expected the call last in the arm: {then_body:?}");
    };
    args[0].clone()
}

/// A copy used in an `if`'s arm reads its source where the copy is, not
/// where the arm uses it. `t = rax; if (rax) { rax = 0; f(t); }` passes the
/// old `rax`; inlining it passed 0 (`open(ttyname(..))` printed `open(0, ..)`).
#[test]
fn a_copy_is_not_inlined_into_an_if_arm_past_a_write_to_its_source() {
    let zero_rax = || PreHirStmt::Assign {
        lhs: PreHirLValue::Var("rax".to_string()),
        rhs: PreHirExpr::Const(0, int(64)),
    };
    for pass in [
        |stmts: &mut Vec<PreHirStmt>| inline_single_use_temps(stmts, &HashSet::default()),
        collapse_adjacent_pure_copy_into_if,
    ] {
        let mut stmts =
            copy_used_in_an_if_arm(PreHirExpr::Var("rax".to_string()), vec![zero_rax()]);
        pass(&mut stmts);
        assert_eq!(
            argument_of_f(&stmts),
            PreHirExpr::Var("xVar0".to_string()),
            "{stmts:?}"
        );

        // Control: with nothing in between, the copy folds.
        let mut stmts = copy_used_in_an_if_arm(PreHirExpr::Var("rax".to_string()), Vec::new());
        assert!(pass(&mut stmts));
        assert_eq!(argument_of_f(&stmts), PreHirExpr::Var("rax".to_string()));
    }
}

/// The same for a load: a store in the arm ahead of the use may change it.
#[test]
fn a_load_is_not_inlined_into_an_if_arm_past_a_store() {
    let load = || PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Var("p".to_string())),
        ty: int(64),
    };
    let store = PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(PreHirExpr::Var("q".to_string())),
            ty: int(64),
        },
        rhs: PreHirExpr::Const(1, int(64)),
    };
    let mut stmts = copy_used_in_an_if_arm(load(), vec![store]);
    inline_single_use_temps(&mut stmts, &HashSet::default());
    assert_eq!(
        argument_of_f(&stmts),
        PreHirExpr::Var("xVar0".to_string()),
        "{stmts:?}"
    );

    // Control: no store, and the load moves to its use.
    let mut stmts = copy_used_in_an_if_arm(load(), Vec::new());
    assert!(inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(argument_of_f(&stmts), load());
}

#[test]
fn inline_single_use_temps_inlines_flag_intrinsic_into_predicate() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar0".to_string()),
            rhs: PreHirExpr::Call {
                origin: None,
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
                        origin: None,
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
                origin: None,
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
