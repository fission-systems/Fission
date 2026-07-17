use super::super::test_support::*;
use super::*;

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn lhs_var(stmt: &HirStmt) -> Option<&str> {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => Some(name.as_str()),
        _ => None,
    }
}

fn expr_var(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => expr_var(expr),
        _ => None,
    }
}

fn expr_contains_shr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Shr,
            ..
        } => true,
        HirExpr::Binary { lhs, rhs, .. } => expr_contains_shr(lhs) || expr_contains_shr(rhs),
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => expr_contains_shr(expr),
        HirExpr::Call { args, .. } => args.iter().any(expr_contains_shr),
        HirExpr::Load { ptr, .. } => expr_contains_shr(ptr),
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            expr_contains_shr(base)
        }
        HirExpr::Index { base, index, .. } => expr_contains_shr(base) || expr_contains_shr(index),
        HirExpr::AggregateCopy { src, .. } => expr_contains_shr(src),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_shr(cond) || expr_contains_shr(then_expr) || expr_contains_shr(else_expr)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

#[test]
fn loop_carried_register_update_reuses_prior_binding_and_param() {
    let rax = reg(0x00, 8);
    let rcx = reg(0x08, 8);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(0)]),
                // Dummy read of param_1 register rcx to establish param arity at entry
                op(
                    99,
                    PcodeOpcode::Copy,
                    Some(varnode(0x99)),
                    vec![rcx.clone()],
                ),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(rax.clone()),
                    vec![rax.clone(), constant(1)],
                ),
                op(
                    3,
                    PcodeOpcode::IntAdd,
                    Some(rcx.clone()),
                    vec![rcx.clone(), constant(4)],
                ),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(0x1020, 2, vec![op(5, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let preheader = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("preheader lowering");
    let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some(init_name)),
        "loop-carried accumulator update should reuse the preheader binding: {loop_body:?}"
    );
    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_1")),
        "loop-carried parameter register update should assign back to the parameter: {loop_body:?}"
    );
}

#[test]
fn win64_ecx_self_loop_without_preheader_uses_param_1() {
    // Replicates the find_first_set_bit pattern: ECX is the first parameter on
    // Windows x64 (param_1 = RCX). The loop has NO preheader that initialises ECX
    // — ECX comes directly from the caller. The loop body right-shifts ECX by 1
    // and then branches back to itself or exits.
    // Expected: the loop-carried update should bind to "param_1", not a new xVar.
    let ecx = reg(0x08, 4);
    let cond = varnode(0x10);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::IntRight,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                op(1, PcodeOpcode::CBranch, None, vec![constant(0x1000), cond]),
            ],
        ),
        block_at(0x1010, 1, vec![op(2, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![0, 1];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_1")),
        "ECX self-loop without preheader should bind to param_1: {loop_body:?}"
    );
}

#[test]
fn win64_ecx_self_loop_with_external_pred_uses_param_1() {
    // Replicates find_first_set_bit more closely:
    //   block_0 (entry): reads ECX but doesn't write it, falls through to block_1
    //   block_1 (loop): ECX >>= 1, CBranch back to block_1 OR exit to block_2
    // block_1 has TWO predecessors: block_0 (external) and block_1 (self).
    // The explicit-merge synthesizer sees 2 preds and may create an xVar phi.
    // Expected: the loop body should still use "param_1" for ECX, not a new xVar.
    let ecx = reg(0x08, 4);
    let unique_out = varnode(0x20); // some output written by block_0's read of ECX
    let cond = varnode(0x10);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                // reads ECX (param_1) but does not write ECX
                op(
                    0,
                    PcodeOpcode::IntAnd,
                    Some(unique_out),
                    vec![ecx.clone(), ecx.clone()],
                ),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::IntRight,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        ),
        block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let _ = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("preheader lowering");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_1")),
        "ECX loop-carried with external predecessor should use param_1: {loop_body:?}"
    );
}

