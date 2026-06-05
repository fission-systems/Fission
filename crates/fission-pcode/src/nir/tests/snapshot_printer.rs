//! Stable printer snapshots for regression (see `insta` / `cargo insta review`).

use super::*;
use indexmap::IndexMap;
use insta::assert_snapshot;

#[test]
fn snapshot_print_hir_function_minimal() {
    let func = HirFunction {
        name: "f_snapshot".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )))],
        calling_convention: CallingConvention::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: IndexMap::new(),
        callee_summaries: IndexMap::new(),
    };
    assert_snapshot!(print_hir_function(&func));
}

#[test]
fn printer_parenthesizes_bitwise_operand_of_equality() {
    let expr = HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Var("flags".to_string())),
            rhs: Box::new(HirExpr::Const(
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
        rhs: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )),
        ty: NirType::Bool,
    };

    assert_eq!(print_expr(&expr), "(flags & 4) == 0");
}

#[test]
fn printer_casts_pointer_subtraction_to_byte_difference() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let ptr_ty = NirType::Ptr(Box::new(uint_ty.clone()));
    let func = HirFunction {
        name: "ptr_diff".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "a".to_string(),
                ty: ptr_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            NirBinding {
                name: "b".to_string(),
                ty: ptr_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            NirBinding {
                name: "mask".to_string(),
                ty: uint_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("mask".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Cast {
                    ty: uint_ty.clone(),
                    expr: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(HirExpr::Var("a".to_string())),
                        rhs: Box::new(HirExpr::Var("b".to_string())),
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                    }),
                }),
                rhs: Box::new(HirExpr::Const(4, uint_ty.clone())),
                ty: uint_ty,
            },
        }],
        ..Default::default()
    };

    let rendered = print_hir_function(&func);
    assert!(rendered.contains("(uint)((uint8_t *)(a) - (uint8_t *)(b)) & 4"));
}
