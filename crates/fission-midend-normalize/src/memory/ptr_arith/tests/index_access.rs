use super::super::{apply_ptr_arith_recovery_pass, apply_zero_index_deref_pass};
use super::{make_binding_with_ty, make_func};
use crate::prelude::*;

#[test]
fn rewrites_zero_index_access_to_deref() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("value".to_owned()),
            rhs: PreHirExpr::Index {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                index: Box::new(PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                elem_ty: elem_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                index: Box::new(PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                elem_ty,
            },
            rhs: PreHirExpr::Var("value".to_owned()),
        },
    ];
    let mut func = make_func(vec![make_binding_with_ty("p", ptr_ty)], body);

    assert!(apply_zero_index_deref_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Load { .. },
            ..
        }
    ));
    assert!(matches!(
        &func.body[1],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { .. },
            ..
        }
    ));
}

fn u64_ty() -> NirType {
    NirType::Int {
        bits: 64,
        signed: false,
    }
}

/// `*(T *)(p + (i << shift))` with a `T`-sized access, `p` bound to `ptr_ty`.
fn shifted_load_body(shift: i64, access_bits: u32) -> Vec<PreHirStmt> {
    let access = NirType::Int {
        bits: access_bits,
        signed: false,
    };
    vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shl,
                    lhs: Box::new(PreHirExpr::Var("i".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(shift, u64_ty())),
                    ty: u64_ty(),
                }),
                ty: u64_ty(),
            }),
            ty: access,
        },
    }]
}

/// `Load(p + (i << 2))` of a 4-byte value is `p[i]`, exactly as `i * 4` is.
#[test]
fn a_shifted_stride_equal_to_the_access_size_recovers_an_index() {
    for (shift, bits) in [(1, 16), (2, 32), (3, 64)] {
        let p_ty = NirType::Ptr(Box::new(NirType::Int {
            bits,
            signed: false,
        }));
        let mut func = make_func(
            vec![make_binding_with_ty("p", p_ty)],
            shifted_load_body(shift, bits),
        );
        assert!(apply_ptr_arith_recovery_pass(&mut func));
        let PreHirStmt::Assign { rhs, .. } = &func.body[0] else {
            panic!("expected assign");
        };
        assert!(
            matches!(rhs, PreHirExpr::Index { .. }),
            "shift {shift} access {bits} bits: {rhs:?}"
        );
    }
}

/// A shift whose stride is not the access size is a field of a wider
/// element, or an access of a different width than the scaling; the shifted
/// form is not rewritten for it.
#[test]
fn a_shifted_stride_that_is_not_the_access_size_is_left_alone() {
    // stride 8 (<< 3), 4-byte access
    let p_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: false,
    }));
    let mut func = make_func(
        vec![make_binding_with_ty("p", p_ty)],
        shifted_load_body(3, 32),
    );
    apply_ptr_arith_recovery_pass(&mut func);
    let PreHirStmt::Assign { rhs, .. } = &func.body[0] else {
        panic!("expected assign");
    };
    assert!(
        !matches!(
            rhs,
            PreHirExpr::Index { .. } | PreHirExpr::FieldAccess { .. }
        ),
        "{rhs:?}"
    );
}

/// A shift count outside 1..=6 is not a scale.
#[test]
fn an_out_of_range_shift_is_not_a_stride() {
    for shift in [0, 7, 63] {
        let p_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let mut func = make_func(
            vec![make_binding_with_ty("p", p_ty)],
            shifted_load_body(shift, 8),
        );
        apply_ptr_arith_recovery_pass(&mut func);
        let PreHirStmt::Assign { rhs, .. } = &func.body[0] else {
            panic!("expected assign");
        };
        assert!(
            !matches!(rhs, PreHirExpr::Index { .. }),
            "shift {shift}: {rhs:?}"
        );
    }
}

/// Load(Add(Var("p"), Mul(Var("i"), Const(4)))) where p: Ptr(uint32) → p[i].
#[test]
fn converts_load_stride_to_index_value() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
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
            ty: elem_ty.clone(),
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(rhs, PreHirExpr::Index { .. }));
    } else {
        panic!("expected assign");
    }
}

/// Store through a typed pointer constant offset should surface as p[k],
/// including negative offsets emitted by post-increment store idioms.
#[test]
fn converts_deref_const_pointer_offset_to_index_lvalue() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: p_ty.clone(),
            }),
            ty: elem_ty.clone(),
        },
        rhs: PreHirExpr::Var("value".to_owned()),
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

    let changed = apply_ptr_arith_recovery_pass(&mut func);

    assert!(changed);
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            lhs:
                PreHirLValue::Index {
                    base,
                    index,
                    elem_ty: index_elem_ty,
                },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "p")
            && matches!(index.as_ref(), PreHirExpr::Const(-1, _))
            && index_elem_ty == &elem_ty
    ));
}

#[test]
fn converts_load_const_pointer_offset_to_index_value() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: p_ty.clone(),
            }),
            ty: elem_ty.clone(),
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

    let changed = apply_ptr_arith_recovery_pass(&mut func);

    assert!(changed);
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs:
                PreHirExpr::Index {
                    base,
                    index,
                    elem_ty: index_elem_ty,
                },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "p")
            && matches!(index.as_ref(), PreHirExpr::Const(2, _))
            && index_elem_ty == &elem_ty
    ));
}

#[test]
fn leaves_const_pointer_offset_deref_when_pointee_size_mismatches_access() {
    let pointee_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let access_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(pointee_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: p_ty.clone(),
            }),
            ty: access_ty,
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

    let changed = apply_ptr_arith_recovery_pass(&mut func);

    assert!(!changed);
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Load { ptr, .. },
            ..
        } if matches!(ptr.as_ref(), PreHirExpr::Binary { .. })
    ));
}