#[test]
fn win64_ecx_zext_then_shr_self_loop_uses_param_1() {
    // Replicates the exact find_first_set_bit bug in a SINGLE self-loop block:
    //   block_0 (entry): just falls through — no ops that write ECX/RCX
    //   block_1 (self-loop): IntZExt ECX→RCX, then IntRight ECX,1→ECX, then CBranch back
    // The IntZExt creates a size=8 RCX binding. The IntRight is size=4 ECX.
    // Without the fix, the RCX binding hijacks ECX via varnode_key_may_alias_output,
    // causing IntRight to read xVar... instead of param_1.
    let ecx = reg(0x08, 4);
    let rcx = reg(0x08, 8);
    let cond = varnode(0x10);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                // Dummy read of param_1 register ecx to establish param arity at entry
                op(
                    99,
                    PcodeOpcode::Copy,
                    Some(varnode(0x99)),
                    vec![ecx.clone()],
                ),
                op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                // IntZExt ECX → RCX (size=8 binding — must NOT hijack the following ECX update)
                op(
                    1,
                    PcodeOpcode::IntZExt,
                    Some(rcx.clone()),
                    vec![ecx.clone()],
                ),
                // IntRight ECX, 1 → ECX (size=4 — the real loop-carried update)
                op(
                    2,
                    PcodeOpcode::IntRight,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        ),
        block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let _ = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("entry lowering");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_1")),
        "ECX IntRight after IntZExt RCX in same block should still bind to param_1, not hijacked xVar: {loop_body:?}"
    );
}

#[test]
fn win64_ecx_intzext_and_shr_in_loop_body_uses_param_1() {
    // Exact replica of find_first_set_bit pattern:
    //   block_0 (entry): fallthrough to block_1
    //   block_1 (loop body): IntRight ECX→ECX, IntZExt ECX→RCX, CBranch back
    // The IntZExt is INSIDE the loop body (not entry). This used to create
    // an explicit-merge binding for RCX (size=8) that hijacked ECX (size=4).
    let ecx = reg(0x08, 4);
    let rcx = reg(0x08, 8);
    let cond = varnode(0x10);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                // Dummy read of param_1 register ecx to establish param arity at entry
                op(
                    99,
                    PcodeOpcode::Copy,
                    Some(varnode(0x99)),
                    vec![ecx.clone()],
                ),
                op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                // IntRight ECX, 1 → ECX (the real loop-carried update)
                op(
                    1,
                    PcodeOpcode::IntRight,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                // IntZExt ECX → RCX (temporary cast inside loop)
                op(
                    2,
                    PcodeOpcode::IntZExt,
                    Some(rcx.clone()),
                    vec![ecx.clone()],
                ),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        ),
        block_at(0x1020, 2, vec![op(4, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let _ = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("entry lowering");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_1")),
        "ECX IntRight with IntZExt RCX inside same loop body should bind to param_1: {loop_body:?}"
    );
}

#[test]
fn loop_carried_register_update_does_not_promote_prior_defined_abi_scratch() {
    let rdx = reg(0x10, 8);
    let mut blocks = vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(rdx.clone()), vec![constant(5)]),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(rdx.clone()),
                vec![rdx.clone(), constant(1)],
            ),
            op(2, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
        ],
    )];
    blocks[0].successors = vec![0];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 1,
    });

    let name = builder.loop_carried_output_binding_name(
        &pcode.blocks[0],
        1,
        &pcode.blocks[0].ops[1],
        &rdx,
    );

    assert_ne!(
        name.as_deref(),
        Some("param_2"),
        "prior-defined ABI scratch should not be promoted to param_2: {name:?}"
    );
    // Hardware register identity is allowed so self-reads keep loop-carried
    // accumulation (`rdx = rdx + 1`) instead of folding the pre-loop constant.
    if let Some(name) = name.as_deref() {
        assert!(
            matches!(name, "rdx" | "edx" | "rax" | "eax"),
            "expected hardware register binding, got {name}"
        );
    }
}

