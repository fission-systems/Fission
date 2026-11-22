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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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

#[test]
fn test_copy_propagation_single_use_same_block() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: true,
        enable_cse: false,
        ..PcodeOptimizerConfig::default()
    });

    let src = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let out = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(tmp.clone()),
                    inputs: vec![src.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1001,
                    output: Some(out.clone()),
                    inputs: vec![tmp.clone(), Varnode::constant(1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1002,
                    output: None,
                    inputs: vec![out.clone()],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].opcode, PcodeOpcode::IntAdd);
    assert_eq!(ops[0].inputs[0], src);
    assert_eq!(ops[1].opcode, PcodeOpcode::Return);
}

#[test]
fn test_cast_copy_chain_propagation_same_size() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: true,
        enable_cse: false,
        ..PcodeOptimizerConfig::default()
    });

    let src = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let cast_tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x200,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let copy_tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x204,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let out = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x208,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Cast,
                    address: 0x1000,
                    output: Some(cast_tmp.clone()),
                    inputs: vec![src.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1001,
                    output: Some(copy_tmp.clone()),
                    inputs: vec![cast_tmp.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntXor,
                    address: 0x1002,
                    output: Some(out.clone()),
                    inputs: vec![copy_tmp, Varnode::constant(2, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x1003,
                    output: None,
                    inputs: vec![out.clone()],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].opcode, PcodeOpcode::IntXor);
    assert_eq!(ops[0].inputs[0], src);
    assert_eq!(ops[1].opcode, PcodeOpcode::Return);
}

#[test]
fn test_copy_propagation_does_not_cross_store_boundary() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: false,
        enable_cse: false,
        ..PcodeOptimizerConfig::default()
    });

    let src = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let out = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(tmp.clone()),
                    inputs: vec![src.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x1001,
                    output: None,
                    inputs: vec![Varnode::constant(0x2000, 8), Varnode::constant(0x55, 1)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1002,
                    output: Some(out),
                    inputs: vec![tmp.clone(), Varnode::constant(1, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    assert_eq!(ops.len(), 3);
    assert_eq!(ops[2].inputs[0], tmp);
}

#[test]
fn test_loop_header_temp_coalescing_preserves_multiequal() {
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_dead_code_elimination: true,
        enable_cse: false,
        ..PcodeOptimizerConfig::default()
    });

    let in_a = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let in_b = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x14,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let phi = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x104,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let out = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x108,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x2000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::MultiEqual,
                    address: 0x2000,
                    output: Some(phi.clone()),
                    inputs: vec![in_a, in_b],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x2001,
                    output: Some(tmp.clone()),
                    inputs: vec![phi.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x2002,
                    output: Some(out.clone()),
                    inputs: vec![tmp, Varnode::constant(4, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x2003,
                    output: None,
                    inputs: vec![out.clone()],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    optimizer.optimize(&mut func);

    let ops = &func.blocks[0].ops;
    assert_eq!(ops.len(), 3);
    assert_eq!(ops[0].opcode, PcodeOpcode::MultiEqual);
    assert_eq!(ops[1].opcode, PcodeOpcode::IntAdd);
    assert_eq!(ops[1].inputs[0], phi);
    assert_eq!(ops[2].opcode, PcodeOpcode::Return);
}

#[test]
fn test_or_all_ones_is_all_ones_at_every_width() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let or_all_ones = |size: u32| PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntOr,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![
            Varnode {
                space_id: 2,
                offset: 0x10,
                size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode::constant(-1, size),
        ],
        asm_mnemonic: None,
    };

    for (size, expected) in [(1, 0xFF), (4, 0xFFFF_FFFF), (8, -1)] {
        let Some(optimized) = optimizer.rules.try_optimize(&or_all_ones(size)) else {
            panic!("x | -1 should fold at size {size}")
        };
        assert_eq!(optimized.opcode, PcodeOpcode::Copy);
        assert_eq!(
            optimized.inputs[0].constant_val, expected,
            "x | -1 at size {size}"
        );
    }
}

fn cse_vn(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 2,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn cse_op(seq: u32, opcode: PcodeOpcode, output: Varnode, inputs: Vec<Varnode>) -> PcodeOp {
    PcodeOp {
        seq_num: seq,
        opcode,
        address: 0x1000 + u64::from(seq),
        output: Some(output),
        inputs,
        asm_mnemonic: None,
    }
}

fn run_cse(ops: Vec<PcodeOp>) -> Vec<PcodeOp> {
    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops,
        }],
    };
    CommonSubexpressionEliminator::new().eliminate(&mut func);
    func.blocks.remove(0).ops
}

#[test]
fn cse_keeps_zero_extensions_of_different_widths_apart() {
    // y:4 = ZEXT(x:1); z:8 = ZEXT(x:1). Rewriting z into COPY y hands back a
    // 4-byte value where an 8-byte one was asked for.
    let x = cse_vn(0x10, 1);
    let ops = run_cse(vec![
        cse_op(0, PcodeOpcode::IntZExt, cse_vn(0x100, 4), vec![x.clone()]),
        cse_op(1, PcodeOpcode::IntZExt, cse_vn(0x108, 8), vec![x]),
    ]);
    assert_eq!(ops[1].opcode, PcodeOpcode::IntZExt);
}

#[test]
fn cse_forgets_an_expression_once_an_input_is_overwritten() {
    // t = a + b; a = c; u = a + b  -- the second `a + b` is a different value.
    let (a, b, c) = (cse_vn(0x10, 4), cse_vn(0x20, 4), cse_vn(0x30, 4));
    let ops = run_cse(vec![
        cse_op(
            0,
            PcodeOpcode::IntAdd,
            cse_vn(0x100, 4),
            vec![a.clone(), b.clone()],
        ),
        cse_op(1, PcodeOpcode::Copy, a.clone(), vec![c]),
        cse_op(2, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
    ]);
    assert_eq!(ops[2].opcode, PcodeOpcode::IntAdd);
}

#[test]
fn cse_forgets_an_expression_once_its_result_is_overwritten() {
    // x = a + b; x = c; y = a + b  -- `x` no longer holds `a + b`.
    let (a, b, c, x) = (
        cse_vn(0x10, 4),
        cse_vn(0x20, 4),
        cse_vn(0x30, 4),
        cse_vn(0x100, 4),
    );
    let ops = run_cse(vec![
        cse_op(
            0,
            PcodeOpcode::IntAdd,
            x.clone(),
            vec![a.clone(), b.clone()],
        ),
        cse_op(1, PcodeOpcode::Copy, x, vec![c]),
        cse_op(2, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
    ]);
    assert_eq!(ops[2].opcode, PcodeOpcode::IntAdd);
}

#[test]
fn cse_forgets_an_expression_when_a_sub_register_of_an_input_is_written() {
    // a is 4 bytes at 0x10; a write to the 2 bytes at 0x12 overlaps it.
    let (a, b) = (cse_vn(0x10, 4), cse_vn(0x20, 4));
    let ops = run_cse(vec![
        cse_op(
            0,
            PcodeOpcode::IntAdd,
            cse_vn(0x100, 4),
            vec![a.clone(), b.clone()],
        ),
        cse_op(1, PcodeOpcode::Copy, cse_vn(0x12, 2), vec![cse_vn(0x40, 2)]),
        cse_op(2, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
    ]);
    assert_eq!(ops[2].opcode, PcodeOpcode::IntAdd);
}

#[test]
fn cse_does_not_cache_a_result_that_overwrote_its_own_input() {
    // a = a + b; c = a + b  -- the second `a` is the new one.
    let (a, b) = (cse_vn(0x10, 4), cse_vn(0x20, 4));
    let ops = run_cse(vec![
        cse_op(
            0,
            PcodeOpcode::IntAdd,
            a.clone(),
            vec![a.clone(), b.clone()],
        ),
        cse_op(1, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
    ]);
    assert_eq!(ops[1].opcode, PcodeOpcode::IntAdd);
}

#[test]
fn cse_forgets_everything_across_a_call_or_store() {
    for barrier in [
        PcodeOpcode::Call,
        PcodeOpcode::CallOther,
        PcodeOpcode::Store,
    ] {
        let (a, b) = (cse_vn(0x10, 4), cse_vn(0x20, 4));
        let mut call = cse_op(1, barrier, cse_vn(0x500, 4), vec![cse_vn(0x50, 8)]);
        call.output = None;
        let ops = run_cse(vec![
            cse_op(
                0,
                PcodeOpcode::IntAdd,
                cse_vn(0x100, 4),
                vec![a.clone(), b.clone()],
            ),
            call,
            cse_op(2, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
        ]);
        assert_eq!(ops[2].opcode, PcodeOpcode::IntAdd, "{barrier:?}");
    }
}

#[test]
fn cse_still_merges_a_repeat_with_unrelated_writes_in_between() {
    let (a, b) = (cse_vn(0x10, 4), cse_vn(0x20, 4));
    let ops = run_cse(vec![
        cse_op(
            0,
            PcodeOpcode::IntAdd,
            cse_vn(0x100, 4),
            vec![a.clone(), b.clone()],
        ),
        cse_op(
            1,
            PcodeOpcode::Copy,
            cse_vn(0x300, 4),
            vec![cse_vn(0x40, 4)],
        ),
        cse_op(2, PcodeOpcode::IntAdd, cse_vn(0x104, 4), vec![a, b]),
    ]);
    assert_eq!(ops[2].opcode, PcodeOpcode::Copy);
}

fn float_cmp(opcode: PcodeOpcode) -> PcodeOp {
    let x = cse_vn(0x10, 8);
    PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(cse_vn(0x100, 1)),
        inputs: vec![x.clone(), x],
        asm_mnemonic: None,
    }
}

#[test]
fn float_self_comparisons_are_not_folded_except_the_strict_less() {
    // NaN is unordered: NaN == NaN and NaN <= NaN are false, NaN != NaN is true.
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    for opcode in [
        PcodeOpcode::FloatEqual,
        PcodeOpcode::FloatNotEqual,
        PcodeOpcode::FloatLessEqual,
    ] {
        assert!(
            optimizer.rules.try_optimize(&float_cmp(opcode)).is_none(),
            "{opcode:?}(x, x) must not fold"
        );
    }
    // x < x is false for every x, NaN included.
    assert!(
        optimizer
            .rules
            .try_optimize(&float_cmp(PcodeOpcode::FloatLess))
            .is_some()
    );
}

fn optimize_ops(ops: Vec<PcodeOp>) -> Vec<PcodeOp> {
    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops,
        }],
    };
    PcodeOptimizer::new(PcodeOptimizerConfig::default()).optimize(&mut func);
    func.blocks.remove(0).ops
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

/// A function input is read but never written, so nothing is known about its
/// bits. The mask was seeded as "all bits zero", and `x & 0xff` was rewritten
/// to `x`.
#[test]
fn and_mask_on_a_function_input_is_kept() {
    let (input, out) = (cse_vn(0x10, 4), cse_vn(0x100, 4));
    let ops = optimize_ops(vec![
        cse_op(
            0,
            PcodeOpcode::IntAnd,
            out.clone(),
            vec![input, Varnode::constant(0xFF, 4)],
        ),
        ret(1, out),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "the mask must survive: {ops:?}"
    );
}

/// `and esi, 0xf` reads the old `esi` while defining the new one.
#[test]
fn in_place_and_on_a_live_in_register_is_kept() {
    let (esi, eax) = (cse_vn(0x20, 4), cse_vn(0x0, 4));
    let ops = optimize_ops(vec![
        cse_op(
            0,
            PcodeOpcode::IntAnd,
            esi.clone(),
            vec![esi.clone(), Varnode::constant(0xF, 4)],
        ),
        cse_op(1, PcodeOpcode::Copy, eax.clone(), vec![esi]),
        ret(2, eax),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "the mask must survive: {ops:?}"
    );
}

/// `mov eax, esi; and eax, 0xf`.
#[test]
fn copy_then_mask_of_a_live_in_register_is_kept() {
    let (esi, eax) = (cse_vn(0x20, 4), cse_vn(0x0, 4));
    let ops = optimize_ops(vec![
        cse_op(0, PcodeOpcode::Copy, eax.clone(), vec![esi]),
        cse_op(
            1,
            PcodeOpcode::IntAnd,
            eax.clone(),
            vec![eax.clone(), Varnode::constant(0xF, 4)],
        ),
        ret(2, eax),
    ]);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "the mask must survive: {ops:?}"
    );
}

/// What the mask is for: a value the program built itself has known bits, and a
/// mask that cannot clear any of them goes away.
#[test]
fn and_mask_after_a_zero_extension_is_still_removed() {
    let (byte, wide, masked) = (cse_vn(0x10, 1), cse_vn(0x100, 4), cse_vn(0x104, 4));
    let ops = optimize_ops(vec![
        cse_op(0, PcodeOpcode::IntZExt, wide.clone(), vec![byte]),
        cse_op(
            1,
            PcodeOpcode::IntAnd,
            masked.clone(),
            vec![wide, Varnode::constant(0xFF, 4)],
        ),
        ret(2, masked),
    ]);
    assert!(
        !ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd),
        "a mask that clears nothing should be dropped: {ops:?}"
    );
}

/// Writing `ax` changes the low half of `eax`, which is a different varnode id.
#[test]
fn a_partial_register_write_makes_the_wide_register_untracked() {
    let (byte, eax, ax, other) = (
        cse_vn(0x10, 1),
        cse_vn(0x0, 4),
        cse_vn(0x0, 2),
        cse_vn(0x30, 2),
    );
    let func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                cse_op(0, PcodeOpcode::IntZExt, eax.clone(), vec![byte]),
                cse_op(1, PcodeOpcode::Copy, ax, vec![other]),
                ret(2, eax.clone()),
            ],
        }],
    };
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    // Not the 0xFF a zero-extended byte would give: `ax` was overwritten.
    assert_eq!(tracker.get_nz_mask(&eax), 0xFFFF_FFFF);
}

