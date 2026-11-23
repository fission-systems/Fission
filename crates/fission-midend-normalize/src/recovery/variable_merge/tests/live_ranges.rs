use super::super::apply_variable_merge_pass;
use crate::prelude::*;

#[test]
fn test_loop_live_range_collector() {
    let b1 = PreHirBinding {
        name: "temp_1".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "temp_2".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_loop_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // temp_1 is defined before the loop
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_1".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            // Loop
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    // Inside the loop, temp_1 is read
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("dummy".to_string()),
                        rhs: PreHirExpr::Var("temp_1".to_string()),
                    },
                    // temp_2 is defined and read inside the loop, after temp_1's read
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("temp_2".to_string()),
                        rhs: PreHirExpr::Const(
                            2,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("dummy2".to_string()),
                        rhs: PreHirExpr::Var("temp_2".to_string()),
                    },
                ]
                .into(),
            },
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    // They should NOT be merged because temp_1 is live across the entire loop body,
    // which overlaps with temp_2.
    assert!(!changed);
    assert_eq!(func.locals.len(), 2);
}

#[test]
fn loop_carried_local_is_not_merged_with_an_earlier_loop_temporary() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let binding = |name: &str| PreHirBinding {
        name: name.to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };
    let mut func = PreHirFunction {
        name: "loop_carried_merge_guard".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![binding("temp_early"), binding("temp_carried")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("temp_early".to_string()),
                    rhs: PreHirExpr::Const(7, int_ty.clone()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("discard_early".to_string()),
                    rhs: PreHirExpr::Var("temp_early".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("discard_carried".to_string()),
                    rhs: PreHirExpr::Var("temp_carried".to_string()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("temp_carried".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("temp_carried".to_string())),
                        rhs: Box::new(PreHirExpr::Const(1, int_ty.clone())),
                        ty: int_ty.clone(),
                    },
                },
            ]
            .into(),
        }],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);

    assert!(
        !changed,
        "loop-carried state must not share an earlier slot"
    );
    assert_eq!(func.locals.len(), 2);
}

#[test]
fn test_unstructured_loop_live_range_collector() {
    let b1 = PreHirBinding {
        name: "temp_1".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "temp_2".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_loop_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // temp_1 is defined before the loop
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_1".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            // Loop header label
            PreHirStmt::Label("loop_start".to_string()),
            // Inside the loop, temp_1 is read
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("dummy".to_string()),
                rhs: PreHirExpr::Var("temp_1".to_string()),
            },
            // temp_2 is defined and read inside the loop, after temp_1's read
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_2".to_string()),
                rhs: PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("dummy2".to_string()),
                rhs: PreHirExpr::Var("temp_2".to_string()),
            },
            // Loop backedge goto
            PreHirStmt::Goto("loop_start".to_string()),
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    // They should NOT be merged because temp_1 is live across the entire unstructured loop body,
    // which overlaps with temp_2.
    assert!(!changed);
    assert_eq!(func.locals.len(), 2);
}
