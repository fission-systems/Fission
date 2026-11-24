use super::counted_loops::try_upgrade_infinite_for_with_tail_update;
use super::loop_expressions::{strip_casts, substitute_var_in_expr};
use super::loop_latch::{parse_break_eq_var, parse_self_add_assign, tail_meaningful_stmts};
use super::merged_induction::try_split_merged_dual_iv_tail;
use super::{apply_break_continue_pass, apply_iv_recovery_pass};
use crate::prelude::*;

fn int(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn ptr_u32() -> NirType {
    NirType::Ptr(Box::new(int(32, false)))
}

fn var(name: &str) -> PreHirExpr {
    PreHirExpr::Var(name.to_string())
}

fn const_i(value: i64) -> PreHirExpr {
    PreHirExpr::Const(value, int(64, true))
}

fn add(lhs: PreHirExpr, rhs: PreHirExpr, ty: NirType) -> PreHirExpr {
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty,
    }
}

fn ne(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Ne,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

#[test]
fn guarded_pointer_dowhile_upgrades_to_for() {
    let mut func = PreHirFunction {
        name: "guarded_pointer_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLe,
                    lhs: Box::new(var("len")),
                    rhs: Box::new(const_i(0)),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("exit".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("end".to_string()),
                rhs: add(var("ptr"), var("len"), ptr_u32()),
            },
            PreHirStmt::DoWhile {
                body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("value".to_string()),
                        rhs: PreHirExpr::Load {
                            ptr: Box::new(var("ptr")),
                            ty: int(32, false),
                        },
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("ptr".to_string()),
                        rhs: add(var("ptr"), const_i(1), ptr_u32()),
                    },
                ]
                .into(),
                cond: ne(var("ptr"), var("end")),
            },
            PreHirStmt::Label("exit".to_string()),
            PreHirStmt::Return(None),
        ],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    let PreHirStmt::For {
        init,
        cond,
        update,
        body,
    } = &func.body[2]
    else {
        panic!("expected guarded do-while to become for");
    };
    assert!(init.is_some());
    assert!(matches!(
        cond,
        Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        }) if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "iVar0")
            && matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "len")
    ));
    assert!(update.is_some());
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs:
                PreHirExpr::Index {
                    base,
                    index,
                    elem_ty
                },
        } if name == "value"
            && matches!(base.as_ref(), PreHirExpr::Var(name) if name == "ptr")
            && matches!(index.as_ref(), PreHirExpr::Var(name) if name == "iVar0")
            && *elem_ty == int(32, false)
    ));
    assert!(func.locals.iter().any(|local| local.name == "iVar0"
        && local.ty
            == (NirType::Int {
                bits: 64,
                signed: true,
            })));
}

#[test]
fn unguarded_pointer_dowhile_stays_dowhile() {
    let mut func = PreHirFunction {
        name: "unguarded_pointer_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("end".to_string()),
                rhs: add(var("ptr"), var("len"), ptr_u32()),
            },
            PreHirStmt::DoWhile {
                body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("ptr".to_string()),
                    rhs: add(var("ptr"), const_i(1), ptr_u32()),
                }]
                .into(),
                cond: ne(var("ptr"), var("end")),
            },
        ],
        ..Default::default()
    };

    assert!(!apply_iv_recovery_pass(&mut func));
    assert!(matches!(func.body[1], PreHirStmt::DoWhile { .. }));
}

#[test]
fn early_return_guarded_pointer_dowhile_upgrades_to_indexed_for() {
    let mut func = PreHirFunction {
        name: "early_return_guarded_pointer_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: int(32, false),
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLe,
                    lhs: Box::new(var("len")),
                    rhs: Box::new(const_i(0)),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    0,
                    int(32, false),
                )))]
                .into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("end".to_string()),
                rhs: add(var("ptr"), var("len"), ptr_u32()),
            },
            PreHirStmt::DoWhile {
                body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("sum".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(var("sum")),
                            rhs: Box::new(PreHirExpr::Load {
                                ptr: Box::new(var("ptr")),
                                ty: int(32, false),
                            }),
                            ty: int(32, false),
                        },
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("ptr".to_string()),
                        rhs: add(var("ptr"), const_i(1), ptr_u32()),
                    },
                ]
                .into(),
                cond: ne(var("ptr"), var("end")),
            },
            PreHirStmt::Return(Some(var("sum"))),
        ],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    let PreHirStmt::For { body, cond, .. } = &func.body[2] else {
        panic!("expected early-return guarded do-while to become for");
    };
    assert!(matches!(
        cond,
        Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        }) if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "iVar0")
            && matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "len")
    ));
    assert!(matches!(
        &body[0],
        PreHirStmt::Assign {
            rhs:
                PreHirExpr::Binary {
                    rhs,
                    ..
                },
            ..
        } if matches!(
            rhs.as_ref(),
            PreHirExpr::Index { base, index, .. }
                if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "ptr")
                    && matches!(index.as_ref(), PreHirExpr::Var(name) if name == "iVar0")
        )
    ));
}