fn reg_vn(offset: u64) -> Varnode {
    Varnode {
        space_id: 2,
        offset,
        size: 4,
        is_constant: false,
        constant_val: 0,
    }
}

fn add_op(seq: u32, out: &Varnode, a: &Varnode, b: &Varnode) -> PcodeOp {
    PcodeOp {
        seq_num: seq,
        opcode: PcodeOpcode::IntAdd,
        address: 0x1000 + u64::from(seq) * 4,
        output: Some(out.clone()),
        inputs: vec![a.clone(), b.clone()],
        asm_mnemonic: None,
    }
}

/// #320: `DeadCodeEliminator::eliminate` marks every op's inputs as used, so a
/// single call removes only the last link of a dead chain. The driver repeats
/// until nothing changes, so a chain `t3 = t2 + d; t2 = t1 + c; t1 = a + b`
/// that nothing reads is gone after `optimize`, not after one pass.
#[test]
fn a_dead_chain_is_removed_by_the_driver_and_a_live_result_is_kept() {
    let (a, b, c, d) = (reg_vn(0x10), reg_vn(0x14), reg_vn(0x18), reg_vn(0x1c));
    let (t1, t2, t3, live) = (reg_vn(0x100), reg_vn(0x104), reg_vn(0x108), reg_vn(0x200));
    let ret = PcodeOp {
        seq_num: 9,
        opcode: PcodeOpcode::Return,
        address: 0x1040,
        output: None,
        inputs: vec![live.clone()],
        asm_mnemonic: None,
    };
    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                add_op(0, &t1, &a, &b),
                add_op(1, &t2, &t1, &c),
                add_op(2, &t3, &t2, &d),
                add_op(3, &live, &a, &d),
                ret,
            ],
        }],
    };
    // Only DCE, so folding and CSE cannot be what removes them.
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig {
        enable_constant_folding: false,
        enable_algebraic_simplification: false,
        enable_copy_propagation: false,
        enable_cse: false,
        enable_identity_removal: false,
        enable_loop_header_temp_coalescing: false,
        enable_subvariable_flow: false,
        enable_dead_code_elimination: true,
        ..PcodeOptimizerConfig::default()
    });
    optimizer.optimize(&mut func);

    let outputs: Vec<u64> = func.blocks[0]
        .ops
        .iter()
        .filter_map(|op| op.output.as_ref().map(|o| o.offset))
        .collect();
    assert_eq!(
        outputs,
        vec![0x200],
        "only the value the return reads survives"
    );
}

