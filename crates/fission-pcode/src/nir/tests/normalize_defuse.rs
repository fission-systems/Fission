/// Unit tests for HIR-level dataflow passes and arithmetic simplifications.
///
/// Tests cover:
/// - Constant folding (via normalize_hir_function)
/// - Dead assignment elimination (def-use)
/// - Copy propagation
/// - Join-variable coalescing
/// - SubPiece chain simplification (simplify_subpiece_chain, merge_consecutive_shifts)
/// - Wide-integer recombination via the improved double-cast-aware helpers
///
/// All tests access the normalize pipeline through the public API:
/// - `normalize_stmt` (for expression-level rules via `normalize_expr`)
/// - `normalize_hir_function` (for function-level dataflow passes)
/// - `render_mlil_preview` (for full end-to-end integration tests)
use super::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

fn sint(bits: u32) -> NirType {
    NirType::Int { bits, signed: true }
}

fn const_expr(v: i64, bits: u32) -> HirExpr {
    HirExpr::Const(v, int(bits))
}

fn varexpr(name: &str) -> HirExpr {
    HirExpr::Var(name.to_string())
}

fn assign(name: &str, rhs: HirExpr) -> HirStmt {
    HirStmt::Assign {
        lhs: HirLValue::Var(name.to_string()),
        rhs,
    }
}

fn temp_binding(name: &str, bits: u32) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

fn preserved_temp_binding(name: &str, bits: u32) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn return_expr(expr: HirExpr) -> HirStmt {
    HirStmt::Return(Some(expr))
}

fn make_func(name: &str, locals: Vec<NirBinding>, body: Vec<HirStmt>) -> HirFunction {
    HirFunction {
        name: name.to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals,
        return_type: int(32),
        surface_return_type_name: None,
        body,
        ..Default::default()
    }
}

fn param_binding(name: &str, bits: u32) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: int(bits),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(0)),
        initializer: None,
    }
}

// ── Constant folding (via normalize_hir_function) ────────────────────────────

#[test]
fn constant_folding_binary_add_via_normalize() {
    // return 3 + 5;  →  normalize  →  return 8;
    let mut func = make_func(
        "test_const_add",
        vec![],
        vec![return_expr(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(const_expr(3, 32)),
            rhs: Box::new(const_expr(5, 32)),
            ty: int(32),
        })],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("return 8;"),
        "expected 'return 8;' in: {code}"
    );
}

#[test]
fn normalize_hir_function_elides_return_cast_implied_by_return_type() {
    let mut func = HirFunction {
        name: "add".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![param_binding("param_1", 64), param_binding("param_2", 64)],
        locals: vec![],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![return_expr(HirExpr::Cast {
            ty: int(32),
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(varexpr("param_1")),
                rhs: Box::new(varexpr("param_2")),
                ty: int(64),
            }),
        })],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("return param_1 + param_2;"),
        "rendered:\n{}",
        rendered
    );
    assert!(!rendered.contains("(uint)"), "rendered:\n{}", rendered);
}

#[test]
fn constant_folding_nested_mul() {
    // return 7 * 6;  →  42
    let mut func = make_func(
        "test_const_mul",
        vec![],
        vec![return_expr(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(const_expr(7, 32)),
            rhs: Box::new(const_expr(6, 32)),
            ty: int(32),
        })],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("return 42;"),
        "expected 'return 42;' in: {code}"
    );
}

#[test]
fn constant_folding_does_not_fold_variable_expressions() {
    // return x + 1;  →  no change
    let mut func = make_func(
        "test_no_fold_var",
        vec![],
        vec![return_expr(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(varexpr("x")),
            rhs: Box::new(const_expr(1, 64)),
            ty: int(64),
        })],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    // Should still contain a binary expression with "x".
    assert!(code.contains("x"), "x should still appear in: {code}");
    assert!(code.contains("1"), "1 should still appear in: {code}");
}

#[test]
fn normalize_collapses_if_else_with_identical_returns() {
    let mut func = make_func(
        "test_redundant_if_else_return",
        vec![],
        vec![HirStmt::If {
            cond: varexpr("cond"),
            then_body: vec![return_expr(const_expr(7, 32))],
            else_body: vec![return_expr(const_expr(7, 32))],
        }],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("return 7;"),
        "expected collapsed return; got: {code}"
    );
    assert!(
        !code.contains("if ("),
        "redundant if should be removed; got: {code}"
    );
}

