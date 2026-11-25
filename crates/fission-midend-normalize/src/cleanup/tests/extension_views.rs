//! `canonicalize_extension_binding_casts`: a widening cast's destination
//! carries the extension kind (the builder lowers `INT_SEXT` to a cast to the
//! signed wider type, `INT_ZEXT` to the unsigned one), and C extends by the
//! operand's type. These pin that the printed C keeps the IR's extension.

use super::*;

fn ty(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn binding(name: &str, ty: NirType) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: None,
        initializer: None,
    }
}

fn var(name: &str) -> PreHirExpr {
    PreHirExpr::Var(name.to_string())
}

fn cast(to: NirType, expr: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Cast {
        ty: to,
        expr: Box::new(expr),
    }
}

/// `wide = <rhs>; return wide;` over the given bindings.
fn assign_wide(bindings: Vec<PreHirBinding>, rhs: PreHirExpr) -> PreHirFunction {
    PreHirFunction {
        name: "test_extension_view".to_string(),
        int_param_offsets: Vec::new(),
        params: bindings,
        locals: vec![binding("wide", ty(64, true))],
        return_type: ty(64, true),
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("wide".to_string()),
                rhs,
            },
            PreHirStmt::Return(Some(var("wide"))),
        ],
        ..Default::default()
    }
}

fn assigned_rhs(func: &PreHirFunction) -> &PreHirExpr {
    let PreHirStmt::Assign { rhs, .. } = &func.body[0] else {
        panic!("expected the assignment to remain: {:#?}", func.body);
    };
    rhs
}

#[test]
fn sign_extension_of_an_unsigned_binding_gets_a_signed_view() {
    // `movsxd rbx, edi` lowers to `(long long)param_1`. Type recovery settled
    // on `param_1: uint`, and C would zero-extend that.
    let mut func = assign_wide(
        vec![binding("param_1", ty(32, false))],
        cast(ty(64, true), var("param_1")),
    );

    assert!(canonicalize_extension_binding_casts(&mut func));

    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, true), cast(ty(32, true), var("param_1"))),
        "the printed C must sign-extend: (long long)(int)param_1"
    );
}

#[test]
fn zero_extension_of_a_signed_binding_gets_an_unsigned_view() {
    // `INT_ZEXT` of an `int` binding: C would sign-extend `(ulong)x`.
    let mut func = assign_wide(
        vec![binding("lane", ty(32, true))],
        cast(ty(64, false), var("lane")),
    );

    assert!(canonicalize_extension_binding_casts(&mut func));

    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, false), cast(ty(32, false), var("lane"))),
    );
}

#[test]
fn extensions_that_already_agree_are_left_alone() {
    for (operand, wide) in [
        (ty(32, true), ty(64, true)),
        (ty(32, false), ty(64, false)),
        (ty(8, false), ty(32, false)),
    ] {
        let mut func = assign_wide(vec![binding("x", operand)], cast(wide, var("x")));
        let before = func.body.clone();

        assert!(!canonicalize_extension_binding_casts(&mut func));
        assert_eq!(func.body, before);
    }
}

#[test]
fn same_width_and_narrowing_casts_are_not_extensions() {
    for to in [ty(32, true), ty(16, true), ty(8, false)] {
        let mut func = assign_wide(vec![binding("x", ty(32, false))], cast(to, var("x")));
        let before = func.body.clone();

        assert!(!canonicalize_extension_binding_casts(&mut func));
        assert_eq!(func.body, before);
    }
}

#[test]
fn constants_keep_the_extension_their_stored_value_carries() {
    // `imul r64, r/m64, imm32`: the stored i64 is already sign-extended.
    let rhs = cast(ty(64, true), PreHirExpr::Const(-0x6db6_db6d, ty(32, false)));
    let mut func = assign_wide(vec![], rhs.clone());

    assert!(!canonicalize_extension_binding_casts(&mut func));
    assert_eq!(assigned_rhs(&func), &rhs);
}

