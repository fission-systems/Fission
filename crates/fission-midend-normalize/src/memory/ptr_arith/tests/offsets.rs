use super::super::apply_ptr_arith_recovery_pass;
use super::{make_binding_with_ty, make_func};
use crate::prelude::*;

#[test]
fn recovers_32_bit_pointer_array_stride() {
    let pointee_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let element_ptr_ty = NirType::Ptr(Box::new(pointee_ty));
    let table_ty = NirType::Ptr(Box::new(element_ptr_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("table".to_owned())),
                rhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(PreHirExpr::Var("index".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
                ty: table_ty.clone(),
            }),
            ty: element_ptr_ty.clone(),
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("table", table_ty)], body);
    func.is_64bit = false;

    assert!(apply_ptr_arith_recovery_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Index {
                base,
                index,
                elem_ty: recovered_elem_ty,
            },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "table")
            && matches!(index.as_ref(), PreHirExpr::Var(name) if name == "index")
            && recovered_elem_ty == &element_ptr_ty
    ));
}

#[test]
fn uses_32_bit_pointer_width_for_pointer_to_integer_casts() {
    let pointer_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: false,
    }));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("diff".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(PreHirExpr::Var("addr".to_owned())),
            rhs: Box::new(PreHirExpr::Var("ptr".to_owned())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty(
                "addr",
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
            make_binding_with_ty("ptr", pointer_ty),
        ],
        body,
    );
    func.is_64bit = false;

    assert!(apply_ptr_arith_recovery_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::Binary {
                rhs,
                ty: NirType::Int { bits: 32, signed: false },
                ..
            },
            ..
        } if matches!(rhs.as_ref(), PreHirExpr::Cast {
            ty: NirType::Int { bits: 32, signed: false },
            expr,
        } if matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "ptr"))
    ));
}

/// Add(Var("p"), Const(8)) where p: Ptr(Aggregate) preserves byte-offset form
/// for later field recovery.
#[test]
fn preserves_aggregate_add_const_as_ptr_offset() {
    let elem_ty = NirType::Aggregate {
        size: 16,
        fields: vec![],
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".to_owned())),
            rhs: Box::new(PreHirExpr::Const(
                8,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(rhs, PreHirExpr::PtrOffset { offset: 8, .. }));
    } else {
        panic!("expected assign");
    }
}

/// Pointer-typed Add(Var("p"), Const(8)) where p: Ptr(Aggregate) still
/// preserves byte-offset form.  The raw p-code builder may type the address
/// expression as a pointer before aggregate fields have been recovered.
#[test]
fn preserves_pointer_typed_aggregate_add_const_as_ptr_offset() {
    let elem_ty = NirType::Aggregate {
        size: 16,
        fields: vec![],
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".to_owned())),
            rhs: Box::new(PreHirExpr::Const(
                8,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Ptr(Box::new(elem_ty)),
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(rhs, PreHirExpr::PtrOffset { offset: 8, .. }));
    } else {
        panic!("expected assign");
    }
}

/// `PtrOffset` carries bytes, so later pointee refinement cannot reinterpret
/// a small offset as an element count (`+4` must not become `+32` for u64*).
#[test]
fn ptr_offset_byte_unit_survives_later_pointee_refinement() {
    let p_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 64,
        signed: false,
    }));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("p".to_owned())),
            offset: 4,
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

    apply_ptr_arith_recovery_pass(&mut func);

    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::PtrOffset { offset: 4, .. },
            ..
        }
    ));
}

/// Scalar address arithmetic keeps its constant in bytes, even when the
/// base binding has a recovered pointee type.
#[test]
fn preserves_scalar_add_const_as_byte_offset() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".to_owned())),
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
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(
            rhs,
            PreHirExpr::PtrOffset {
                base,
                offset: 4,
            } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "p")
        ));
    } else {
        panic!("expected assign");
    }
}

#[test]
fn recovers_reversed_scalar_pointer_add_as_byte_offset() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let ptr_ty = NirType::Ptr(Box::new(elem_ty));
    let scalar_ty = NirType::Int {
        bits: 64,
        signed: true,
    };
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("cursor".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Const(-4, scalar_ty.clone())),
            rhs: Box::new(PreHirExpr::Var("base".to_owned())),
            ty: scalar_ty,
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("base", ptr_ty)], body);

    assert!(apply_ptr_arith_recovery_pass(&mut func));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs: PreHirExpr::PtrOffset { base, offset: -4 },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "base")
    ));
}