#[test]
fn loop_carried_gpr32_update_with_prior_wide_def_does_not_rebind_param() {
    let r8 = reg(0x80, 8);
    let r8d = reg(0x80, 4);
    let rax = reg(0x00, 8);
    let rdx = reg(0x10, 8);
    let mut blocks = vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(r8.clone()), vec![rdx]),
            op(
                1,
                PcodeOpcode::IntSub,
                Some(r8.clone()),
                vec![r8.clone(), rax],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(r8d.clone()),
                vec![r8d.clone(), constant(4)],
            ),
            op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
        ],
    )];
    blocks[0].successors = vec![0];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("loop body lowering");

    assert!(
        !loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some("param_3")),
        "R8D mask derived from a prior R8 temp must not mutate param_3: {loop_body:?}"
    );
    let mask_stmt = loop_body
        .iter()
        .find(|stmt| lhs_var(stmt).is_some() && format!("{stmt:?}").contains("Const(4"))
        .expect("materialized R8D mask");
    assert_eq!(
        lhs_var(mask_stmt),
        loop_body.iter().find_map(lhs_var),
        "R8D mask should keep the prior wide materialized binding instead of creating a fresh narrow temp: {loop_body:?}"
    );
}

#[test]
fn loop_carried_register_update_reuses_wide_prior_for_gpr32_update() {
    let rax = reg(0x00, 8);
    let eax = reg(0x00, 4);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(rax), vec![constant(0)]),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(eax.clone()),
                    vec![eax, constant(1)],
                ),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let preheader = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("preheader lowering");
    let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some(init_name)),
        "32-bit loop update should reuse the 64-bit zero initializer binding: {loop_body:?}"
    );
}

#[test]
fn loop_carried_proof_rejects_register_phase_killed_before_backedge() {
    let edx = reg(0x8, 4);
    let observed = varnode(0x180);
    let mut blocks = vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(observed), vec![edx.clone()]),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(edx.clone()),
                vec![edx.clone(), constant(1)],
            ),
            op(2, PcodeOpcode::Copy, Some(edx.clone()), vec![constant(7)]),
            op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
        ],
    )];
    blocks[0].successors = vec![0];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder
            .prove_loop_carried_register_update(0, 1, &edx)
            .is_none(),
        "a register phase killed before the latch must not receive a stable loop binding"
    );
    assert!(
        builder
            .prove_loop_carried_register_update(0, 2, &edx)
            .is_none(),
        "a later phase whose input was already killed must not inherit the prior iteration"
    );
}

#[test]
fn loop_carried_proof_accepts_exact_self_loop_definition() {
    let edx = reg(0x8, 4);
    let mut blocks = vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(edx.clone()),
                vec![edx.clone(), constant(1)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
        ],
    )];
    blocks[0].successors = vec![0];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    let proof = builder
        .prove_loop_carried_register_update(0, 0, &edx)
        .expect("self-reading latch definition should have a carried proof");
    assert_eq!(proof.definition_site(), (0, 0));
    assert_eq!(proof.loop_head(), 0);
}

/// Byte accumulator loop: `xor eax,eax; L: add al,[ptr]; movzx eax,al; ptr++; jnz L`.
/// The size-1 `al` self-update must be loop-carried so the add is not folded to
/// a plain load (last-byte-only collapse).
#[test]
fn loop_carried_byte_accumulator_with_movzx_preserves_add() {
    // x86 register layout: EAX@0 size4, AL@0 size1, EDX@0x10 size4
    let eax = reg(0x0, 4);
    let al = reg(0x0, 1);
    let edx = reg(0x10, 4);
    let loaded = varnode(0x170);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                // xor eax,eax seed
                op(0, PcodeOpcode::Copy, Some(eax.clone()), vec![constant(0)]),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                // load byte
                op(
                    2,
                    PcodeOpcode::Load,
                    Some(loaded.clone()),
                    vec![constant(3), edx.clone()],
                ),
                // add al, loaded
                op(
                    3,
                    PcodeOpcode::IntAdd,
                    Some(al.clone()),
                    vec![al.clone(), loaded],
                ),
                // movzx eax, al (value-preserving widen — must not kill carried al)
                op(4, PcodeOpcode::IntZExt, Some(eax.clone()), vec![al.clone()]),
                // ptr++
                op(
                    5,
                    PcodeOpcode::IntAdd,
                    Some(edx.clone()),
                    vec![edx.clone(), constant(1)],
                ),
                op(6, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        // Exit successor so the latch is a proper loop tail (not an infinite SCC only).
        block_at(
            0x1020,
            2,
            vec![op(7, PcodeOpcode::Return, None, vec![eax.clone()])],
        ),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        PreviewBuilder::is_loop_carried_register_update_candidate(&al),
        "size-1 AL must be a loop-carried candidate"
    );
    assert!(
        !builder.loop_bodies.is_empty(),
        "synthetic CFG must identify a loop body"
    );
    // op_idx is the in-block vector index (Load=0, IntAdd=1, ZExt=2, …), not seq_num.
    let proof = builder
        .prove_loop_carried_register_update(1, 1, &al)
        .expect("byte self-add with passthrough movzx must prove loop-carried");
    assert_eq!(proof.definition_site(), (1, 1));

    let code = crate::midend::render_mlil_preview(&pcode, "byte_accum", 0x1000, &options)
        .expect("render byte accumulator");
    // Must keep a self-update / accumulate form — not `al = *mem` alone.
    let collapsed_to_load_only = code.lines().any(|l| {
        let t = l.trim();
        t.starts_with("al = *") && !t.contains('+')
    });
    assert!(
        !collapsed_to_load_only,
        "must not collapse to bare `al = *mem` without add:\n{code}"
    );
    assert!(
        code.contains('+') || code.contains("+= "),
        "expected AL/byte accumulation in loop body:\n{code}"
    );
}

