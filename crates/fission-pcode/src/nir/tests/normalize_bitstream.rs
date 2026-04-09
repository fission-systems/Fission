use super::*;

#[test]
fn normalize_hir_function_rewrites_flush_bits_to_pseudo_intrinsic() {
    let mut func = HirFunction {
        name: "flush_fn".to_string(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "slot_20".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: false,
            })),
            surface_type_name: None,
            origin: None,
            initializer: Some(HirExpr::Cast {
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
                expr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("param_1".to_string())),
                    offset: 0x20,
                }),
            }),
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::Lt,
                lhs: Box::new(HirExpr::Const(
                    7,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                rhs: Box::new(HirExpr::Var("bit_count".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("slot_20".to_string())),
                        ty: NirType::Int {
                            bits: 8,
                            signed: false,
                        },
                    },
                    rhs: HirExpr::Var("accum".to_string()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("out_idx".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("out_idx".to_string())),
                        rhs: Box::new(HirExpr::Const(
                            1,
                            NirType::Int {
                                bits: 32,
                                signed: true,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("accum".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Shr,
                        lhs: Box::new(HirExpr::Var("accum".to_string())),
                        rhs: Box::new(HirExpr::Const(
                            8,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("bit_count".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(HirExpr::Var("bit_count".to_string())),
                        rhs: Box::new(HirExpr::Const(
                            8,
                            NirType::Int {
                                bits: 32,
                                signed: true,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    },
                },
            ],
            else_body: Vec::new(),
        }],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("FLUSH_BITS("), "{rendered}");
}

#[test]
fn normalize_hir_function_rewrites_table_driven_emit_to_intrinsic() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let mut func = HirFunction {
        name: "emit_fn".to_string(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            NirBinding {
                name: "slot_40".to_string(),
                ty: NirType::Ptr(Box::new(uint_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(uint_ty.clone())),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0x40,
                    }),
                }),
            },
            NirBinding {
                name: "slot_44".to_string(),
                ty: NirType::Ptr(Box::new(uint_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(uint_ty.clone())),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0x44,
                    }),
                }),
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("accum".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Or,
                    lhs: Box::new(HirExpr::Var("accum".to_string())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(HirExpr::Index {
                            base: Box::new(HirExpr::Var("slot_40".to_string())),
                            index: Box::new(idx.clone()),
                            elem_ty: uint_ty.clone(),
                        }),
                        rhs: Box::new(HirExpr::Var("bit_count".to_string())),
                        ty: uint_ty.clone(),
                    }),
                    ty: uint_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("bit_count".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("bit_count".to_string())),
                    rhs: Box::new(HirExpr::Index {
                        base: Box::new(HirExpr::Var("slot_44".to_string())),
                        index: Box::new(idx),
                        elem_ty: uint_ty.clone(),
                    }),
                    ty: uint_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("EMIT_CODE(param_1, slot_40[idx], slot_44[idx]);"));
}

#[test]
fn normalize_hir_function_rewrites_slot_based_write_bits_to_intrinsic() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let slot_idx = HirExpr::Index {
        base: Box::new(HirExpr::Var("slot_280".to_string())),
        index: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 64,
                signed: false,
            },
        )),
        elem_ty: uint_ty.clone(),
    };
    let bitcount_idx = HirExpr::Index {
        base: Box::new(HirExpr::Var("slot_284".to_string())),
        index: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 64,
                signed: false,
            },
        )),
        elem_ty: uint_ty.clone(),
    };
    let mut func = HirFunction {
        name: "slot_write_bits_fn".to_string(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            NirBinding {
                name: "slot_280".to_string(),
                ty: NirType::Ptr(Box::new(uint_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(uint_ty.clone())),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0x280,
                    }),
                }),
            },
            NirBinding {
                name: "slot_284".to_string(),
                ty: NirType::Ptr(Box::new(uint_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(uint_ty.clone())),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_1".to_string())),
                        offset: 0x284,
                    }),
                }),
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("slot_280".to_string())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty: uint_ty.clone(),
                },
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Or,
                    lhs: Box::new(slot_idx.clone()),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(HirExpr::Var("value".to_string())),
                        rhs: Box::new(bitcount_idx.clone()),
                        ty: uint_ty.clone(),
                    }),
                    ty: uint_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("slot_284".to_string())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty: uint_ty.clone(),
                },
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(bitcount_idx),
                    rhs: Box::new(HirExpr::Var("width".to_string())),
                    ty: uint_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("WRITE_BITS(param_1, value, width);"),
        "{rendered}"
    );
}