// ---- constant folding reads each constant at its own width -------------------
//
// A 4-byte `-16` can be held as 0xfffffff0 (zero-extended) or as -16
// (sign-extended); both are the same p-code value, and a fold must give the
// same answer for either.

fn fold(opcode: PcodeOpcode, out_size: u32, lhs: (i64, u32), rhs: (i64, u32)) -> (i64, u32) {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let op = PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(Varnode {
            space_id: 1,
            offset: 0x100,
            size: out_size,
            is_constant: false,
            constant_val: 0,
        }),
        inputs: vec![
            Varnode::constant(lhs.0, lhs.1),
            Varnode::constant(rhs.0, rhs.1),
        ],
        asm_mnemonic: None,
    };
    let folded = optimizer
        .rules
        .try_optimize(&op)
        .expect("a constant fold applies");
    assert_eq!(folded.opcode, PcodeOpcode::Copy, "{opcode:?}");
    let value = &folded.inputs[0];
    assert!(value.is_constant);
    // The value at the output's width, however the constant is stored.
    let bits = out_size * 8;
    let masked = if bits >= 64 {
        value.constant_val as u64
    } else {
        (value.constant_val as u64) & ((1u64 << bits) - 1)
    };
    (masked as i64, out_size)
}

const NEG16_ZEXT: i64 = 0xffff_fff0;
const NEG16_SEXT: i64 = -16;

