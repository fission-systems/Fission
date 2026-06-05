//! Address-keyed CFG snapshots for Ghidra parity and regression fixtures.

use super::{CfgBuilder, CfgResult, ControlFlowGraph};
use crate::PcodeFunction;
use serde::{Deserialize, Serialize};

/// Directed CFG edge keyed by basic-block start addresses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AddressEdge {
    pub from: u64,
    pub to: u64,
}

/// Canonical CFG snapshot used by parity harnesses and checked-in fixtures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddressCfgSnapshot {
    pub model: String,
    pub function_address: u64,
    pub block_starts: Vec<u64>,
    pub edges: Vec<AddressEdge>,
    pub exit_blocks: Vec<u64>,
}

impl AddressCfgSnapshot {
    pub fn from_pcode_cfg_builder(func: &PcodeFunction) -> CfgResult<Self> {
        let cfg = CfgBuilder::from_pcode(func)?;
        Ok(Self::from_cfg_graph("pcode_cfg_builder", func, &cfg))
    }

    pub fn from_pcode_structuring(func: &PcodeFunction) -> Self {
        Self::from_structuring_edges("pcode_structuring", func)
    }

    pub fn canonicalize(&mut self) {
        self.block_starts.sort_unstable();
        self.block_starts.dedup();
        self.edges.sort_unstable();
        self.edges.dedup();
        self.exit_blocks.sort_unstable();
        self.exit_blocks.dedup();
    }

    fn from_cfg_graph(model: &str, func: &PcodeFunction, cfg: &ControlFlowGraph) -> Self {
        let mut snapshot = Self {
            model: model.to_string(),
            function_address: func
                .blocks
                .first()
                .map(|block| block.start_address)
                .unwrap_or(cfg.function_address),
            block_starts: func.blocks.iter().map(|block| block.start_address).collect(),
            edges: Vec::new(),
            exit_blocks: Vec::new(),
        };

        for block in &cfg.blocks {
            if block.is_exit {
                snapshot.exit_blocks.push(block.start_address);
            }
            for edge in &block.successors {
                let Some(target) = cfg.blocks.get(edge.target) else {
                    continue;
                };
                snapshot.edges.push(AddressEdge {
                    from: block.start_address,
                    to: target.start_address,
                });
            }
        }

        snapshot.canonicalize();
        snapshot
    }

    fn from_structuring_edges(model: &str, func: &PcodeFunction) -> Self {
        let edges = crate::nir::structuring_cfg_edges(func);
        let mut snapshot = Self {
            model: model.to_string(),
            function_address: func
                .blocks
                .first()
                .map(|block| block.start_address)
                .unwrap_or(0),
            block_starts: func.blocks.iter().map(|block| block.start_address).collect(),
            edges,
            exit_blocks: func
                .blocks
                .iter()
                .filter(|block| {
                    block.ops.iter().any(|op| {
                        matches!(
                            op.opcode,
                            crate::PcodeOpcode::Return | crate::PcodeOpcode::BranchInd
                        )
                    })
                })
                .map(|block| block.start_address)
                .collect(),
        };
        snapshot.canonicalize();
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PcodeBasicBlock, PcodeOp, PcodeOpcode, Varnode};

    fn const_vn(val: u64) -> Varnode {
        Varnode {
            space_id: 0,
            offset: val,
            size: 8,
            is_constant: true,
            constant_val: val as i64,
        }
    }

    fn branch_op(address: u64, target: u64) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Branch,
            address,
            output: None,
            inputs: vec![const_vn(target)],
            asm_mnemonic: None,
        }
    }

    fn ret_op(address: u64) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Return,
            address,
            output: None,
            inputs: vec![],
            asm_mnemonic: None,
        }
    }

    #[test]
    fn address_cfg_snapshot_exports_sorted_unique_edges() {
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![],
                    ops: vec![branch_op(0x1000, 0x1020)],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![],
                    ops: vec![branch_op(0x1010, 0x1020)],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![ret_op(0x1020)],
                },
            ],
        };

        let snapshot = AddressCfgSnapshot::from_pcode_cfg_builder(&func).expect("cfg");
        assert_eq!(snapshot.block_starts, vec![0x1000, 0x1010, 0x1020]);
        assert_eq!(
            snapshot.edges,
            vec![
                AddressEdge {
                    from: 0x1000,
                    to: 0x1020
                },
                AddressEdge {
                    from: 0x1010,
                    to: 0x1020
                }
            ]
        );
        assert_eq!(snapshot.exit_blocks, vec![0x1020]);
    }
}
