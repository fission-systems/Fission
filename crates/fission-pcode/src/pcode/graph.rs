//! Graphviz DOT exporter for Pcode
//!
//! Generates a DOT graph representation of the Pcode function,
//! including control flow and data flow information.

use crate::pcode::optimizer::DefUseTracker;
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::fmt::Write;

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

pub struct PcodeGraph;

impl PcodeGraph {
    /// Generate DOT graph for a Pcode function
    pub fn to_dot(func: &PcodeFunction, def_use: Option<&DefUseTracker>) -> String {
        let mut dot = String::new();
        let _ = writeln!(dot, "digraph PcodeFunction {{");
        let _ = writeln!(dot, "  node [shape=box, fontname=\"Courier New\"];");
        let _ = writeln!(dot, "  rankdir=TB;");

        // Generate nodes for blocks
        for block in &func.blocks {
            let _ = writeln!(dot, "  subgraph cluster_block_{} {{", block.index);
            let _ = writeln!(
                dot,
                "    label=\"Block {} @ 0x{:X}\";",
                block.index, block.start_address
            );
            let _ = writeln!(dot, "    style=filled;");
            let _ = writeln!(dot, "    color=lightgrey;");

            // Generate nodes for operations
            for op in &block.ops {
                let op_id = format!("op_{}_{}", block.index, op.seq_num);
                let label = Self::format_op_label(op, def_use);

                let color = match op.opcode {
                    PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Return => "#ffcccc", // Control flow: Light Red

                    PcodeOpcode::Load | PcodeOpcode::Store => "#ccffcc", // Memory: Light Green

                    PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => "#ffffff", // Move/Cast: White

                    _ => "#ccccff", // Arithmetic/Logic: Light Blue
                };

                let _ = writeln!(
                    dot,
                    "    {} [label=\"{}\", style=filled, fillcolor=\"{}\"];",
                    op_id, label, color
                );

                // Connect operations within block (sequential flow)
                // This is implicit in the cluster, but we can add invisible edges to enforce order
            }

            // Connect ops sequentially
            for i in 0..block.ops.len().saturating_sub(1) {
                let op1 = &block.ops[i];
                let op2 = &block.ops[i + 1];
                let _ = writeln!(
                    dot,
                    "    op_{}_{} -> op_{}_{} [style=invis];",
                    block.index, op1.seq_num, block.index, op2.seq_num
                );
            }

            let _ = writeln!(dot, "  }}");
        }

        // Generate edges for control flow
        // Since PcodeFunction structure here is flat blocks, we need to infer edges from Branch/CBranch
        // Or if the PcodeFunction had explicit edges.
        // Assuming we can infer from the last op of each block.

        for block in &func.blocks {
            if let Some(last_op) = block.ops.last() {
                match last_op.opcode {
                    PcodeOpcode::Branch | PcodeOpcode::CBranch => {
                        // Target is usually the first input (const address)
                        if !last_op.inputs.is_empty() && last_op.inputs[0].is_constant {
                            let target_addr = last_op.inputs[0].constant_val as u64;
                            // Find target block
                            if let Some(target_block) =
                                func.blocks.iter().find(|b| b.start_address == target_addr)
                            {
                                let src_id = format!("op_{}_{}", block.index, last_op.seq_num);
                                // Target is the first op of target block
                                if let Some(first_op) = target_block.ops.first() {
                                    let dst_id =
                                        format!("op_{}_{}", target_block.index, first_op.seq_num);
                                    let label = if last_op.opcode == PcodeOpcode::CBranch {
                                        " [label=\"True\"]"
                                    } else {
                                        ""
                                    };
                                    let _ = writeln!(dot, "    {} -> {}{};", src_id, dst_id, label);
                                }
                            }
                        }

                        // For CBranch, there is also a fallthrough
                        if last_op.opcode == PcodeOpcode::CBranch {
                            // Fallthrough to next block index?
                            // This requires knowing block order. Assuming blocks are sorted by index.
                            if let Some(next_block) =
                                func.blocks.iter().find(|b| b.index == block.index + 1)
                            {
                                let src_id = format!("op_{}_{}", block.index, last_op.seq_num);
                                if let Some(first_op) = next_block.ops.first() {
                                    let dst_id =
                                        format!("op_{}_{}", next_block.index, first_op.seq_num);
                                    let _ = writeln!(
                                        dot,
                                        "    {} -> {} [label=\"False\", style=dashed];",
                                        src_id, dst_id
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        // Fallthrough for non-branching ops at end of block
                        if let Some(next_block) =
                            func.blocks.iter().find(|b| b.index == block.index + 1)
                        {
                            let src_id = format!("op_{}_{}", block.index, last_op.seq_num);
                            if let Some(first_op) = next_block.ops.first() {
                                let dst_id =
                                    format!("op_{}_{}", next_block.index, first_op.seq_num);
                                let _ =
                                    writeln!(dot, "    {} -> {} [style=dashed];", src_id, dst_id);
                            }
                        }
                    }
                }
            }
        }

        // Optional: Data flow edges (Def-Use)
        // This can make the graph very messy, so maybe make it optional or different style
        if let Some(tracker) = def_use {
            // Draw data flow edges
            // From Def op to Use op
            // We need to iterate all varnodes
            // But tracker is indexed by VarnodeId.
            // We can iterate blocks/ops and query tracker.

            for block in &func.blocks {
                for op in &block.ops {
                    // For each input, find its definition
                    for input in &op.inputs {
                        if !input.is_constant {
                            if let Some(def_op_ref) = tracker.get_def(input) {
                                // Find the defining op
                                // We need seq_num for the ID.
                                // OpRef has block_idx and op_idx (index in vec)
                                if let Some(def_block) = func.blocks.get(def_op_ref.block_idx) {
                                    if let Some(def_op) = def_block.ops.get(def_op_ref.op_idx) {
                                        let src_id =
                                            format!("op_{}_{}", def_block.index, def_op.seq_num);
                                        let dst_id = format!("op_{}_{}", block.index, op.seq_num);

                                        // Use a different color for data flow
                                        let _ = writeln!(
                                            dot,
                                            "    {} -> {} [color=blue, constraint=false, style=dotted];",
                                            src_id, dst_id
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = writeln!(dot, "}}");
        dot
    }

    fn format_op_label(op: &PcodeOp, def_use: Option<&DefUseTracker>) -> String {
        let mut s = String::new();

        // Assembly instruction (if available)
        if let Some(ref asm) = op.asm_mnemonic {
            let _ = write!(s, "[0x{:X}] {}\\l", op.address, asm);
        } else {
            let _ = write!(s, "[0x{:X}]\\l", op.address);
        }

        // Output
        if let Some(out) = &op.output {
            let _ = write!(s, "{}", Self::format_varnode(out));
            if let Some(tracker) = def_use {
                let mask = tracker.get_nz_mask(out);
                if mask != u64::MAX {
                    let _ = write!(s, "\\nNZ:{:X}", mask);
                }
            }
            let _ = write!(s, " = ");
        }

        // Opcode
        let _ = write!(s, "{:?}", op.opcode);

        // Inputs
        if !op.inputs.is_empty() {
            let _ = write!(s, "(");
            for (i, input) in op.inputs.iter().enumerate() {
                if i > 0 {
                    let _ = write!(s, ", ");
                }
                let _ = write!(s, "{}", Self::format_varnode(input));
            }
            let _ = write!(s, ")");
        }

        s
    }

    fn format_varnode(vn: &Varnode) -> String {
        if vn.is_constant {
            format!("#0x{:X}", vn.constant_val)
        } else {
            // Format based on space
            match vn.space_id {
                0 => format!("const_0x{:X}", vn.offset), // Should be handled by is_constant
                1 => format!("u_{:X}:{}", vn.offset, vn.size), // Unique
                2 => format!("r_{:X}:{}", vn.offset, vn.size), // Register
                3 => format!("m_{:X}:{}", vn.offset, vn.size), // Memory (RAM)
                _ => format!("s{}_{:X}:{}", vn.space_id, vn.offset, vn.size),
            }
        }
    }
}
