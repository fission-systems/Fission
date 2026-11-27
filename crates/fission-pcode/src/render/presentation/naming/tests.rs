use super::apply_semantic_naming;
use crate::midend::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType,
};

fn int_ty(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn ptr_ty() -> NirType {
    NirType::Ptr(Box::new(int_ty(32, true)))
}

fn param(name: &str, ty: NirType) -> NirBinding {
    NirBinding {
        name: name.into(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(0)),
        initializer: None,
    }
}

fn local(name: &str, ty: NirType) -> NirBinding {
    NirBinding {
        name: name.into(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

#[test]
fn pointer_dereference_renames_generic_param() {
    // int sum(int *param_1) { return *param_1; }
    let mut func = HirFunction {
        name: "sum".into(),
        params: vec![param("param_1", ptr_ty())],
        return_type: int_ty(32, true),
        body: vec![HirStmt::Return(Some(HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".into())),
            ty: int_ty(32, true),
        }))],
        ..Default::default()
    };
    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.params[0].name, "ptr");
    assert_eq!(
        func.body,
        vec![HirStmt::Return(Some(HirExpr::Load {
            ptr: Box::new(HirExpr::Var("ptr".into())),
            ty: int_ty(32, true),
        }))]
    );
}

#[test]
fn equal_score_pointer_names_use_original_name_as_stable_tie_break() {
    let mut func = HirFunction {
        name: "f".into(),
        locals: vec![local("uVar2", ptr_ty()), local("uVar1", ptr_ty())],
        return_type: NirType::Unknown,
        body: vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("uVar2".into())),
                ty: int_ty(32, true),
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("uVar1".into())),
                ty: int_ty(32, true),
            }),
        ],
        ..Default::default()
    };

    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.locals[0].name, "p");
    assert_eq!(func.locals[1].name, "ptr");
}

#[test]
fn loop_counter_gets_short_letter_name() {
    // for (uVar1 = 0; uVar1 < param_2; uVar1 = uVar1 + 1) {}
    let mut func = HirFunction {
        name: "f".into(),
        params: vec![
            param("param_1", ptr_ty()),
            param("param_2", int_ty(32, true)),
        ],
        locals: vec![local("uVar1", int_ty(32, true))],
        return_type: NirType::Unknown,
        body: vec![HirStmt::For {
            init: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("uVar1".into()),
                rhs: HirExpr::Const(0, int_ty(32, true)),
            })),
            cond: Some(HirExpr::Binary {
                op: HirBinaryOp::Lt,
                lhs: Box::new(HirExpr::Var("uVar1".into())),
                rhs: Box::new(HirExpr::Var("param_2".into())),
                ty: NirType::Bool,
            }),
            update: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("uVar1".into()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("uVar1".into())),
                    rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                    ty: int_ty(32, true),
                },
            })),
            body: vec![],
        }],
        ..Default::default()
    };
    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.locals[0].name, "i");
}

#[test]
fn semantic_naming_does_not_duplicate_an_existing_binding_name() {
    // The loop counter candidate wants `i`, but a distinct recovered local
    // already owns that name.  Both bindings must remain addressable in the
    // rendered function rather than collapsing into duplicate declarations.
    let mut func = HirFunction {
        name: "f".into(),
        locals: vec![
            local("uVar1", int_ty(32, true)),
            local("i", int_ty(32, true)),
        ],
        return_type: NirType::Unknown,
        body: vec![HirStmt::For {
            init: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("uVar1".into()),
                rhs: HirExpr::Const(0, int_ty(32, true)),
            })),
            cond: Some(HirExpr::Binary {
                op: HirBinaryOp::Lt,
                lhs: Box::new(HirExpr::Var("uVar1".into())),
                rhs: Box::new(HirExpr::Const(2, int_ty(32, true))),
                ty: NirType::Bool,
            }),
            update: Some(Box::new(HirStmt::Assign {
                lhs: HirLValue::Var("uVar1".into()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("uVar1".into())),
                    rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                    ty: int_ty(32, true),
                },
            })),
            body: vec![HirStmt::Expr(HirExpr::Var("i".into()))],
        }],
        ..Default::default()
    };

    assert!(!apply_semantic_naming(&mut func));
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["uVar1", "i"]
    );
}

#[test]
fn semantic_naming_does_not_rename_a_pointer_local_to_a_parameter_name() {
    // The first pointer local takes `ptr`; the second may be proposed as `p`,
    // but a parameter already owns that identifier in the function scope.
    let mut func = HirFunction {
        name: "f".into(),
        params: vec![param("p", ptr_ty())],
        locals: vec![local("xVar1", ptr_ty()), local("xVar2", ptr_ty())],
        return_type: NirType::Unknown,
        body: vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("xVar1".into())),
                ty: int_ty(32, true),
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("xVar2".into())),
                ty: int_ty(32, true),
            }),
        ],
        ..Default::default()
    };

    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.params[0].name, "p");
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ptr", "xVar2"]
    );
    assert_eq!(
        func.body,
        vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("ptr".into())),
                ty: int_ty(32, true),
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("xVar2".into())),
                ty: int_ty(32, true),
            }),
        ]
    );
}

