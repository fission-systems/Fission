//! Taint through the interpreter. `write_space` clears the shadow of the bytes
//! it overwrites, so a result's shadow has to be set *after* its concrete
//! bytes are stored; set before, it is wiped by the very write that follows.

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

fn step(state: &mut MachineState, opcode: PcodeOpcode, inputs: &[Varnode], out: &Varnode) {
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
}

#[test]
fn copy_keeps_the_taint_of_its_source() {
    let mut state = MachineState::new();
    let (src, out) = (slot(0x100, 4), slot(0x200, 4));
    state.write_space(1, 0x100, &[1, 0, 0, 0]).unwrap();
    state.set_shadow_memory(1, 0x100, 77);
    step(&mut state, PcodeOpcode::Copy, &[src], &out);
    assert_eq!(state.get_shadow_memory(1, 0x200), Some(77));
}

#[test]
fn an_add_of_a_tainted_value_is_tainted() {
    let mut state = MachineState::new();
    let (a, b, out) = (slot(0x100, 4), slot(0x110, 4), slot(0x200, 4));
    state.write_space(1, 0x100, &[1, 0, 0, 0]).unwrap();
    state.write_space(1, 0x110, &[2, 0, 0, 0]).unwrap();
    state.set_shadow_memory(1, 0x100, 77);
    // A solver node must exist for the id; the add builds on it or a constant.
    step(&mut state, PcodeOpcode::IntAdd, &[a, b], &out);
    assert!(
        state.get_shadow_memory(1, 0x200).is_some(),
        "the sum depends on a tainted input"
    );
}
