use super::super::apply_use_driven_type_infer_pass;
use super::super::copy_constraints::collect_pointer_assignment_base_constraints;
use super::{make_binding, make_func, make_typed_binding};
use crate::HashMap;
use crate::prelude::*;

#[test]
fn copy_lifetime_legacy_constraints_preserve_multi_definition_scalar_flag() {
    let scalar = NirType::Int {
        bits: 32,
        signed: true,
    };
    let word = NirType::Int {
        bits: 64,
        signed: false,
    };
    let mut func = make_func(
        vec![
            make_typed_binding("carrier", word.clone(), NirBindingOrigin::Temp),
            make_typed_binding("flag", scalar.clone(), NirBindingOrigin::Temp),
        ],
        vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Const(0, word),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("flag".into()),
                rhs: PreHirExpr::Var("carrier".into()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("condition".into()),
                then_body: vec![PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("flag".into()),
                    rhs: PreHirExpr::Const(1, scalar.clone()),
                }]
                .into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Call {
                    origin: None,
                    target: "produce_pointer".into(),
                    args: vec![],
                    ty: NirType::Ptr(Box::new(NirType::Unknown)),
                },
            },
        ],
        NirType::Unknown,
    );
    for _ in 0..4 {
        apply_use_driven_type_infer_pass(&mut func);
    }
    assert_eq!(func.locals[1].ty, scalar);
}

#[test]
fn copy_lifetime_legacy_constraints_delegate_stable_pointer_alias() {
    let pointer = NirType::Ptr(Box::new(NirType::Float { bits: 32 }));
    let mut func = make_func(
        vec![
            make_typed_binding("source", pointer.clone(), NirBindingOrigin::Temp),
            make_binding("alias"),
        ],
        vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("alias".into()),
            rhs: PreHirExpr::Var("source".into()),
        }],
        NirType::Unknown,
    );
    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(func.locals[1].ty, pointer);
}

#[test]
fn operation_copy_pointer_definition_survives_null_check_and_carrier_reuse() {
    let word = NirType::Int {
        bits: 64,
        signed: false,
    };
    let pointer = NirType::Ptr(Box::new(NirType::Unknown));
    let mut func = make_func(
        vec![
            make_typed_binding("carrier", word.clone(), NirBindingOrigin::Temp),
            make_typed_binding("saved", word.clone(), NirBindingOrigin::StackOffset(-8)),
        ],
        vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Call {
                    origin: None,
                    target: "pointer_result".into(),
                    args: vec![],
                    ty: pointer.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("saved".into()),
                rhs: PreHirExpr::Var("carrier".into()),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Var("saved".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![].into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Const(1, word.clone()),
            },
        ],
        NirType::Unknown,
    );
    func.is_64bit = true;
    for _ in 0..4 {
        apply_use_driven_type_infer_pass(&mut func);
    }
    assert_eq!(func.locals[1].ty, pointer);

    // Another destination value invalidates the pointer-only definition
    // proof even when the first assignment is an adjacent typed COPY.
    func.body.push(PreHirStmt::Assign {
        lhs: PreHirLValue::Var("saved".into()),
        rhs: PreHirExpr::Const(1, word.clone()),
    });
    func.locals[1].ty = word;
    apply_use_driven_type_infer_pass(&mut func);
    assert!(!matches!(func.locals[1].ty, NirType::Ptr(_)));
}

#[test]
fn copy_lifetime_does_not_reverse_pointer_into_redefined_unknown_source() {
    let mut func = make_func(
        vec![
            make_binding("carrier"),
            make_typed_binding(
                "saved",
                NirType::Ptr(Box::new(NirType::Unknown)),
                NirBindingOrigin::Temp,
            ),
        ],
        vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Call {
                    origin: None,
                    target: "unknown_result_a".into(),
                    args: vec![],
                    ty: NirType::Unknown,
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("saved".into()),
                rhs: PreHirExpr::Var("carrier".into()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("carrier".into()),
                rhs: PreHirExpr::Call {
                    origin: None,
                    target: "unknown_result_b".into(),
                    args: vec![],
                    ty: NirType::Unknown,
                },
            },
        ],
        NirType::Unknown,
    );
    for _ in 0..4 {
        apply_use_driven_type_infer_pass(&mut func);
    }
    assert_eq!(func.locals[0].ty, NirType::Unknown);
}

#[test]
fn propagates_pointer_use_back_through_copy_edge() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("p".to_owned()),
            rhs: PreHirExpr::Var("param_1".to_owned()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                ty: uint_ty.clone(),
            },
            rhs: PreHirExpr::Const(7, uint_ty.clone()),
        },
    ];
    let mut func = PreHirFunction {
        name: "copy_ptr".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![make_typed_binding(
            "param_1",
            NirType::Unknown,
            NirBindingOrigin::ParamIndex(0),
        )],
        locals: vec![make_binding("p")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    let expected = NirType::Ptr(Box::new(uint_ty));
    assert_eq!(func.locals[0].ty, expected);
    assert_eq!(func.params[0].ty, func.locals[0].ty);
}

#[test]
fn propagates_pointer_use_back_through_scaled_pointer_assignment() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("p".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(4, u64_ty.clone())),
                    ty: u64_ty.clone(),
                }),
                ty: u64_ty,
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                ty: uint_ty.clone(),
            },
            rhs: PreHirExpr::Const(7, uint_ty.clone()),
        },
    ];
    let mut func = PreHirFunction {
        name: "scaled_ptr".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![make_typed_binding(
            "param_1",
            NirType::Unknown,
            NirBindingOrigin::ParamIndex(0),
        )],
        locals: vec![make_binding("p"), make_binding("idx")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    let expected = NirType::Ptr(Box::new(uint_ty));
    assert_eq!(func.locals[0].ty, expected);
    assert_eq!(func.params[0].ty, func.locals[0].ty);
    assert_eq!(func.locals[1].ty, NirType::Unknown);
}

#[test]
fn no_bilateral_pointer_promotion_on_variable_addition() {
    let mut constraints = HashMap::default();
    let known_binding_types = HashMap::default();
    let ptr_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: true,
    }));

    // ptr = a + b (neither has integer offset evidence)
    let add_vars = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("a".into())),
        rhs: Box::new(PreHirExpr::Var("b".into())),
        ty: NirType::Unknown,
    };
    collect_pointer_assignment_base_constraints(
        &add_vars,
        &ptr_ty,
        &known_binding_types,
        &mut constraints,
    );
    assert!(
        constraints.is_empty(),
        "neither operand in unconstrained a + b should receive pointer promotion"
    );

    // ptr = a + 16 (16 is a numeric offset constant)
    let add_const = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("a".into())),
        rhs: Box::new(PreHirExpr::Const(
            16,
            NirType::Int {
                bits: 64,
                signed: false,
            },
        )),
        ty: NirType::Unknown,
    };
    collect_pointer_assignment_base_constraints(
        &add_const,
        &ptr_ty,
        &known_binding_types,
        &mut constraints,
    );
    assert!(constraints.contains_key("a"));
    assert!(!constraints.contains_key("b"));
}