#[test]
fn tail_label_counted_loop_becomes_break_guarded_for() {
    let mut func = PreHirFunction {
        name: "tail_label_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: const_i(0),
            },
            PreHirStmt::Label("head".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("sum".to_string()),
                rhs: add(var("sum"), var("i"), int(64, true)),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: add(var("i"), const_i(1), int(64, true)),
            },
            PreHirStmt::If {
                cond: ne(var("i"), var("n")),
                then_body: vec![PreHirStmt::Goto("head".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(None),
        ],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    assert!(matches!(func.body[0], PreHirStmt::Assign { .. }));
    let PreHirStmt::For {
        init,
        cond,
        update,
        body,
    } = &func.body[1]
    else {
        panic!("expected tail label loop to become for");
    };
    assert!(init.is_none());
    assert!(cond.is_none());
    assert!(update.is_none());
    assert_eq!(body.len(), 3);
    assert!(matches!(
        body.last(),
        Some(PreHirStmt::If {
            cond:
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs,
                    rhs,
                    ..
                },
            then_body,
            else_body,
        }) if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "i")
            && matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "n")
            && matches!(then_body.as_slice(), [PreHirStmt::Break])
            && else_body.is_empty()
    ));
    assert!(matches!(func.body[2], PreHirStmt::Return(None)));
    assert!(
        !func
            .body
            .iter()
            .any(|stmt| matches!(stmt, PreHirStmt::Label(label) if label == "head"))
    );
}

#[test]
fn tail_label_loop_allows_body_local_goto() {
    let mut func = PreHirFunction {
        name: "tail_label_loop_with_local_goto".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Label("head".to_string()),
            PreHirStmt::If {
                cond: var("flag"),
                then_body: vec![PreHirStmt::Goto("inside".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("sum".to_string()),
                rhs: add(var("sum"), const_i(1), int(64, true)),
            },
            PreHirStmt::Label("inside".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: add(var("i"), const_i(1), int(64, true)),
            },
            PreHirStmt::If {
                cond: ne(var("i"), var("n")),
                then_body: vec![PreHirStmt::Goto("head".to_string())].into(),
                else_body: Vec::new().into(),
            },
        ],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    let PreHirStmt::For { body, .. } = &func.body[0] else {
        panic!("expected local-goto tail loop to become for");
    };
    assert!(
        body.iter()
            .any(|stmt| matches!(stmt, PreHirStmt::Label(label) if label == "inside"))
    );
}

#[test]
fn tail_label_loop_rejects_nonlocal_body_goto() {
    let mut func = PreHirFunction {
        name: "tail_label_loop_with_external_goto".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Label("head".to_string()),
            PreHirStmt::If {
                cond: var("flag"),
                then_body: vec![PreHirStmt::Goto("exit".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: add(var("i"), const_i(1), int(64, true)),
            },
            PreHirStmt::If {
                cond: ne(var("i"), var("n")),
                then_body: vec![PreHirStmt::Goto("head".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("exit".to_string()),
        ],
        ..Default::default()
    };

    assert!(!apply_iv_recovery_pass(&mut func));
    assert!(matches!(func.body[0], PreHirStmt::Label(_)));
}

#[test]
fn tail_label_loop_rejects_multiple_backedges_to_head() {
    let mut func = PreHirFunction {
        name: "tail_label_loop_with_multiple_backedges".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Label("head".to_string()),
            PreHirStmt::If {
                cond: var("retry"),
                then_body: vec![PreHirStmt::Goto("head".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: add(var("i"), const_i(1), int(64, true)),
            },
            PreHirStmt::If {
                cond: ne(var("i"), var("n")),
                then_body: vec![PreHirStmt::Goto("head".to_string())].into(),
                else_body: Vec::new().into(),
            },
        ],
        ..Default::default()
    };

    assert!(!apply_iv_recovery_pass(&mut func));
    assert!(matches!(func.body[0], PreHirStmt::Label(_)));
}

#[test]
fn for_loop_dataflow_simple() {
    // init: i = 0
    // while (i < n) {
    //   t = i + 1;
    //   i = t;
    // }
    // The dataflow walk should trace i through t back to i + 1, verifying that i is loop-carried
    // and has a valid linear iteration update!
    let mut func = PreHirFunction {
        name: "dataflow_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: const_i(0),
            },
            PreHirStmt::While {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(var("i")),
                    rhs: Box::new(var("n")),
                    ty: NirType::Bool,
                },
                body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("t".to_string()),
                        rhs: add(var("i"), const_i(1), int(64, true)),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: var("t"),
                    },
                ]
                .into(),
            },
        ],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    let PreHirStmt::For {
        init,
        cond,
        update,
        body,
    } = &func.body[0]
    else {
        panic!("Expected loop to become a For loop!");
    };
    assert!(init.is_some());
    assert!(cond.is_some());
    assert!(update.is_some());
    // The body should have had the update statement (i = t) removed, and only contain t = i + 1.
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            ..
        } if lhs == "t"
    ));
}