#[test]
fn signed_less_reads_narrow_negative_constants_as_negative() {
    for neg in [NEG16_ZEXT, NEG16_SEXT] {
        // -16 <s 0 is true for a 4-byte operand
        assert_eq!(
            fold(PcodeOpcode::IntSLess, 1, (neg, 4), (0, 4)).0,
            1,
            "{neg}"
        );
        assert_eq!(
            fold(PcodeOpcode::IntSLess, 1, (0, 4), (neg, 4)).0,
            0,
            "{neg}"
        );
        assert_eq!(
            fold(PcodeOpcode::IntSLessEqual, 1, (neg, 4), (0, 4)).0,
            1,
            "{neg}"
        );
    }
}

#[test]
fn unsigned_less_reads_narrow_negative_constants_as_large() {
    for neg in [NEG16_ZEXT, NEG16_SEXT] {
        // 0xfffffff0 <u 1 is false; 1 <u 0xfffffff0 is true
        assert_eq!(
            fold(PcodeOpcode::IntLess, 1, (neg, 4), (1, 4)).0,
            0,
            "{neg}"
        );
        assert_eq!(
            fold(PcodeOpcode::IntLess, 1, (1, 4), (neg, 4)).0,
            1,
            "{neg}"
        );
    }
}

#[test]
fn right_shifts_of_a_narrow_negative_constant() {
    for neg in [NEG16_ZEXT, NEG16_SEXT] {
        // logical: 0xfffffff0 >> 4 = 0x0fffffff
        assert_eq!(
            fold(PcodeOpcode::IntRight, 4, (neg, 4), (4, 4)).0,
            0x0fff_ffff,
            "{neg}"
        );
        // arithmetic: -16 >>s 4 = -1 = 0xffffffff
        assert_eq!(
            fold(PcodeOpcode::IntSRight, 4, (neg, 4), (4, 4)).0,
            0xffff_ffff,
            "{neg}"
        );
    }
}

