#[cfg(test)]
mod tests {
    use crate::pcode::graph::PcodeGraph;
    use crate::pcode::optimizer::DefUseTracker;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    #[test]
    fn test_dot_generation() {
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![],
                    ops: vec![
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
                            inputs: vec![Varnode::constant(10, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Branch,
                            address: 0x1004,
                            output: None,
                            inputs: vec![Varnode::constant(0x1010, 8)], // Target address
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x1010,
                        output: None,
                        inputs: vec![],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let mut tracker = DefUseTracker::new();
        tracker.build(&func);

        let dot = PcodeGraph::to_dot(&func, Some(&tracker));

        println!("{}", dot);

        assert!(dot.contains("digraph PcodeFunction"));
        assert!(dot.contains("cluster_block_0"));
        assert!(dot.contains("cluster_block_1"));
        // Opcode formatting is Debug trait, so it might be "Copy" not "COPY"
        // And label format is "u_100:4\nNZ:A = Copy(#0xA)"
        assert!(dot.contains("Copy(#0xA)"));
        assert!(dot.contains("->")); // Edges
    }

    #[test]
    fn to_dot_omits_fallthrough_after_return_and_branchind_and_handles_ram_branch() {
        let reg = Varnode {
            space_id: 1,
            offset: 0,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let ram_target = Varnode {
            space_id: 3,
            offset: 0x1020,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1000,
                        output: None,
                        inputs: vec![ram_target],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::BranchInd,
                        address: 0x1010,
                        output: None,
                        inputs: vec![reg.clone()],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x1020,
                        output: None,
                        inputs: vec![reg.clone()],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1030,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1030,
                        output: Some(Varnode {
                            space_id: 3,
                            offset: 0x100,
                            size: 8,
                            is_constant: false,
                            constant_val: 0,
                        }),
                        inputs: vec![reg],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let dot = PcodeGraph::to_dot(&func, None);
        assert!(
            dot.contains("op_0_0 -> op_2_0;"),
            "missing non-constant code-space branch edge in dot:\n{dot}"
        );
        assert!(
            !dot.contains("op_1_0 -> op_2_0"),
            "BranchInd must not emit fallthrough edge in dot:\n{dot}"
        );
        assert!(
            !dot.contains("op_2_0 -> op_3_0"),
            "Return must not emit fallthrough edge in dot:\n{dot}"
        );
    }
}