#[test]
fn for_loop_dataflow_invalid_no_loop_var() {
    // init: i = 0
    // while (i < n) {
    //   t = 42;
    //   i = t;
    // }
    // This is not a linear/affine recurrence of i, so it should fail to upgrade!
    let mut func = PreHirFunction {
        name: "invalid_dataflow_loop".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: const_i(0),
            },
            PreHirStmt::While {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(var("i")),
                    rhs: Box::new(var("n")),
                    ty: NirType::Bool,
                },
                body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("t".to_string()),
                        rhs: const_i(42),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: var("t"),
                    },
                ]
                .into(),
            },
        ],
        ..Default::default()
    };

    assert!(!apply_iv_recovery_pass(&mut func));
}

// ── for(;;) with tail-label update pattern ─────────────────────────────

fn eq_expr(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Eq,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

/// Build:
///   i = 0;
///   for (;;) {
///       <work_stmts>
///   update_label:
///       i = i + 1;
///       if (i == limit) break;
///   }
fn make_infinite_for_with_tail(
    work_stmts: Vec<PreHirStmt>,
    iter_var: &str,
    limit: &str,
    update_label: &str,
) -> (Vec<PreHirStmt>, NirType) {
    let ty = int(64, false);
    let body = {
        let mut b = work_stmts;
        b.push(PreHirStmt::Label(update_label.to_string()));
        b.push(PreHirStmt::Assign {
            lhs: PreHirLValue::Var(iter_var.to_string()),
            rhs: add(var(iter_var), const_i(1), ty.clone()),
        });
        b.push(PreHirStmt::If {
            cond: eq_expr(var(iter_var), var(limit)),
            then_body: vec![PreHirStmt::Break].into(),
            else_body: vec![].into(),
        });
        b
    };
    let stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(iter_var.to_string()),
            rhs: const_i(0),
        },
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body: body.into(),
        },
    ];
    (stmts, ty)
}

#[test]
fn upgrades_infinite_for_with_tail_update_no_inner_goto() {
    // Simple case: for(;;) body has no goto to the update label.
    // Expected: for (i = 0; i != limit; i = i + 1) { work }
    let (mut stmts, _ty) = make_infinite_for_with_tail(
        vec![PreHirStmt::Expr(PreHirExpr::Const(42, int(64, false)))],
        "i",
        "limit",
        "update_lbl",
    );

    assert!(try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
    assert_eq!(stmts.len(), 1, "init should be absorbed into for-init");

    let PreHirStmt::For {
        init,
        cond,
        update,
        body,
    } = &stmts[0]
    else {
        panic!("expected For");
    };
    assert!(init.is_some(), "init should be set");
    assert!(cond.is_some(), "cond should be set");
    assert!(update.is_some(), "update should be set");
    assert_eq!(body.len(), 1, "body should have work stmt only");

    // Condition should be the negation: i != limit
    let cond = cond.as_ref().unwrap();
    assert!(
        matches!(
            cond,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Ne,
                ..
            }
        ),
        "cond should be Ne (inverted from Eq), got: {cond:?}"
    );
}