#[test]
fn division_and_remainder_of_narrow_negative_constants() {
    for neg in [NEG16_ZEXT, NEG16_SEXT] {
        // signed: -16 / 3 = -5, -16 % 3 = -1
        assert_eq!(
            fold(PcodeOpcode::IntSDiv, 4, (neg, 4), (3, 4)).0,
            0xffff_fffb,
            "{neg}"
        );
        assert_eq!(
            fold(PcodeOpcode::IntSRem, 4, (neg, 4), (3, 4)).0,
            0xffff_ffff,
            "{neg}"
        );
        // unsigned: 0xfffffff0 / 3 = 0x55555550, % 3 = 0
        assert_eq!(
            fold(PcodeOpcode::IntDiv, 4, (neg, 4), (3, 4)).0,
            0x5555_5550,
            "{neg}"
        );
        assert_eq!(fold(PcodeOpcode::IntRem, 4, (neg, 4), (3, 4)).0, 0, "{neg}");
    }
}

#[test]
fn equal_compares_constants_at_their_width() {
    // The same 4-byte value stored two ways is equal.
    assert_eq!(
        fold(PcodeOpcode::IntEqual, 1, (NEG16_ZEXT, 4), (NEG16_SEXT, 4)).0,
        1
    );
    assert_eq!(
        fold(
            PcodeOpcode::IntNotEqual,
            1,
            (NEG16_ZEXT, 4),
            (NEG16_SEXT, 4)
        )
        .0,
        0
    );
}

#[test]
fn overflowed_const_divisor_does_not_panic_in_div_or_rem() {
    let wrapped_zero = Varnode::constant(0x1_0000_0000u64 as i64, 4);
    let tmp = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let res = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    for opcode in [
        PcodeOpcode::IntDiv,
        PcodeOpcode::IntSDiv,
        PcodeOpcode::IntRem,
        PcodeOpcode::IntSRem,
    ] {
        let mut func = PcodeFunction {
            blocks: vec![crate::pcode::PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1000,
                        output: Some(tmp.clone()),
                        inputs: vec![
                            Varnode::constant(0xFFFF_FFFFu64 as i64, 4),
                            Varnode::constant(1, 4),
                        ],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode,
                        address: 0x1004,
                        output: Some(res.clone()),
                        inputs: vec![Varnode::constant(100, 4), tmp.clone()],
                        asm_mnemonic: None,
                    },
                    ret(2, res.clone()),
                ],
            }],
        };
        let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
        optimizer.optimize(&mut func);

        // Also check direct rule fold with an untruncated constant_val representing 0 mod 2^32.
        let direct_div = PcodeOp {
            seq_num: 0,
            opcode,
            address: 0x1000,
            output: Some(res.clone()),
            inputs: vec![Varnode::constant(100, 4), wrapped_zero.clone()],
            asm_mnemonic: None,
        };
        assert!(optimizer.rules.try_optimize(&direct_div).is_none());
    }
}

#[test]
fn dce_preserves_super_register_on_subregister_read_and_preserves_callother() {
    let rax = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let al = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let unused_out = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x80,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(rax.clone()),
                    inputs: vec![Varnode::constant(0x1234, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallOther,
                    address: 0x1004,
                    output: Some(unused_out),
                    inputs: vec![Varnode::constant(5, 4)],
                    asm_mnemonic: None,
                },
                ret(2, al),
            ],
        }],
    };

    let dce = DeadCodeEliminator::new();
    let mut modified = false;
    dce.eliminate(&mut func, &mut modified);
    assert!(!modified);
    assert_eq!(func.blocks[0].ops.len(), 3);
}

