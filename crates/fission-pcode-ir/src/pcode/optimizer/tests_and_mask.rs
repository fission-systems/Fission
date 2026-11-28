//! Regression tests for `OptimizationRules::try_and_mask` soundness (#1437).
//!
//! `DefUseTracker::get_nz_mask` is a may-be-1 upper bound, not a must-be-1
//! lower bound. Eliminating one operand of `IntAnd` to `COPY(other)` is only
//! sound when the dropped operand is a constant whose bits cover the kept
//! operand's nonzero mask.

use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

fn vn(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 2,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn op(seq: u32, opcode: PcodeOpcode, output: Varnode, inputs: Vec<Varnode>) -> PcodeOp {
    PcodeOp {
        seq_num: seq,
        opcode,
        address: 0x1000 + u64::from(seq),
        output: Some(output),
        inputs,
        asm_mnemonic: None,
    }
}

fn ret(seq: u32, value: Varnode) -> PcodeOp {
    PcodeOp {
        seq_num: seq,
        opcode: PcodeOpcode::Return,
        address: 0x1000 + u64::from(seq),
        output: None,
        inputs: vec![value],
        asm_mnemonic: None,
    }
}

fn single_block(ops: Vec<PcodeOp>) -> PcodeFunction {
    PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops,
        }],
    }
}

fn optimize_ops(ops: Vec<PcodeOp>) -> Vec<PcodeOp> {
    let mut func = single_block(ops);
    PcodeOptimizer::new(PcodeOptimizerConfig::default()).optimize(&mut func);
    func.blocks.remove(0).ops
}

#[test]
fn try_and_mask_keeps_two_unknown_operands() {
    // Counterexample: x = 0x1234_5678, y = 0x0000_0000 => x & y = 0,
    // whereas the unsound COPY(x) rewrite produced 0x1234_5678.
    let (x, y, out) = (vn(0x10, 4), vn(0x14, 4), vn(0x100, 4));
    let ops = optimize_ops(vec![
        op(0, PcodeOpcode::IntAnd, out.clone(), vec![x, y]),
        ret(1, out),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "x & y for two unknown operands must not be rewritten to COPY: {ops:?}"
    );
}

#[test]
fn try_and_mask_keeps_different_non_constant_nz_masks() {
    // Counterexample: x = 0, byte = 0xAB => x & zext(byte) = 0,
    // whereas the unsound COPY(zext(byte)) rewrite produced 0xAB.
    let (x, byte, narrow, out) = (vn(0x10, 4), vn(0x14, 1), vn(0x100, 4), vn(0x104, 4));
    let ops_wide_lhs = optimize_ops(vec![
        op(0, PcodeOpcode::IntZExt, narrow.clone(), vec![byte.clone()]),
        op(
            1,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![x.clone(), narrow.clone()],
        ),
        ret(2, out.clone()),
    ]);
    assert!(
        ops_wide_lhs.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "x & zext(byte) must keep IntAnd: {ops_wide_lhs:?}"
    );

    let ops_narrow_lhs = optimize_ops(vec![
        op(0, PcodeOpcode::IntZExt, narrow.clone(), vec![byte]),
        op(1, PcodeOpcode::IntAnd, out.clone(), vec![narrow, x]),
        ret(2, out),
    ]);
    assert!(
        ops_narrow_lhs.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "zext(byte) & x must keep IntAnd: {ops_narrow_lhs:?}"
    );
}

#[test]
fn try_and_mask_keeps_left_constant_with_unknown_rhs() {
    // Counterexample: y = 0 => 0xFF & y = 0,
    // whereas the unsound COPY(0xFF) rewrite produced 0xFF.
    let (y, out) = (vn(0x10, 4), vn(0x100, 4));
    let ops = optimize_ops(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![Varnode::constant(0xFF, 4), y],
        ),
        ret(1, out),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "0xFF & y must not fold to COPY(0xFF): {ops:?}"
    );
}

#[test]
fn try_and_mask_keeps_unknown_lhs_with_right_constant() {
    let (x, out) = (vn(0x10, 4), vn(0x100, 4));
    let ops = optimize_ops(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![x, Varnode::constant(0xFF, 4)],
        ),
        ret(1, out),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "x & 0xFF must keep IntAnd: {ops:?}"
    );
}

