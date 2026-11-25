use super::super::apply_ptr_arith_recovery_pass;
use super::super::pointer_types::propagate_pointer_assignment_types;
use super::{make_binding_with_ty, make_func};
use crate::prelude::*;

#[test]
fn refines_ptr_unknown_to_array_on_uniform_accesses() {
    let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
    let body = vec![
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 0,
            }),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 4,
            }),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 8,
            }),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
    ];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    let NirType::Ptr(inner) = &func.locals[0].ty else {
        panic!("expected refined pointer ty");
    };
    assert!(matches!(inner.as_ref(), NirType::Int { bits: 32, .. }));
}

#[test]
fn recovers_dynamic_index_scale_on_unknown_ptr() {
    let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(PreHirExpr::Var("i".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            }),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty("p", p_ty),
            make_binding_with_ty(
                "i",
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        ],
        body,
    );
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(rhs, PreHirExpr::Index { .. }));
    } else {
        panic!("expected index assignment");
    }
}

#[test]
fn preserves_aggregate_on_sparse_diverse_accesses() {
    let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
    let body = vec![
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 4,
            }),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 11,
            }),
            ty: NirType::Int {
                bits: 8,
                signed: false,
            },
        }),
        PreHirStmt::Expr(PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                offset: 20,
            }),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
    ];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    let NirType::Ptr(inner) = &func.locals[0].ty else {
        panic!("expected refined pointer ty");
    };
    assert!(matches!(inner.as_ref(), NirType::Aggregate { .. }));
}

#[test]
fn mixed_scalar_then_pointer_assignment_does_not_promote_local_to_pointer() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let ptr_u32 = NirType::Ptr(Box::new(u32_ty.clone()));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_owned()),
            rhs: PreHirExpr::Const(0, u32_ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_owned()),
            rhs: PreHirExpr::Var("p".to_owned()),
        },
    ];
    let mut func = make_func(
        vec![
            make_binding_with_ty("p", ptr_u32),
            make_binding_with_ty("x", NirType::Unknown),
        ],
        body,
    );
    func.is_64bit = true;

    assert!(!propagate_pointer_assignment_types(&mut func));
    assert_eq!(func.locals[1].ty, NirType::Unknown);
}
