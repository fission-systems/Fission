//! 128-bit integer p-code, executed rather than truncated.
//!
//! Both engines took the low eight bytes of a varnode wider than that. For
//! `pxor xmm0, xmm0` that leaves the top half of the register holding whatever
//! was there before. For x86-64's `div r64` it is worse and much less exotic:
//! the dividend is the 128-bit `RDX:RAX`, so dropping the top half makes every
//! 64-bit division right only while `RDX` happens to be zero.
//!
//! These drive the evaluator directly with hand-built p-code, because the
//! point is the *semantics* of one op at a time -- a test that runs a binary
//! and checks the answer cannot say which of thirty ops was wrong.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::eval::{Evaluator, StepResult};
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_solver::Solver;

/// Somewhere to put operands: the unique space, which is scratch.
fn slot(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 1,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

struct Bench {
    state: MachineState,
    solver: Solver,
}

impl Bench {
    fn new() -> Self {
        Self {
            state: MachineState::new(),
            solver: Solver::new(),
        }
    }

    fn put(&mut self, vn: &Varnode, value: u128) {
        let bytes = value.to_le_bytes();
        self.state
            .write_space(vn.space_id, vn.offset, &bytes[..vn.size as usize])
            .expect("write");
    }

    fn get(&mut self, vn: &Varnode) -> u128 {
        let data = self
            .state
            .read_space(vn.space_id, vn.offset, vn.size as usize)
            .expect("read");
        let mut bytes = [0u8; 16];
        let take = data.len().min(16);
        bytes[..take].copy_from_slice(&data[..take]);
        u128::from_le_bytes(bytes)
    }

    fn run(&mut self, opcode: PcodeOpcode, inputs: &[Varnode], out: &Varnode) {
        let op = PcodeOp {
            seq_num: 0,
            opcode,
            address: 0x1000,
            output: Some(out.clone()),
            inputs: inputs.to_vec(),
            asm_mnemonic: None,
        };
        let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
        let step = evaluator.step(&op).expect("step");
        assert!(matches!(step, StepResult::Next));
        assert!(
            evaluator.unimplemented.is_none(),
            "{opcode:?} was not implemented at this width"
        );
    }

    /// Run a binary op on 16-byte operands and read back the 16-byte result.
    fn binary_128(&mut self, opcode: PcodeOpcode, a: u128, b: u128) -> u128 {
        let (va, vb, vo) = (slot(0x00, 16), slot(0x20, 16), slot(0x40, 16));
        self.put(&va, a);
        self.put(&vb, b);
        self.put(&vo, 0xDEAD_BEEF_DEAD_BEEF_DEAD_BEEF_DEAD_BEEF);
        self.run(opcode, &[va.clone(), vb.clone()], &vo);
        self.get(&vo)
    }
}

#[test]
fn a_128_bit_dividend_is_not_its_low_half() {
    let mut bench = Bench::new();

    // What `div rcx` does after `mov rdx, 1; xor rax, rax`: divide 2^64 by 4.
    // Truncated to eight bytes the dividend is zero and the answer is zero.
    let dividend = 1u128 << 64;
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntDiv, dividend, 4),
        dividend / 4
    );
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntRem, dividend + 3, 4),
        (dividend + 3) % 4
    );

    // And the signed forms, where truncation also loses the sign.
    let negative = (-(1i128 << 70)) as u128;
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntSDiv, negative, 8) as i128,
        -(1i128 << 70) / 8
    );
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntSRem, negative - 1, 8) as i128,
        (-(1i128 << 70) - 1) % 8
    );
}

#[test]
fn a_64_by_64_multiply_keeps_its_high_half() {
    let mut bench = Bench::new();
    // `mul rax` with both operands at 2^40: the product needs 81 bits, and
    // every bit above the 64th used to be dropped.
    let a = 1u128 << 40;
    assert_eq!(bench.binary_128(PcodeOpcode::IntMult, a, a), 1u128 << 80);
}

