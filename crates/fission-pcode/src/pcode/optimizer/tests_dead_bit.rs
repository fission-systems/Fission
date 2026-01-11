use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

#[test]
fn test_dead_bit_elimination() {
    // Test case: A = B & 0xFF00; C = A & 0xF000;
    // Here, A is only consumed at bits 0xF000.
    // B & 0xFF00 provides bits 0xFF00.
    // The mask 0xFF00 clears bits ~0xFF00.
    // Consumed bits are 0xF000.
    // Cleared bits are 0x00FF (and upper bits).
    // 0xF000 & 0x00FF == 0.
    // So the mask 0xFF00 is NOT redundant for bits 0xF000?
    // Wait.
    // Mask = 0xFF00. ~Mask = ...FFFF00FF.
    // Consume = 0xF000.
    // ~Mask & Consume = (...FFFF00FF) & 0xF000 = 0.
    // So yes, the mask is redundant for the consumed bits.
    // Wait, if B has garbage in 0x0F00, and we do B & 0xFF00, we keep 0x0F00.
    // If consumer only wants 0xF000, then 0x0F00 doesn't matter.
    // So A = B is valid.

    let mut func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // Op 0: A = B & 0xFF00
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }), // A
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x200,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // B
                        Varnode::constant(0xFF00, 4),
                    ],
                    asm_mnemonic: None,
                },
                // Op 1: C = A & 0xF000
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1004,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x300,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }), // C
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // A
                        Varnode::constant(0xF000, 4),
                    ],
                    asm_mnemonic: None,
                },
                // Op 2: Return C (force C to be consumed)
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1008,
                    output: None,
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x300,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // C
                    ],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut config = PcodeOptimizerConfig::default();
    // Disable DCE to ensure we see the transformation of Op 0, not its removal (though it shouldn't be removed as it's used)
    config.enable_dead_code_elimination = false;

    let mut optimizer = PcodeOptimizer::new(config);
    optimizer.optimize(&mut func);

    // Check Op 0. It should be converted to Copy (or removed if identity, but Copy is safer check)
    // A = B & 0xFF00 -> A = B
    let op0 = &func.blocks[0].ops[0];
    assert_eq!(
        op0.opcode,
        PcodeOpcode::Copy,
        "Op 0 should be optimized to Copy"
    );
    assert_eq!(op0.inputs.len(), 1);
    assert_eq!(op0.inputs[0].offset, 0x200); // B
}

#[test]
fn test_dead_bit_elimination_or() {
    // Test case: A = B | 0x100; C = A & 0xFF;
    // A is consumed at 0xFF.
    // Mask 0x100 affects bit 8.
    // Consume mask 0xFF (bits 0-7).
    // (Mask & Consume) = 0x100 & 0xFF = 0.
    // So the OR operation is redundant for the consumed bits.
    // A = B (effectively).

    let mut func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // Op 0: A = B | 0x100
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntOr,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }), // A
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x200,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // B
                        Varnode::constant(0x100, 4),
                    ],
                    asm_mnemonic: None,
                },
                // Op 1: C = A & 0xFF
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1004,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x300,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }), // C
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // A
                        Varnode::constant(0xFF, 4),
                    ],
                    asm_mnemonic: None,
                },
                // Op 2: Return C
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1008,
                    output: None,
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x300,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }, // C
                    ],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut config = PcodeOptimizerConfig::default();
    config.enable_dead_code_elimination = false;

    let mut optimizer = PcodeOptimizer::new(config);
    optimizer.optimize(&mut func);

    // Check Op 0. Should be Copy.
    let op0 = &func.blocks[0].ops[0];
    assert_eq!(
        op0.opcode,
        PcodeOpcode::Copy,
        "Op 0 should be optimized to Copy"
    );
}
