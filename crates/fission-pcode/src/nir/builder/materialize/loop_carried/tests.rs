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
        HirExpr::PtrOffset { base, .. } => expr_contains_shr(base),
        HirExpr::Index { base, index, .. } => {
            expr_contains_shr(base) || expr_contains_shr(index)
        }
        HirExpr::AggregateCopy { src, .. } => expr_contains_shr(src),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_shr(cond)
                || expr_contains_shr(then_expr)
                || expr_contains_shr(else_expr)
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
        loop_body.iter().any(|stmt| lhs_var(stmt) == Some("param_1")),
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
                op(0, PcodeOpcode::IntAnd, Some(unique_out), vec![ecx.clone(), ecx.clone()]),
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
        loop_body.iter().any(|stmt| lhs_var(stmt) == Some("param_1")),
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
            vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
        ),
        block_at(
            0x1010,
            1,
            vec![
                // IntZExt ECX → RCX (size=8 binding — must NOT hijack the following ECX update)
                op(1, PcodeOpcode::IntZExt, Some(rcx.clone()), vec![ecx.clone()]),
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
        loop_body.iter().any(|stmt| lhs_var(stmt) == Some("param_1")),
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
            vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
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
                op(2, PcodeOpcode::IntZExt, Some(rcx.clone()), vec![ecx.clone()]),
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
        loop_body.iter().any(|stmt| lhs_var(stmt) == Some("param_1")),
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

    assert_ne!(name.as_deref(), Some("param_2"));
    assert!(
        name.is_none(),
        "prior-defined ABI scratch should not be promoted to param_2: {name:?}"
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