#[test]
fn the_whole_register_is_cleared_and_compared() {
    let mut bench = Bench::new();
    let ones = u128::MAX;

    // `pxor xmm0, xmm0`. Truncated, the top eight bytes kept their old value.
    assert_eq!(bench.binary_128(PcodeOpcode::IntXor, ones, ones), 0);
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntAnd, ones, 1u128 << 100),
        1u128 << 100
    );
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntOr, 0, 1u128 << 100),
        1u128 << 100
    );

    // A comparison of two values differing only above the 64th bit. This is
    // the shape that made `pcmpeqb`-based `strlen` take the wrong branch.
    let (va, vb, vo) = (slot(0x00, 16), slot(0x20, 16), slot(0x40, 1));
    bench.put(&va, 1u128 << 100);
    bench.put(&vb, 0);
    bench.run(PcodeOpcode::IntNotEqual, &[va.clone(), vb.clone()], &vo);
    assert_eq!(bench.get(&vo), 1, "differs above bit 64, and must say so");
    bench.run(PcodeOpcode::IntEqual, &[va, vb], &vo);
    assert_eq!(bench.get(&vo), 0);
}

#[test]
fn shifts_cross_the_64_bit_boundary_and_survive_their_own_width() {
    let mut bench = Bench::new();
    assert_eq!(bench.binary_128(PcodeOpcode::IntLeft, 1, 100), 1u128 << 100);
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntRight, 1u128 << 100, 100),
        1
    );
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntSRight, (-1i128) as u128, 100) as i128,
        -1
    );

    // A shift at or past the width is defined here and panics in Rust, so the
    // guard is load-bearing rather than defensive.
    assert_eq!(bench.binary_128(PcodeOpcode::IntLeft, 1, 128), 0);
    assert_eq!(bench.binary_128(PcodeOpcode::IntRight, u128::MAX, 200), 0);
    assert_eq!(
        bench.binary_128(PcodeOpcode::IntSRight, (-1i128) as u128, 200) as i128,
        -1
    );
}

#[test]
fn widening_and_narrowing_use_the_operands_own_width() {
    let mut bench = Bench::new();

    // SEXT from 8 bytes to 16: the sign comes from bit 63, not bit 127.
    let (src, dst) = (slot(0x00, 8), slot(0x20, 16));
    bench.put(&src, (-1i64) as u64 as u128);
    bench.run(PcodeOpcode::IntSExt, std::slice::from_ref(&src), &dst);
    assert_eq!(bench.get(&dst), u128::MAX);

    bench.run(PcodeOpcode::IntZExt, &[src], &dst);
    assert_eq!(bench.get(&dst), u64::MAX as u128);

    // SUBPIECE taking the high half of a 16-byte value -- which is how the
    // quotient and remainder come back out of a 128-bit divide.
    let (wide, half) = (slot(0x40, 16), slot(0x60, 8));
    bench.put(&wide, 0x1122_3344_5566_7788_99AA_BBCC_DDEE_FF00);
    let eight = Varnode {
        space_id: 0,
        offset: 8,
        size: 8,
        is_constant: true,
        constant_val: 8,
    };
    bench.run(PcodeOpcode::SubPiece, &[wide, eight], &half);
    assert_eq!(bench.get(&half), 0x1122_3344_5566_7788);
}

/// Run a two-operand flag op on `size`-byte operands and read the one-byte answer.
fn flag(bench: &mut Bench, opcode: PcodeOpcode, size: u32, a: u128, b: u128) -> bool {
    let (va, vb, vo) = (slot(0x00, size), slot(0x20, size), slot(0x40, 1));
    bench.put(&va, a);
    bench.put(&vb, b);
    bench.put(&vo, 0xFF);
    bench.run(opcode, &[va.clone(), vb.clone()], &vo);
    bench.get(&vo) != 0
}