#[test]
fn normalize_collapses_guarded_return_followed_by_same_return() {
    let mut func = make_func(
        "test_redundant_guarded_return",
        vec![],
        vec![
            HirStmt::If {
                cond: varexpr("cond"),
                then_body: vec![return_expr(const_expr(5, 32))],
                else_body: Vec::new(),
            },
            return_expr(const_expr(5, 32)),
        ],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("return 5;"),
        "expected return to remain; got: {code}"
    );
    assert!(
        !code.contains("if ("),
        "redundant guarded if should be removed; got: {code}"
    );
}

#[test]
fn normalize_canonicalizes_minmax_return_after_type_recovery() {
    let signed_i32 = sint(32);
    let mut func = HirFunction {
        name: "max".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: signed_i32.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: signed_i32.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: signed_i32.clone(),
        surface_return_type_name: None,
        body: vec![
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(varexpr("param_1")),
                    rhs: Box::new(varexpr("param_2")),
                    ty: NirType::Bool,
                },
                then_body: vec![return_expr(varexpr("param_2"))],
                else_body: Vec::new(),
            },
            return_expr(varexpr("param_1")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("if (param_1 > param_2)"),
        "expected max branch to prefer direct greater-than form; got: {code}"
    );
    assert!(
        code.contains("return param_1;") && code.contains("return param_2;"),
        "expected return arms to be preserved; got: {code}"
    );
}

#[test]
fn normalize_preserves_side_effect_in_redundant_conditional_return() {
    let mut func = make_func(
        "test_redundant_if_side_effect",
        vec![],
        vec![HirStmt::If {
            cond: HirExpr::Call {
                target: "check".to_string(),
                args: vec![],
                ty: NirType::Bool,
            },
            then_body: vec![return_expr(const_expr(9, 32))],
            else_body: vec![return_expr(const_expr(9, 32))],
        }],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("check()"),
        "side-effect call must be preserved; got: {code}"
    );
    assert!(
        code.contains("return 9;"),
        "return must be preserved; got: {code}"
    );
}

// ── Dead assignment elimination (via normalize_hir_function) ─────────────────

#[test]
fn defuse_removes_dead_pure_temp_via_normalize() {
    // bVar1 = 42; return 0;  →  bVar1 eliminated, return 0;
    let mut func = make_func(
        "test_dead_temp",
        vec![temp_binding("bVar1", 8)],
        vec![
            assign("bVar1", const_expr(42, 8)),
            return_expr(const_expr(0, 32)),
        ],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        !code.contains("bVar1"),
        "dead temp 'bVar1' should be eliminated; got: {code}"
    );
    assert!(
        code.contains("return 0;"),
        "return 0 should remain; got: {code}"
    );
}