#[test]
fn upgrades_infinite_for_and_replaces_inner_goto_with_continue() {
    // for(;;) body contains `if (early) goto update_lbl` — should become `continue`.
    let work_stmts = vec![
        PreHirStmt::If {
            cond: var("early"),
            then_body: vec![PreHirStmt::Goto("update_lbl".to_string())].into(),
            else_body: vec![].into(),
        },
        PreHirStmt::Expr(PreHirExpr::Const(99, int(64, false))),
    ];
    let (mut stmts, _ty) = make_infinite_for_with_tail(work_stmts, "i", "limit", "update_lbl");

    assert!(try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));

    let PreHirStmt::For { body, .. } = &stmts[0] else {
        panic!("expected For");
    };
    // Body should have the if-continue and the expr, no label or break-if.
    assert_eq!(body.len(), 2);
    let PreHirStmt::If { then_body, .. } = &body[0] else {
        panic!("expected If");
    };
    assert_eq!(
        then_body.as_slice(),
        [PreHirStmt::Continue],
        "goto should have been replaced with continue"
    );
}

#[test]
fn does_not_upgrade_infinite_for_when_no_init_before_loop() {
    // No assignment to `i` immediately before the loop → no transformation.
    let body = vec![
        PreHirStmt::Label("lbl".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("i".to_string()),
            rhs: add(var("i"), const_i(1), int(64, false)),
        },
        PreHirStmt::If {
            cond: eq_expr(var("i"), var("limit")),
            then_body: vec![PreHirStmt::Break].into(),
            else_body: vec![].into(),
        },
    ];
    let mut stmts = vec![
        PreHirStmt::Expr(PreHirExpr::Const(0, int(64, false))), // unrelated stmt, not an init for i
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body: body.into(),
        },
    ];

    assert!(!try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
}

#[test]
fn does_not_upgrade_when_body_has_explicit_continue() {
    // An existing `continue` in the body blocks the transformation.
    let work_stmts = vec![
        PreHirStmt::Continue, // existing continue — safety check must reject
        PreHirStmt::Expr(PreHirExpr::Const(1, int(64, false))),
    ];
    let (mut stmts, _) = make_infinite_for_with_tail(work_stmts, "i", "limit", "update_lbl");

    assert!(!try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
}

#[test]
fn splits_merged_dual_iv_tail_on_infinite_for() {
    let ty = int(64, false);
    let body = vec![
        PreHirStmt::Expr(PreHirExpr::Const(1, ty.clone())),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("off".to_string()),
            rhs: add(var("off"), const_i(1), ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("off".to_string()),
            rhs: add(var("off"), var("cols"), ty.clone()),
        },
        PreHirStmt::If {
            cond: eq_expr(var("rows"), var("off")),
            then_body: vec![PreHirStmt::Break].into(),
            else_body: vec![].into(),
        },
    ];
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("off".to_string()),
            rhs: const_i(0),
        },
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body: body.into(),
        },
    ];
    let mut locals = Vec::new();
    assert!(try_split_merged_dual_iv_tail(&mut stmts, 1, &mut locals));
    assert_eq!(stmts.len(), 3, "off init + row init + for");
    assert_eq!(locals.len(), 1, "fresh row counter local");

    let PreHirStmt::For { body, .. } = &stmts[2] else {
        panic!("expected For");
    };
    let tail = tail_meaningful_stmts(body, 3).expect("tail triple");
    let (_, step_one) = &tail[0];
    let (_, step_k) = &tail[1];
    let (_, break_if) = &tail[2];

    let (row, _) = parse_self_add_assign(step_one).expect("row += 1");
    assert_ne!(row, "off");
    let (off, addend) = parse_self_add_assign(step_k).expect("off += cols");
    assert_eq!(off, "off");
    assert!(matches!(strip_casts(&addend), PreHirExpr::Var(v) if v == "cols"));
    let (_, break_var) = parse_break_eq_var(break_if).expect("rows == row");
    assert_eq!(break_var, row);
}