#[test]
fn def_use_algebraic_rules_do_not_fold_across_clobbers_or_blocks() {
    let rules = crate::pcode::optimizer::rules::OptimizationRules::new();
    let reg = |offset: u64, size: u32| Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let base = reg(0x10, 4);
    let x = reg(0x20, 4);
    let y = reg(0x30, 4);

    // 1. Intervening clobber of `base` prevents `try_ptr_arith`.
    let clobbered_ptr_arith = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                add_op(0, &x, &base, &Varnode::constant(10, 4)),
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1004,
                    output: Some(reg(0x10, 1)),
                    inputs: vec![Varnode::constant(0, 1)],
                    asm_mnemonic: None,
                },
                add_op(2, &y, &x, &Varnode::constant(20, 4)),
            ],
        }],
    };
    let mut tracker = DefUseTracker::new();
    tracker.build(&clobbered_ptr_arith);
    assert!(
        rules
            .try_optimize_with_tracker_at(
                &clobbered_ptr_arith.blocks[0].ops[2],
                crate::pcode::optimizer::passes::def_use::OpRef {
                    block_idx: 0,
                    op_idx: 2,
                },
                &tracker,
                &clobbered_ptr_arith,
            )
            .is_none()
    );

    // 2. Self-overwrite `x = x + 10; y = x + 20` cannot fold to `y = x + 30`.
    let self_overwrite = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                add_op(0, &x, &x, &Varnode::constant(10, 4)),
                add_op(1, &y, &x, &Varnode::constant(20, 4)),
            ],
        }],
    };
    tracker.build(&self_overwrite);
    assert!(
        rules
            .try_optimize_with_tracker_at(
                &self_overwrite.blocks[0].ops[1],
                crate::pcode::optimizer::passes::def_use::OpRef {
                    block_idx: 0,
                    op_idx: 1,
                },
                &tracker,
                &self_overwrite,
            )
            .is_none()
    );

    // 3. Inter-block def cannot fold across blocks.
    let cross_block = PcodeFunction {
        blocks: vec![
            crate::pcode::PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![add_op(0, &x, &base, &Varnode::constant(10, 4))],
            },
            crate::pcode::PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![],
                ops: vec![add_op(1, &y, &x, &Varnode::constant(20, 4))],
            },
        ],
    };
    tracker.build(&cross_block);
    assert!(
        rules
            .try_optimize_with_tracker_at(
                &cross_block.blocks[1].ops[0],
                crate::pcode::optimizer::passes::def_use::OpRef {
                    block_idx: 1,
                    op_idx: 0,
                },
                &tracker,
                &cross_block,
            )
            .is_none()
    );

    // 4. `try_pull_sub_indirect` must not fold after `ptr` is overwritten.
    let clobbered_pull_sub = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                add_op(0, &x, &base, &Varnode::constant(10, 4)),
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1004,
                    output: Some(base.clone()),
                    inputs: vec![Varnode::constant(99, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x1008,
                    output: Some(y.clone()),
                    inputs: vec![x.clone(), base.clone()],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    tracker.build(&clobbered_pull_sub);
    assert!(
        rules
            .try_optimize_with_tracker_at(
                &clobbered_pull_sub.blocks[0].ops[2],
                crate::pcode::optimizer::passes::def_use::OpRef {
                    block_idx: 0,
                    op_idx: 2,
                },
                &tracker,
                &clobbered_pull_sub,
            )
            .is_none()
    );

    // 5. `try_indirect_collapse` must not fold across an intervening `Call` boundary.
    let call_boundary_ptrsub = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::PtrSub,
                    address: 0x1000,
                    output: Some(x.clone()),
                    inputs: vec![base.clone(), Varnode::constant(10, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Call,
                    address: 0x1004,
                    output: None,
                    inputs: vec![Varnode::constant(0x2000, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::PtrSub,
                    address: 0x1008,
                    output: Some(y),
                    inputs: vec![x, Varnode::constant(20, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    tracker.build(&call_boundary_ptrsub);
    assert!(
        rules
            .try_optimize_with_tracker_at(
                &call_boundary_ptrsub.blocks[0].ops[2],
                crate::pcode::optimizer::passes::def_use::OpRef {
                    block_idx: 0,
                    op_idx: 2,
                },
                &tracker,
                &call_boundary_ptrsub,
            )
            .is_none()
    );
}

#[test]
fn copy_propagation_and_loop_header_respect_unique_spaces_and_subregister_clobbers() {
    let copy_prop = CopyPropagator::new();
    let loop_coal = LoopHeaderTempCoalescer::new();
    let mut tracker = DefUseTracker::new();

    // Rust-Sleigh `unique` temp (space 2) propagates, while `REGISTER_SPACE_ID` (space 1) does not.
    let rs_src = Varnode {
        space_id: crate::space::RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let rs_tmp = Varnode {
        space_id: crate::space::RUST_SLEIGH_UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let leg_reg = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x10,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let mut rs_func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(rs_tmp.clone()),
                    inputs: vec![rs_src.clone()],
                    asm_mnemonic: None,
                },
                ret(1, rs_tmp.clone()),
            ],
        }],
    };
    tracker.build(&rs_func);
    assert!(copy_prop.eliminate(&mut rs_func, &tracker));
    assert_eq!(rs_func.blocks[0].ops[1].inputs[0], rs_src);

    let mut reg_func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(leg_reg.clone()),
                    inputs: vec![rs_src.clone()],
                    asm_mnemonic: None,
                },
                ret(1, leg_reg.clone()),
            ],
        }],
    };
    tracker.build(&reg_func);
    assert!(!copy_prop.eliminate(&mut reg_func, &tracker));

    // Sub-register overwrite (`AL` clobbering `RAX`) blocks propagation.
    let rs_al = Varnode {
        space_id: crate::space::RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let mut clobber_copy = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(rs_tmp.clone()),
                    inputs: vec![rs_src.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1002,
                    output: Some(rs_al.clone()),
                    inputs: vec![Varnode::constant(7, 1)],
                    asm_mnemonic: None,
                },
                ret(2, rs_tmp.clone()),
            ],
        }],
    };
    tracker.build(&clobber_copy);
    assert!(!copy_prop.eliminate(&mut clobber_copy, &tracker));

    // LoopHeaderTempCoalescer: propagates Rust-Sleigh unique, blocks on sub-register clobber.
    let phi = Varnode {
        space_id: crate::space::RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let mut clobber_loop = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::MultiEqual,
                    address: 0x1000,
                    output: Some(phi.clone()),
                    inputs: vec![leg_reg.clone(), rs_src.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1001,
                    output: Some(rs_tmp.clone()),
                    inputs: vec![phi.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1002,
                    output: Some(rs_al),
                    inputs: vec![Varnode::constant(9, 1)],
                    asm_mnemonic: None,
                },
                ret(3, rs_tmp),
            ],
        }],
    };
    tracker.build(&clobber_loop);
    assert!(!loop_coal.eliminate(&mut clobber_loop, &tracker));
}

