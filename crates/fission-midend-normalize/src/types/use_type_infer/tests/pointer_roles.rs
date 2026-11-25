use super::super::apply_use_driven_type_infer_pass;
use super::{make_binding, make_func, make_typed_binding};
use crate::prelude::*;

#[test]
fn aggregate_store_value_promotes_binding_without_scalar_or_address_role() {
    let aggregate = NirType::Aggregate {
        size: 16,
        fields: Vec::new(),
    };
    let pointer = NirType::Ptr(Box::new(aggregate.clone()));
    let value = make_typed_binding(
        "value",
        NirType::Int {
            bits: 16,
            signed: false,
        },
        NirBindingOrigin::ParamIndex(0),
    );
    let mut func = make_func(
        vec![make_typed_binding("out", pointer, NirBindingOrigin::Temp)],
        vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("out".to_owned())),
                ty: aggregate.clone(),
            },
            rhs: PreHirExpr::Var("value".to_owned()),
        }],
        NirType::Unknown,
    );
    func.params.push(value);

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[0].ty, aggregate);
}

/// `x = a * b` with float result upgrades int temps and `uint*` bases.
#[test]
fn float_mul_promotes_int_operands_and_uint_pointer_base() {
    let float_ty = NirType::Float { bits: 32 };
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("a".to_owned()),
            rhs: PreHirExpr::Index {
                base: Box::new(PreHirExpr::Var("p".to_owned())),
                index: Box::new(PreHirExpr::Const(0, uint_ty.clone())),
                elem_ty: uint_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("prod".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Var("b".to_owned())),
                ty: float_ty.clone(),
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding(
                "p",
                NirType::Ptr(Box::new(uint_ty.clone())),
                NirBindingOrigin::ParamIndex(0),
            ),
            make_typed_binding("a", uint_ty.clone(), NirBindingOrigin::Temp),
            make_typed_binding("b", uint_ty.clone(), NirBindingOrigin::Temp),
            make_typed_binding("prod", uint_ty, NirBindingOrigin::Temp),
        ],
        body,
        NirType::Unknown,
    );
    // Move `p` to params so ParamIndex origin matches production shape.
    let p = func.locals.remove(0);
    func.params.push(p);

    let changed = apply_use_driven_type_infer_pass(&mut func);
    assert!(changed, "float mul must promote operand/base types");
    assert_eq!(
        func.params[0].ty,
        NirType::Ptr(Box::new(float_ty.clone())),
        "uint* base of float mul load must become float*"
    );
    assert_eq!(
        func.locals.iter().find(|b| b.name == "a").map(|b| &b.ty),
        Some(&float_ty),
        "float mul lhs operand must become float"
    );
    assert_eq!(
        func.locals.iter().find(|b| b.name == "b").map(|b| &b.ty),
        Some(&float_ty),
        "float mul rhs operand must become float"
    );
}

/// A float store through a default uint pointer refines the complete alias
/// chain back to the original parameter.
#[test]
fn float_deref_store_refines_pointer_alias_chain() {
    let float_ty = NirType::Float { bits: 32 };
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let uint_ptr = NirType::Ptr(Box::new(uint_ty.clone()));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("param_home".to_owned()),
            rhs: PreHirExpr::Var("out".to_owned()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("store_ptr".to_owned()),
            rhs: PreHirExpr::Var("param_home".to_owned()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("store_ptr".to_owned())),
                ty: uint_ty,
            },
            rhs: PreHirExpr::Var("value".to_owned()),
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding("param_home", uint_ptr.clone(), NirBindingOrigin::Temp),
            make_typed_binding("store_ptr", uint_ptr.clone(), NirBindingOrigin::Temp),
            make_typed_binding("value", float_ty.clone(), NirBindingOrigin::Temp),
        ],
        body,
        NirType::Unknown,
    );
    func.params.push(make_typed_binding(
        "out",
        uint_ptr,
        NirBindingOrigin::ParamIndex(2),
    ));

    let changed = apply_use_driven_type_infer_pass(&mut func);
    assert!(changed, "float store must refine pointer aliases");
    let float_ptr = NirType::Ptr(Box::new(float_ty));
    assert_eq!(func.params[0].ty, float_ptr);
    assert_eq!(
        func.locals
            .iter()
            .find(|binding| binding.name == "param_home")
            .map(|binding| &binding.ty),
        Some(&float_ptr)
    );
    assert_eq!(
        func.locals
            .iter()
            .find(|binding| binding.name == "store_ptr")
            .map(|binding| &binding.ty),
        Some(&float_ptr)
    );
}

/// Load { ptr: Var("p"), ty: uint32 } → p: Ptr(uint32)
#[test]
fn infers_ptr_from_load() {
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("x".to_owned()),
        rhs: PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Var("p".to_owned())),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        },
    }];
    let mut func = make_func(
        vec![make_binding("p"), make_binding("x")],
        body,
        NirType::Unknown,
    );
    let changed = apply_use_driven_type_infer_pass(&mut func);
    assert!(changed);
    assert_eq!(
        func.locals[0].ty,
        NirType::Ptr(Box::new(NirType::Int {
            bits: 32,
            signed: false
        }))
    );
}