#[test]
fn recovers_byte_affine_last_element_offset() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let index_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let scaled_index = PreHirExpr::Binary {
        op: PreHirBinaryOp::Mul,
        lhs: Box::new(PreHirExpr::Var("index".to_owned())),
        rhs: Box::new(PreHirExpr::Const(4, index_ty.clone())),
        ty: index_ty.clone(),
    };
    let address = PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("base".to_owned())),
            rhs: Box::new(scaled_index),
            ty: index_ty.clone(),
        }),
        rhs: Box::new(PreHirExpr::Const(4, index_ty.clone())),
        ty: index_ty.clone(),
    };
    let mut func = make_func(
        vec![
            make_binding_with_ty("base", ptr_ty),
            make_binding_with_ty("index", index_ty),
        ],
        vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("value".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(address),
                ty: elem_ty,
            },
        }],
    );

    apply_ptr_arith_recovery_pass(&mut func);
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            rhs:
                PreHirExpr::Load {
                    ptr,
                    ty: NirType::Int { bits: 32, signed: true },
                },
            ..
        } if matches!(ptr.as_ref(), PreHirExpr::PtrOffset {
            base,
            offset: -4,
        } if matches!(base.as_ref(), PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "base")
            && matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "index")))
    ));
}

/// Pointer-valued Add(Var("p"), Mul(Var("i"), Const(4))) where p: Ptr(uint32)
/// keeps address semantics and becomes Add(Var("p"), Var("i")).
#[test]
fn converts_add_stride_to_pointer_add() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Binary {
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
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(
            rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs,
                rhs,
                ty: NirType::Ptr(_),
            } if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "p")
                && matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "i")
        ));
    } else {
        panic!("expected assign");
    }
}

#[test]
fn rescales_byte_cast_const_offset_to_typed_pointer_add() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty));
    let byte_ptr_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 8,
        signed: false,
    }));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("result".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Cast {
                ty: byte_ptr_ty,
                expr: Box::new(PreHirExpr::Var("p".to_owned())),
            }),
            rhs: Box::new(PreHirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: false,
            })),
        },
    }];
    let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(
            rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs,
                rhs,
                ty: NirType::Ptr(_),
            } if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "p")
                && matches!(rhs.as_ref(), PreHirExpr::Const(1, _))
        ));
    } else {
        panic!("expected assign");
    }
}

#[test]
fn casts_pointer_rhs_in_integer_subtraction() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("diff".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(PreHirExpr::Var("addr".to_owned())),
            rhs: Box::new(PreHirExpr::Var("p".to_owned())),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty(
                "addr",
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
            make_binding_with_ty("p", p_ty),
        ],
        body,
    );
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(
            rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs,
                rhs,
                ty: NirType::Int { bits: 64, signed: false },
            } if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == "addr")
                && matches!(rhs.as_ref(), PreHirExpr::Cast {
                    ty: NirType::Int { bits: 64, signed: false },
                    expr,
                } if matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "p"))
        ));
    } else {
        panic!("expected assign");
    }
}

#[test]
fn casts_both_pointer_operands_in_integer_subtraction() {
    let elem_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let p_ty = NirType::Ptr(Box::new(elem_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("diff".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(PreHirExpr::Var("lhs".to_owned())),
            rhs: Box::new(PreHirExpr::Var("rhs".to_owned())),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty("lhs", p_ty.clone()),
            make_binding_with_ty("rhs", p_ty),
        ],
        body,
    );
    let changed = apply_ptr_arith_recovery_pass(&mut func);
    assert!(changed);
    if let PreHirStmt::Assign { rhs, .. } = &func.body[0] {
        assert!(matches!(
            rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs,
                rhs,
                ty: NirType::Int { bits: 64, signed: false },
            } if matches!(lhs.as_ref(), PreHirExpr::Cast { expr, .. }
                    if matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "lhs"))
                && matches!(rhs.as_ref(), PreHirExpr::Cast { expr, .. }
                    if matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "rhs"))
        ));
    } else {
        panic!("expected assign");
    }
}

#[test]
fn byte_cast_sub_element_const_preserves_unscaled_byte_offset() {
    // `(uint8_t *)(int *p) + 1` with multi-byte pointee → PtrOffset(+1)
    // (Byte offsets on byte-cast pointers are preserved unscaled).
    let p_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 32,
        signed: true,
    }));
    let byte_ptr_ty = NirType::Ptr(Box::new(NirType::Int {
        bits: 8,
        signed: false,
    }));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("q".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Cast {
                ty: byte_ptr_ty,
                expr: Box::new(PreHirExpr::Var("p".to_owned())),
            }),
            rhs: Box::new(PreHirExpr::Const(
                1,
                NirType::Int {
                    bits: 64,
                    signed: true,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![
            make_binding_with_ty("p", p_ty),
            make_binding_with_ty(
                "q",
                NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            ),
        ],
        body,
    );
    assert!(apply_ptr_arith_recovery_pass(&mut func));
    match &func.body[0] {
        PreHirStmt::Assign {
            rhs: PreHirExpr::PtrOffset { offset: 1, .. },
            ..
        } => {}
        other => panic!("expected PtrOffset(+1), got {other:?}"),
    }
}