#[test]
fn consume_mask_tracks_int_sext_sign_bit_callother_and_overlapping_aliases() {
    let b_sext = cse_vn(0x10, 1);
    let b_zext = cse_vn(0x14, 1);
    let sext_wide = cse_vn(0x20, 4);
    let zext_wide = cse_vn(0x24, 4);
    let hi_sext = cse_vn(0x30, 4);
    let hi_zext = cse_vn(0x34, 4);
    let co_in = cse_vn(0x40, 4);
    let co_out = cse_vn(0x50, 4);

    let rax = cse_vn(0, 8);
    let al = cse_vn(0, 1);
    let masked_rax = cse_vn(0x60, 8);

    let func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                cse_op(
                    0,
                    PcodeOpcode::IntSExt,
                    sext_wide.clone(),
                    vec![b_sext.clone()],
                ),
                cse_op(
                    1,
                    PcodeOpcode::IntRight,
                    hi_sext.clone(),
                    vec![sext_wide, Varnode::constant(8, 4)],
                ),
                ret(2, hi_sext),
                cse_op(
                    3,
                    PcodeOpcode::IntZExt,
                    zext_wide.clone(),
                    vec![b_zext.clone()],
                ),
                cse_op(
                    4,
                    PcodeOpcode::IntRight,
                    hi_zext.clone(),
                    vec![zext_wide, Varnode::constant(8, 4)],
                ),
                ret(5, hi_zext),
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::CallOther,
                    address: 0x1010,
                    output: Some(co_out),
                    inputs: vec![co_in.clone()],
                    asm_mnemonic: None,
                },
                cse_op(
                    7,
                    PcodeOpcode::IntAnd,
                    rax.clone(),
                    vec![masked_rax, Varnode::constant(0xFF, 8)],
                ),
                ret(8, al),
            ],
        }],
    };
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);

    // IntSExt sign bit (bit 7 = 0x80) is consumed when upper byte of wide output is read.
    assert_eq!(tracker.get_consume_mask(&b_sext) & 0x80, 0x80);
    // IntZExt does NOT consume input sign bit when only bits >= 8 of output are read.
    assert_eq!(tracker.get_consume_mask(&b_zext) & 0x80, 0);
    // CallOther consumes its inputs even when its own output is unused.
    assert_eq!(tracker.get_consume_mask(&co_in), u64::MAX);
    // Sub-register read `al` forces overlapping `rax` write to stay consumed (not folded to 0).
    assert_ne!(tracker.get_consume_mask(&rax), 0);
}

#[test]
fn try_and_mask_skips_zero_fold_when_consume_mask_is_zero() {
    let rules = crate::pcode::optimizer::rules::OptimizationRules::new();
    let input = cse_vn(0x10, 4);
    let unused_out = cse_vn(0x100, 4);
    let func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![cse_op(
                0,
                PcodeOpcode::IntAnd,
                unused_out.clone(),
                vec![input, Varnode::constant(0x0F, 4)],
            )],
        }],
    };
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    assert_eq!(tracker.get_consume_mask(&unused_out), 0);
    assert!(
        rules
            .try_optimize_with_tracker(&func.blocks[0].ops[0], &tracker, &func)
            .is_none(),
        "try_and_mask must not rewrite a dead IntAnd to #0 when consume == 0"
    );
}

