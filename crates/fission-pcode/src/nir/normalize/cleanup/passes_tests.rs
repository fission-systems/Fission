use super::*;
use super::utils::*;
use crate::nir::*;
use std::collections::HashSet;

fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

fn preserved_temp_binding(name: &str, bits: u32) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn temp_binding(name: &str, bits: u32) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

#[test]
fn recursive_empty_if_cleanup_prunes_nested_pure_empty_guard() {
    let mut stmts = vec![HirStmt::Block(vec![HirStmt::If {
        cond: HirExpr::Var("xVar12".to_string()),
        then_body: Vec::new(),
        else_body: Vec::new(),
    }])];

    assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts));
    assert!(stmts.is_empty());
}

#[test]
fn recursive_empty_if_cleanup_preserves_side_effectful_empty_guard() {
    let mut stmts = vec![HirStmt::If {
        cond: HirExpr::Call {
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
        [HirStmt::Expr(HirExpr::Call { target, .. })] if target == "unknown_predicate"
    ));
}

#[test]
fn collapse_trivial_assign_returns_skips_preserved_temp() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar0".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("eax".to_string())),
                rhs: Box::new(HirExpr::Var("ecx".to_string())),
                ty: int(32),
            },
        },
        HirStmt::Return(Some(HirExpr::Var("uVar0".to_string()))),
    ];

    assert!(!collapse_trivial_assign_returns(
        &mut stmts,
        &HashSet::from(["uVar0"]),
    ));
    assert!(matches!(stmts[0], HirStmt::Assign { .. }));
    assert!(matches!(stmts[1], HirStmt::Return(Some(HirExpr::Var(_)))));
}

#[test]
fn collapse_loop_exit_alias_return_rewrites_do_while_exit_copy() {
    let mut stmts = vec![
        HirStmt::DoWhile {
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("sum".to_string())),
                        rhs: Box::new(HirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("exit_sum".to_string()),
                    rhs: HirExpr::Var("sum".to_string()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("ptr".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("ptr".to_string())),
                        rhs: Box::new(HirExpr::Const(1, int(64))),
                        ty: int(64),
                    },
                },
            ],
            cond: HirExpr::Var("keep_going".to_string()),
        },
        HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let HirStmt::DoWhile { body, .. } = &stmts[0] else {
        panic!("expected do/while");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[1],
        HirStmt::Return(Some(HirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_rhs_mutated_after_copy() {
    let mut stmts = vec![
        HirStmt::DoWhile {
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("exit_sum".to_string()),
                    rhs: HirExpr::Var("sum".to_string()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: HirExpr::Const(0, int(32)),
                },
            ],
            cond: HirExpr::Var("keep_going".to_string()),
        },
        HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn eliminate_redundant_var_assigns_removes_adjacent_duplicate_assign() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar84".to_string()),
            rhs: HirExpr::Const(0, int(64)),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar84".to_string()),
            rhs: HirExpr::Const(0, int(64)),
        },
        HirStmt::Return(Some(HirExpr::Var("uVar84".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        &stmts[0],
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Const(0, _),
        } if name == "uVar84"
    ));
}

#[test]
fn eliminate_redundant_var_assigns_keeps_self_dependent_duplicate() {
    let rhs = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("sum".to_string())),
        rhs: Box::new(HirExpr::Const(1, int(32))),
        ty: int(32),
    };
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("sum".to_string()),
            rhs: rhs.clone(),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("sum".to_string()),
            rhs,
        },
    ];

    assert!(!eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(stmts.len(), 2);
}

#[test]
fn eliminate_redundant_var_assigns_removes_exact_self_assign() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar29".to_string()),
            rhs: HirExpr::Var("xVar29".to_string()),
        },
        HirStmt::Return(Some(HirExpr::Var("xVar29".to_string()))),
    ];

    assert!(eliminate_redundant_var_assigns(&mut stmts));
    assert_eq!(
        stmts,
        vec![HirStmt::Return(Some(HirExpr::Var("xVar29".to_string())))]
    );
}

#[test]
fn cast_elision_rewrites_self_widening_assignment_to_self_assign() {
    let mut func = HirFunction {
        name: "test_self_widening_cast".to_string(),
        locals: vec![NirBinding {
            name: "uVar84".to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar84".to_string()),
                rhs: HirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(HirExpr::Var("uVar84".to_string())),
                },
            },
            HirStmt::Return(Some(HirExpr::Var("uVar84".to_string()))),
        ],
        ..Default::default()
    };

    assert!(cast_elision_pass(&mut func));
    assert_eq!(
        func.body[0],
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar84".to_string()),
            rhs: HirExpr::Var("uVar84".to_string()),
        }
    );
}

#[test]
fn cast_elision_keeps_self_narrowing_assignment_to_wide_binding() {
    let mut func = HirFunction {
        name: "test_self_narrowing_cast".to_string(),
        locals: vec![NirBinding {
            name: "xVar29".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("xVar29".to_string()),
            rhs: HirExpr::Cast {
                ty: int(32),
                expr: Box::new(HirExpr::Var("xVar29".to_string())),
            },
        }],
        ..Default::default()
    };

    assert!(!cast_elision_pass(&mut func));
}

