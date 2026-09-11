//! An opcode an engine does not implement has to say so.
//!
//! Both engines treat an unknown p-code opcode as a no-op and emit a
//! `tracing::warn`, which in a normal run goes nowhere. `EmulatorMetrics` has
//! carried `unimplemented_opcodes`, a `note_unimplemented`, a `top_unimplemented`
//! report and an `srd` diff for it from the beginning -- and nothing ever wrote
//! to it. A run that silently drops semantics is a wrong answer, and the whole
//! point of a coverage number is that a zero in it means something.
//!
//! So this pins the wiring rather than the coverage: it feeds the compiler an
//! opcode nothing lowers and checks the counter moves. Without it, "no opcode
//! was lowered to nothing" is indistinguishable from "nobody is counting".

use fission_emulator::jit::compiler::{GuestInsn, JitCompiler};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};

/// `CPoolRef` is a JVM p-code op. No x86 or ARM SLEIGH spec emits one, which
/// is exactly what makes it a safe probe: it can only appear here.
fn cpool_ref() -> PcodeOp {
    PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::CPoolRef,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 2,
            offset: 0,
            size: 8,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![Varnode::constant(1, 8), Varnode::constant(2, 8)],
        asm_mnemonic: None,
    }
}

#[test]
fn the_compiler_counts_an_opcode_it_lowers_to_nothing() {
    let mut jit = JitCompiler::new().expect("jit");
    assert!(
        jit.unimplemented_ops.is_empty(),
        "a fresh compiler has met nothing yet"
    );

    let insns = vec![GuestInsn {
        pc: 0x1000,
        len: 4,
        ops: vec![cpool_ref()],
    }];
    jit.compile_translation_block(&insns, 2, 1)
        .expect("the block still compiles -- the op lowers to nothing, it does not fail");

    assert_eq!(
        jit.unimplemented_ops.get("CPoolRef"),
        Some(&1),
        "the op was dropped without being counted: {:?}",
        jit.unimplemented_ops
    );
}