#[test]
fn try_and_mask_folds_zero_masks() {
    let rules = OptimizationRules::new();
    let (x, zero_var, out) = (vn(0x10, 4), vn(0x100, 4), vn(0x104, 4));

    for inputs in [
        vec![x.clone(), Varnode::constant(0, 4)],
        vec![Varnode::constant(0, 4), x.clone()],
    ] {
        let func = single_block(vec![
            op(0, PcodeOpcode::IntAnd, out.clone(), inputs),
            ret(1, out.clone()),
        ]);
        let mut tracker = DefUseTracker::new();
        tracker.build(&func);
        let opt = rules
            .try_optimize_with_tracker(&func.blocks[0].ops[0], &tracker, &func)
            .expect("zero-masked IntAnd should fold to 0");
        assert_eq!(opt.opcode, PcodeOpcode::Copy);
        assert!(opt.inputs[0].is_zero());
    }

    // Tracked zero-mask variable ANDed with unknown register also folds to 0.
    let func = single_block(vec![
        op(
            0,
            PcodeOpcode::Copy,
            zero_var.clone(),
            vec![Varnode::constant(0, 4)],
        ),
        op(1, PcodeOpcode::IntAnd, out.clone(), vec![zero_var, x]),
        ret(2, out),
    ]);
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    let opt = rules
        .try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker, &func)
        .expect("tracked zero-mask IntAnd should fold to 0");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert!(opt.inputs[0].is_zero());
}

#[test]
fn try_and_mask_eliminates_valid_identity_masks() {
    let rules = OptimizationRules::new();
    let (byte, wide, y, out) = (vn(0x10, 1), vn(0x100, 4), vn(0x14, 4), vn(0x104, 4));

    // Full-width constant on LHS: 0xFFFF_FFFF & y => COPY(y), never COPY(const).
    let func_lhs_full = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![Varnode::constant(0xFFFF_FFFF, 4), y.clone()],
        ),
        ret(1, out.clone()),
    ]);
    let mut tracker = DefUseTracker::new();
    tracker.build(&func_lhs_full);
    let opt = rules
        .try_optimize_with_tracker(&func_lhs_full.blocks[0].ops[0], &tracker, &func_lhs_full)
        .expect("0xFFFF_FFFF & y should simplify to COPY(y)");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert_eq!(opt.inputs, vec![y]);

    // Left constant covering zext(byte): 0xFF & zext(byte) => COPY(zext(byte)).
    let func_lhs_zext = single_block(vec![
        op(0, PcodeOpcode::IntZExt, wide.clone(), vec![byte.clone()]),
        op(
            1,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![Varnode::constant(0xFF, 4), wide.clone()],
        ),
        ret(2, out.clone()),
    ]);
    tracker.build(&func_lhs_zext);
    let opt = rules
        .try_optimize_with_tracker(&func_lhs_zext.blocks[0].ops[1], &tracker, &func_lhs_zext)
        .expect("0xFF & zext(byte) should simplify to COPY(zext(byte))");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert_eq!(opt.inputs, vec![wide.clone()]);

    // Right constant covering zext(byte): zext(byte) & 0xFF => COPY(zext(byte)).
    let func_rhs_zext = single_block(vec![
        op(0, PcodeOpcode::IntZExt, wide.clone(), vec![byte]),
        op(
            1,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![wide.clone(), Varnode::constant(0xFF, 4)],
        ),
        ret(2, out),
    ]);
    tracker.build(&func_rhs_zext);
    let opt = rules
        .try_optimize_with_tracker(&func_rhs_zext.blocks[0].ops[1], &tracker, &func_rhs_zext)
        .expect("zext(byte) & 0xFF should simplify to COPY(zext(byte))");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert_eq!(opt.inputs, vec![wide]);
}