#[test]
fn carry_and_signed_overflow_flags_exist_at_every_width() {
    let mut bench = Bench::new();

    // 16 bytes. `sign_extend` computed `64 - 16 * 8`, which underflows.
    let max = i128::MAX as u128;
    let min = i128::MIN as u128;
    assert!(flag(&mut bench, PcodeOpcode::IntSCarry, 16, max, 1));
    assert!(!flag(&mut bench, PcodeOpcode::IntSCarry, 16, u128::MAX, 1)); // -1 + 1
    assert!(flag(&mut bench, PcodeOpcode::IntSBorrow, 16, min, 1));
    assert!(!flag(&mut bench, PcodeOpcode::IntSBorrow, 16, 0, 1));
    assert!(flag(&mut bench, PcodeOpcode::IntCarry, 16, u128::MAX, 1));
    assert!(!flag(&mut bench, PcodeOpcode::IntCarry, 16, 1, 1));

    // 9 bytes: neither the low eight bytes nor the full 128 bits.
    let max9 = (1u128 << 71) - 1;
    assert!(flag(&mut bench, PcodeOpcode::IntSCarry, 9, max9, 1));
    assert!(!flag(&mut bench, PcodeOpcode::IntSCarry, 9, max9 - 1, 1));
    assert!(flag(
        &mut bench,
        PcodeOpcode::IntCarry,
        9,
        (1u128 << 72) - 1,
        1
    ));
    assert!(!flag(
        &mut bench,
        PcodeOpcode::IntCarry,
        9,
        (1u128 << 72) - 2,
        1
    ));

    // Eight bytes was always fine and must stay so.
    assert!(flag(
        &mut bench,
        PcodeOpcode::IntSCarry,
        8,
        i64::MAX as u128,
        1
    ));
}

#[test]
fn a_zero_width_operand_has_no_sign_to_overflow() {
    let mut bench = Bench::new();
    // `sign_extend(_, 0)` shifted by 64.
    assert!(!flag(&mut bench, PcodeOpcode::IntSCarry, 0, 0, 0));
    assert!(!flag(&mut bench, PcodeOpcode::IntSBorrow, 0, 0, 0));
}

#[test]
fn pointer_arithmetic_at_wide_width_adds_and_scales() {
    let mut bench = Bench::new();
    let (ptr, idx, elem, out) = (slot(0x00, 16), slot(0x20, 8), slot(0x30, 8), slot(0x40, 16));
    let base = 1u128 << 70;
    bench.put(&ptr, base);
    bench.put(&idx, 3);
    bench.put(&elem, 4);
    // PTRADD ptr, index, element_size == ptr + index * element_size.
    bench.run(
        PcodeOpcode::PtrAdd,
        &[ptr.clone(), idx.clone(), elem.clone()],
        &out,
    );
    assert_eq!(bench.get(&out), base + 12);
    // PTRSUB is an addition: a field offset, whatever its name says.
    bench.run(PcodeOpcode::PtrSub, &[ptr.clone(), idx.clone()], &out);
    assert_eq!(bench.get(&out), base + 3);
}

#[test]
fn leading_zeros_count_the_whole_varnode() {
    let mut bench = Bench::new();
    let run = |bench: &mut Bench, size: u32, value: u128| -> u128 {
        let (v, out) = (slot(0x00, size), slot(0x40, 8));
        bench.put(&v, value);
        bench.run(PcodeOpcode::LzCount, std::slice::from_ref(&v), &out);
        bench.get(&out)
    };
    // `64 - bits` underflowed for anything wider than eight bytes.
    assert_eq!(run(&mut bench, 16, 1u128 << 100), 27);
    assert_eq!(run(&mut bench, 16, 0), 128);
    assert_eq!(run(&mut bench, 16, 1u128 << 127), 0);
    assert_eq!(run(&mut bench, 12, 1u128 << 80), 15); // 96 bits, top set bit is 80
    assert_eq!(run(&mut bench, 8, 1), 63);
}

#[test]
fn a_piece_with_an_eight_byte_low_half_does_not_shift_by_64() {
    let mut bench = Bench::new();
    let (hi, lo, out) = (slot(0x00, 8), slot(0x20, 8), slot(0x40, 8));
    bench.put(&hi, 0xAAAA);
    bench.put(&lo, 0x1234);
    bench.run(PcodeOpcode::Piece, &[hi, lo], &out);
    // Eight bytes cannot hold both halves; the interpreter keeps the low one,
    // and `<< 64` is a panic (or, without overflow checks, `hi | lo`).
    assert_eq!(bench.get(&out), 0x1234);
}

fn constant(value: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 0,
        offset: value,
        size,
        is_constant: true,
        constant_val: value as i64,
    }
}

