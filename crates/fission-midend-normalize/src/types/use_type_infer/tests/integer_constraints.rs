use super::super::apply_use_driven_type_infer_pass;
use super::{make_binding, make_func, make_typed_binding};
use crate::prelude::*;

/// SLt comparison → operand is signed int
#[test]
fn infers_signed_from_slt() {
    let body = vec![PreHirStmt::If {
        cond: PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        },
        then_body: vec![].into(),
        else_body: vec![].into(),
    }];
    let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(
        func.locals[0].ty,
        NirType::Int {
            bits: 32,
            signed: true
        }
    );
}

#[test]
fn signed_compare_promotes_unsigned_params_and_return() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let body = vec![PreHirStmt::If {
        cond: PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Var("b".to_owned())),
            ty: NirType::Bool,
        },
        then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("b".to_owned())))].into(),
        else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("a".to_owned())))].into(),
    }];
    let mut func = PreHirFunction {
        name: "signed_max".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("b", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: u32_ty,
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    let signed_i32 = NirType::Int {
        bits: 32,
        signed: true,
    };
    assert_eq!(func.params[0].ty, signed_i32);
    assert_eq!(func.params[1].ty, signed_i32);
    assert_eq!(func.return_type, signed_i32);
}

#[test]
fn logical_shr_demotes_signed_param_to_unsigned() {
    // count_bits-style: signed stack param used with INT_RIGHT must become uint
    // so C `>>` is logical and `0xFFFFFFFF` terminates.
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("x".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs: Box::new(PreHirExpr::Var("x".to_owned())),
            rhs: Box::new(PreHirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: i32_ty.clone(),
        },
    }];
    let mut func = PreHirFunction {
        name: "count_bits".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![make_typed_binding(
            "x",
            i32_ty,
            NirBindingOrigin::ParamIndex(0),
        )],
        locals: vec![],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body,
        ..Default::default()
    };
    func.is_64bit = false;

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(
        func.params[0].ty,
        NirType::Int {
            bits: 32,
            signed: false,
        },
        "logical SHR must force unsigned param typing"
    );
}

#[test]
fn logical_shr_unsigned_constraint_reaches_param_through_copy_alias() {
    let i64_ty = NirType::Int {
        bits: 64,
        signed: true,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("shifted".into()),
            rhs: PreHirExpr::Var("count".into()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("shifted".into()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(PreHirExpr::Var("shifted".into())),
                rhs: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                ty: i64_ty.clone(),
            },
        },
    ];
    let mut func = PreHirFunction {
        name: "test".into(),
        params: vec![make_typed_binding(
            "count",
            i64_ty.clone(),
            NirBindingOrigin::ParamIndex(0),
        )],
        locals: vec![make_typed_binding(
            "shifted",
            i64_ty,
            NirBindingOrigin::Temp,
        )],
        return_type: NirType::Unknown,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, u64_ty);
    assert_eq!(func.locals[0].ty, func.params[0].ty);
}

#[test]
fn signed_neutral_arithmetic_result_promotes_operand_signedness() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("a".to_owned())),
        rhs: Box::new(PreHirExpr::Var("b".to_owned())),
        ty: i32_ty.clone(),
    }))];
    let mut func = PreHirFunction {
        name: "add".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("b", u32_ty, NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: i32_ty.clone(),
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, i32_ty);
    assert_eq!(func.params[1].ty, i32_ty);
}

#[test]
fn signed_casted_arithmetic_result_promotes_operand_signedness() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
        ty: i32_ty.clone(),
        expr: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Var("b".to_owned())),
            ty: u32_ty.clone(),
        }),
    }))];
    let mut func = PreHirFunction {
        name: "add".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("b", u32_ty, NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: i32_ty.clone(),
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, i32_ty);
    assert_eq!(func.params[1].ty, i32_ty);
}

#[test]
fn store_value_only_unsigned_param_defaults_to_signed_int() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = PreHirFunction {
        name: "fill".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding(
                "param_1",
                NirType::Ptr(Box::new(u32_ty.clone())),
                NirBindingOrigin::ParamIndex(0),
            ),
            make_typed_binding("param_2", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                ty: u32_ty.clone(),
            },
            rhs: PreHirExpr::Var("param_2".to_owned()),
        }],
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[1].ty, i32_ty);
}

#[test]
fn store_value_param_keeps_unsigned_when_used_in_unsigned_comparison() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = PreHirFunction {
        name: "fill_guarded".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding(
                "param_1",
                NirType::Ptr(Box::new(u32_ty.clone())),
                NirBindingOrigin::ParamIndex(0),
            ),
            make_typed_binding("param_2", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(PreHirExpr::Var("param_2".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(10, u32_ty.clone())),
                    ty: NirType::Bool,
                },
                then_body: Vec::new().into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                    ty: u32_ty.clone(),
                },
                rhs: PreHirExpr::Var("param_2".to_owned()),
            },
        ],
        ..Default::default()
    };

    assert!(!apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[1].ty, u32_ty);
}

#[test]
fn signed_compare_without_width_evidence_does_not_invent_type() {
    let body = vec![PreHirStmt::If {
        cond: PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Var("b".to_owned())),
            ty: NirType::Bool,
        },
        then_body: vec![].into(),
        else_body: vec![].into(),
    }];
    let mut func = make_func(
        vec![make_binding("a"), make_binding("b")],
        body,
        NirType::Unknown,
    );
    assert!(!apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.locals[0].ty, NirType::Unknown);
    assert_eq!(func.locals[1].ty, NirType::Unknown);
}

#[test]
fn signed_compare_uses_constant_width_evidence() {
    let body = vec![PreHirStmt::If {
        cond: PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 16,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        },
        then_body: vec![].into(),
        else_body: vec![].into(),
    }];
    let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(
        func.locals[0].ty,
        NirType::Int {
            bits: 16,
            signed: true
        }
    );
}
