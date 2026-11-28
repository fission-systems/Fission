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
    let zext_in = vec![byte.clone()];
    let ops_wide_lhs = optimize_ops(vec![
        op(0, PcodeOpcode::IntZExt, narrow.clone(), zext_in),
        op(
            1,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![x.clone(), narrow.clone()],
        ),
        ret(2, out.clone()),
    ]);
    let has_wide_lhs_and = ops_wide_lhs
        .iter()
        .any(|op| op.opcode == PcodeOpcode::IntAnd);
    assert!(
        has_wide_lhs_and,
        "x & zext(byte) must keep IntAnd: {ops_wide_lhs:?}"
    );

    let ops_narrow_lhs = optimize_ops(vec![
        op(0, PcodeOpcode::IntZExt, narrow.clone(), vec![byte]),
        op(1, PcodeOpcode::IntAnd, out.clone(), vec![narrow, x]),
        ret(2, out),
    ]);
    let has_narrow_lhs_and = ops_narrow_lhs
        .iter()
        .any(|op| op.opcode == PcodeOpcode::IntAnd);
    assert!(
        has_narrow_lhs_and,
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
    let target = &func_lhs_full.blocks[0].ops[0];
    let opt = rules
        .try_optimize_with_tracker(target, &tracker, &func_lhs_full)
        .expect("0xFFFF_FFFF & y should simplify to COPY(y)");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert_eq!(opt.inputs, vec![y]);

    // Left constant covering zext(byte): 0xFF & zext(byte) => COPY(zext(byte)).
    let zext_in = vec![byte.clone()];
    let func_lhs_zext = single_block(vec![
        op(0, PcodeOpcode::IntZExt, wide.clone(), zext_in),
        op(
            1,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![Varnode::constant(0xFF, 4), wide.clone()],
        ),
        ret(2, out.clone()),
    ]);
    tracker.build(&func_lhs_zext);
    let target = &func_lhs_zext.blocks[0].ops[1];
    let opt = rules
        .try_optimize_with_tracker(target, &tracker, &func_lhs_zext)
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
    let target = &func_rhs_zext.blocks[0].ops[1];
    let opt = rules
        .try_optimize_with_tracker(target, &tracker, &func_rhs_zext)
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
        (3, 0x0000_FFFF, 0x00FF_FFFF),
        (4, 0x0000_FFFF, 0xFFFF_FFFF),
        (5, 0x0000_FFFF_FFFF, 0x00FF_FFFF_FFFF),
        (6, 0x0000_FFFF_FFFF, 0xFFFF_FFFF_FFFF),
        (7, 0x0000_FFFF_FFFF_FFFF, 0x00FF_FFFF_FFFF_FFFF),
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
        let target = &func_unknown.blocks[0].ops[0];
        let res = rules.try_optimize_with_tracker(target, &tracker, &func_unknown);
        assert!(
            res.is_none(),
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
        let target = &func_lhs_partial.blocks[0].ops[0];
        let res = rules.try_optimize_with_tracker(target, &tracker, &func_lhs_partial);
        assert!(
            res.is_none(),
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
        let target = &func_lhs_full.blocks[0].ops[0];
        let opt = rules
            .try_optimize_with_tracker(target, &tracker, &func_lhs_full)
            .expect("full-width mask on LHS should simplify to COPY(y)");
        assert_eq!(opt.opcode, PcodeOpcode::Copy);
        assert_eq!(opt.inputs, vec![y]);
    }
}

#[test]
fn try_and_mask_guards_wide_simd_and_malformed_shapes() {
    let rules = OptimizationRules::new();
    let (v16, x16, y16, t16, out16, u16) = (
        vn(0x10, 16),
        vn(0x20, 16),
        vn(0x30, 16),
        vn(0x40, 16),
        vn(0x100, 16),
        vn(0x110, 16),
    );
    let mut tracker = DefUseTracker::new();

    // 1. 16-byte (> 64-bit) LHS and RHS constants must not be eliminated by
    //    either `try_and_mask` or `dead_bit::try_dead_bit_elimination`.
    for inputs in [
        vec![Varnode::constant(-1, 16), y16.clone()],
        vec![y16.clone(), Varnode::constant(-1, 16)],
    ] {
        let func_wide = single_block(vec![
            op(0, PcodeOpcode::IntAnd, out16.clone(), inputs),
            ret(1, out16.clone()),
        ]);
        tracker.build(&func_wide);
        let target = &func_wide.blocks[0].ops[0];
        assert!(
            rules
                .try_optimize_with_tracker(target, &tracker, &func_wide)
                .is_none(),
            "size=16: >64-bit IntAnd must not be eliminated by 64-bit masks"
        );
    }

    // 2. 128-bit `x = v << 64` has low-64 `nz_mask == 0`, but upper bits 64..127
    //    may be nonzero; `x & y` must NOT fold to 0 via `and_mask == 0`.
    let func_shift64 = single_block(vec![
        op(
            0,
            PcodeOpcode::IntLeft,
            x16.clone(),
            vec![v16.clone(), Varnode::constant(64, 4)],
        ),
        op(
            1,
            PcodeOpcode::IntAnd,
            out16.clone(),
            vec![x16.clone(), y16.clone()],
        ),
        ret(2, out16.clone()),
    ]);
    tracker.build(&func_shift64);
    let target = &func_shift64.blocks[0].ops[1];
    assert!(
        rules
            .try_optimize_with_tracker(target, &tracker, &func_shift64)
            .is_none(),
        "size=16: (v << 64) & y must not fold to 0"
    );

    // 3. 128-bit consume-mask check: `u = (( (v << 64) | 0x0F ) & y) >> 4`
    //    consumes upper bits >= 64 that are unrepresented in u64 masks.
    let func_consume16 = single_block(vec![
        op(
            0,
            PcodeOpcode::IntLeft,
            t16.clone(),
            vec![v16, Varnode::constant(64, 4)],
        ),
        op(
            1,
            PcodeOpcode::IntOr,
            x16.clone(),
            vec![t16, Varnode::constant(0x0F, 16)],
        ),
        op(2, PcodeOpcode::IntAnd, out16.clone(), vec![x16, y16]),
        op(
            3,
            PcodeOpcode::IntRight,
            u16.clone(),
            vec![out16, Varnode::constant(4, 4)],
        ),
        ret(4, u16),
    ]);
    tracker.build(&func_consume16);
    let target = &func_consume16.blocks[0].ops[2];
    assert!(
        rules
            .try_optimize_with_tracker(target, &tracker, &func_consume16)
            .is_none(),
        "size=16: 128-bit IntAnd must not fold to 0 under 64-bit consume mask"
    );

    // 4. Sign-extended narrow constants (-1:1 = 0xFF, -128:1 = 0x80) must be
    //    masked to their declared 1-byte width, never sign-extended to u64::MAX
    //    or 0xFFFF_FFFF_FFFF_FF80.
    let (x4, out4, out1) = (vn(0x50, 4), vn(0x120, 4), vn(0x130, 1));
    let func_narrow_neg = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out4.clone(),
            vec![x4, Varnode::constant(-128, 1)],
        ),
        ret(1, out4),
    ]);
    tracker.build(&func_narrow_neg);
    let target = &func_narrow_neg.blocks[0].ops[0];
    assert!(
        rules
            .try_optimize_with_tracker(target, &tracker, &func_narrow_neg)
            .is_none(),
        "mismatched width + negative 1-byte constant must not be eliminated"
    );

    // Same-width 1-byte `x:1 & (-128:1)` keeps only bit 7 and must stay IntAnd.
    let x1 = vn(0x60, 1);
    let func_u8_neg128 = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out1.clone(),
            vec![x1.clone(), Varnode::constant(-128, 1)],
        ),
        ret(1, out1.clone()),
    ]);
    tracker.build(&func_u8_neg128);
    let target = &func_u8_neg128.blocks[0].ops[0];
    assert!(
        rules
            .try_optimize_with_tracker(target, &tracker, &func_u8_neg128)
            .is_none(),
        "x:1 & (-128:1) clears bits 0..6 and must not be eliminated"
    );

    // Same-width 1-byte `(-1:1) & x:1` has full 1-byte mask 0xFF and simplifies to COPY(x:1).
    let func_u8_neg1 = single_block(vec![
        op(
            0,
            PcodeOpcode::IntAnd,
            out1.clone(),
            vec![Varnode::constant(-1, 1), x1.clone()],
        ),
        ret(1, out1.clone()),
    ]);
    tracker.build(&func_u8_neg1);
    let target = &func_u8_neg1.blocks[0].ops[0];
    let opt = rules
        .try_optimize_with_tracker(target, &tracker, &func_u8_neg1)
        .expect("(-1:1) & x:1 should simplify to COPY(x:1)");
    assert_eq!(opt.opcode, PcodeOpcode::Copy);
    assert_eq!(opt.inputs, vec![x1.clone()]);

    // 5. Malformed operations (output: None, size == 0, or inputs.len() != 2)
    //    must conservatively return None.
    let no_out_op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAnd,
        address: 0x1000,
        output: None,
        inputs: vec![x1.clone(), Varnode::constant(-1, 1)],
        asm_mnemonic: None,
    };
    let zero_size_op = op(
        0,
        PcodeOpcode::IntAnd,
        vn(0x140, 0),
        vec![vn(0x70, 0), Varnode::constant(0, 0)],
    );
    let unary_and_op = op(0, PcodeOpcode::IntAnd, out1, vec![x1]);
    for malformed in [no_out_op, zero_size_op, unary_and_op] {
        let func_malformed = single_block(vec![malformed]);
        tracker.build(&func_malformed);
        let target = &func_malformed.blocks[0].ops[0];
        assert!(
            rules
                .try_optimize_with_tracker(target, &tracker, &func_malformed)
                .is_none(),
            "malformed IntAnd must return None"
        );
    }
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

    let eval_vn = |vn: &Varnode, lhs: &Varnode, rhs: &Varnode, lv: u8, rv: u8| -> u8 {
        if vn.is_constant {
            (vn.constant_val as u64 & 0xFF) as u8
        } else if vn == lhs {
            lv
        } else if vn == rhs {
            rv
        } else {
            panic!("unexpected varnode in optimized IntAnd: {vn:?}");
        }
    };
    let eval_op = |op: &PcodeOp, lhs: &Varnode, rhs: &Varnode, lv: u8, rv: u8| -> u8 {
        match op.opcode {
            PcodeOpcode::Copy => eval_vn(&op.inputs[0], lhs, rhs, lv, rv),
            PcodeOpcode::IntAnd => {
                let l = eval_vn(&op.inputs[0], lhs, rhs, lv, rv);
                let r = eval_vn(&op.inputs[1], lhs, rhs, lv, rv);
                l & r
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
                    "u8 const AND mismatch: c={c:#04x} x={xv:#04x}"
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
                    "overlapping masks ({m1:#04x}, {m2:#04x}) must stay IntAnd"
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
                        "u8 var-var AND mismatch: m1={m1:#04x} m2={m2:#04x}"
                    );
                }
            }
        }
    }
}
