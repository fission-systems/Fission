/// Tests for Pcode optimizer

use super::*;
use crate::analysis::pcode::{PcodeOp, PcodeOpcode, Varnode};

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
            Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 },
            Varnode::constant(0, 4),
        ],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
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
            Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 },
            Varnode::constant(0, 4),
        ],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
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
            Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 },
            Varnode::constant(0, 4),
        ],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
}

// ===== Tests for RuleTrivialArith =====

#[test]
fn test_trivial_arith_equal() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntEqual,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), vn.clone()],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // true
}

#[test]
fn test_trivial_arith_notequal() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntNotEqual,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), vn.clone()],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

#[test]
fn test_trivial_arith_less() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 4, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::IntLess,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), vn.clone()],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

// ===== Tests for RuleTrivialBool =====

#[test]
fn test_trivial_bool_and_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 1, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolAnd,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert_eq!(optimized.inputs[0], vn);
}

#[test]
fn test_trivial_bool_and_false() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 1, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolAnd,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), Varnode::constant(0, 1)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0); // false
}

#[test]
fn test_trivial_bool_or_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 1, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolOr,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // true
}

#[test]
fn test_trivial_bool_xor_true() {
    let optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    
    let vn = Varnode { space_id: 2, offset: 0x10, size: 1, is_constant: false, constant_val: 0 };
    let op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::BoolXor,
        address: 0x1000,
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![vn.clone(), Varnode::constant(1, 1)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
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
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
        inputs: vec![Varnode::constant(5, 4), Varnode::constant(3, 4)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
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
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
        inputs: vec![Varnode::constant(7, 4), Varnode::constant(6, 4)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
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
        output: Some(Varnode { space_id: 1, offset: 0x100, size: 1, is_constant: false, constant_val: 0 }),
        inputs: vec![Varnode::constant(5, 4), Varnode::constant(10, 4)],
    };
    
    let optimized = optimizer.rules.try_optimize(&op).unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 1); // 5 < 10 is true
}
// ===== Phase 2: Tests for Advanced Rules =====

#[test]
fn test_shift_bitops_left_zero() {
    use crate::analysis::pcode::{PcodeFunction, PcodeBasicBlock};
    
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
                    output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![Varnode::constant(0xf000, 4)],
                },
                // Result = V << 20  (shifts all bits out of 32-bit range)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntLeft,
                    address: 0x1000,
                    output: Some(Varnode { space_id: 1, offset: 0x200, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![
                        Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 },
                        Varnode::constant(20, 4),
                    ],
                },
            ],
        }],
    };
    
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    
    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker);
    
    assert!(result.is_some());
    let optimized = result.unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_shift_bitops_right_zero() {
    use crate::analysis::pcode::{PcodeFunction, PcodeBasicBlock};
    
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
                    output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                },
                // Result = V >> 8  (shifts all bits out)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntRight,
                    address: 0x1000,
                    output: Some(Varnode { space_id: 1, offset: 0x200, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![
                        Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 },
                        Varnode::constant(8, 4),
                    ],
                },
            ],
        }],
    };
    
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    
    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker);
    
    assert!(result.is_some());
    let optimized = result.unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_and_mask_always_zero() {
    use crate::analysis::pcode::{PcodeFunction, PcodeBasicBlock};
    
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
                    output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                },
                // Result = V & 0xf0  (no overlapping bits)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(Varnode { space_id: 1, offset: 0x200, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![
                        Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 },
                        Varnode::constant(0xf0, 4),
                    ],
                },
            ],
        }],
    };
    
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    
    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker);
    
    assert!(result.is_some());
    let optimized = result.unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].constant_val, 0);
}

#[test]
fn test_and_mask_noop() {
    use crate::analysis::pcode::{PcodeFunction, PcodeBasicBlock};
    
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
                    output: Some(Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![Varnode::constant(0x0f, 4)],
                },
                // Result = V & 0xff  (mask doesn't clear any bits)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x1000,
                    output: Some(Varnode { space_id: 1, offset: 0x200, size: 4, is_constant: false, constant_val: 0 }),
                    inputs: vec![
                        Varnode { space_id: 1, offset: 0x100, size: 4, is_constant: false, constant_val: 0 },
                        Varnode::constant(0xff, 4),
                    ],
                },
            ],
        }],
    };
    
    let mut tracker = DefUseTracker::new();
    tracker.build(&func);
    
    let rules = OptimizationRules::new();
    let result = rules.try_optimize_with_tracker(&func.blocks[0].ops[1], &tracker);
    
    assert!(result.is_some());
    let optimized = result.unwrap();
    assert_eq!(optimized.opcode, PcodeOpcode::Copy);
    assert!(!optimized.inputs[0].is_constant);
    assert_eq!(optimized.inputs[0].offset, 0x100); // Should copy V
}