#[test]
fn try_and_mask_folds_disjoint_non_constant_masks() {
    let rules = OptimizationRules::new();
    let (x, y, a, b, out) = (
        vn(0x10, 4),
        vn(0x14, 4),
        vn(0x100, 4),
        vn(0x104, 4),
        vn(0x108, 4),
    );
    let func = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            a.clone(),
            vec![x, Varnode::constant(0x0F, 4)],
        ),
        op(
            1,
            PcodeOpcode::IntAnd,
            b.clone(),
            vec![y, Varnode::constant(0xF0, 4)],
        ),
        op(2, PcodeOpcode::IntAnd, out.clone(), vec![a, b]),
        ret(3, out),
    ]);
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    let opt = rules
        .try_optimize_with_tracker(&func.blocks[0].ops[2], &tracker, &func)
        .expect("(x & 0x0F) & (y & 0xF0) has disjoint NZ masks and must fold to 0");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert!(opt.inputs[0].is_zero());
}

#[test]
fn try_and_mask_preserves_semantics_across_widths() {
    let rules = OptimizationRules::new();
    let cases: &[(u32, i64, i64)] = &[
        (1, 0x0F, 0xFF),
        (2, 0x00FF, 0xFFFF),
        (4, 0x0000_FFFF, 0xFFFF_FFFF),
        (8, 0xFFFF_FFFF, -1),
    ];

    for &(size, partial_mask, full_mask) in cases {
        let (x, y, out) = (vn(0x10, size), vn(0x20, size), vn(0x100, size));

        // Two unknown operands must not be eliminated.
        let func_unknown = single_block(vec![
            op(
                0,
                PcodeOpcode::IntAnd,
                out.clone(),
                vec![x.clone(), y.clone()],
            ),
            ret(1, out.clone()),
        ]);
        let mut tracker = DefUseTracker::new();
        tracker.build(&func_unknown);
        assert!(
            rules
                .try_optimize_with_tracker(&func_unknown.blocks[0].ops[0], &tracker, &func_unknown)
                .is_none(),
            "size={size}: x & y must not be eliminated"
        );

        // Partial mask on LHS must not fold to COPY(const).
        let func_lhs_partial = single_block(vec![
            op(
                0,
                PcodeOpcode::IntAnd,
                out.clone(),
                vec![Varnode::constant(partial_mask, size), y.clone()],
            ),
            ret(1, out.clone()),
        ]);
        tracker.build(&func_lhs_partial);
        assert!(
            rules
                .try_optimize_with_tracker(
                    &func_lhs_partial.blocks[0].ops[0],
                    &tracker,
                    &func_lhs_partial,
                )
                .is_none(),
            "size={size}: partial_mask & y must not be eliminated"
        );

        // Full-width mask on LHS must simplify to COPY(y), never COPY(const).
        let func_lhs_full = single_block(vec![
            op(
                0,
                PcodeOpcode::IntAnd,
                out.clone(),
                vec![Varnode::constant(full_mask, size), y.clone()],
            ),
            ret(1, out),
        ]);
        tracker.build(&func_lhs_full);
        let opt = rules
            .try_optimize_with_tracker(&func_lhs_full.blocks[0].ops[0], &tracker, &func_lhs_full)
            .expect("full-width mask on LHS should simplify to COPY(y)");
        assert_eq!(opt.opcode, PcodeOpcode::Copy);
        assert_eq!(opt.inputs, vec![y]);
    }

    // 16-byte (> 64-bit) operands must not be eliminated using 64-bit NZ masks.
    let (wide_y, wide_out) = (vn(0x10, 16), vn(0x100, 16));
    let func_wide = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            wide_out.clone(),
            vec![Varnode::constant(-1, 16), wide_y],
        ),
        ret(1, wide_out),
    ]);
    let mut tracker = DefUseTracker::new();
    tracker.build(&func_wide);
    assert!(
        rules
            .try_optimize_with_tracker(&func_wide.blocks[0].ops[0], &tracker, &func_wide)
            .is_none(),
        "size=16: >64-bit IntAnd must not be eliminated by 64-bit NZ masks"
    );
}

