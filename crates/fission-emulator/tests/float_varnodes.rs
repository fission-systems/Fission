//! Float p-code on the sizes real code uses: single-precision constants and
//! x87 80-bit registers.
//!
//! A four-byte float constant is the raw bits of an `f32`; reading them as the
//! low half of an `f64` gives a denormal near zero, so `x + 1.0f` added nothing.
//! The x87 store wrote an `f64`'s bits where the 80-bit significand belongs and
//! left the exponent bytes zero, so every value stored then loaded came back
//! as zero.

use fission_emulator::MachineState;
use fission_emulator::pcode::eval::Evaluator;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};
use fission_solver::Solver;

fn slot(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 1,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
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

fn run(state: &mut MachineState, opcode: PcodeOpcode, inputs: &[Varnode], out: &Varnode) {
    let mut solver = Solver::new();
    let mut ev = Evaluator::new(state, &mut solver);
    ev.step(&PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(out.clone()),
        inputs: inputs.to_vec(),
        asm_mnemonic: None,
    })
    .expect("step");
    assert!(ev.unimplemented.is_none(), "{opcode:?} not implemented");
}

#[test]
fn a_single_precision_constant_is_read_as_an_f32() {
    let mut state = MachineState::new();
    let x = slot(0x100, 4);
    let out = slot(0x200, 4);
    state
        .write_space(x.space_id, x.offset, &2.0f32.to_bits().to_le_bytes())
        .unwrap();
    run(
        &mut state,
        PcodeOpcode::FloatAdd,
        &[x, constant(1.0f32.to_bits() as u64, 4)],
        &out,
    );
    let got = state.read_space(out.space_id, out.offset, 4).unwrap();
    assert_eq!(f32::from_le_bytes(got[..4].try_into().unwrap()), 3.0);
}

#[test]
fn an_x87_value_survives_being_stored_and_loaded() {
    let mut state = MachineState::new();
    let (a, b, out) = (slot(0x100, 10), slot(0x120, 10), slot(0x140, 10));
    // 1.5 = 0xC000.. x 2^0 ; 2.25 = 0x9000.. x 2^1, in x87 extended encoding.
    let enc = |frac: u64, se: u16| {
        let mut v = [0u8; 10];
        v[..8].copy_from_slice(&frac.to_le_bytes());
        v[8..].copy_from_slice(&se.to_le_bytes());
        v
    };
    state
        .write_space(a.space_id, a.offset, &enc(0xc000_0000_0000_0000, 0x3fff))
        .unwrap();
    state
        .write_space(b.space_id, b.offset, &enc(0x9000_0000_0000_0000, 0x4000))
        .unwrap();
    run(&mut state, PcodeOpcode::FloatAdd, &[a, b], &out);
    // 3.75 = 0xF000.. x 2^1
    let got = state.read_space(out.space_id, out.offset, 10).unwrap();
    assert_eq!(got, enc(0xf000_0000_0000_0000, 0x4000).to_vec());
}