/// Binding priority: stack-param seed must win over anonymous merge temps so
/// IntRight writeback stays on the same name as loop-header reads.
#[test]
fn loop_carried_stack_param_seed_preferred_over_anonymous_merge_temp() {
    // Prove that when a loop-carried IntRight has a stack-param seed, the
    // binding name is the formal param (not a fresh uVar). Covered end-to-end
    // by decompiling the m32 O2 control_flow binary; this unit checks the
    // lower-block shape that previously lost `>>= 1`.
    let eax = reg(0x0, 4);
    let ecx = reg(0x4, 4);
    let edx = reg(0x8, 4);
    let zf = reg(0x206, 1);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::IntXor,
                    Some(edx.clone()),
                    vec![edx.clone(), edx.clone()],
                ),
                // Non-constant seed so constant folding cannot erase the loop.
                op(
                    1,
                    PcodeOpcode::Load,
                    Some(eax.clone()),
                    vec![constant(3), reg(0x14, 4)],
                ),
                op(
                    2,
                    PcodeOpcode::IntEqual,
                    Some(zf.clone()),
                    vec![eax.clone(), constant(0)],
                ),
                op(
                    3,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1030), zf.clone()],
                ),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(4, PcodeOpcode::Copy, Some(ecx.clone()), vec![eax.clone()]),
                op(
                    5,
                    PcodeOpcode::IntAnd,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                op(
                    6,
                    PcodeOpcode::IntAdd,
                    Some(edx.clone()),
                    vec![edx.clone(), ecx],
                ),
                op(
                    7,
                    PcodeOpcode::IntRight,
                    Some(eax.clone()),
                    vec![eax.clone(), constant(1)],
                ),
                op(
                    8,
                    PcodeOpcode::IntEqual,
                    Some(zf.clone()),
                    vec![eax.clone(), constant(0)],
                ),
                op(9, PcodeOpcode::BoolNegate, Some(varnode(0x50)), vec![zf]),
                op(
                    10,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1010), varnode(0x50)],
                ),
            ],
        ),
        block_at(
            0x1030,
            2,
            vec![
                op(11, PcodeOpcode::Copy, Some(eax.clone()), vec![edx]),
                op(12, PcodeOpcode::Return, None, vec![]),
            ],
        ),
    ];
    blocks[0].successors = vec![1, 2];
    blocks[1].successors = vec![1, 2];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.is_64bit = false;
    options.pointer_size = 4;
    options.pe_x64_only = false;
    options.calling_convention = CallingConvention::X86_32;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let _entry = builder.lower_block_stmts(&pcode.blocks[0]).expect("entry");
    let loop_body = builder.lower_block_stmts(&pcode.blocks[1]).expect("loop");

    let shr_stmt = loop_body.iter().find(|s| {
        matches!(
            s,
            HirStmt::Assign {
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Shr | HirBinaryOp::Sar,
                    ..
                },
                ..
            }
        )
    });
    assert!(
        shr_stmt.is_some(),
        "expected IntRight assignment in loop body: {loop_body:?}"
    );
    if let Some(HirStmt::Assign {
        lhs: HirLValue::Var(ind),
        rhs: HirExpr::Binary { lhs: a, .. },
        ..
    }) = shr_stmt
    {
        assert!(
            matches!(a.as_ref(), HirExpr::Var(name) if name == ind),
            "shift must be self-update on the induction binding, got {shr_stmt:?}"
        );
        // Prefer formal/stable names over pure anonymous temps when possible.
        assert!(
            !ind.starts_with("uVar") || loop_body.iter().any(|s| {
                matches!(
                    s,
                    HirStmt::Assign {
                        lhs: HirLValue::Var(name),
                        rhs: HirExpr::Var(src),
                        ..
                    } if name == "ecx" && src == ind
                )
            }),
            "induction binding {ind} should remain readable as loop-carried self-update: {loop_body:?}"
        );
    }
}

