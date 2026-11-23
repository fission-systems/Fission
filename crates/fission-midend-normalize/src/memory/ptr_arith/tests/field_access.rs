use super::super::apply_ptr_arith_recovery_pass;
use super::{make_binding_with_ty, make_func};
use crate::prelude::*;

#[test]
fn recovers_aggregate_field_access() {
    let field_8 = fission_midend_core::StructField {
        offset: 8,
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
        name: "field_8".to_owned(),
    };
    let agg_ty = NirType::Aggregate {
        size: 24,
        fields: vec![field_8],
    };
    let p_ty = NirType::Ptr(Box::new(agg_ty));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("p".to_owned())),
                    offset: 8,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("p".to_owned())),
                    offset: 16,
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
            rhs: PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        },
    ];
    let mut func = make_func(
        vec![
            make_binding_with_ty("p", p_ty),
            make_binding_with_ty(
                "x",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ),
        ],
        body,
    );
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);

    // Verify the load is now FieldAccess
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(
            matches!(rhs, PreHirExpr::FieldAccess { field_name, offset, .. } if field_name == "field_8" && *offset == 8),
            "expected FieldAccess for field_8, got {:?}",
            rhs
        );
    } else {
        panic!("expected assignment");
    }

    // Verify the store/deref is now FieldAccess (synthetic fallback).
    // The name is hex like every other synthetic field name, so offset 16
    // is `field_10` -- decimal here spelled a member the emitted typedef
    // does not declare.
    if let PreHirStmt::Assign { lhs, .. } = &func.body[1] {
        assert!(
            matches!(lhs, PreHirLValue::FieldAccess { field_name, offset, .. } if field_name == "field_10" && *offset == 16),
            "expected FieldAccess for field_10 (offset 16), got {:?}",
            lhs
        );
    } else {
        panic!("expected assignment");
    }
}

#[test]
fn aggregate_field_access_scales_surviving_typed_pointer_element_offset() {
    let agg_ty = NirType::Aggregate {
        size: 16,
        fields: vec![fission_midend_core::StructField {
            offset: 8,
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
            name: "field_8".to_owned(),
        }],
    };
    let original_int_ptr = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: true,
    }));
    let body = vec![PreHirStmt::Expr(PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Cast {
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                )),
                ty: original_int_ptr,
            }),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 64,
                signed: false,
            })),
        }),
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
    })];
    let mut func = make_func(
        vec![make_binding_with_ty("p", NirType::Ptr(Box::new(agg_ty)))],
        body,
    );

    assert!(apply_ptr_arith_recovery_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Expr(PreHirExpr::FieldAccess { field_name, offset, .. })
            if field_name == "field_8" && *offset == 8
    ));
}

#[test]
fn scalar_constant_index_on_aggregate_pointer_recovers_exact_member() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let aggregate_ty = NirType::Aggregate {
        size: 8,
        fields: vec![
            fission_midend_core::StructField {
                offset: 0,
                ty: u32_ty.clone(),
                name: "flags".to_owned(),
            },
            fission_midend_core::StructField {
                offset: 4,
                ty: u32_ty.clone(),
                name: "value".to_owned(),
            },
        ],
    };
    let constant_one = || {
        PreHirExpr::Const(
            1,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )
    };
    let aggregate_index = || PreHirExpr::Index {
        base: Box::new(PreHirExpr::Var("node".to_owned())),
        index: Box::new(constant_one()),
        elem_ty: u32_ty.clone(),
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var("node".to_owned())),
                index: Box::new(constant_one()),
                elem_ty: u32_ty.clone(),
            },
            rhs: PreHirExpr::Const(7, u32_ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("result".to_owned()),
            rhs: aggregate_index(),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var("node".to_owned())),
                index: Box::new(PreHirExpr::Var("i".to_owned())),
                elem_ty: u32_ty.clone(),
            },
            rhs: PreHirExpr::Const(9, u32_ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var("items".to_owned())),
                index: Box::new(constant_one()),
                elem_ty: u32_ty.clone(),
            },
            rhs: PreHirExpr::Const(11, u32_ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("aggregate_result".to_owned()),
            rhs: PreHirExpr::Index {
                base: Box::new(PreHirExpr::Var("node".to_owned())),
                index: Box::new(constant_one()),
                elem_ty: aggregate_ty.clone(),
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_binding_with_ty("node", NirType::Ptr(Box::new(aggregate_ty.clone()))),
            make_binding_with_ty("items", NirType::Ptr(Box::new(u32_ty.clone()))),
            make_binding_with_ty("i", u32_ty.clone()),
            make_binding_with_ty("result", u32_ty.clone()),
            make_binding_with_ty("aggregate_result", aggregate_ty),
        ],
        body,
    );

    assert!(apply_ptr_arith_recovery_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::FieldAccess { base, field_name, offset: 4, ty },
            ..
        } if field_name == "value"
            && ty == &u32_ty
            && matches!(base.as_ref(), PreHirExpr::Var(name) if name == "node")
    ));
    assert!(matches!(
        &func.body[1],
        PreHirStmt::Assign {
            rhs: PreHirExpr::FieldAccess { field_name, offset: 4, .. },
            ..
        } if field_name == "value"
    ));
    assert!(matches!(
        &func.body[2],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index { index, .. },
            ..
        } if matches!(index.as_ref(), PreHirExpr::Var(name) if name == "i")
    ));
    assert!(matches!(
        &func.body[3],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index { base, .. },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "items")
    ));
    assert!(matches!(
        &func.body[4],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Index {
                elem_ty: NirType::Aggregate { .. },
                ..
            },
            ..
        }
    ));
}

