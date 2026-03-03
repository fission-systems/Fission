/// Tests for Pcode optimizer
use super::*;
use crate::pcode::{PcodeOp, PcodeOpcode, Varnode};

#[test]
fn test_xor_with_zero() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntXor,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![
            Varnode {
                space_id: 2,
                offset: 0x10,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode::constant(0, 4),
        ],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert_eq!(optimized.inputs.len(), 1);
}

#[test]
fn test_and_with_zero() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAnd,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![
            Varnode {
                space_id: 2,
                offset: 0x10,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode::constant(0, 4),
        ],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_add_with_zero() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![
            Varnode {
                space_id: 2,
                offset: 0x10,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode::constant(0, 4),
        ],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
}

// ===== Tests for RuleTrivialArith =====

#[test]
fn test_trivial_arith_equal() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntEqual,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), vn.clone()],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // true
}

#[test]
fn test_trivial_arith_notequal() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntNotEqual,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), vn.clone()],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

#[test]
fn test_trivial_arith_less() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntLess,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), vn.clone()],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

// ===== Tests for RuleTrivialBool =====

#[test]
fn test_trivial_bool_and_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolAnd,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert_eq!(optimized.inputs[0], vn);
}

#[test]
fn test_trivial_bool_and_false() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolAnd,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), Varnode::constant(0, 1)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

#[test]
fn test_trivial_bool_or_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolOr,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // true
}

#[test]
fn test_trivial_bool_xor_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let vn = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolXor,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::BoolNegate);
    assert_eq!(optimized.inputs[0], vn);
}

// ===== Tests for RuleCollapseConstants =====

#[test]
fn test_collapse_constants_add() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![Varnode::constant(5, 4), Varnode::constant(3, 4)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 8);
}

#[test]
fn test_collapse_constants_mult() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntMult,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![Varnode::constant(7, 4), Varnode::constant(6, 4)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 42);
}

#[test]
fn test_collapse_constants_comparison() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());

    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntLess,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: 1,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![Varnode::constant(5, 4), Varnode::constant(10, 4)],
        asm_mnemonic: None,
    };

    let Some(optimized) = optimizer.rules.try_optimize(&op) else {
        panic!("optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // 5 < 10 is true
}
// ===== Phase 2: Tests for Advanced Rules =====

#[test]
fn test_shift_bitops_left_zero() {
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // V = 0xf000
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(0xf000, 4)],
                    asm_mnemonic: None,
                },
                // Result = V << 20  (shifts all bits out of 32-bit range)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntLeft,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x200,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(20, 4),
                    ],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut tracker = DefUseTracker::new();
    tracker.build(&func);

    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker, &func);

    assert!(result.is_some());
    let Some(optimized) = result else {
        panic!("tracker optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_shift_bitops_right_zero() {
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // V = 0x0f
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                    asm_mnemonic: None,
                },
                // Result = V >> 8  (shifts all bits out)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntRight,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x200,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(8, 4),
                    ],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut tracker = DefUseTracker::new();
    tracker.build(&func);

    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker, &func);

    assert!(result.is_some());
    let Some(optimized) = result else {
        panic!("tracker optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_and_mask_always_zero() {
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // V = 0x0f
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                    asm_mnemonic: None,
                },
                // Result = V & 0xf0  (no overlapping bits)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x200,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(0xf0, 4),
                    ],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut tracker = DefUseTracker::new();
    tracker.build(&func);

    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker, &func);

    assert!(result.is_some());
    let Some(optimized) = result else {
        panic!("tracker optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_and_mask_noop() {
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![
                // V = 0x0f
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                    asm_mnemonic: None,
                },
                // Result = V & 0xff  (mask doesn't clear any bits)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x200,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![
                        Varnode {
                            space_id: 1,
                            offset: 0x100,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(0xff, 4),
                    ],
                    asm_mnemonic: None,
                },
                // Consume Result so consume mask is non-zero
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1000,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: 1,
                        offset: 0x200,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let mut tracker = DefUseTracker::new();
    tracker.build(&func);

    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker, &func);

    assert!(result.is_some());
    let Some(optimized) = result else {
        panic!("tracker optimization should succeed")
    };
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(!optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].offset, 0x100); // Should copy V
}
// ===== Tests for CSE and New Rules =====