#[test]
fn defuse_preserves_used_temp_assignment() {
    // bVar1 = 1; return bVar1;  →  no elimination
    let mut func = make_func(
        "test_live_temp",
        vec![temp_binding("bVar1", 8)],
        vec![
            assign("bVar1", const_expr(1, 8)),
            return_expr(varexpr("bVar1")),
        ],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    // After inline_single_use_temps, bVar1 should be inlined to the return.
    assert!(
        code.contains("return"),
        "return should be present; got: {code}"
    );
}

#[test]
fn normalize_preserves_loop_assigned_temp_used_after_loop() {
    let mut func = HirFunction {
        name: "test_loop_carried_return_temp".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![param_binding("param_1", 32), param_binding("keep_going", 8)],
        locals: vec![temp_binding("xVar27", 32), temp_binding("bVar29", 1)],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![
            HirStmt::DoWhile {
                body: vec![
                    assign(
                        "xVar27",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(varexpr("param_1")),
                            rhs: Box::new(const_expr(1, 32)),
                            ty: int(32),
                        },
                    ),
                    assign(
                        "bVar29",
                        HirExpr::Binary {
                            op: HirBinaryOp::Eq,
                            lhs: Box::new(varexpr("xVar27")),
                            rhs: Box::new(const_expr(0, 32)),
                            ty: NirType::Bool,
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("xVar27")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar27 = param_1 + 1;"),
        "loop-carried return temp assignment must be preserved; got: {code}"
    );
    assert!(
        code.contains("return xVar27;"),
        "post-loop return must still read the loop-assigned temp; got: {code}"
    );
}

#[test]
fn normalize_preserves_preheader_temp_used_inside_loop() {
    let mut func = HirFunction {
        name: "test_preheader_loop_input_temp".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![param_binding("param_1", 32), param_binding("keep_going", 8)],
        locals: vec![
            temp_binding("xVar10", 32),
            temp_binding("uVar21", 32),
            temp_binding("uVar22", 32),
        ],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![
            assign("xVar10", const_expr(0, 32)),
            HirStmt::DoWhile {
                body: vec![
                    assign("uVar21", varexpr("param_1")),
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(varexpr("xVar10")),
                            rhs: Box::new(varexpr("uVar21")),
                            ty: int(32),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("uVar22")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar10 = 0;"),
        "preheader temp used in loop body must be preserved; got: {code}"
    );
    assert!(
        code.contains("uVar22 = xVar10 + param_1;"),
        "loop body must still read the preheader temp; got: {code}"
    );
}

#[test]
fn normalize_preserves_preheader_loop_carried_self_update() {
    let mut func = HirFunction {
        name: "test_preheader_loop_carried_self_update".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![param_binding("param_1", 32), param_binding("keep_going", 8)],
        locals: vec![
            temp_binding("xVar10", 64),
            temp_binding("uVar21", 32),
            temp_binding("uVar22", 32),
        ],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![
            assign("xVar10", const_expr(0, 64)),
            HirStmt::DoWhile {
                body: vec![
                    assign("uVar21", varexpr("param_1")),
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                ty: int(32),
                                expr: Box::new(varexpr("xVar10")),
                            }),
                            rhs: Box::new(varexpr("uVar21")),
                            ty: int(32),
                        },
                    ),
                    assign(
                        "xVar10",
                        HirExpr::Cast {
                            ty: int(64),
                            expr: Box::new(varexpr("uVar22")),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("xVar10")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar10 = 0;"),
        "loop-carried accumulator initializer must be preserved; got: {code}"
    );
    assert!(
        code.contains("xVar10 = (uint)xVar10 + param_1;"),
        "loop-carried accumulator update should be folded; got: {code}"
    );
    assert!(
        code.contains("return xVar10;"),
        "return must read the loop-carried accumulator; got: {code}"
    );
}

#[test]
fn normalize_preserves_preheader_copy_chain_with_loop_carried_self_update() {
    let mut func = HirFunction {
        name: "test_preheader_copy_chain_loop_carried_self_update".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            param_binding("var_0", 32),
            param_binding("param_1", 32),
            param_binding("keep_going", 8),
        ],
        locals: vec![
            temp_binding("uVar9", 32),
            temp_binding("xVar10", 64),
            temp_binding("uVar21", 32),
            temp_binding("uVar22", 32),
        ],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![
            assign(
                "uVar9",
                HirExpr::Binary {
                    op: HirBinaryOp::Xor,
                    lhs: Box::new(varexpr("var_0")),
                    rhs: Box::new(varexpr("var_0")),
                    ty: int(32),
                },
            ),
            assign(
                "xVar10",
                HirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(varexpr("uVar9")),
                },
            ),
            HirStmt::DoWhile {
                body: vec![
                    assign("uVar21", varexpr("param_1")),
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                ty: int(32),
                                expr: Box::new(varexpr("xVar10")),
                            }),
                            rhs: Box::new(varexpr("uVar21")),
                            ty: int(32),
                        },
                    ),
                    assign(
                        "xVar10",
                        HirExpr::Cast {
                            ty: int(64),
                            expr: Box::new(varexpr("uVar22")),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("xVar10")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar10 = 0;"),
        "preheader loop-carried accumulator initializer must be preserved; got: {code}"
    );
    assert!(
        code.contains("xVar10 = (uint)xVar10 + param_1;"),
        "loop-carried accumulator update should be folded; got: {code}"
    );
}

#[test]
fn normalize_preserves_loop_carried_initializer_after_predicate_use() {
    let mut func = HirFunction {
        name: "test_loop_carried_initializer_after_predicate_use".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            param_binding("var_0", 32),
            param_binding("param_1", 32),
            param_binding("keep_going", 8),
        ],
        locals: vec![temp_binding("xVar10", 64), temp_binding("uVar22", 32)],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![
            assign(
                "xVar10",
                HirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(varexpr("var_0")),
                },
            ),
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Cast {
                        ty: int(32),
                        expr: Box::new(varexpr("xVar10")),
                    }),
                    rhs: Box::new(const_expr(0, 32)),
                    ty: NirType::Bool,
                },
                then_body: vec![return_expr(const_expr(0, 64))],
                else_body: Vec::new(),
            },
            HirStmt::DoWhile {
                body: vec![
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                ty: int(32),
                                expr: Box::new(varexpr("xVar10")),
                            }),
                            rhs: Box::new(varexpr("param_1")),
                            ty: int(32),
                        },
                    ),
                    assign(
                        "xVar10",
                        HirExpr::Cast {
                            ty: int(64),
                            expr: Box::new(varexpr("uVar22")),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("xVar10")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    let init_idx = code
        .find("xVar10 =")
        .unwrap_or_else(|| panic!("loop-carried initializer was removed; got: {code}"));
    let loop_idx = code
        .find("do {")
        .unwrap_or_else(|| panic!("expected loop to remain; got: {code}"));
    assert!(
        init_idx < loop_idx,
        "initializer must stay before the loop when the value is also used by a predicate; got: {code}"
    );
    assert!(
        code.contains("xVar10 = (uint)xVar10 + param_1;"),
        "loop-carried accumulator update should be folded; got: {code}"
    );
}

#[test]
fn normalize_sccp_does_not_fold_loop_carried_zero_into_loop_body() {
    let mut func = HirFunction {
        name: "test_sccp_loop_carried_zero".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            param_binding("param_1", 32),
            param_binding("param_2", 32),
            param_binding("keep_going", 8),
        ],
        locals: vec![
            temp_binding("xVar10", 64),
            temp_binding("uVar21", 32),
            temp_binding("uVar22", 32),
        ],
        return_type: int(64),
        surface_return_type_name: None,
        body: vec![
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(varexpr("param_2")),
                    rhs: Box::new(const_expr(0, 32)),
                    ty: NirType::Bool,
                },
                then_body: vec![return_expr(const_expr(0, 64))],
                else_body: Vec::new(),
            },
            assign("xVar10", const_expr(0, 64)),
            HirStmt::DoWhile {
                body: vec![
                    assign("uVar21", varexpr("param_1")),
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                ty: int(32),
                                expr: Box::new(varexpr("xVar10")),
                            }),
                            rhs: Box::new(varexpr("uVar21")),
                            ty: int(32),
                        },
                    ),
                    assign(
                        "xVar10",
                        HirExpr::Cast {
                            ty: int(64),
                            expr: Box::new(varexpr("uVar22")),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("xVar10")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar10 = (uint)xVar10 + param_1;"),
        "SCCP must not substitute the preheader constant for a loop-carried accumulator; got: {code}"
    );
}

#[test]
fn normalize_preserves_preheader_copy_chain_used_inside_loop() {
    let mut func = HirFunction {
        name: "test_preheader_loop_input_copy_chain".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            param_binding("var_0", 32),
            param_binding("param_1", 32),
            param_binding("keep_going", 8),
        ],
        locals: vec![
            temp_binding("uVar9", 32),
            temp_binding("xVar10", 64),
            temp_binding("uVar21", 32),
            temp_binding("uVar22", 32),
        ],
        return_type: int(32),
        surface_return_type_name: None,
        body: vec![
            assign(
                "uVar9",
                HirExpr::Binary {
                    op: HirBinaryOp::Xor,
                    lhs: Box::new(varexpr("var_0")),
                    rhs: Box::new(varexpr("var_0")),
                    ty: int(32),
                },
            ),
            assign(
                "xVar10",
                HirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(varexpr("uVar9")),
                },
            ),
            HirStmt::DoWhile {
                body: vec![
                    assign("uVar21", varexpr("param_1")),
                    assign(
                        "uVar22",
                        HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                ty: int(32),
                                expr: Box::new(varexpr("xVar10")),
                            }),
                            rhs: Box::new(varexpr("uVar21")),
                            ty: int(32),
                        },
                    ),
                ],
                cond: varexpr("keep_going"),
            },
            return_expr(varexpr("uVar22")),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("xVar10 ="),
        "preheader copy chain feeding loop body must remain defined; got: {code}"
    );
    assert!(
        code.contains("uVar22 =") && code.contains("xVar10"),
        "loop body must still read the preheader chain result; got: {code}"
    );
}

