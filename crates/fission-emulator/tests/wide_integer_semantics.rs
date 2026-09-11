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

use fission_emulator::MachineState;
use fission_emulator::pcode::eval::{Evaluator, StepResult};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
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
    bench.run(PcodeOpcode::IntSExt, &[src.clone()], &dst);
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
