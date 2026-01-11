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
}