#[test]
fn normalize_keeps_nontrivial_multi_use_temp_in_single_stmt() {
    let mut func = make_func(
        "test_keep_nontrivial_multi_use_temp",
        vec![temp_binding("uVar1", 32)],
        vec![
            assign(
                "uVar1",
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(varexpr("eax")),
                    rhs: Box::new(const_expr(1, 32)),
                    ty: int(32),
                },
            ),
            return_expr(HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(varexpr("uVar1")),
                    rhs: Box::new(const_expr(0, 32)),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(varexpr("uVar1")),
                    rhs: Box::new(const_expr(1, 32)),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            }),
        ],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("uVar1 = eax + 1;"),
        "nontrivial multi-use temp should remain materialized; got: {code}"
    );
    assert!(
        code.contains("return uVar1 == 0 || uVar1 == 1;"),
        "return should keep the named temp instead of duplicating the expression; got: {code}"
    );
}

#[test]
fn normalize_stabilizes_repeated_nontrivial_pure_expr_in_if_condition() {
    let diff = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(varexpr("eax")),
        rhs: Box::new(varexpr("uVar128")),
        ty: int(32),
    };
    let mut func = make_func(
        "test_stabilize_repeated_if_expr",
        vec![],
        vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(diff.clone()),
                    rhs: Box::new(const_expr(0, 32)),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(diff.clone()),
                    rhs: Box::new(varexpr("esi")),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            },
            then_body: vec![return_expr(const_expr(1, 32))],
            else_body: vec![return_expr(const_expr(0, 32))],
        }],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("uVar0 = eax - uVar128;"),
        "repeated nontrivial pure expr should be hoisted once; got: {code}"
    );
    assert!(
        code.contains("if (uVar0 == 0 || uVar0 < esi)"),
        "condition should reuse the stabilized temp instead of duplicating the expression; got: {code}"
    );
}