#[test]
fn a_truncating_inner_cast_takes_the_view_instead_of_a_new_layer() {
    // `(long long)(uint)wide64`: the inner cast only truncates, so reading it
    // as `(int)` keeps the same 32 bits and gives the sign extension.
    let mut func = assign_wide(
        vec![binding("wide64", ty(64, false))],
        cast(ty(64, true), cast(ty(32, false), var("wide64"))),
    );

    assert!(canonicalize_extension_binding_casts(&mut func));

    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, true), cast(ty(32, true), var("wide64"))),
    );
}

#[test]
fn an_extending_inner_cast_keeps_its_own_kind() {
    // `(ulong)(int)byte`: the inner cast sign-extends a `uchar`; retyping it
    // to `(uint)` would turn that into a zero extension. It gets its own view
    // and the outer cast gets another.
    let mut func = assign_wide(
        vec![binding("byte", ty(8, false))],
        cast(ty(64, false), cast(ty(32, true), var("byte"))),
    );

    assert!(canonicalize_extension_binding_casts(&mut func));

    assert_eq!(
        assigned_rhs(&func),
        &cast(
            ty(64, false),
            cast(
                ty(32, false),
                cast(ty(32, true), cast(ty(8, true), var("byte")))
            )
        ),
    );
}

#[test]
fn extensions_in_conditions_calls_and_stores_are_canonicalized() {
    let sext = || cast(ty(64, true), var("param_1"));
    let viewed = || cast(ty(64, true), cast(ty(32, true), var("param_1")));
    let mut func = PreHirFunction {
        name: "test_extension_view_positions".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            binding("param_1", ty(32, false)),
            binding("p", NirType::Ptr(Box::new(ty(64, true)))),
        ],
        return_type: NirType::Unknown,
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
                    lhs: Box::new(sext()),
                    rhs: Box::new(PreHirExpr::Const(0, ty(64, true))),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    origin: None,
                    target: "consume".to_string(),
                    args: vec![sext()],
                    ty: NirType::Unknown,
                })]
                .into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(sext()),
                        ty: NirType::Ptr(Box::new(ty(64, true))),
                    }),
                    ty: ty(64, true),
                },
                rhs: sext(),
            },
        ],
        ..Default::default()
    };

    assert!(canonicalize_extension_binding_casts(&mut func));

    let dump = format!("{:?}", func.body);
    assert_eq!(
        dump.matches(&format!("{:?}", viewed())).count(),
        4,
        "every extension of param_1 must carry the signed view:\n{dump}"
    );
}

#[test]
fn the_normalize_pipeline_keeps_a_sign_extension_of_an_unsigned_parameter() {
    // The whole pipeline, not the pass alone: the extension has to survive
    // every cast cleanup that runs before and after the boundary.
    // `param_1` is compared unsigned, so it stays `uint`, and it is
    // sign-extended (`movsxd`) into a 64-bit product.
    let mut func = PreHirFunction {
        name: "test_extension_view_pipeline".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: ty(32, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        return_type: ty(64, true),
        body: vec![
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(var("param_1")),
                    rhs: Box::new(PreHirExpr::Const(10, ty(32, false))),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, ty(64, true))))]
                    .into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(cast(ty(64, true), var("param_1"))),
                rhs: Box::new(PreHirExpr::Const(3, ty(64, true))),
                ty: ty(64, true),
            })),
        ],
        ..Default::default()
    };

    crate::normalize_hir_function(&mut func);

    let param_ty = &func.params[0].ty;
    let dump = format!("{:?}", func.body);
    let signed_view = format!(
        "{:?}",
        cast(ty(64, true), cast(ty(32, true), var("param_1")))
    );
    assert!(
        matches!(param_ty, NirType::Int { signed: true, .. }) || dump.contains(&signed_view),
        "param_1 is {param_ty:?}, so its sign extension must print as \
         (long long)(int)param_1:\n{dump}"
    );
}

// ── cast elision must leave the extension for the view to restore ──────────
//
// `cast_elision_pass` runs before type refinement settles binding types (the
// cast-elision stage itself re-runs the type fixed point after stripping). A
// widening cast is the only place this IR records sign- versus
// zero-extension, so stripping one before the operand's type is final leaves
// `canonicalize_extension_binding_casts` nothing to restore: `wide = x` then
// extends by whatever `x` ends up typed as.