#[test]
fn collapse_common_exit_guard_chain_wraps_body_and_preserves_exit_label() {
    let mut stmts = vec![
        HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLe,
                lhs: Box::new(HirExpr::Var("rows".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Goto("exit".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLe,
                lhs: Box::new(HirExpr::Var("cols".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Goto("exit".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("loop".to_string()),
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("ptr".to_string())),
                ty: int(32),
            },
            rhs: HirExpr::Var("value".to_string()),
        },
        HirStmt::Goto("loop".to_string()),
        HirStmt::Label("exit".to_string()),
        HirStmt::Return(None),
    ];

    assert!(collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts.len(), 3);
    let HirStmt::If {
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
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            ..
        }
    ));
    assert_eq!(then_body.len(), 3);
    assert!(matches!(&then_body[0], HirStmt::Label(label) if label == "loop"));
    assert!(matches!(&stmts[1], HirStmt::Label(label) if label == "exit"));
    assert!(matches!(&stmts[2], HirStmt::Return(None)));
}

#[test]
fn collapse_common_exit_guard_chain_rejects_mixed_targets() {
    let original = vec![
        HirStmt::If {
            cond: HirExpr::Var("a".to_string()),
            then_body: vec![HirStmt::Goto("exit_a".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Var("b".to_string()),
            then_body: vec![HirStmt::Goto("exit_b".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("x".to_string()),
            rhs: HirExpr::Const(1, int(32)),
        },
        HirStmt::Label("exit_a".to_string()),
        HirStmt::Return(None),
        HirStmt::Label("exit_b".to_string()),
        HirStmt::Return(None),
    ];
    let mut stmts = original.clone();

    assert!(!collapse_common_exit_guard_chain(&mut stmts));
    assert_eq!(stmts, original);
}

#[test]
fn collapse_loop_exit_alias_return_rewrites_guarded_for_exit_copy() {
    let mut stmts = vec![
        HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLe,
                lhs: Box::new(HirExpr::Var("len".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Goto("exit_zero".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("sum".to_string()),
            rhs: HirExpr::Const(0, int(32)),
        },
        HirStmt::For {
            init: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("i".to_string()),
                rhs: HirExpr::Const(0, int(64)),
            })),
            cond: Some(HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("i".to_string())),
                rhs: Box::new(HirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(HirExpr::Var("len".to_string())),
                }),
                ty: NirType::Bool,
            }),
            update: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("i".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("i".to_string())),
                    rhs: Box::new(HirExpr::Const(1, int(64))),
                    ty: int(64),
                },
            })),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("sum".to_string())),
                        rhs: Box::new(HirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("exit_sum".to_string()),
                    rhs: HirExpr::Var("sum".to_string()),
                },
            ],
        },
        HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
        HirStmt::Label("exit_zero".to_string()),
        HirStmt::Return(Some(HirExpr::Const(0, int(32)))),
    ];

    assert!(collapse_loop_exit_alias_returns(&mut stmts));
    let HirStmt::For { body, .. } = &stmts[2] else {
        panic!("expected for loop");
    };
    assert!(!body.iter().any(|stmt| matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } if name == "exit_sum"
    )));
    assert!(matches!(
        &stmts[3],
        HirStmt::Return(Some(HirExpr::Var(name))) if name == "sum"
    ));
}

#[test]
fn collapse_loop_exit_alias_return_rejects_non_alias_expression() {
    let mut stmts = vec![
        HirStmt::DoWhile {
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("exit_sum".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("sum".to_string())),
                    rhs: Box::new(HirExpr::Var("value".to_string())),
                    ty: int(32),
                },
            }],
            cond: HirExpr::Var("keep_going".to_string()),
        },
        HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
    ];

    assert!(!collapse_loop_exit_alias_returns(&mut stmts));
}

#[test]
fn eliminate_dead_temp_assigns_removes_dead_preserved_temp() {
    let mut stmts = vec![HirStmt::Assign {
        lhs: HirLValue::Var("uVar0".to_string()),
        rhs: HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("eax".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        },
    }];

    assert!(eliminate_dead_temp_assigns(
        &mut stmts,
        &HashSet::from(["uVar0"]),
    ));
    assert!(stmts.is_empty());
}