/// `INSERT` at a position past the destination's width inserts nothing.
/// `wrapping_shl` took the position modulo 64 and overwrote the low bits.
#[test]
fn an_insert_past_the_width_leaves_the_destination() {
    let mut bench = Bench::new();
    let (dest, src, out) = (slot(0x00, 8), slot(0x10, 8), slot(0x20, 8));
    bench.put(&dest, 0x1111_2222_3333_4444);
    bench.put(&src, 0xAB);
    bench.run(
        PcodeOpcode::Insert,
        &[dest.clone(), src.clone(), constant(64, 4), constant(8, 4)],
        &out,
    );
    assert_eq!(bench.get(&out), 0x1111_2222_3333_4444);
    // And in range, for contrast.
    bench.run(
        PcodeOpcode::Insert,
        &[dest.clone(), src.clone(), constant(8, 4), constant(8, 4)],
        &out,
    );
    assert_eq!(bench.get(&out), 0x1111_2222_3333_AB44);
}

/// A signed carry on operands wider than the 128-bit path takes reaches the
/// 64-bit one, whose `1i128 << (bits - 1)` overflowed for 32-byte operands.
#[test]
fn a_signed_carry_on_32_byte_operands_does_not_overflow() {
    let mut bench = Bench::new();
    let (a, b, out) = (slot(0x00, 32), slot(0x40, 32), slot(0x80, 1));
    for (vn, value) in [(&a, 1u8), (&b, 2u8)] {
        let mut bytes = [0u8; 32];
        bytes[0] = value;
        bench
            .state
            .write_space(vn.space_id, vn.offset, &bytes)
            .expect("write");
    }
    for opcode in [PcodeOpcode::IntSCarry, PcodeOpcode::IntSBorrow] {
        let op = PcodeOp {
            seq_num: 0,
            opcode,
            address: 0x1000,
            output: Some(out.clone()),
            inputs: vec![a.clone(), b.clone()],
            asm_mnemonic: None,
        };
        let mut solver = Solver::new();
        let mut evaluator = Evaluator::new(&mut bench.state, &mut solver);
        evaluator.step(&op).expect("step");
    }
    assert_eq!(bench.get(&out), 0, "1 - 2 does not overflow");
}

fn put_bytes(state: &mut MachineState, vn: &Varnode, bytes: &[u8]) {
    state.write_space(vn.space_id, vn.offset, bytes).unwrap();
}

fn get_bytes(state: &mut MachineState, vn: &Varnode) -> Vec<u8> {
    state
        .read_space(vn.space_id, vn.offset, vn.size as usize)
        .unwrap()
}

fn run_wide_op(state: &mut MachineState, opcode: PcodeOpcode, inputs: &[Varnode], out: &Varnode) {
    let op = PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(out.clone()),
        inputs: inputs.to_vec(),
        asm_mnemonic: None,
    };
    let mut solver = Solver::new();
    let mut evaluator = Evaluator::new(state, &mut solver);
    let step = evaluator.step(&op).expect("step");
    assert!(matches!(step, StepResult::Next));
    assert!(
        evaluator.unimplemented.is_none(),
        "{opcode:?} was not implemented for wide varnodes"
    );
}