#[test]
fn subvariable_flow_masks_narrowed_const_and_allocates_disjoint_unique_temps() {
    let mut pass = SubvariableFlowOptimizer::new();
    let a = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let out = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let low = Varnode {
        space_id: crate::space::UNIQUE_SPACE_ID,
        offset: 0x30,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    // Narrow `out = (a + a) + 0x1234_5678` where only low byte is consumed via SubPiece.
    let sum_reg = Varnode {
        space_id: crate::space::REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                add_op(0, &sum_reg, &a, &a),
                cse_op(
                    1,
                    PcodeOpcode::IntOr,
                    out.clone(),
                    vec![sum_reg, Varnode::constant(0x1234_5678, 4)],
                ),
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1008,
                    output: Some(low.clone()),
                    inputs: vec![out.clone(), Varnode::constant(0, 4)],
                    asm_mnemonic: None,
                },
                ret(3, low),
            ],
        }],
    };
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    assert!(pass.eliminate(&mut func, &tracker));

    // Check narrowed constant had both `offset` and `constant_val` truncated to 1 byte (0x78).
    let const_in = func.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.inputs.iter())
        .find(|vn| vn.is_constant && vn.size == 1)
        .expect("narrowed 1-byte constant");
    assert_eq!(const_in.offset, 0x78);
    assert_eq!(const_in.constant_val, 0x78);

    // Check the two `SubPiece(a, 0)` ops received disjoint unique-space outputs, never `REGISTER_SPACE_ID`.
    let sp_outs: Vec<&Varnode> = func.blocks[0]
        .ops
        .iter()
        .filter(|op| op.opcode == PcodeOpcode::SubPiece && op.inputs[0] == a)
        .filter_map(|op| op.output.as_ref())
        .collect();
    assert_eq!(sp_outs.len(), 2);
    assert_ne!(sp_outs[0].offset, sp_outs[1].offset);
    assert!(crate::space::is_unique_space_id(sp_outs[0].space_id));
    assert!(crate::space::is_unique_space_id(sp_outs[1].space_id));
}

#[test]
fn cse_clears_available_exprs_across_intra_block_cbranch() {
    let mut cse = CommonSubexpressionEliminator::new();
    let a = cse_vn(0x10, 4);
    let b = cse_vn(0x14, 4);
    let out1 = cse_vn(0x100, 4);
    let out2 = cse_vn(0x104, 4);
    let cond = cse_vn(0x200, 1);

    let mut func = PcodeFunction {
        blocks: vec![crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                cse_op(0, PcodeOpcode::IntAdd, out1, vec![a.clone(), b.clone()]),
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x1004,
                    output: None,
                    inputs: vec![Varnode::constant(0x1010, 8), cond],
                    asm_mnemonic: None,
                },
                cse_op(2, PcodeOpcode::IntAdd, out2, vec![a, b]),
            ],
        }],
    };

    assert!(
        !cse.eliminate(&mut func),
        "intra-block CBranch must clear CSE availability"
    );
    assert_eq!(func.blocks[0].ops[2].opcode, PcodeOpcode::IntAdd);
}

// ---- RAM is not a temporary space ------------------------------------------
//
// The Rust-Sleigh runtime numbers registers 4/5, temporaries 2 and RAM 3; the
// legacy numbering puts temporaries in 3. Treating every space-3 varnode as a
// temporary forwards a global's stored value into its reload, after which
// nothing reads the global and DCE deletes the store.

fn ram_vn(offset: u64) -> Varnode {
    Varnode {
        space_id: 3,
        offset,
        size: 4,
        is_constant: false,
        constant_val: 0,
    }
}

fn assert_copy_kept(ops: &[PcodeOp], output: &Varnode, input: &Varnode) {
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Copy
            && op.output.as_ref() == Some(output)
            && op.inputs.as_slice() == std::slice::from_ref(input)),
        "`{output:?} = COPY {input:?}` must survive: {ops:?}"
    );
}

/// `mov [g], eax; mov ecx, [g]` lifted with the runtime numbering: the store
/// to `ram:g` and the reload from it are memory, not a temporary and its use.
#[test]
fn a_ram_store_and_its_reload_survive_copy_propagation_and_dce() {
    let reg = |offset| Varnode {
        space_id: crate::space::RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let (eax, ecx, global) = (reg(0x0), reg(0x8), ram_vn(0x404000));
    let ops = optimize_ops(vec![
        cse_op(0, PcodeOpcode::Copy, global.clone(), vec![eax.clone()]),
        cse_op(1, PcodeOpcode::Copy, ecx.clone(), vec![global.clone()]),
        ret(2, ecx.clone()),
    ]);
    assert_copy_kept(&ops, &global, &eax);
    assert_copy_kept(&ops, &ecx, &global);
}

/// With no register in either numbering, nothing says whether space 3 is RAM
/// or unique, so no varnode is a temporary.
#[test]
fn space_three_is_not_a_temporary_without_numbering_evidence() {
    let (source, global, sink) = (ram_vn(0x404000), ram_vn(0x404010), ram_vn(0x404020));
    let ops = optimize_ops(vec![
        cse_op(0, PcodeOpcode::Copy, global.clone(), vec![source.clone()]),
        cse_op(1, PcodeOpcode::Copy, sink.clone(), vec![global.clone()]),
        ret(2, sink.clone()),
    ]);
    assert_copy_kept(&ops, &global, &source);
    assert_copy_kept(&ops, &sink, &global);
}