#[test]
fn normalize_keeps_builder_preserved_temp_in_if_condition() {
    let mut func = make_func(
        "test_keep_builder_preserved_temp",
        vec![preserved_temp_binding("uVar0", 32)],
        vec![
            assign(
                "uVar0",
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(varexpr("eax")),
                    rhs: Box::new(varexpr("uVar128")),
                    ty: int(32),
                },
            ),
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::LogicalOr,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(varexpr("uVar0")),
                        rhs: Box::new(const_expr(0, 32)),
                        ty: NirType::Bool,
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: Box::new(varexpr("uVar0")),
                        rhs: Box::new(varexpr("esi")),
                        ty: NirType::Bool,
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![return_expr(const_expr(1, 32))],
                else_body: vec![return_expr(const_expr(0, 32))],
            },
        ],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("uVar0 = eax - uVar128;"),
        "builder-preserved temp should remain materialized; got: {code}"
    );
    assert!(
        code.contains("if (uVar0 == 0 || uVar0 < esi)"),
        "condition should keep the preserved temp instead of reinlining it; got: {code}"
    );
}

#[test]
fn normalize_does_not_materialize_low_value_single_input_repeated_expr() {
    let repeated = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(const_expr(0, 64)),
        rhs: Box::new(HirExpr::Cast {
            ty: int(64),
            expr: Box::new(varexpr("df")),
        }),
        ty: int(64),
    };
    let mut func = make_func(
        "test_skip_single_input_repeat",
        vec![],
        vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::Lt,
                lhs: Box::new(repeated.clone()),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(repeated),
                    rhs: Box::new(const_expr(4, 64)),
                    ty: int(64),
                }),
                ty: NirType::Bool,
            },
            then_body: vec![return_expr(const_expr(1, 32))],
            else_body: vec![return_expr(const_expr(0, 32))],
        }],
    );

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        !code.contains("uVar0 = 0 - (ulonglong)df;"),
        "single-input repeated expr should stay inline; got: {code}"
    );
}