#[test]
fn recovers_row_stride_fill_inner_loop_in_infinite_for() {
    let ty = int(64, false);
    let u32_ty = int(32, false);
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("tmp".to_string()),
            rhs: PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(var("row_off")),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor".to_string()),
            rhs: add(var("matrix"), var("tmp"), ptr_u32()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("tmp".to_string()),
            rhs: add(var("tmp"), var("cols"), ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("end".to_string()),
            rhs: add(var("matrix"), var("tmp"), ptr_u32()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("end".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(var("end")),
                rhs: Box::new(var("cursor")),
                ty: ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("end".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(PreHirExpr::Cast {
                    ty: ty.clone(),
                    expr: Box::new(var("end")),
                }),
                rhs: Box::new(PreHirExpr::Const(4, ty.clone())),
                ty: ty.clone(),
            },
        },
        PreHirStmt::Label("inner".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(var("cursor")),
                ty: u32_ty.clone(),
            },
            rhs: var("value"),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor".to_string()),
            rhs: add(var("cursor"), const_i(1), ptr_u32()),
        },
        PreHirStmt::Goto("inner".to_string()),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("row".to_string()),
            rhs: add(var("row"), const_i(1), ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("row_off".to_string()),
            rhs: add(var("row_off"), var("cols"), ty.clone()),
        },
        PreHirStmt::If {
            cond: eq_expr(var("rows"), var("row")),
            then_body: vec![PreHirStmt::Break].into(),
            else_body: vec![].into(),
        },
    ];
    let mut func = PreHirFunction {
        name: "row_stride_fill".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body: body.into(),
        }],
        ..Default::default()
    };

    assert!(apply_iv_recovery_pass(&mut func));
    let PreHirStmt::For { body, .. } = &func.body[0] else {
        panic!("expected For");
    };
    assert!(
        body.iter()
            .any(|stmt| matches!(stmt, PreHirStmt::For { .. })),
        "expected nested inner for, got {body:?}"
    );
    let inner = body
        .iter()
        .find_map(|stmt| {
            if let PreHirStmt::For {
                init,
                cond,
                update,
                body: inner_body,
            } = stmt
            {
                Some((
                    init.is_some(),
                    cond.is_some(),
                    update.is_some(),
                    inner_body.len(),
                ))
            } else {
                None
            }
        })
        .expect("inner for");
    assert!(inner.0 && inner.1 && inner.2);
    assert_eq!(inner.3, 1);
}

#[test]
fn recovers_multiple_breaks_in_loop() {
    let mut func = PreHirFunction {
        name: "test_multi_break".to_string(),
        body: vec![
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("cond1".to_string()),
                        then_body: vec![PreHirStmt::Goto("loop_end".to_string())].into(),
                        else_body: vec![].into(),
                    },
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("cond2".to_string()),
                        then_body: vec![PreHirStmt::Goto("loop_end".to_string())].into(),
                        else_body: vec![].into(),
                    },
                ]
                .into(),
            },
            PreHirStmt::Label("loop_end".to_string()),
            PreHirStmt::Return(None),
        ],
        ..Default::default()
    };

    assert!(apply_break_continue_pass(&mut func));
    let PreHirStmt::While { body, .. } = &func.body[0] else {
        panic!("expected While");
    };
    for stmt in body.iter() {
        let PreHirStmt::If { then_body, .. } = stmt else {
            panic!("expected If");
        };
        assert_eq!(then_body.as_slice(), &[PreHirStmt::Break]);
    }
}

#[test]
fn substitution_reaches_every_call_argument() {
    let var = |name: &str| PreHirExpr::Var(name.to_string());
    let mut expr = PreHirExpr::Call {
        origin: None,
        target: "observe".to_string(),
        args: vec![var("old"), var("other"), var("old")],
        ty: NirType::Unknown,
    };

    assert!(substitute_var_in_expr(&mut expr, "old", &var("new")));
    let PreHirExpr::Call { args, .. } = &expr else {
        panic!("call kept");
    };
    assert_eq!(args, &[var("new"), var("other"), var("new")]);
}

fn goto_if(cond: &str, label: &str) -> PreHirStmt {
    PreHirStmt::If {
        cond: PreHirExpr::Var(cond.to_string()),
        then_body: vec![PreHirStmt::Goto(label.to_string())].into(),
        else_body: vec![].into(),
    }
}

#[test]
fn goto_past_code_after_loop_is_not_a_break() {
    // `skip` is reached past `side_effect()`, which a break would run.
    let mut func = PreHirFunction {
        body: vec![
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![goto_if("c", "skip")].into(),
            },
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "side_effect".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            }),
            PreHirStmt::Label("skip".to_string()),
            PreHirStmt::Return(None),
        ],
        ..Default::default()
    };
    assert!(!apply_break_continue_pass(&mut func));
}

#[test]
fn goto_to_label_before_do_while_is_not_a_continue() {
    // A do-while's `continue` evaluates the condition; jumping back to
    // `top` re-enters the body unconditionally.
    let mut func = PreHirFunction {
        body: vec![
            PreHirStmt::Label("top".to_string()),
            PreHirStmt::DoWhile {
                body: vec![goto_if("c", "top")].into(),
                cond: PreHirExpr::Var("more".to_string()),
            },
            PreHirStmt::Return(None),
        ],
        ..Default::default()
    };
    assert!(!apply_break_continue_pass(&mut func));
}