#[test]
fn wide_shifts_on_32_byte_varnodes_handle_full_width_and_large_counts() {
    let mut state = MachineState::new();
    let a = slot(0x00, 32);
    let out = slot(0x40, 32);

    // Build a 32-byte pattern: [0x01, 0x02, ..., 0x20]
    let pattern: Vec<u8> = (1u8..=32u8).collect();
    put_bytes(&mut state, &a, &pattern);

    // IntLeft by 0, 1, 7, 40, 64, 128, 255, 256, and >256 bits
    for shift in [0u64, 1, 7, 40, 64, 128, 255, 256, 300] {
        run_wide_op(
            &mut state,
            PcodeOpcode::IntLeft,
            &[a.clone(), constant(shift, 4)],
            &out,
        );
        let got = get_bytes(&mut state, &out);
        let mut expected = vec![0u8; 32];
        if shift < 256 {
            let byte_s = (shift / 8) as usize;
            let bit_s = (shift % 8) as u32;
            for i in byte_s..32 {
                let lo = pattern[i - byte_s];
                let prev = if i > byte_s {
                    pattern[i - byte_s - 1]
                } else {
                    0
                };
                expected[i] = if bit_s == 0 {
                    lo
                } else {
                    (lo << bit_s) | (prev >> (8 - bit_s))
                };
            }
        }
        assert_eq!(got, expected, "IntLeft by {shift}");
    }

    // IntRight by 0, 5, 40, 64, 128, 255, 256 bits
    for shift in [0u64, 5, 40, 64, 128, 255, 256] {
        run_wide_op(
            &mut state,
            PcodeOpcode::IntRight,
            &[a.clone(), constant(shift, 4)],
            &out,
        );
        let got = get_bytes(&mut state, &out);
        let mut expected = vec![0u8; 32];
        if shift < 256 {
            let byte_s = (shift / 8) as usize;
            let bit_s = (shift % 8) as u32;
            for (i, slot) in expected.iter_mut().enumerate() {
                let lo = pattern.get(i + byte_s).copied().unwrap_or(0);
                let hi = pattern.get(i + byte_s + 1).copied().unwrap_or(0);
                *slot = if bit_s == 0 {
                    lo
                } else {
                    (lo >> bit_s) | (hi << (8 - bit_s))
                };
            }
        }
        assert_eq!(got, expected, "IntRight by {shift}");
    }

    // IntSRight on negative (MSB = 1) and positive (MSB = 0) 32-byte values
    let mut neg = pattern.clone();
    neg[31] = 0x85;
    put_bytes(&mut state, &a, &neg);
    for shift in [0u64, 4, 72, 255, 256] {
        run_wide_op(
            &mut state,
            PcodeOpcode::IntSRight,
            &[a.clone(), constant(shift, 4)],
            &out,
        );
        let got = get_bytes(&mut state, &out);
        let mut expected = vec![0xFFu8; 32];
        if shift < 256 {
            let byte_s = (shift / 8) as usize;
            let bit_s = (shift % 8) as u32;
            for (i, slot) in expected.iter_mut().enumerate() {
                let lo = neg.get(i + byte_s).copied().unwrap_or(0xFF);
                let hi = neg.get(i + byte_s + 1).copied().unwrap_or(0xFF);
                *slot = if bit_s == 0 {
                    lo
                } else {
                    (lo >> bit_s) | (hi << (8 - bit_s))
                };
            }
        }
        assert_eq!(got, expected, "IntSRight(neg) by {shift}");
    }

    let mut pos = pattern;
    pos[31] = 0x45;
    put_bytes(&mut state, &a, &pos);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntSRight,
        &[a, constant(256, 4)],
        &out,
    );
    assert_eq!(get_bytes(&mut state, &out), vec![0u8; 32]);
}