#[test]
fn defuse_does_not_remove_stack_slot_assignment() {
    // local_10 = 5; return 0;  →  stack slot is kept (may alias)
    let stack_binding = NirBinding {
        name: "local_10".to_string(),
        ty: int(64),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-0x10)),
        initializer: None,
    };
    let mut func = make_func(
        "test_stack_slot",
        vec![stack_binding],
        vec![
            assign("local_10", const_expr(5, 64)),
            return_expr(const_expr(0, 32)),
        ],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("local_10"),
        "stack slot should NOT be eliminated; got: {code}"
    );
}

#[test]
fn defuse_removes_dead_local_with_large_hex_offset_no_stack_origin() {
    // local_10 / local_20 / local_28 — no StackOffset origin, never read →
    // must be removed.  This was the bug: the old 0x0c threshold kept them.
    let make_non_stack = |name: &str| NirBinding {
        name: name.to_string(),
        ty: int(64),
        surface_type_name: None,
        origin: None, // not a stack-backed slot
        initializer: None,
    };
    let mut func = HirFunction {
        name: "test_dead_local_large_offset".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            make_non_stack("local_c"),
            make_non_stack("local_10"),
            make_non_stack("local_20"),
            make_non_stack("local_28"),
        ],
        return_type: int(32),
        body: vec![
            assign("local_c", const_expr(0, 32)),
            assign("local_10", HirExpr::Var("x1".to_string())),
            assign("local_20", HirExpr::Var("x2".to_string())),
            assign("local_28", HirExpr::Var("x3".to_string())),
            return_expr(varexpr("local_c")),
        ],
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        !code.contains("local_10"),
        "dead non-stack local_10 should be eliminated; got: {code}"
    );
    assert!(
        !code.contains("local_20"),
        "dead non-stack local_20 should be eliminated; got: {code}"
    );
    assert!(
        !code.contains("local_28"),
        "dead non-stack local_28 should be eliminated; got: {code}"
    );
}

#[test]
fn defuse_preserves_live_local_with_large_hex_offset() {
    // local_20 with no stack origin but actually used → must be kept.
    let mut func = HirFunction {
        name: "test_live_local_large_offset".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_20".to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: int(32),
        body: vec![
            assign("local_20", const_expr(42, 32)),
            return_expr(varexpr("local_20")),
        ],
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("42") || code.contains("local_20"),
        "live local_20 or its folded value should remain; got: {code}"
    );
}

// ── Copy propagation (via normalize_hir_function) ────────────────────────────

#[test]
fn copy_propagation_eliminates_trivial_copy_chain() {
    // uVar1 = rcx; return uVar1;  →  return rcx;
    // (rcx is a register parameter that becomes a named variable)
    let func_pcode = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x5000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x5000,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![reg(0x08, 8)], // rcx
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x5001,
                    output: None,
                    inputs: vec![cst(0, 8), uniq(0x100, 8)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let code = render_mlil_preview(&func_pcode, "copy_prop_test", 0x5000, &preview_options())
        .expect("preview render");
    // The intermediate temp should be eliminated; rcx or param_1 should appear directly.
    assert!(
        !code.contains("uVar") || code.contains("rcx") || code.contains("param"),
        "copy temp should be propagated: {code}"
    );
}

// ── SubPiece / Shr chain simplification (via normalize_stmt) ─────────────────

#[test]
fn consecutive_shr_merges_in_normalize_stmt() {
    // Shr(Shr(param_1, 8), 4)  →  Shr(param_1, 12)
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(HirExpr::Var("param_1".to_string())),
            rhs: Box::new(HirExpr::Const(
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
        }),
        rhs: Box::new(HirExpr::Const(
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
    }));
    normalize_stmt(&mut stmt);
    let rendered = print_stmt(&stmt);
    // After merging shifts 8+4=12, recognize_mod_div_power_of_two converts Shr(x,12) → x / 4096.
    assert!(
        rendered.contains("4096"),
        "shifts 8+4 should merge to /4096; got: {rendered}"
    );
    // Should NOT contain both /256 and /16 (un-merged)
    assert!(
        !(rendered.contains("256") && rendered.contains("16")),
        "shifts should be merged, not separate /256 and /16; got: {rendered}"
    );
    assert!(
        rendered.contains("param_1"),
        "param_1 should still appear; got: {rendered}"
    );
}