#[test]
fn test_cse_basic() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: false,
        ..PcodeOptimizerConfig::default()
    });

    let vn_a = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_b = Varnode {
        space_id: 2,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_x = Varnode {
        space_id: 1,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_y = Varnode {
        space_id: 1,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    let op1 = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000,
        output: Some(vn_x.clone()),
        inputs: vec![vn_a.clone(), vn_b.clone()],
        asm_mnemonic: None,
    };

    let op2 = PcodeOp {
        seq_num: 1,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1004,
        output: Some(vn_y.clone()),
        inputs: vec![vn_a.clone(), vn_b.clone()],
        asm_mnemonic: None,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![op1, op2],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    assert_eq!(ops[0].opcode, PcodeOpcode::IntAdd);
    assert_eq!(ops[1].opcode, PcodeOpcode::Copy);
    assert_eq!(ops[1].inputs[0], vn_x);
}

#[test]
fn test_ptr_arith() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: false,
        ..PcodeOptimizerConfig::default()
    });

    let vn_base = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_x = Varnode {
        space_id: 1,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_y = Varnode {
        space_id: 1,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    // x = base + 10
    let op1 = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000,
        output: Some(vn_x.clone()),
        inputs: vec![vn_base.clone(), Varnode::constant(10, 4)],
        asm_mnemonic: None,
    };

    // y = x + 20
    let op2 = PcodeOp {
        seq_num: 1,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1004,
        output: Some(vn_y.clone()),
        inputs: vec![vn_x.clone(), Varnode::constant(20, 4)],
        asm_mnemonic: None,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![op1, op2],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    // op2 should become base + 30
    assert_eq!(ops[1].opcode, PcodeOpcode::IntAdd);
    assert_eq!(ops[1].inputs[0], vn_base);
    assert!(ops[1].inputs[1].is_constant);
    assert_eq!(ops[1].inputs[1].constant_val, 30);
}

#[test]
fn test_pull_sub_indirect() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: false,
        ..PcodeOptimizerConfig::default()
    });

    let vn_ptr = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_x = Varnode {
        space_id: 1,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_y = Varnode {
        space_id: 1,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    // x = ptr + 10
    let op1 = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000,
        output: Some(vn_x.clone()),
        inputs: vec![vn_ptr.clone(), Varnode::constant(10, 4)],
        asm_mnemonic: None,
    };

    // y = x - ptr
    let op2 = PcodeOp {
        seq_num: 1,
        opcode: PcodeOpcode::IntSub,
        address: 0x1004,
        output: Some(vn_y.clone()),
        inputs: vec![vn_x.clone(), vn_ptr.clone()],
        asm_mnemonic: None,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![op1, op2],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    // op2 should become COPY 10
    assert_eq!(ops[1].opcode, PcodeOpcode::Copy);
    assert!(ops[1].inputs[0].is_constant);
    assert_eq!(ops[1].inputs[0].constant_val, 10);
}

#[test]
fn test_indirect_collapse() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: false,
        ..PcodeOptimizerConfig::default()
    });

    let vn_base = Varnode {
        space_id: 2,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_x = Varnode {
        space_id: 1,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let vn_y = Varnode {
        space_id: 1,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    // x = PTRSUB(base, 10)
    let op1 = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::PtrSub,
        address: 0x1000,
        output: Some(vn_x.clone()),
        inputs: vec![vn_base.clone(), Varnode::constant(10, 4)],
        asm_mnemonic: None,
    };

    // y = PTRSUB(x, 20)
    let op2 = PcodeOp {
        seq_num: 1,
        opcode: PcodeOpcode::PtrSub,
        address: 0x1004,
        output: Some(vn_y.clone()),
        inputs: vec![vn_x.clone(), Varnode::constant(20, 4)],
        asm_mnemonic: None,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            ops: vec![op1, op2],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    // op2 should become PTRSUB(base, 30)
    assert_eq!(ops[1].opcode, PcodeOpcode::PtrSub);
    assert_eq!(ops[1].inputs[0], vn_base);
    assert!(ops[1].inputs[1].is_constant);
    assert_eq!(ops[1].inputs[1].constant_val, 30);
}
