use super::utils::*;
use super::*;
// prelude via parent
use crate::HashSet;
use fission_midend_core::*;

fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

fn preserved_temp_binding(name: &str, bits: u32) -> DirBinding {
    DirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn temp_binding(name: &str, bits: u32) -> DirBinding {
    DirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

#[test]
fn recursive_empty_if_cleanup_prunes_nested_pure_empty_guard() {
    let mut stmts = vec![DirStmt::Block(vec![DirStmt::If {
        cond: DirExpr::Var("xVar12".to_string()),
        then_body: Vec::new(),
        else_body: Vec::new(),
    }])];

    assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts));
    assert!(stmts.is_empty());
}

#[test]
fn recursive_empty_if_cleanup_preserves_side_effectful_empty_guard() {
    let mut stmts = vec![DirStmt::If {
        cond: DirExpr::Call {
            target: "unknown_predicate".to_string(),
            args: Vec::new(),
            ty: NirType::Bool,
        },
        then_body: Vec::new(),
        else_body: Vec::new(),
    }];

    assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts));
    assert!(matches!(
        &stmts[..],
        [DirStmt::Expr(DirExpr::Call { target, .. })] if target == "unknown_predicate"
    ));
}

#[test]
fn collapse_trivial_assign_returns_skips_preserved_temp() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar0".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Sub,
                lhs: Box::new(DirExpr::Var("eax".to_string())),
                rhs: Box::new(DirExpr::Var("ecx".to_string())),
                ty: int(32),
            },
        },
        DirStmt::Return(Some(DirExpr::Var("uVar0".to_string()))),
    ];

    assert!(!collapse_trivial_assign_returns(
        &mut stmts,
        &["uVar0"].into_iter().collect::<HashSet<_>>(),
    ));
    assert!(matches!(stmts[0], DirStmt::Assign { .. }));
    assert!(matches!(stmts[1], DirStmt::Return(Some(DirExpr::Var(_)))));
}

