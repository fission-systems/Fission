use super::super::apply_use_driven_type_infer_pass;
use super::{make_binding, make_func, make_typed_binding};
use crate::prelude::*;

#[test]
fn unknown_call_only_value_returns_promote_native_word_return_type() {
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Call {
        origin: None,
        target: "param_1".to_owned(),
        args: vec![
            PreHirExpr::Var("param_2".to_owned()),
            PreHirExpr::Var("param_3".to_owned()),
        ],
        ty: NirType::Unknown,
    }))];
    let mut func = make_func(Vec::new(), body, NirType::Unknown);
    func.is_64bit = false;

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(
        func.return_type,
        NirType::Int {
            bits: 32,
            signed: false
        }
    );
}

#[test]
fn unknown_call_return_promotion_requires_all_value_returns_to_be_calls() {
    let body = vec![PreHirStmt::If {
        cond: PreHirExpr::Var("flag".to_owned()),
        then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Call {
            origin: None,
            target: "param_1".to_owned(),
            args: Vec::new(),
            ty: NirType::Unknown,
        }))]
        .into(),
        else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "fallback".to_owned(),
        )))]
        .into(),
    }];
    let mut func = make_func(Vec::new(), body, NirType::Unknown);
    func.is_64bit = false;

    assert!(!apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.return_type, NirType::Unknown);
}

#[test]
fn wrapping_return_use_narrows_wide_integer_params() {
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = PreHirFunction {
        name: "add32".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding("param_1", u64_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("param_2", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ],
        locals: vec![],
        return_type: u32_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
            rhs: Box::new(PreHirExpr::Var("param_2".to_owned())),
            ty: u64_ty,
        }))],
        ..Default::default()
    };

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, u32_ty);
    assert_eq!(func.params[1].ty, u32_ty);
}

#[test]
fn wrapping_return_use_does_not_narrow_param_with_unconstrained_use() {
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = PreHirFunction {
        name: "add32_with_call".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![make_typed_binding(
            "param_1",
            u64_ty.clone(),
            NirBindingOrigin::ParamIndex(0),
        )],
        locals: vec![],
        return_type: u32_ty,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "observe64".to_owned(),
                args: vec![PreHirExpr::Var("param_1".to_owned())],
                ty: NirType::Unknown,
            }),
            PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
        ],
        ..Default::default()
    };

    assert!(!apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, u64_ty);
}

/// Return(Var("r")) + known return_type → r gets return_type
#[test]
fn infers_type_from_return_context() {
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Var("r".to_owned())))];
    let ret_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = make_func(vec![make_binding("r")], body, ret_ty.clone());
    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(func.locals[0].ty, ret_ty);
}