#[test]
fn try_and_mask_exhaustive_u8_bitvector_soundness() {
    let rules = OptimizationRules::new();
    let (x, y, a, b, out) = (
        vn(0x10, 1),
        vn(0x14, 1),
        vn(0x100, 1),
        vn(0x104, 1),
        vn(0x108, 1),
    );

    let eval_vn = |vn: &Varnode, lhs_vn: &Varnode, rhs_vn: &Varnode, lv: u8, rv: u8| -> u8 {
        if vn.is_constant {
            (vn.constant_val as u64 & 0xFF) as u8
        } else if vn == lhs_vn {
            lv
        } else if vn == rhs_vn {
            rv
        } else {
            panic!("unexpected varnode in optimized IntAnd: {vn:?}");
        }
    };
    let eval_op = |op: &PcodeOp, lhs_vn: &Varnode, rhs_vn: &Varnode, lv: u8, rv: u8| -> u8 {
        match op.opcode {
            PcodeOpcode::Copy => eval_vn(&op.inputs[0], lhs_vn, rhs_vn, lv, rv),
            PcodeOpcode::IntAnd => {
                eval_vn(&op.inputs[0], lhs_vn, rhs_vn, lv, rv)
                    & eval_vn(&op.inputs[1], lhs_vn, rhs_vn, lv, rv)
            }
            other => panic!("unexpected opcode from IntAnd optimization: {other:?}"),
        }
    };

    // 1. Exhaustively verify `c & x` and `x & c` for all 256 8-bit constants
    //    and all 256 concrete values of `x`.
    let mut tracker = DefUseTracker::new();
    for c in 0u8..=255 {
        let c_vn = Varnode::constant(i64::from(c), 1);
        for inputs in [vec![c_vn.clone(), x.clone()], vec![x.clone(), c_vn]] {
            let func = single_block(vec![
                op(0, PcodeOpcode::IntAnd, out.clone(), inputs),
                ret(1, out.clone()),
            ]);
            tracker.build(&func);
            let orig = &func.blocks[0].ops[0];
            let effective = rules
                .try_optimize_with_tracker(orig, &tracker, &func)
                .unwrap_or_else(|| orig.clone());

            for xv in 0u8..=255 {
                let actual = eval_op(&effective, &x, &y, xv, 0);
                let expected = xv & c;
                assert_eq!(
                    actual, expected,
                    "8-bit constant AND mismatch for c={c:#04x}, x={xv:#04x}: {effective:?}"
                );
            }
        }
    }

    // 2. Verify non-constant `(x & m1) & (y & m2)` over representative nonzero
    //    mask pairs against all 256x256 concrete `(x, y)` inputs.
    let masks: &[u8] = &[0x00, 0x0F, 0xF0, 0x33, 0x55, 0xAA, 0xFF];
    for &m1 in masks {
        for &m2 in masks {
            let func = single_block(vec![
                op(
                    0,
                    PcodeOpcode::IntAnd,
                    a.clone(),
                    vec![x.clone(), Varnode::constant(i64::from(m1), 1)],
                ),
                op(
                    1,
                    PcodeOpcode::IntAnd,
                    b.clone(),
                    vec![y.clone(), Varnode::constant(i64::from(m2), 1)],
                ),
                op(
                    2,
                    PcodeOpcode::IntAnd,
                    out.clone(),
                    vec![a.clone(), b.clone()],
                ),
                ret(3, out.clone()),
            ]);
            tracker.build(&func);
            let orig = &func.blocks[0].ops[2];
            let effective = rules
                .try_optimize_with_tracker(orig, &tracker, &func)
                .unwrap_or_else(|| orig.clone());

            if (m1 & m2) == 0 {
                assert_eq!(effective.opcode, PcodeOpcode::Copy);
                assert!(effective.inputs[0].is_zero());
            } else {
                assert_eq!(
                    effective.opcode,
                    PcodeOpcode::IntAnd,
                    "overlapping non-constant masks ({m1:#04x}, {m2:#04x}) must stay IntAnd"
                );
            }

            for xv in 0u8..=255 {
                let av = xv & m1;
                for yv in 0u8..=255 {
                    let bv = yv & m2;
                    let actual = eval_op(&effective, &a, &b, av, bv);
                    let expected = av & bv;
                    assert_eq!(
                        actual, expected,
                        "u8 var-var AND mismatch: m1={m1:#04x} m2={m2:#04x} x={xv:#04x} y={yv:#04x}"
                    );
                }
            }
        }
    }
}
