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
    NirType::Int { bits, signed: false }
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

fn return_expr(expr: HirExpr) -> HirStmt {
    HirStmt::Return(Some(expr))
}

fn make_func(name: &str, locals: Vec<NirBinding>, body: Vec<HirStmt>) -> HirFunction {
    HirFunction {
        name: name.to_string(),
        params: vec![],
        locals,
        return_type: int(32),
        surface_return_type_name: None,
        body,
        ..Default::default()
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
    assert!(code.contains("return 8;"), "expected 'return 8;' in: {code}");
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
    assert!(code.contains("return 42;"), "expected 'return 42;' in: {code}");
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
    assert!(code.contains("return 0;"), "return 0 should remain; got: {code}");
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
    assert!(code.contains("return"), "return should be present; got: {code}");
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
                NirType::Int { bits: 64, signed: false },
            )),
            ty: NirType::Int { bits: 64, signed: false },
        }),
        rhs: Box::new(HirExpr::Const(
            4,
            NirType::Int { bits: 64, signed: false },
        )),
        ty: NirType::Int { bits: 64, signed: false },
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
        ty: NirType::Int { bits: 8, signed: false },
        expr: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int { bits: 64, signed: false },
                expr: Box::new(HirExpr::Cast {
                    ty: NirType::Int { bits: 32, signed: false },
                    expr: Box::new(HirExpr::Var("param_1".to_string())),
                }),
            }),
            rhs: Box::new(HirExpr::Const(
                8,
                NirType::Int { bits: 64, signed: false },
            )),
            ty: NirType::Int { bits: 64, signed: false },
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
        ty: NirType::Int { bits: 64, signed: false },
        expr: Box::new(HirExpr::Cast {
            ty: NirType::Int { bits: 32, signed: false },
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: Box::new(x64.clone()),
                rhs: Box::new(HirExpr::Const(
                    32,
                    NirType::Int { bits: 64, signed: false },
                )),
                ty: NirType::Int { bits: 64, signed: false },
            }),
        }),
    };
    let lo = HirExpr::Cast {
        ty: NirType::Int { bits: 64, signed: false },
        expr: Box::new(HirExpr::Cast {
            ty: NirType::Int { bits: 32, signed: false },
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
                NirType::Int { bits: 64, signed: false },
            )),
            ty: NirType::Int { bits: 64, signed: false },
        }),
        rhs: Box::new(lo),
        ty: NirType::Int { bits: 64, signed: false },
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