#[test]
fn pointer_result_does_not_promote_known_integer_offset_param() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let ptr_ty = NirType::Ptr(Box::new(u32_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("end".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("base".to_owned())),
            rhs: Box::new(PreHirExpr::Cast {
                ty: u64_ty.clone(),
                expr: Box::new(PreHirExpr::Var("count".to_owned())),
            }),
            ty: ptr_ty.clone(),
        },
    }];
    let mut func = make_func(
        vec![make_typed_binding(
            "end",
            ptr_ty.clone(),
            NirBindingOrigin::TempPreserved,
        )],
        body,
        NirType::Unknown,
    );
    func.params = vec![
        make_typed_binding("base", ptr_ty.clone(), NirBindingOrigin::ParamIndex(0)),
        make_typed_binding("count", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
    ];

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(func.params[0].ty, ptr_ty);
    assert_eq!(func.params[1].ty, u64_ty);
}

#[test]
fn transitive_address_evidence_blocks_stale_scalar_copy_constraint() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("alias".to_owned()),
            rhs: PreHirExpr::Var("input".to_owned()),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor".to_owned()),
            rhs: PreHirExpr::Cast {
                ty: ptr_ty.clone(),
                expr: Box::new(PreHirExpr::Var("alias".to_owned())),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("value".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("cursor".to_owned())),
                ty: u8_ty,
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding("alias", u64_ty, NirBindingOrigin::TempPreserved),
            make_typed_binding("cursor", ptr_ty.clone(), NirBindingOrigin::TempPreserved),
            make_binding("value"),
        ],
        body,
        NirType::Unknown,
    );
    func.params = vec![make_typed_binding(
        "input",
        ptr_ty.clone(),
        NirBindingOrigin::ParamIndex(0),
    )];

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(func.params[0].ty, ptr_ty);
}

#[test]
fn scalar_only_local_pointer_constraint_converges_once() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("p".to_owned()),
            rhs: PreHirExpr::Cast {
                ty: byte_ptr_ty.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_owned())),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("sum".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("x".to_owned())),
                rhs: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                ty: u64_ty.clone(),
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding("x", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
            make_typed_binding("p", byte_ptr_ty, NirBindingOrigin::TempPreserved),
            make_typed_binding("sum", u64_ty.clone(), NirBindingOrigin::TempPreserved),
        ],
        body,
        NirType::Unknown,
    );
    func.is_64bit = true;

    assert!(apply_use_driven_type_infer_pass(&mut func));
    let x = func.locals.iter().find(|local| local.name == "x").unwrap();
    assert_eq!(x.ty, u64_ty);
    assert!(!apply_use_driven_type_infer_pass(&mut func));
}

#[test]
fn pointer_defined_local_with_scalar_address_arithmetic_stays_pointer() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor_base".to_owned()),
            rhs: PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::Var("input".to_owned())),
                offset: 4,
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("end".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("cursor_base".to_owned())),
                rhs: Box::new(PreHirExpr::Var("span".to_owned())),
                ty: u64_ty.clone(),
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding(
                "cursor_base",
                byte_ptr_ty.clone(),
                NirBindingOrigin::TempPreserved,
            ),
            make_typed_binding("span", u64_ty.clone(), NirBindingOrigin::TempPreserved),
            make_typed_binding("end", u64_ty, NirBindingOrigin::TempPreserved),
        ],
        body,
        NirType::Unknown,
    );
    func.params = vec![make_typed_binding(
        "input",
        byte_ptr_ty.clone(),
        NirBindingOrigin::ParamIndex(0),
    )];
    func.is_64bit = true;

    apply_use_driven_type_infer_pass(&mut func);
    let cursor_base = func
        .locals
        .iter()
        .find(|local| local.name == "cursor_base")
        .unwrap();
    assert!(matches!(cursor_base.ty, NirType::Ptr(_)));
    assert!(!apply_use_driven_type_infer_pass(&mut func));
}

#[test]
fn exact_scalar_return_constraint_demotes_non_address_pointer_local() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Var("acc".to_owned())))];
    let mut func = make_func(
        vec![make_typed_binding(
            "acc",
            byte_ptr_ty,
            NirBindingOrigin::StackOffset(-4),
        )],
        body,
        u64_ty.clone(),
    );

    apply_use_driven_type_infer_pass(&mut func);

    assert_eq!(func.locals[0].ty, u64_ty);
}

/// Deref store lhs: *p = val → p: Ptr(val_ty)
#[test]
fn infers_ptr_from_deref_store() {
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(PreHirExpr::Var("p".to_owned())),
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
    }];
    let mut func = make_func(vec![make_binding("p")], body, NirType::Unknown);
    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(
        func.locals[0].ty,
        NirType::Ptr(Box::new(NirType::Int {
            bits: 64,
            signed: false
        }))
    );
}

#[test]
fn call_target_use_promotes_binding_to_pointer() {
    let body = vec![PreHirStmt::Return(Some(PreHirExpr::Call {
        origin: None,
        target: "((code *)param_1)".to_owned(),
        args: vec![
            PreHirExpr::Var("param_2".to_owned()),
            PreHirExpr::Var("param_3".to_owned()),
        ],
        ty: NirType::Unknown,
    }))];
    let mut func = make_func(
        vec![
            make_typed_binding(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
                NirBindingOrigin::ParamIndex(0),
            ),
            make_typed_binding(
                "param_2",
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
                NirBindingOrigin::ParamIndex(1),
            ),
            make_typed_binding(
                "param_3",
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
                NirBindingOrigin::ParamIndex(2),
            ),
        ],
        body,
        NirType::Unknown,
    );
    func.is_64bit = false;

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.locals[0].ty, NirType::Ptr(Box::new(NirType::Unknown)));
}