#[test]
fn wide_zext_sext_piece_subpiece_and_bitwise_on_32_and_64_byte_varnodes() {
    let mut state = MachineState::new();
    let x16 = slot(0x00, 16);
    let y16 = slot(0x10, 16);
    let w32 = slot(0x20, 32);
    let w32_b = slot(0x40, 32);
    let w32_out = slot(0x60, 32);
    let z64 = slot(0x80, 64);
    let s16_out = slot(0xC0, 16);
    let b1 = slot(0xE0, 1);

    // IntZExt 16 -> 32 and 16 -> 64 must zero upper bytes even over 0xFF garbage.
    let lo16: Vec<u8> = (0x80u8..0x90u8).collect();
    let hi16: Vec<u8> = (0x10u8..0x20u8).collect();
    put_bytes(&mut state, &x16, &lo16);
    put_bytes(&mut state, &y16, &hi16);
    put_bytes(&mut state, &w32, &[0xFF; 32]);
    put_bytes(&mut state, &z64, &[0xFF; 64]);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntZExt,
        std::slice::from_ref(&x16),
        &w32,
    );
    let mut expected_z32 = vec![0u8; 32];
    expected_z32[..16].copy_from_slice(&lo16);
    assert_eq!(get_bytes(&mut state, &w32), expected_z32);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntZExt,
        std::slice::from_ref(&x16),
        &z64,
    );
    let mut expected_z64 = vec![0u8; 64];
    expected_z64[..16].copy_from_slice(&lo16);
    assert_eq!(get_bytes(&mut state, &z64), expected_z64);

    // IntSExt 16 -> 32 (negative lo16[15]=0x8f and positive hi16[15]=0x1f)
    run_wide_op(
        &mut state,
        PcodeOpcode::IntSExt,
        std::slice::from_ref(&x16),
        &w32,
    );
    let mut expected_s32 = vec![0xFFu8; 32];
    expected_s32[..16].copy_from_slice(&lo16);
    assert_eq!(get_bytes(&mut state, &w32), expected_s32);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntSExt,
        std::slice::from_ref(&y16),
        &w32,
    );
    let mut expected_s32_pos = vec![0u8; 32];
    expected_s32_pos[..16].copy_from_slice(&hi16);
    assert_eq!(get_bytes(&mut state, &w32), expected_s32_pos);

    // Piece(hi16, lo16) -> w32, then SubPiece at offsets 0, 5, 16, 32
    run_wide_op(
        &mut state,
        PcodeOpcode::Piece,
        &[y16.clone(), x16.clone()],
        &w32,
    );
    let mut joined = Vec::with_capacity(32);
    joined.extend_from_slice(&lo16);
    joined.extend_from_slice(&hi16);
    assert_eq!(get_bytes(&mut state, &w32), joined);

    for (off, expected) in [
        (0u64, lo16.as_slice()),
        (5, &joined[5..21]),
        (16, hi16.as_slice()),
        (32, &[0u8; 16]),
    ] {
        run_wide_op(
            &mut state,
            PcodeOpcode::SubPiece,
            &[w32.clone(), constant(off, 4)],
            &s16_out,
        );
        assert_eq!(
            get_bytes(&mut state, &s16_out),
            expected,
            "SubPiece off={off}"
        );
    }

    // Bitwise + arithmetic carry/borrow across byte 16 + comparisons + bit counts
    let mut a32 = [0u8; 32];
    let mut b32 = [0u8; 32];
    a32[15] = 0xFF;
    b32[15] = 0x01;
    put_bytes(&mut state, &w32, &a32);
    put_bytes(&mut state, &w32_b, &b32);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntAdd,
        &[w32.clone(), w32_b.clone()],
        &w32_out,
    );
    let sum = get_bytes(&mut state, &w32_out);
    assert_eq!(sum[15], 0x00);
    assert_eq!(sum[16], 0x01);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntSub,
        &[w32_out.clone(), w32_b.clone()],
        &w32_out,
    );
    assert_eq!(get_bytes(&mut state, &w32_out), a32);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntOr,
        &[w32.clone(), w32_b.clone()],
        &w32_out,
    );
    assert_eq!(get_bytes(&mut state, &w32_out)[15], 0xFF);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntAnd,
        &[w32.clone(), w32_b.clone()],
        &w32_out,
    );
    assert_eq!(get_bytes(&mut state, &w32_out)[15], 0x01);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntXor,
        &[w32.clone(), w32_b.clone()],
        &w32_out,
    );
    assert_eq!(get_bytes(&mut state, &w32_out)[15], 0xFE);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntNegate,
        std::slice::from_ref(&w32_b),
        &w32_out,
    );
    assert_eq!(get_bytes(&mut state, &w32_out)[15], 0xFE);
    assert_eq!(get_bytes(&mut state, &w32_out)[31], 0xFF);

    run_wide_op(
        &mut state,
        PcodeOpcode::IntEqual,
        &[w32.clone(), w32.clone()],
        &b1,
    );
    assert_eq!(get_bytes(&mut state, &b1)[0], 1);
    run_wide_op(
        &mut state,
        PcodeOpcode::IntLess,
        &[w32_b.clone(), w32.clone()],
        &b1,
    );
    assert_eq!(get_bytes(&mut state, &b1)[0], 1);

    // Signed compare: negative w32_out < positive w32
    run_wide_op(&mut state, PcodeOpcode::IntSLess, &[w32_out, w32], &b1);
    assert_eq!(get_bytes(&mut state, &b1)[0], 1);

    // Bit counts on 32-byte value with byte 15 = 0x01 (1 one, lz = 16*8 + 7 = 135)
    let count_out = slot(0xF0, 8);
    run_wide_op(
        &mut state,
        PcodeOpcode::PopCount,
        std::slice::from_ref(&w32_b),
        &count_out,
    );
    assert_eq!(
        u64::from_le_bytes(get_bytes(&mut state, &count_out).try_into().unwrap()),
        1
    );
    run_wide_op(&mut state, PcodeOpcode::LzCount, &[w32_b], &count_out);
    assert_eq!(
        u64::from_le_bytes(get_bytes(&mut state, &count_out).try_into().unwrap()),
        16 * 8 + 7
    );
}