/// Size-1 self-loop with trailing ZExt must still prove carried (ZExt is
/// value-preserving and must not kill the narrow definition).
#[test]
fn loop_carried_proof_accepts_byte_self_add_before_zext() {
    let eax = reg(0x0, 4);
    let al = reg(0x0, 1);
    let mut blocks = vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(al.clone()),
                vec![al.clone(), constant(1)],
            ),
            op(1, PcodeOpcode::IntZExt, Some(eax), vec![al.clone()]),
            op(2, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
        ],
    )];
    blocks[0].successors = vec![0];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    let proof = builder
        .prove_loop_carried_register_update(0, 0, &al)
        .expect("size-1 self-add before ZExt should prove loop-carried");
    assert_eq!(proof.definition_site(), (0, 0));
}

#[test]
fn loop_carried_backedge_update_reuses_external_header_seed_binding() {
    let rdx = reg(0x10, 8);
    let edx = reg(0x10, 4);
    let rbx = reg(0x18, 8);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(rdx.clone()), vec![constant(6)]),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(2, PcodeOpcode::Copy, Some(rbx), vec![rdx.clone()]),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(
                    4,
                    PcodeOpcode::IntSub,
                    Some(edx.clone()),
                    vec![edx, constant(2)],
                ),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(0x1030, 3, vec![op(6, PcodeOpcode::Return, None, vec![])]),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![2];
    blocks[2].successors = vec![1, 3];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let preheader = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("preheader lowering");
    let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
    let latch = builder
        .lower_block_stmts(&pcode.blocks[2])
        .expect("latch lowering");

    assert!(
        latch.iter().any(|stmt| lhs_var(stmt) == Some(init_name)),
        "backedge update should assign to the external loop-header seed binding: {latch:?}"
    );
    assert!(
        !latch.iter().any(|stmt| lhs_var(stmt) == Some("param_2")),
        "internal ABI register accumulator must not be promoted to param_2: {latch:?}"
    );
}

#[test]
fn aarch64_loop_carried_register_update_reuses_wide_prior_for_w_gpr_update() {
    let x20 = reg(0x40a0, 8);
    let w20 = reg(0x40a0, 4);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(x20), vec![constant(0)]),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(w20.clone()),
                    vec![w20, constant(1)],
                ),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![1];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.calling_convention = CallingConvention::AArch64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let preheader = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("preheader lowering");
    let init_name = lhs_var(&preheader[0]).expect("preheader init binding");
    let loop_body = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("loop lowering");

    assert!(
        loop_body
            .iter()
            .any(|stmt| lhs_var(stmt) == Some(init_name)),
        "AArch64 W-register loop update should reuse the X-register initializer binding: {loop_body:?}"
    );
}