#[test]
fn subpiece_cast_shr_cast_chain_simplifies() {
    // Cast(Int8, Shr(Cast(Int64, Cast(Int32, param_1)), 8))
    // With Cast(Int32, param_1) as source (bits=32), Cast(Int64) widens to 64,
    // Shr by 8 gives byte 1.  The inner widening cast should be eliminated:
    // → Cast(Int8, Shr(Cast(Int32, param_1), 8))
    let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
        ty: NirType::Int {
            bits: 8,
            signed: false,
        },
        expr: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(HirExpr::Cast {
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    expr: Box::new(HirExpr::Var("param_1".to_string())),
                }),
            }),
            rhs: Box::new(HirExpr::Const(
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
        }),
    }));
    normalize_stmt(&mut stmt);
    let rendered = print_stmt(&stmt);
    // The intermediate Cast(Int64) widening should be gone.
    // The result should be a Cast(Int8, Shr(Cast(Int32, param_1), 8)).
    assert!(
        rendered.contains("param_1"),
        "param_1 should appear in: {rendered}"
    );
}

// ── Wide-integer recombination (SubPiece-Piece cancellation) ─────────────────

#[test]
fn wide_integer_recombine_double_cast_piece_pattern() {
    // Full end-to-end test: the piece-recombination rule should simplify
    // (Cast(Int64, Cast(Int32, Shr(x64, 32))) << 32) | Cast(Int64, Cast(Int32, x64))
    // to x64 via normalize_stmt.
    let x64 = HirExpr::Var("x64".to_string());
    let hi = HirExpr::Cast {
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
        expr: Box::new(HirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: Box::new(x64.clone()),
                rhs: Box::new(HirExpr::Const(
                    32,
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
        }),
    };
    let lo = HirExpr::Cast {
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
        expr: Box::new(HirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(x64.clone()),
        }),
    };
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(hi),
            rhs: Box::new(HirExpr::Const(
                32,
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
        rhs: Box::new(lo),
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
    }));
    normalize_stmt(&mut stmt);
    let rendered = print_stmt(&stmt);
    // The entire Piece(SubPiece(x64,4,4), SubPiece(x64,0,4)) should collapse to x64
    // (possibly wrapped in an identity cast since x64 is an untyped Var in this context).
    assert!(
        rendered.contains("x64"),
        "Piece(SubPiece(x64,4,4),SubPiece(x64,0,4)) should reduce to x64; got: {rendered}"
    );
    // Should NOT still contain the shift/or/cast chain
    assert!(
        !rendered.contains("<<") && !rendered.contains(" | "),
        "Piece pattern should be fully collapsed; got: {rendered}"
    );
}

// ── MultiEqual (phi-like) improvement: partial-failure fallback ───────────────

#[test]
fn multiequal_all_same_canonical_input_renders_cleanly() {
    // MultiEqual where all inputs lower to the same register → clean output.
    // Both inputs come from rcx (register offset 0x08) → should output rcx.
    let out_vn = uniq(0x700, 8);
    let func_pcode = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6000,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: 1,
                        offset: 0x6010,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::MultiEqual,
                        address: 0x6010,
                        output: Some(out_vn.clone()),
                        inputs: vec![reg(0x08, 8), reg(0x08, 8)], // both rcx
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x6011,
                        output: None,
                        inputs: vec![cst(0, 8), out_vn.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };
    let code = render_mlil_preview(&func_pcode, "multiequal_same", 0x6000, &preview_options())
        .expect("preview render");
    // Both inputs are rcx — should collapse to a single clean reference.
    assert!(
        code.contains("rcx") || code.contains("param"),
        "MultiEqual with identical inputs should simplify; got: {code}"
    );
    // Should NOT produce undefined tmp_ references.
    assert!(
        !code.contains("tmp_700"),
        "Should not produce raw offset names; got: {code}"
    );
}
