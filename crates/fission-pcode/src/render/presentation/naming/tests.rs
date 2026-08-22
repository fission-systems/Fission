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
        params: vec![param("param_1", ptr_ty()), param("param_2", int_ty(32, true))],
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
