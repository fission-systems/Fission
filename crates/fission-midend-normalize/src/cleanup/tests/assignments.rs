use super::*;

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
                origin: None,
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
                origin: None,
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
                origin: None,
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