#[test]
fn size_naming_renames_memcpy_length_argument() {
    // memcpy(dst, src, param_3);
    let mut func = HirFunction {
        name: "f".into(),
        params: vec![
            param("param_1", ptr_ty()),
            param("param_2", ptr_ty()),
            param("param_3", int_ty(64, false)),
        ],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Expr(HirExpr::Call {
            origin: None,
            target: "memcpy".into(),
            args: vec![
                HirExpr::Var("param_1".into()),
                HirExpr::Var("param_2".into()),
                HirExpr::Var("param_3".into()),
            ],
            ty: NirType::Unknown,
        })],
        ..Default::default()
    };
    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.params[2].name, "n");
}

#[test]
fn does_not_rename_already_meaningful_names() {
    // A binding not matching the generic param_N/xVarN shape is left alone.
    let mut func = HirFunction {
        name: "f".into(),
        params: vec![param("count", int_ty(32, true))],
        return_type: int_ty(32, true),
        body: vec![HirStmt::Return(Some(HirExpr::Var("count".into())))],
        ..Default::default()
    };
    assert!(!apply_semantic_naming(&mut func));
    assert_eq!(func.params[0].name, "count");
}

#[test]
fn size_naming_strips_module_and_import_prefixes() {
    for target in [
        "msvcrt.dll!memcpy",
        "__imp_memcpy",
        "ucrtbase.dll!__imp_memcpy",
    ] {
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![
                param("param_1", ptr_ty()),
                param("param_2", ptr_ty()),
                param("param_3", int_ty(64, false)),
            ],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Expr(HirExpr::Call {
                origin: None,
                target: target.into(),
                args: vec![
                    HirExpr::Var("param_1".into()),
                    HirExpr::Var("param_2".into()),
                    HirExpr::Var("param_3".into()),
                ],
                ty: NirType::Unknown,
            })],
            ..Default::default()
        };
        assert!(apply_semantic_naming(&mut func), "target={target}");
        assert_eq!(func.params[2].name, "n", "target={target}");
    }
}

#[test]
fn pointer_rename_rewrites_local_slot_initializers() {
    let byte_ptr_ty = NirType::Ptr(Box::new(int_ty(8, false)));
    let mut slot_1 = local("slot_1", byte_ptr_ty.clone());
    slot_1.initializer = Some(HirExpr::Cast {
        ty: byte_ptr_ty.clone(),
        expr: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".into())),
            offset: 1,
        }),
    });

    let mut func = HirFunction {
        name: "u32_transform_buf".into(),
        params: vec![param("param_1", byte_ptr_ty)],
        locals: vec![slot_1],
        return_type: int_ty(32, false),
        body: vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("param_1".into())),
                ty: int_ty(8, false),
            }),
            HirStmt::Return(Some(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("slot_1".into())),
                ty: int_ty(8, false),
            })),
        ],
        ..Default::default()
    };

    assert!(apply_semantic_naming(&mut func));
    assert_eq!(func.params[0].name, "ptr");
    assert_eq!(
        func.locals[0].initializer,
        Some(HirExpr::Cast {
            ty: NirType::Ptr(Box::new(int_ty(8, false))),
            expr: Box::new(HirExpr::PtrOffset {
                base: Box::new(HirExpr::Var("ptr".into())),
                offset: 1,
            }),
        })
    );
    assert!(
        super::super::invariants::check_hir_presentation_invariants(&func, &func).is_ok(),
        "renamed slot_1 initializer should pass presentation invariants cleanly"
    );
}

#[test]
fn rename_updates_address_of_local_and_va_start_last_named_param() {
    let mut func = HirFunction {
        name: "f".into(),
        params: vec![param("param_1", ptr_ty())],
        locals: vec![local("local_8", int_ty(32, true))],
        return_type: int_ty(32, true),
        body: vec![
            HirStmt::VaStart {
                va_list: HirExpr::Var("local_8".into()),
                last_named_param: "param_1".into(),
            },
            HirStmt::Return(Some(HirExpr::Cast {
                ty: int_ty(32, true),
                expr: Box::new(HirExpr::AddressOfLocal("local_8".into())),
            })),
        ],
        ..Default::default()
    };
    super::util::rename_var_everywhere(&mut func, "param_1", "fmt");
    super::util::rename_var_everywhere(&mut func, "local_8", "ap");
    assert_eq!(
        func.body,
        vec![
            HirStmt::VaStart {
                va_list: HirExpr::Var("ap".into()),
                last_named_param: "fmt".into(),
            },
            HirStmt::Return(Some(HirExpr::Cast {
                ty: int_ty(32, true),
                expr: Box::new(HirExpr::AddressOfLocal("ap".into())),
            })),
        ]
    );
}
