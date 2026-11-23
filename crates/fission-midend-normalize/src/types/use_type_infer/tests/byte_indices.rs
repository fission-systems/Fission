use super::super::apply_use_driven_type_infer_pass;
use super::super::byte_index_accumulators::{
    collect_byte_index_accumulator_evidence, narrow_byte_index_accumulators,
};
use super::{make_func, make_typed_binding};
use crate::prelude::*;
use crate::{HashMap, HashSet};

#[test]
fn narrows_byte_accumulator_used_as_byte_pointer_offset() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
    let body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("idx".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                ty: u8_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("idx".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                rhs: Box::new(PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("q".to_owned())),
                    ty: u8_ty.clone(),
                }),
                ty: u32_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                ty: byte_ptr_ty.clone(),
            },
        },
    ];
    let mut func = make_func(
        vec![
            make_typed_binding("p", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
            make_typed_binding("q", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
            make_typed_binding("cursor", byte_ptr_ty, NirBindingOrigin::TempPreserved),
            make_typed_binding("idx", u32_ty, NirBindingOrigin::TempPreserved),
        ],
        body,
        NirType::Unknown,
    );

    let changed = apply_use_driven_type_infer_pass(&mut func);
    assert!(changed);
    let idx = func
        .locals
        .iter()
        .find(|local| local.name == "idx")
        .unwrap();
    assert_eq!(idx.ty, u8_ty);
}

#[test]
fn does_not_narrow_plain_large_pointer_offset() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
    let body = vec![PreHirStmt::Assign {
        lhs: PreHirLValue::Var("cursor".to_owned()),
        rhs: PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".to_owned())),
            rhs: Box::new(PreHirExpr::Var("idx".to_owned())),
            ty: byte_ptr_ty.clone(),
        },
    }];
    let mut func = make_func(
        vec![
            make_typed_binding("p", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
            make_typed_binding("cursor", byte_ptr_ty, NirBindingOrigin::TempPreserved),
            make_typed_binding("idx", u32_ty.clone(), NirBindingOrigin::TempPreserved),
        ],
        body,
        NirType::Unknown,
    );

    apply_use_driven_type_infer_pass(&mut func);
    let idx = func
        .locals
        .iter()
        .find(|local| local.name == "idx")
        .unwrap();
    assert_eq!(idx.ty, u32_ty);
}

fn byte_ptr_ty() -> NirType {
    NirType::Ptr(Box::new(NirType::Int {
        bits: 8,
        signed: false,
    }))
}

fn wide_int_ty() -> NirType {
    NirType::Int {
        bits: 32,
        signed: false,
    }
}

fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name.to_owned()),
        rhs,
    }
}

fn var(name: &str) -> PreHirExpr {
    PreHirExpr::Var(name.to_owned())
}

fn byte_const(v: i64) -> PreHirExpr {
    PreHirExpr::Const(
        v,
        NirType::Int {
            bits: 8,
            signed: false,
        },
    )
}

fn qualifying_accumulator_body(name: &str, ptr_name: &str) -> Vec<PreHirStmt> {
    vec![
        // byte_seed_defs
        assign(name, byte_const(0)),
        // byte_update_defs (`x = x + <byte const>` recognized as an
        // accumulator update)
        assign(
            name,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(var(name)),
                rhs: Box::new(byte_const(1)),
                ty: wide_int_ty(),
            },
        ),
        // byte_pointer_offset_uses
        PreHirStmt::Expr(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var(ptr_name)),
            rhs: Box::new(var(name)),
            ty: byte_ptr_ty(),
        }),
    ]
}

#[test]
fn narrow_byte_index_accumulators_narrows_multiple_candidates_in_one_pass() {
    let mut body = qualifying_accumulator_body("x", "bptr");
    body.extend(qualifying_accumulator_body("z", "bptr"));
    let mut func = make_func(
        vec![
            make_typed_binding("bptr", byte_ptr_ty(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("x", wide_int_ty(), NirBindingOrigin::Temp),
            make_typed_binding("z", wide_int_ty(), NirBindingOrigin::Temp),
        ],
        body,
        NirType::Unknown,
    );

    assert!(narrow_byte_index_accumulators(&mut func));
    let expected = NirType::Int {
        bits: 8,
        signed: false,
    };
    assert_eq!(func.locals[1].ty, expected, "x should narrow to byte");
    assert_eq!(func.locals[2].ty, expected, "z should narrow to byte");
}

#[test]
fn narrow_byte_index_accumulators_skips_candidate_with_disallowed_use() {
    let mut body = qualifying_accumulator_body("x", "bptr");
    // A plain, unrecognized use of `x` elsewhere disqualifies it.
    body.push(PreHirStmt::Expr(PreHirExpr::Call {
        origin: None,
        target: "sink".to_owned(),
        args: vec![var("x")],
        ty: NirType::Unknown,
    }));
    let mut func = make_func(
        vec![
            make_typed_binding("bptr", byte_ptr_ty(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("x", wide_int_ty(), NirBindingOrigin::Temp),
        ],
        body,
        NirType::Unknown,
    );

    assert!(!narrow_byte_index_accumulators(&mut func));
    assert_eq!(func.locals[1].ty, wide_int_ty());
}

#[test]
fn byte_index_accumulator_evidence_excludes_only_the_self_referential_candidate() {
    // `x = x + (byte)z`: recognized as an accumulator update for `x`, so
    // `x`'s own reference inside that rhs must NOT also count as a
    // disallowed use of `x` -- but `z`, a *different* candidate embedded
    // in the same rhs (via the cast), must still be recorded normally,
    // since from `z`'s perspective this is just an ordinary use, not a
    // self-update.
    let candidates: HashSet<String> = ["x".to_owned(), "z".to_owned()].into_iter().collect();
    let known_binding_types: HashMap<String, NirType> = [
        ("x".to_owned(), wide_int_ty()),
        ("z".to_owned(), wide_int_ty()),
    ]
    .into_iter()
    .collect();
    let body = vec![assign(
        "x",
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("x")),
            rhs: Box::new(PreHirExpr::Cast {
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
                expr: Box::new(var("z")),
            }),
            ty: wide_int_ty(),
        },
    )];

    let mut evidence = HashMap::default();
    collect_byte_index_accumulator_evidence(
        &body,
        &candidates,
        &known_binding_types,
        &mut evidence,
    );

    let x_evidence = evidence.get("x").expect("x should have evidence");
    assert_eq!(x_evidence.def_count, 1);
    assert_eq!(x_evidence.byte_update_defs, 1);
    assert_eq!(
        x_evidence.disallowed_uses, 0,
        "x's own reference inside its recognized self-update must not double-count"
    );

    let z_evidence = evidence.get("z").expect("z should have evidence");
    assert_eq!(
        z_evidence.disallowed_uses, 1,
        "z, embedded in x's update rhs, is still an ordinary (disallowed) use from z's own perspective"
    );
    assert_eq!(z_evidence.def_count, 0);
}