#[test]
fn prune_unreachable_after_return_stops_at_label_boundary() {
    let mut stmts = vec![
        HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        HirStmt::Assign {
            lhs: HirLValue::Var("dead".to_string()),
            rhs: HirExpr::Const(1, int(32)),
        },
        HirStmt::Goto("kept".to_string()),
        HirStmt::Label("kept".to_string()),
        HirStmt::Return(None),
    ];

    assert!(prune_unreachable_after_terminal(&mut stmts));
    assert_eq!(
        stmts,
        vec![
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            HirStmt::Label("kept".to_string()),
            HirStmt::Return(None),
        ]
    );
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp() {
    let mut func = HirFunction {
        name: "test_preserved_prune".to_string(),
        params: vec![],
        locals: vec![preserved_temp_binding("uVar0", 32)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_plain_temp_with_nontrivial_name() {
    let mut func = HirFunction {
        name: "test_plain_temp_prune".to_string(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_removes_dead_preserved_temp_with_nontrivial_name() {
    let mut func = HirFunction {
        name: "test_preserved_named_register_prune".to_string(),
        params: vec![],
        locals: vec![preserved_temp_binding("rcx", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32))))],
        ..Default::default()
    };

    assert!(prune_unused_temp_bindings(&mut func));
    assert!(func.locals.is_empty());
}

#[test]
fn prune_unused_temp_bindings_keeps_used_plain_temp_with_nontrivial_name() {
    let mut func = HirFunction {
        name: "test_plain_temp_used".to_string(),
        params: vec![],
        locals: vec![temp_binding("rcx", 64)],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Var("rcx".to_string())))],
        ..Default::default()
    };

    assert!(!prune_unused_temp_bindings(&mut func));
    assert_eq!(func.locals.len(), 1);
    assert_eq!(func.locals[0].name, "rcx");
}

#[test]
fn prune_unused_temp_bindings_keeps_side_effect_assignment_target() {
    let mut func = HirFunction {
        name: "test_side_effect_lhs_preserved".to_string(),
        params: vec![],
        locals: vec![preserved_temp_binding("xVar30", 64)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("xVar30".to_string()),
            rhs: HirExpr::Call {
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
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Const(0, int(32)),
        },
        HirStmt::Label("loop_head".to_string()),
        HirStmt::If {
            cond: HirExpr::Var("xVar0".to_string()),
            then_body: vec![HirStmt::Goto("loop_head".to_string())],
            else_body: Vec::new(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::new()));
    assert!(matches!(
        &stmts[2],
        HirStmt::If {
            cond: HirExpr::Var(name),
            ..
        } if name == "xVar0"
    ));
}

#[test]
fn inline_single_use_temps_keeps_same_linear_segment_inline() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Const(1, int(32)),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar1".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("xVar0".to_string())),
                rhs: Box::new(HirExpr::Const(2, int(32))),
                ty: int(32),
            },
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::new()));
    assert_eq!(stmts.len(), 1);
    let HirStmt::Assign { rhs, .. } = &stmts[0] else {
        panic!("expected assignment");
    };
    assert!(!expr_contains_var(rhs, "xVar0"));
}

#[test]
fn inline_single_use_temps_inlines_flag_intrinsic_into_predicate() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Call {
                target: "__sborrow".to_string(),
                args: vec![
                    HirExpr::Var("param_1".to_string()),
                    HirExpr::Const(1, int(32)),
                ],
                ty: NirType::Bool,
            },
        },
        HirStmt::If {
            cond: HirExpr::Var("xVar0".to_string()),
            then_body: Vec::new(),
            else_body: Vec::new(),
        },
    ];

    assert!(inline_single_use_temps(&mut stmts, &HashSet::new()));
    assert_eq!(stmts.len(), 1);
    let HirStmt::If { cond, .. } = &stmts[0] else {
        panic!("expected if");
    };
    assert!(matches!(cond, HirExpr::Call { target, .. } if target == "__sborrow"));
}

#[test]
fn inline_loop_condition_trailing_temps_substitutes_condition_chain() {
    let mut func = HirFunction {
        name: "test_loop_cond_inline".to_string(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::DoWhile {
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("sum".to_string())),
                        rhs: Box::new(HirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar38".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(HirExpr::Var("ptr".to_string())),
                        rhs: Box::new(HirExpr::Var("end".to_string())),
                        ty: int(64),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar39".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(HirExpr::Var("xVar38".to_string())),
                        rhs: Box::new(HirExpr::Const(0, int(64))),
                        ty: NirType::Bool,
                    },
                },
            ],
            cond: HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("xVar39".to_string())),
                ty: NirType::Bool,
            },
        }],
        ..Default::default()
    };

    assert!(inline_loop_condition_trailing_temps(
        &mut func,
    ));
    let HirStmt::DoWhile { body, cond } = &func.body[0] else {
        panic!("expected do-while");
    };
    assert_eq!(body.len(), 1);
    assert!(matches!(
        cond,
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } if matches!(
            expr.as_ref(),
            HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs,
                ..
            } if matches!(
                lhs.as_ref(),
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    ..
                }
            )
        )
    ));
}

#[test]
fn inline_single_use_temps_keeps_unknown_call_out_of_predicate() {
    let mut stmts = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Call {
                target: "unknown_helper".to_string(),
                args: vec![HirExpr::Var("param_1".to_string())],
                ty: int(32),
            },
        },
        HirStmt::If {
            cond: HirExpr::Var("xVar0".to_string()),
            then_body: Vec::new(),
            else_body: Vec::new(),
        },
    ];

    assert!(!inline_single_use_temps(&mut stmts, &HashSet::new()));
    assert_eq!(stmts.len(), 2);
}