/// `wide = (long long)x; return wide;` with `x` bound to `x_ty`.
fn sext_of(x_ty: NirType) -> PreHirFunction {
    assign_wide(vec![binding("x", x_ty)], cast(ty(64, true), var("x")))
}

/// What later type recovery does: settle `x` on a 32-bit unsigned type.
fn settle_x_as_uint(func: &mut PreHirFunction) {
    for b in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if b.name == "x" {
            b.ty = ty(32, false);
        }
    }
}

#[test]
fn cast_elision_keeps_an_extension_of_an_unknown_typed_operand() {
    // `x` has no scalar type yet, so nothing establishes that `wide = x`
    // extends the way `INT_SEXT` does.
    let mut func = sext_of(NirType::Unknown);
    cast_elision_pass(&mut func);
    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, true), var("x")),
        "an extension of an operand of unknown type must stay explicit"
    );

    settle_x_as_uint(&mut func);
    canonicalize_extension_binding_casts(&mut func);
    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, true), cast(ty(32, true), var("x"))),
        "once `x` settles as uint the sign extension must still print"
    );
}

#[test]
fn cast_elision_keeps_an_extension_whose_operand_may_still_be_retyped() {
    // Signedness agrees *now*; the type fixed point that runs after cast
    // elision can still settle `x` the other way.
    let mut func = sext_of(ty(32, true));
    cast_elision_pass(&mut func);
    settle_x_as_uint(&mut func);
    canonicalize_extension_binding_casts(&mut func);
    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(64, true), cast(ty(32, true), var("x"))),
        "a sign extension must survive a later retype of its operand"
    );
}

#[test]
fn cast_elision_keeps_an_extension_returned_directly() {
    let mut func = PreHirFunction {
        name: "test_return_extension".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![binding("x", NirType::Unknown)],
        return_type: ty(64, true),
        body: vec![PreHirStmt::Return(Some(cast(ty(64, true), var("x"))))],
        ..Default::default()
    };
    cast_elision_pass(&mut func);
    assert_eq!(
        func.body[0],
        PreHirStmt::Return(Some(cast(ty(64, true), var("x")))),
        "a returned extension of an unknown-typed operand must stay explicit"
    );
}

#[test]
fn cast_elision_keeps_a_conversion_from_a_float_binding() {
    // A float binding is not in the pass's scalar type table, so it reads
    // as unknown there; the conversion must not be dropped on that account.
    let mut func = PreHirFunction {
        name: "test_float_conversion".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![binding("f", NirType::Float { bits: 64 })],
        locals: vec![binding("i", ty(32, true))],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("i".to_string()),
                rhs: cast(ty(32, true), var("f")),
            },
            PreHirStmt::Return(Some(var("i"))),
        ],
        return_type: ty(32, true),
        ..Default::default()
    };
    cast_elision_pass(&mut func);
    assert_eq!(
        assigned_rhs(&func),
        &cast(ty(32, true), var("f")),
        "float -> int conversion must stay explicit"
    );
}

#[test]
fn cast_elision_still_strips_casts_that_cannot_extend() {
    // Same width: a reinterpretation the assignment performs anyway.
    let mut same_width = PreHirFunction {
        name: "test_same_width".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![binding("x", ty(32, false))],
        locals: vec![binding("y", ty(32, true))],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("y".to_string()),
                rhs: cast(ty(32, true), var("x")),
            },
            PreHirStmt::Return(Some(var("y"))),
        ],
        return_type: ty(32, true),
        ..Default::default()
    };
    assert!(cast_elision_pass(&mut same_width));
    assert_eq!(assigned_rhs(&same_width), &var("x"));

    // Widen then truncate back to the operand's width: the extended bits are
    // all discarded, whichever extension it was.
    let mut round_trip = PreHirFunction {
        name: "test_round_trip".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![binding("x", ty(32, false))],
        locals: vec![binding("y", ty(32, false))],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("y".to_string()),
                rhs: cast(ty(64, true), var("x")),
            },
            PreHirStmt::Return(Some(var("y"))),
        ],
        return_type: ty(32, false),
        ..Default::default()
    };
    assert!(cast_elision_pass(&mut round_trip));
    assert_eq!(assigned_rhs(&round_trip), &var("x"));
}