#[test]
fn collapse_loop_exit_alias_return_rewrites_do_while_exit_copy() {
    let mut stmts = vec![
        DirStmt::DoWhile {
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("sum".to_string())),
                        rhs: Box::new(DirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("exit_sum".to_string()),
                    rhs: DirExpr::Var("sum".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("ptr".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("ptr".to_string())),
                        rhs: Box::new(DirExpr::Const(1, int(64))),
                        ty: int(64),
                    },
                },
            ],
            cond: DirExpr::Var("keep_going".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let DirStmt::DoWhile { body, .. } = &stmts[0] else {
        panic!("expected do/while");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[1],
        DirStmt::Return(Some(DirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_rhs_mutated_after_copy() {
    let mut stmts = vec![
        DirStmt::DoWhile {
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("exit_sum".to_string()),
                    rhs: DirExpr::Var("sum".to_string()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: DirExpr::Const(0, int(32)),
                },
            ],
            cond: DirExpr::Var("keep_going".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn eliminate_redundant_var_assigns_removes_adjacent_duplicate_assign() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar84".to_string()),
            rhs: DirExpr::Const(0, int(64)),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar84".to_string()),
            rhs: DirExpr::Const(0, int(64)),
        },
        DirStmt::Return(Some(DirExpr::Var("uVar84".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs: DirExpr::Const(0, _),
        } if name == "uVar84"
    ));
}

#[test]
fn eliminate_redundant_var_assigns_keeps_self_dependent_duplicate() {
    let rhs = DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(DirExpr::Var("sum".to_string())),
        rhs: Box::new(DirExpr::Const(1, int(32))),
        ty: int(32),
    };
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("sum".to_string()),
            rhs: rhs.clone(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("sum".to_string()),
            rhs,
        },
    ];

    assert!(!eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
}

#[test]
fn eliminate_redundant_var_assigns_removes_exact_self_assign() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar29".to_string()),
            rhs: DirExpr::Var("xVar29".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("xVar29".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(
        stmts,
        vec![DirStmt::Return(Some(DirExpr::Var("xVar29".to_string())))]
    );
}

#[test]
fn eliminate_redundant_var_assigns_recurses_into_nested_block() {
    // Structured O0 bodies often wrap the real statements in a single Block;
    // self-assigns inside that nest must still be removed (measured recursive
    // dual-call / iterative loop noise: `uVar = uVar`).
    let mut stmts = vec![DirStmt::Block(vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar1".to_string()),
            rhs: DirExpr::Var("param_1".to_string()),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar1".to_string()),
            rhs: DirExpr::Var("uVar1".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("uVar1".to_string()))),
    ])];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    match &stmts[0] {
        DirStmt::Block(body) => {
            assert_eq!(
                body.len(),
                2,
                "self-assign inside Block must be dropped: {body:?}"
            );
            assert!(
                !body.iter().any(|s| matches!(
                    s,
                    DirStmt::Assign {
                        lhs: DirLValue::Var(a),
                        rhs: DirExpr::Var(b),
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
    let mut func = DirFunction {
        name: "test_self_widening_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![DirBinding {
            name: "uVar84".to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("uVar84".to_string()),
                rhs: DirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(DirExpr::Var("uVar84".to_string())),
                },
            },
            DirStmt::Return(Some(DirExpr::Var("uVar84".to_string()))),
        ],
        ..Default::default()
    };

    assert!(cast_elision_pass(&mut func));
    assert_eq!(
        func.body[0],
        DirStmt::Assign {
            lhs: DirLValue::Var("uVar84".to_string()),
            rhs: DirExpr::Var("uVar84".to_string()),
        }
    );
}

#[test]
fn cast_elision_keeps_self_narrowing_assignment_to_wide_binding() {
    let mut func = DirFunction {
        name: "test_self_narrowing_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![DirBinding {
            name: "xVar29".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![DirStmt::Assign {
            lhs: DirLValue::Var("xVar29".to_string()),
            rhs: DirExpr::Cast {
                ty: int(32),
                expr: Box::new(DirExpr::Var("xVar29".to_string())),
            },
        }],
        ..Default::default()
    };

    assert!(!cast_elision_pass(&mut func));
}

#[test]
fn collapse_common_exit_guard_chain_wraps_body_and_preserves_exit_label() {
    let mut stmts = vec![
        DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLe,
                lhs: Box::new(DirExpr::Var("rows".to_string())),
                rhs: Box::new(DirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Goto("exit".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLe,
                lhs: Box::new(DirExpr::Var("cols".to_string())),
                rhs: Box::new(DirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Goto("exit".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Label("loop".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Deref {
                ptr: Box::new(DirExpr::Var("ptr".to_string())),
                ty: int(32),
            },
            rhs: DirExpr::Var("value".to_string()),
        },
        DirStmt::Goto("loop".to_string()),
        DirStmt::Label("exit".to_string()),
        DirStmt::Return(None),
    ];

    assert!(collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts.len(), 3);
    let DirStmt::If {
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
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            ..
        }
    ));
    assert_eq!(then_body.len(), 3);
    assert!(matches!(&then_body[0], DirStmt::Label(label) if label == "loop"));
    assert!(matches!(&stmts[1], DirStmt::Label(label) if label == "exit"));
    assert!(matches!(&stmts[2], DirStmt::Return(None)));
}

#[test]
fn collapse_common_exit_guard_chain_rejects_mixed_targets() {
    let original = vec![
        DirStmt::If {
            cond: DirExpr::Var("a".to_string()),
            then_body: vec![DirStmt::Goto("exit_a".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::If {
            cond: DirExpr::Var("b".to_string()),
            then_body: vec![DirStmt::Goto("exit_b".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("x".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
        DirStmt::Label("exit_a".to_string()),
        DirStmt::Return(None),
        DirStmt::Label("exit_b".to_string()),
        DirStmt::Return(None),
    ];
    let mut stmts = original.clone();

    assert!(!collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts, original);
}

#[test]
fn collapse_loop_exit_alias_return_rewrites_guarded_for_exit_copy() {
    let mut stmts = vec![
        DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLe,
                lhs: Box::new(DirExpr::Var("len".to_string())),
                rhs: Box::new(DirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Goto("exit_zero".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("sum".to_string()),
            rhs: DirExpr::Const(0, int(32)),
        },
        DirStmt::For {
            init: Some(Box::new(DirStmt::Assign {
                lhs: DirLValue::Var("i".to_string()),
                rhs: DirExpr::Const(0, int(64)),
            })),
            cond: Some(DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs: Box::new(DirExpr::Var("i".to_string())),
                rhs: Box::new(DirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(DirExpr::Var("len".to_string())),
                }),
                ty: NirType::Bool,
            }),
            update: Some(Box::new(DirStmt::Assign {
                lhs: DirLValue::Var("i".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("i".to_string())),
                    rhs: Box::new(DirExpr::Const(1, int(64))),
                    ty: int(64),
                },
            })),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("sum".to_string())),
                        rhs: Box::new(DirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("exit_sum".to_string()),
                    rhs: DirExpr::Var("sum".to_string()),
                },
            ],
        },
        DirStmt::Return(Some(DirExpr::Var("exit_sum".to_string()))),
        DirStmt::Label("exit_zero".to_string()),
        DirStmt::Return(Some(DirExpr::Const(0, int(32)))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let DirStmt::For { body, .. } = &stmts[2] else {
        panic!("expected for loop");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[3],
        DirStmt::Return(Some(DirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_non_alias_expression() {
    let mut stmts = vec![
        DirStmt::DoWhile {
            body: vec![DirStmt::Assign {
                lhs: DirLValue::Var("exit_sum".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("sum".to_string())),
                    rhs: Box::new(DirExpr::Var("value".to_string())),
                    ty: int(32),
                },
            }],
            cond: DirExpr::Var("keep_going".to_string()),
        },
        DirStmt::Return(Some(DirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn recover_guarded_loop_tail_accumulator_return_rewrites_stale_temp_return() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("count".to_string()),
            rhs: DirExpr::Const(0, int(32)),
        },
        DirStmt::If {
            cond: DirExpr::Var("value".to_string()),
            then_body: vec![DirStmt::Goto("loop_body".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Return(Some(DirExpr::Var("bit".to_string()))),
        DirStmt::Label("loop_body".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("bit".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs: Box::new(DirExpr::Var("value".to_string())),
                rhs: Box::new(DirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("count".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("count".to_string())),
                rhs: Box::new(DirExpr::Var("bit".to_string())),
                ty: int(32),
            },
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("value".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: Box::new(DirExpr::Var("value".to_string())),
                rhs: Box::new(DirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
    ];

    assert!(recover_guarded_loop_tail_accumulator_returns(&mut stmts));
    assert_eq!(stmts.len(), 3);
    assert!(
        matches!(&stmts[1], DirStmt::While { cond: DirExpr::Var(name), .. } if name == "value")
    );
    let DirStmt::While { body, .. } = &stmts[1] else {
        panic!("expected while");
    };
    assert_eq!(body.len(), 3);
    assert!(matches!(
        &stmts[2],
        DirStmt::Return(Some(DirExpr::Var(name))) if name == "count"
    ));
}

#[test]
fn recover_guarded_loop_tail_accumulator_return_rejects_without_cond_update() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("count".to_string()),
            rhs: DirExpr::Const(0, int(32)),
        },
        DirStmt::If {
            cond: DirExpr::Var("value".to_string()),
            then_body: vec![DirStmt::Goto("loop_body".to_string())],
            else_body: Vec::new(),
        },
        DirStmt::Return(Some(DirExpr::Var("bit".to_string()))),
        DirStmt::Label("loop_body".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("bit".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs: Box::new(DirExpr::Var("value".to_string())),
                rhs: Box::new(DirExpr::Const(1, int(32))),
                ty: int(32),
            },
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("count".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("count".to_string())),
                rhs: Box::new(DirExpr::Var("bit".to_string())),
                ty: int(32),
            },
        },
    ];

    assert!(!recover_guarded_loop_tail_accumulator_returns(&mut stmts));
}

#[test]
fn eliminate_dead_temp_assigns_removes_dead_preserved_temp() {
    let mut stmts = vec![DirStmt::Assign {
        lhs: DirLValue::Var("uVar0".to_string()),
        rhs: DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("eax".to_string())),
            rhs: Box::new(DirExpr::Const(1, int(32))),
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
        DirStmt::Assign {
            lhs: DirLValue::Var("reg".to_string()),
            rhs: DirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![DirExpr::Var("rsp".to_string()), DirExpr::Const(16, int(64))],
                ty: NirType::Bool,
            },
        },
        DirStmt::Return(Some(DirExpr::Var("rsp".to_string()))),
    ];

    assert!(eliminate_dead_temp_assigns(&mut stmts, &HashSet::default()));
    assert_eq!(
        stmts,
        vec![DirStmt::Return(Some(DirExpr::Var("rsp".to_string())))]
    );
}

#[test]
fn eliminate_dead_temp_assigns_keeps_used_reg_flag_artifact() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("reg".to_string()),
            rhs: DirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![DirExpr::Var("rsp".to_string()), DirExpr::Const(16, int(64))],
                ty: NirType::Bool,
            },
        },
        DirStmt::If {
            cond: DirExpr::Var("reg".to_string()),
            then_body: vec![DirStmt::Return(Some(DirExpr::Const(1, int(32))))],
            else_body: vec![DirStmt::Return(Some(DirExpr::Const(0, int(32))))],
        },
    ];

    assert!(!eliminate_dead_temp_assigns(
        &mut stmts,
        &HashSet::default()
    ));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } if name == "reg"
    ));
}

#[test]
fn prune_unreachable_after_return_stops_at_label_boundary() {
    let mut stmts = vec![
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Assign {
            lhs: DirLValue::Var("dead".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
        DirStmt::Goto("kept".to_string()),
        DirStmt::Label("kept".to_string()),
        DirStmt::Return(None),
    ];

    assert!(prune_unreachable_after_terminal(&mut stmts));
    assert_eq!(
        stmts,
        vec![
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
            DirStmt::Label("kept".to_string()),
            DirStmt::Return(None),
        ]
    );
}

#[test]
fn prune_unreachable_after_terminal_keeps_protected_lsda_landing_pad() {
    let mut stmts = vec![
        DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        DirStmt::Label("landing_pad".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("cleanup".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
    ];

    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().insert("landing_pad".to_string());
    });
    let changed = prune_unreachable_after_terminal(&mut stmts);
    crate::pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().clear();
    });

    assert!(!changed);
    assert_eq!(stmts.len(), 3);
    assert_eq!(stmts[1], DirStmt::Label("landing_pad".to_string()));
}

#[test]
fn single_pred_label_inline_drains_unprotected_dead_zone() {
    let mut stmts = vec![
        DirStmt::Goto("a".to_string()),
        DirStmt::Label("dead".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("unreached".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
        DirStmt::Label("a".to_string()),
        DirStmt::Return(None),
    ];

    assert!(single_pred_label_inline(&mut stmts));
    assert_eq!(stmts, vec![DirStmt::Return(None)]);
}

#[test]
fn single_pred_label_inline_keeps_protected_lsda_landing_pad_in_dead_zone() {
    let mut stmts = vec![
        DirStmt::Goto("a".to_string()),
        DirStmt::Label("landing_pad".to_string()),
        DirStmt::Assign {
            lhs: DirLValue::Var("cleanup".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
        DirStmt::Label("a".to_string()),
        DirStmt::Return(None),
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
            DirStmt::Goto("a".to_string()),
            DirStmt::Label("landing_pad".to_string()),
            DirStmt::Assign {
                lhs: DirLValue::Var("cleanup".to_string()),
                rhs: DirExpr::Const(1, int(32)),
            },
            DirStmt::Label("a".to_string()),
            DirStmt::Return(None),
        ]
    );
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp() {
    let mut func = DirFunction {
        name: "test_preserved_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("uVar0", 32)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![DirStmt::Return(Some(DirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_plain_temp_with_nontrivial_name() {
    let mut func = DirFunction {
        name: "test_plain_temp_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![DirStmt::Return(Some(DirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp_with_nontrivial_name() {
    let mut func = DirFunction {
        name: "test_preserved_named_register_prune".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![DirStmt::Return(Some(DirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_keeps_used_plain_temp_with_nontrivial_name() {
    let mut func = DirFunction {
        name: "test_plain_temp_used".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![DirStmt::Return(Some(DirExpr::Var("rcx".to_string())))],
        ..Default::default()
    };

    assert!(!prune_unused_temp_bindings(&mut func));
    assert_eq!(func.locals.len(), 1);
    assert_eq!(func.locals[0].name, "rcx");
}

#[test]
fn prune_unused_temp_bindings_keeps_side_effect_assignment_target() {
    let mut func = DirFunction {
        name: "test_side_effect_lhs_preserved".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![preserved_temp_binding("xVar30", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![DirStmt::Assign {
            lhs: DirLValue::Var("xVar30".to_string()),
            rhs: DirExpr::Call {
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
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar0".to_string()),
            rhs: DirExpr::Const(0, int(32)),
        },
        DirStmt::Label("loop_head".to_string()),
        DirStmt::If {
            cond: DirExpr::Var("xVar0".to_string()),
            then_body: vec![DirStmt::Goto("loop_head".to_string())],
            else_body: Vec::new(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert!(matches!(
        &stmts[2],
        DirStmt::If {
            cond: DirExpr::Var(name),
            ..
        } if name == "xVar0"
    ));
}

#[test]
fn inline_single_use_temps_keeps_same_linear_segment_inline() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar0".to_string()),
            rhs: DirExpr::Const(1, int(32)),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar1".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("xVar0".to_string())),
                rhs: Box::new(DirExpr::Const(2, int(32))),
                ty: int(32),
            },
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 1);
    let DirStmt::Assign { rhs, .. } = &stmts[0] else {
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
    let x = DirExpr::Var("param_18".into());
    let expr = DirExpr::Binary {
        op: DirBinaryOp::LogicalOr,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs: Box::new(x.clone()),
            rhs: Box::new(DirExpr::Const(0, signed.clone())),
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(x.clone()),
            rhs: Box::new(DirExpr::Const(0, signed.clone())),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    };
    let out = normalize_boolean_logic(&expr).expect("fold");
    match out {
        DirExpr::Binary {
            op: DirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => {
            assert!(matches!(lhs.as_ref(), DirExpr::Var(n) if n == "param_18"));
            assert!(matches!(rhs.as_ref(), DirExpr::Const(0, _)));
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
        DirStmt::Assign {
            lhs: DirLValue::Var("iVar36".to_string()),
            rhs: DirExpr::Var("param_18".to_string()),
        },
        DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::LogicalOr,
                lhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Eq,
                    lhs: Box::new(DirExpr::Var("iVar36".to_string())),
                    rhs: Box::new(DirExpr::Const(0, signed.clone())),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::SLt,
                    lhs: Box::new(DirExpr::Var("iVar36".to_string())),
                    rhs: Box::new(DirExpr::Const(0, signed.clone())),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Break],
            else_body: vec![],
        },
    ];
    assert!(
        inline_single_use_temps(&mut stmts, &HashSet::default()),
        "pure copy into multi-use predicate should inline: {stmts:?}"
    );
    assert_eq!(stmts.len(), 1, "temp assign removed: {stmts:?}");
    match &stmts[0] {
        DirStmt::If { cond, .. } => {
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
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar23".to_string()),
            rhs: DirExpr::Var("param_10".to_string()),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar23".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Mul,
                lhs: Box::new(DirExpr::Var("xVar23".to_string())),
                rhs: Box::new(DirExpr::Var("xVar23".to_string())),
                ty: ty.clone(),
            },
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("param_10".to_string()),
            rhs: DirExpr::Var("xVar23".to_string()),
        },
    ];
    assert!(
        collapse_temp_self_square_assigns(&mut stmts),
        "expected square collapse: {stmts:?}"
    );
    assert_eq!(stmts.len(), 1, "{stmts:?}");
    match &stmts[0] {
        DirStmt::Assign {
            lhs: DirLValue::Var(dst),
            rhs:
                DirExpr::Binary {
                    op: DirBinaryOp::Mul,
                    lhs,
                    rhs,
                    ..
                },
        } => {
            assert_eq!(dst, "param_10");
            assert!(matches!(lhs.as_ref(), DirExpr::Var(n) if n == "param_10"));
            assert!(matches!(rhs.as_ref(), DirExpr::Var(n) if n == "param_10"));
        }
        other => panic!("expected param_10 = param_10 * param_10, got {other:?}"),
    }
}

#[test]
fn inline_single_use_temps_preserves_rhs_across_dependency_redefinition() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar0".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Mul,
                lhs: Box::new(DirExpr::Var("address_reg".to_string())),
                rhs: Box::new(DirExpr::Const(4, int(64))),
                ty: int(64),
            },
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("address_reg".to_string()),
            rhs: DirExpr::Var("base".to_string()),
        },
        DirStmt::Assign {
            lhs: DirLValue::Var("address_reg".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("address_reg".to_string())),
                rhs: Box::new(DirExpr::Var("xVar0".to_string())),
                ty: int(64),
            },
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 3);
    let DirStmt::Assign { rhs, .. } = &stmts[2] else {
        panic!("expected address assignment");
    };
    assert!(expr_contains_var(rhs, "xVar0"));
}

#[test]
fn inline_single_use_temps_inlines_flag_intrinsic_into_predicate() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar0".to_string()),
            rhs: DirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![
                    DirExpr::Var("param_1".to_string()),
                    DirExpr::Const(1, int(32)),
                ],
                ty: NirType::Bool,
            },
        },
        DirStmt::If {
            cond: DirExpr::Var("xVar0".to_string()),
            then_body: Vec::new(),
            else_body: Vec::new(),
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 1);
    let DirStmt::If { cond, .. } = &stmts[0] else {
        panic!("expected if");
    };
    assert!(matches!(cond, DirExpr::Call { target, .. } if target == "__sborrow"));
}

#[test]
fn inline_loop_condition_trailing_temps_substitutes_condition_chain() {
    let mut func = DirFunction {
        name: "test_loop_cond_inline".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![DirStmt::DoWhile {
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("sum".to_string())),
                        rhs: Box::new(DirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar38".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(DirExpr::Var("ptr".to_string())),
                        rhs: Box::new(DirExpr::Var("end".to_string())),
                        ty: int(64),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar39".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Eq,
                        lhs: Box::new(DirExpr::Var("xVar38".to_string())),
                        rhs: Box::new(DirExpr::Const(0, int(64))),
                        ty: NirType::Bool,
                    },
                },
            ],
            cond: DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("xVar39".to_string())),
                ty: NirType::Bool,
            },
        }],
        ..Default::default()
    };

    assert!(inline_loop_condition_trailing_temps(&mut func,));
    let DirStmt::DoWhile { body, cond } = &func.body[0] else {
        panic!("expected do-while");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(
        cond,
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } if matches!(
            expr.as_ref(),
            DirExpr::Binary {
                op: DirBinaryOp::Eq,
                lhs,
                ..
            } if matches!(
                lhs.as_ref(),
                DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    ..
                }
            )
        )
    ));
}

#[test]
fn inline_single_use_temps_keeps_unknown_call_out_of_predicate() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("xVar0".to_string()),
            rhs: DirExpr::Call {
                target: "unknown_helper".to_string(),
                args: vec![DirExpr::Var("param_1".to_string())],
                ty: int(32),
            },
        },
        DirStmt::If {
            cond: DirExpr::Var("xVar0".to_string()),
            then_body: Vec::new(),
            else_body: Vec::new(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::default()));
    assert_eq!(stmts.len(), 2);
}

#[test]
fn switch_norm_folds_range_check_guard() {
    let mut func = DirFunction {
        name: "test_switch_norm".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::Lt,
                lhs: Box::new(DirExpr::Var("x".to_string())),
                rhs: Box::new(DirExpr::Const(5, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Switch {
                expr: DirExpr::Var("x".to_string()),
                cases: vec![DirSwitchCase {
                    values: vec![1],
                    body: vec![DirStmt::Return(Some(DirExpr::Const(10, int(32))))],
                }],
                default: Vec::new(),
            }],
            else_body: vec![DirStmt::Return(Some(DirExpr::Const(20, int(32))))],
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
    let DirStmt::Switch {
        expr,
        cases,
        default,
    } = &func.body[0]
    else {
        panic!("expected switch statement");
    };
    assert_eq!(expr, &DirExpr::Var("x".to_string()));
    assert_eq!(cases.len(), 1);
    assert_eq!(default.len(), 1);
    assert!(matches!(
        &default[0],
        DirStmt::Return(Some(DirExpr::Const(20, _)))
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

    let mut func = DirFunction {
        name: "test_constant_ptr".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            // Exact match
            DirStmt::Assign {
                lhs: DirLValue::Var("p1".to_string()),
                rhs: DirExpr::Const(0x140004000, int(64)),
            },
            // Inside bounds (offset 8)
            DirStmt::Assign {
                lhs: DirLValue::Var("p2".to_string()),
                rhs: DirExpr::Const(0x140003008, int(64)),
            },
            // Outside any bounds / no match
            DirStmt::Assign {
                lhs: DirLValue::Var("p3".to_string()),
                rhs: DirExpr::Const(0x140005000, int(64)),
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
    let DirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    assert_eq!(rhs1, &DirExpr::AddressOfGlobal("g_exact".to_string()));

    // Assert offset match: PtrOffset { base: AddressOfGlobal("g_data"), offset: 8 }
    let DirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    assert!(
        matches!(rhs2, DirExpr::PtrOffset { base, offset: 8 } if matches!(base.as_ref(), DirExpr::AddressOfGlobal(name) if name == "g_data"))
    );

    // Assert no match: stays Const(0x140005000)
    let DirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    assert_eq!(rhs3, &DirExpr::Const(0x140005000, int(64)));
}

#[test]
fn condexe_folding_merges_sequential_siblings() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = DirFunction {
        name: "test_condexe_siblings".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            DirStmt::If {
                cond: DirExpr::Var("a".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(1, int(32)),
                }],
                else_body: Vec::new(),
            },
            DirStmt::If {
                cond: DirExpr::Var("a".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("y".to_string()),
                    rhs: DirExpr::Const(2, int(32)),
                }],
                else_body: Vec::new(),
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
    let DirStmt::If {
        cond,
        then_body,
        else_body,
    } = &func.body[0]
    else {
        panic!();
    };
    assert_eq!(cond, &DirExpr::Var("a".to_string()));
    assert_eq!(then_body.len(), 2);
    assert!(else_body.is_empty());
}

#[test]
fn condexe_folding_merges_nested_ifs() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = DirFunction {
        name: "test_condexe_nested".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![DirStmt::If {
            cond: DirExpr::Var("a".to_string()),
            then_body: vec![DirStmt::If {
                cond: DirExpr::Var("a".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(1, int(32)),
                }],
                else_body: Vec::new(),
            }],
            else_body: Vec::new(),
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
    let DirStmt::If {
        cond,
        then_body,
        else_body,
    } = &func.body[0]
    else {
        panic!();
    };
    assert_eq!(cond, &DirExpr::Var("a".to_string()));
    assert_eq!(then_body.len(), 1);
    assert!(else_body.is_empty());

    let DirStmt::Assign { lhs, .. } = &then_body[0] else {
        panic!();
    };
    assert_eq!(lhs, &DirLValue::Var("x".to_string()));
}

#[test]
fn condexe_folding_preserves_safety_on_assignment() {
    use crate::cleanup::apply_condexe_folding_pass;

    let mut func = DirFunction {
        name: "test_condexe_safety".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![
            DirStmt::If {
                cond: DirExpr::Var("a".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("a".to_string()),
                    rhs: DirExpr::Const(0, int(32)),
                }],
                else_body: Vec::new(),
            },
            DirStmt::If {
                cond: DirExpr::Var("a".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(1, int(32)),
                }],
                else_body: Vec::new(),
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

    let mut func = DirFunction {
        name: "test_deindirect_const".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![DirStmt::Expr(DirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![
                DirExpr::Const(0x401000, int(64)),
                DirExpr::Var("param_1".to_string()),
            ],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let DirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let DirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression");
    };
    assert_eq!(target, "target_func");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], DirExpr::Var("param_1".to_string()));
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

    let mut func = DirFunction {
        name: "test_deindirect_var".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![DirBinding {
            name: "fn_ptr".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: Some(DirExpr::Const(0x401000, int(64))),
        }],
        body: vec![DirStmt::Expr(DirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![DirExpr::Var("fn_ptr".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let DirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let DirExpr::Call { target, args, .. } = expr else {
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

    let iat_load = DirExpr::Load {
        ptr: Box::new(DirExpr::Const(IAT_SLOT_ADDR as i64, int(64))),
        ty: NirType::Unknown,
    };

    let mut func = DirFunction {
        name: "test_deindirect_iat_load".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![DirStmt::Expr(DirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args: vec![iat_load, DirExpr::Var("param_1".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries,
        ..Default::default()
    };

    assert!(
        apply_deindirect_pass(&mut func),
        "deindirect pass must rewrite IAT-load indirect call"
    );
    let DirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let DirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression after deindirect");
    };
    assert_eq!(
        target, "InitializeCriticalSection",
        "call target must be resolved from IAT slot"
    );
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], DirExpr::Var("param_1".to_string()));
}

// Dual-layer C rendering of CALLIND opaque targets lives in fission-pcode render
// (ADR 0011). midend-core::format_expr_key is a deterministic key/diagnostic printer only.
#[test]
fn diagnostic_print_expr_renders_callind_opaque_call_shape() {
    use fission_midend_core::util_dir::format_expr_key;

    let expr = DirExpr::Call {
        target: "__fission_callind_opaque".to_string(),
        args: vec![
            DirExpr::Var("fn_ptr".to_string()),
            DirExpr::Var("arg1".to_string()),
            DirExpr::Var("arg2".to_string()),
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

    let mut func = DirFunction {
        name: "test_subvar_trim".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![
            DirBinding {
                name: "b".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            DirBinding {
                name: "y".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "z".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            DirBinding {
                name: "res1".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res2".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // x = (u32)b
            DirStmt::Assign {
                lhs: DirLValue::Var("x".to_string()),
                rhs: DirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(DirExpr::Var("b".to_string())),
                },
            },
            // res1 = (u8)x
            DirStmt::Assign {
                lhs: DirLValue::Var("res1".to_string()),
                rhs: DirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(DirExpr::Var("x".to_string())),
                },
            },
            // z = y & 0xff
            DirStmt::Assign {
                lhs: DirLValue::Var("z".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::And,
                    lhs: Box::new(DirExpr::Var("y".to_string())),
                    rhs: Box::new(DirExpr::Const(0xff, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            },
            // res2 = (u8)z
            DirStmt::Assign {
                lhs: DirLValue::Var("res2".to_string()),
                rhs: DirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(DirExpr::Var("z".to_string())),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_subvar_trim_pass(&mut func));

    // Expected:
    // res1 = b (cast elided and replaced with original byte variable b)
    // res2 = (u8)y (bitwise AND elided, cast moved directly to y)
    let DirStmt::Assign { rhs: rhs1, .. } = &func.body[1] else {
        panic!();
    };
    assert_eq!(rhs1, &DirExpr::Var("b".to_string()));

    let DirStmt::Assign { rhs: rhs2, .. } = &func.body[3] else {
        panic!();
    };
    if let DirExpr::Cast { ty, expr } = rhs2 {
        assert_eq!(ty, &u8_ty);
        assert_eq!(expr.as_ref(), &DirExpr::Var("y".to_string()));
    } else {
        panic!("expected cast of y, got {:?}", rhs2);
    }
}

#[test]
fn fuse_if_goto_allows_returns_in_segment() {
    // if (c) goto L; return 1; L: return 0;
    // → if (!c) { return 1; } L: return 0;
    let mut stmts = vec![
        DirStmt::If {
            cond: DirExpr::Var("c".to_string()),
            then_body: vec![DirStmt::Goto("L".to_string())],
            else_body: vec![],
        },
        DirStmt::Return(Some(DirExpr::Const(1, int(32)))),
        DirStmt::Label("L".to_string()),
        DirStmt::Return(Some(DirExpr::Const(0, int(32)))),
    ];
    assert!(
        fuse_single_predecessor_boundaries(&mut stmts),
        "expected fuse: {stmts:?}"
    );
    assert!(
        matches!(
            &stmts[..],
            [
                DirStmt::If {
                    then_body,
                    else_body,
                    ..
                },
                DirStmt::Label(l),
                DirStmt::Return(Some(DirExpr::Const(0, _))),
            ] if else_body.is_empty()
                && matches!(then_body.as_slice(), [DirStmt::Return(Some(DirExpr::Const(1, _)))])
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
        DirStmt::Assign {
            lhs: DirLValue::Var("eax".to_string()),
            rhs: DirExpr::Const(7, int(32)),
        },
        DirStmt::Return(Some(DirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        DirStmt::Return(Some(DirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_eax_const_even_if_preserved() {
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("eax".to_string()),
            rhs: DirExpr::Const(7, int(32)),
        },
        DirStmt::Return(Some(DirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &["eax"].into_iter().collect::<HashSet<_>>(),
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        DirStmt::Return(Some(DirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_rax_param_add() {
    use fission_midend_core::ir::{DirBinaryOp, DirExpr, DirLValue, DirStmt, NirType};
    let mut stmts = vec![
        DirStmt::Assign {
            lhs: DirLValue::Var("rax".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("param_1".to_string())),
                rhs: Box::new(DirExpr::Const(
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
        DirStmt::Return(Some(DirExpr::Var("rax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &crate::HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        DirStmt::Return(Some(DirExpr::Binary {
            op: DirBinaryOp::Add,
            ..
        })) => {}
        other => panic!("expected folded return add, got {other:?}"),
    }
}

fn rescue_undeclared_bindings_declares_stack_local_names() {
    let mut func = DirFunction {
        name: "f".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![],
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("local_0".to_string()),
                rhs: DirExpr::Var("param_2".to_string()),
            },
            DirStmt::Return(Some(DirExpr::Var("local_0".to_string()))),
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