const X64_CODE: u64 = 0x5000_0000;
const X64_DATA: u64 = 0x5001_0000;

fn x64_machine(code: &[u8], count: u64, interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(count));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");

    emu.state
        .page_map
        .map_region(X64_CODE, 0x1000, prot::RX, true);
    emu.state
        .page_map
        .map_region(X64_DATA, 0x1000, prot::RW, true);
    let ram = emu.state.ram_space();
    let mut bytes = code.to_vec();
    bytes.resize(0x40, 0x90);
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, X64_CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = X64_CODE;
    emu
}

fn write_xmm(emu: &mut Emulator, name: &str, value: u128) {
    let &(space, offset, size) = emu
        .register_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing register {name}"));
    assert_eq!(size, 16);
    emu.state
        .write_space(space, offset, &value.to_le_bytes())
        .unwrap();
}

fn read_xmm(emu: &mut Emulator, name: &str) -> u128 {
    let &(space, offset, size) = emu
        .register_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing register {name}"));
    assert_eq!(size, 16);
    let bytes = emu.state.read_space(space, offset, 16).unwrap();
    u128::from_le_bytes(bytes[..16].try_into().unwrap())
}

#[test]
fn x86_64_palignr_matches_native_across_engines_and_counts() {
    let xmm0_in = 0x79c2326d_8d229d8f_8927b2e1_8790dcdf_u128;
    let xmm1_in = 0x00000000_0000003f_ffffffff_ffffffb8_u128;

    // Issue #1054 repro: palignr xmm0, xmm1, 5 (66 0f 3a 0f c1 05)
    // plus boundary counts imm8 = 0, 16, 20, 32.
    let cases = [
        (5u8, 0xe18790dc_df000000_00000000_3fffffff_u128),
        (0u8, xmm1_in),
        (16u8, xmm0_in),
        (20u8, xmm0_in >> 32),
        (32u8, 0u128),
    ];

    for (imm8, expected) in cases {
        let code = [0x66, 0x0f, 0x3a, 0x0f, 0xc1, imm8];
        for interpret in [false, true] {
            let engine = if interpret { "interpreter" } else { "JIT" };
            let mut emu = x64_machine(&code, 1, interpret);
            write_xmm(&mut emu, "XMM0", xmm0_in);
            write_xmm(&mut emu, "XMM1", xmm1_in);
            emu.run().expect("run");
            assert_eq!(emu.inst_count, 1, "{engine}: imm8={imm8}");
            assert_eq!(
                read_xmm(&mut emu, "XMM0"),
                expected,
                "{engine}: palignr xmm0, xmm1, {imm8}"
            );
        }
    }

    // Memory operand form: palignr xmm0, [rdi], 5 (66 0f 3a 0f 07 05)
    let mem_code = [0x66, 0x0f, 0x3a, 0x0f, 0x07, 0x05];
    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = x64_machine(&mem_code, 1, interpret);
        write_xmm(&mut emu, "XMM0", xmm0_in);
        emu.write_register_u64("RDI", X64_DATA).unwrap();
        let ram = emu.state.ram_space();
        emu.state
            .write_space(ram, X64_DATA, &xmm1_in.to_le_bytes())
            .unwrap();
        emu.run().expect("run");
        assert_eq!(
            read_xmm(&mut emu, "XMM0"),
            0xe18790dc_df000000_00000000_3fffffff_u128,
            "{engine}: palignr xmm0, [rdi], 5"
        );
    }
}