#[test]
fn direct_scalar_access_on_aggregate_pointer_preserves_member_width() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let aggregate_with_byte_first = NirType::Aggregate {
        size: 8,
        fields: vec![
            fission_midend_core::StructField {
                offset: 0,
                ty: u8_ty.clone(),
                name: "flags".to_owned(),
            },
            fission_midend_core::StructField {
                offset: 4,
                ty: u32_ty.clone(),
                name: "value".to_owned(),
            },
        ],
    };
    let aggregate_with_word_first = NirType::Aggregate {
        size: 8,
        fields: vec![fission_midend_core::StructField {
            offset: 0,
            ty: u32_ty.clone(),
            name: "word".to_owned(),
        }],
    };
    let byte_cast_base = || PreHirExpr::Cast {
        ty: NirType::Ptr(Box::new(u8_ty.clone())),
        expr: Box::new(PreHirExpr::Var("bytes".to_owned())),
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("loaded".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(byte_cast_base()),
                ty: u8_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(byte_cast_base()),
                ty: u8_ty.clone(),
            },
            rhs: PreHirExpr::Const(0x5a, u8_ty.clone()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("narrow_load".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("words".to_owned())),
                ty: u8_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("words".to_owned())),
                ty: u8_ty.clone(),
            },
            rhs: PreHirExpr::Const(0x5a, u8_ty.clone()),
        },
    ];
    let mut func = make_func(
        vec![
            make_binding_with_ty("bytes", NirType::Ptr(Box::new(aggregate_with_byte_first))),
            make_binding_with_ty("words", NirType::Ptr(Box::new(aggregate_with_word_first))),
            make_binding_with_ty("loaded", u8_ty.clone()),
            make_binding_with_ty("narrow_load", u8_ty.clone()),
        ],
        body,
    );

    apply_ptr_arith_recovery_pass(&mut func);
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::FieldAccess {
                field_name,
                offset: 0,
                ty,
                ..
            },
            ..
        } if field_name == "flags" && ty == &u8_ty
    ));
    assert!(matches!(
        &func.body[1],
        PreHirStmt::Assign {
            lhs: PreHirLValue::FieldAccess {
                field_name,
                offset: 0,
                ty,
                ..
            },
            ..
        } if field_name == "flags" && ty == &u8_ty
    ));
    assert!(matches!(
        &func.body[2],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Load { ty, .. },
            ..
        } if ty == &u8_ty
    ));
    assert!(matches!(
        &func.body[3],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { ty, .. },
            ..
        } if ty == &u8_ty
    ));
}

#[test]
fn wide_stride_load_recovers_field0_of_indexed_aggregate() {
    let p_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: false,
    }));
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    // Load(*(p + i * 8)) — stride 8, access 4 → pairs[i].field_0 shape
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("v".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(PreHirExpr::Var("i".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(8, u32_ty.clone())),
                    ty: u32_ty.clone(),
                }),
                ty: NirType::Unknown,
            }),
            ty: u32_ty.clone(),
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty("p", p_ty),
            make_binding_with_ty("i", u32_ty.clone()),
            make_binding_with_ty("v", u32_ty),
        ],
        body,
    );
    assert!(apply_ptr_arith_recovery_pass(&mut func));
    match &func.body[0] {
        PreHirStmt::Assign {
            rhs:
                PreHirExpr::FieldAccess {
                    field_name,
                    offset: 0,
                    base,
                    ..
                },
            ..
        } if field_name == "field_0" => {
            assert!(
                matches!(base.as_ref(), PreHirExpr::Index { .. }),
                "expected Index base, got {base:?}"
            );
        }
        other => panic!("expected FieldAccess(field_0) of Index, got {other:?}"),
    }
}

#[test]
fn field_access_rejects_mismatched_width_and_oob_offset() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let agg_ty = NirType::Aggregate {
        size: 8,
        fields: vec![fission_midend_core::StructField {
            offset: 0,
            ty: u64_ty.clone(),
            name: "quad".to_owned(),
        }],
    };
    let p_ty = NirType::Ptr(Box::new(agg_ty));

    // 1-byte store to offset 0 (where field is 8 bytes)
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("p".to_owned())),
                    offset: 0,
                }),
                ty: u8_ty.clone(),
            },
            rhs: PreHirExpr::Const(1, u8_ty.clone()),
        },
        // 4-byte store to offset 16 (beyond struct size 8)
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("p".to_owned())),
                    offset: 16,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
            rhs: PreHirExpr::Const(
                2,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
    ];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

    // Neither access should be rewritten to FieldAccess
    apply_ptr_arith_recovery_pass(&mut func);

    match &func.body[0] {
        PreHirStmt::Assign { lhs, .. } => {
            assert!(
                !matches!(lhs, PreHirLValue::FieldAccess { .. }),
                "mismatched store width should not become FieldAccess"
            );
        }
        _ => panic!("expected Assign"),
    }
    match &func.body[1] {
        PreHirStmt::Assign { lhs, .. } => {
            assert!(
                !matches!(lhs, PreHirLValue::FieldAccess { .. }),
                "out of bounds offset should not become FieldAccess"
            );
        }
        _ => panic!("expected Assign"),
    }
}