#[test]
fn m32_popcount_loop_carries_add_and_shr() {
    // Minimal x86-32 count_bits O2 shape:
    //   entry: edx=0; eax=param seed; if eax==0 exit
    //   loop:  ecx=eax; ecx&=1; edx+=ecx; eax>>=1; if eax!=0 loop
    //   exit:  return edx
    //
    // Invariant: loop-carried IntAdd/IntRight must keep self-reads as the
    // accumulator/induction binding (not Const(0) / fresh temp), so NIR stays
    // `edx = edx + ecx` and `ind = ind >> 1`.
    let eax = reg(0x0, 4);
    let ecx = reg(0x4, 4);
    let edx = reg(0x8, 4);
    let zf = reg(0x206, 1);
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::IntXor,
                    Some(edx.clone()),
                    vec![edx.clone(), edx.clone()],
                ),
                // Seed EAX with a prior definition that is NOT a register ABI
                // param (x86-32 stack args). Hardware-register fallback must
                // still keep the IntRight self-update on EAX.
                op(
                    1,
                    PcodeOpcode::Copy,
                    Some(eax.clone()),
                    vec![constant(0x55)],
                ),
                op(
                    2,
                    PcodeOpcode::IntEqual,
                    Some(zf.clone()),
                    vec![eax.clone(), constant(0)],
                ),
                op(
                    3,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1030), zf.clone()],
                ),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(4, PcodeOpcode::Copy, Some(ecx.clone()), vec![eax.clone()]),
                op(
                    5,
                    PcodeOpcode::IntAnd,
                    Some(ecx.clone()),
                    vec![ecx.clone(), constant(1)],
                ),
                op(
                    6,
                    PcodeOpcode::IntAdd,
                    Some(edx.clone()),
                    vec![edx.clone(), ecx.clone()],
                ),
                op(
                    7,
                    PcodeOpcode::IntRight,
                    Some(eax.clone()),
                    vec![eax.clone(), constant(1)],
                ),
                op(
                    8,
                    PcodeOpcode::IntEqual,
                    Some(zf.clone()),
                    vec![eax.clone(), constant(0)],
                ),
                op(
                    9,
                    PcodeOpcode::BoolNegate,
                    Some(varnode(0x50)),
                    vec![zf.clone()],
                ),
                op(
                    10,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1010), varnode(0x50)],
                ),
            ],
        ),
        block_at(
            0x1030,
            2,
            vec![
                op(11, PcodeOpcode::Copy, Some(eax.clone()), vec![edx.clone()]),
                op(12, PcodeOpcode::Return, None, vec![]),
            ],
        ),
    ];
    blocks[0].successors = vec![1, 2];
    blocks[1].successors = vec![1, 2];
    blocks[2].successors = vec![];
    let pcode = pcode_function(blocks);
    let mut options = test_options();
    options.is_64bit = false;
    options.pointer_size = 4;
    options.pe_x64_only = false;
    options.calling_convention = CallingConvention::X86_32;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let _entry = builder.lower_block_stmts(&pcode.blocks[0]).expect("entry");
    let loop_body = builder.lower_block_stmts(&pcode.blocks[1]).expect("loop");

    // IntAdd must keep both operands (edx + ecx), not collapse to ecx alone.
    let add_stmt = loop_body.iter().find(|s| {
        matches!(
            s,
            HirStmt::Assign {
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    ..
                },
                ..
            }
        )
    });
    assert!(
        add_stmt.is_some(),
        "expected loop-carried add assignment, got {loop_body:?}"
    );
    if let Some(HirStmt::Assign {
        lhs: HirLValue::Var(acc),
        rhs: HirExpr::Binary { lhs: a, rhs: b, .. },
        ..
    }) = add_stmt
    {
        assert!(
            matches!(a.as_ref(), HirExpr::Var(name) if name == acc)
                && matches!(b.as_ref(), HirExpr::Var(_)),
            "add should be self-update acc = acc + bit (not Const(0)+ecx), got {add_stmt:?}"
        );
    }

    // IntRight must assign back into the loop-carried induction register.
    let shr_stmt = loop_body.iter().find(|s| {
        matches!(
            s,
            HirStmt::Assign {
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Shr | HirBinaryOp::Sar,
                    ..
                },
                ..
            }
        )
    });
    assert!(
        shr_stmt.is_some(),
        "expected loop-carried shift assignment, got {loop_body:?}"
    );
    if let Some(HirStmt::Assign {
        lhs: HirLValue::Var(ind),
        rhs: HirExpr::Binary { lhs: a, .. },
        ..
    }) = shr_stmt
    {
        assert!(
            matches!(a.as_ref(), HirExpr::Var(name) if name == ind),
            "shift should be self-update ind = ind >> 1, got {shr_stmt:?}"
        );
    }
}
