use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{
    Assignment, AssignmentOp, AssignmentWrite, AssignmentWriteVariable,
    DynamicValueType, Execution,
};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_assignment(
        &mut self,
        assign: &Assignment,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();
        let rhs = self.lower_expr(
            &assign.right,
            current_address,
            next_address,
            next2_address,
            sleigh,
            execution,
            &mut ops,
        )?;

        match &assign.var {
            AssignmentWrite::Variable { value, op } => {
                let rhs = match op {
                    Some(op) => {
                        self.apply_assignment_op(op, rhs, current_address, &mut ops)?
                    }
                    None => rhs,
                };
                match value {
                    AssignmentWriteVariable::Bitrange(id) => {
                        let bitrange = sleigh.bitrange(*id);
                        let container =
                            self.varnode_from_sleigh(sleigh, bitrange.varnode)?;
                        let bit_start = bitrange.bits.start();
                        let bit_len = bitrange.bits.len().get();
                        self.assign_to_bitrange_target(
                            container,
                            bit_start,
                            bit_len,
                            rhs,
                            current_address,
                            &mut ops,
                        )?;
                    }
                    AssignmentWriteVariable::DynVarnode { value_id, attach_id } => {
                        let out = self.resolve_dynamic_varnode_target(
                            *value_id,
                            *attach_id,
                            sleigh,
                        )?;
                        ops.push(PcodeOp {
                            seq_num: self.take_seq(),
                            opcode: PcodeOpcode::Copy,
                            address: current_address,
                            output: Some(out),
                            inputs: vec![rhs],
                            asm_mnemonic: Some("ASSIGN".to_string()),
                        });
                    }
                    _ => {
                        let out = self.lower_assignment_target(value, sleigh, execution)?;
                        ops.push(PcodeOp {
                            seq_num: self.take_seq(),
                            opcode: PcodeOpcode::Copy,
                            address: current_address,
                            output: Some(out),
                            inputs: vec![rhs],
                            asm_mnemonic: Some("ASSIGN".to_string()),
                        });
                    }
                }
                Ok(ops)
            }
            AssignmentWrite::Memory { mem, addr } => {
                let addr_vn =
                    self.lower_expr(
                        addr,
                        current_address,
                        next_address,
                        next2_address,
                        sleigh,
                        execution,
                        &mut ops,
                    )?;
                self.emit_store(mem, addr_vn, rhs, current_address, &mut ops);
                Ok(ops)
            }
            AssignmentWrite::TableExport { table_id, op, size } => {
                let rhs = match op {
                    Some(op) => {
                        self.apply_assignment_op(op, rhs, current_address, &mut ops)?
                    }
                    None => rhs,
                };

                let requested_size = if let Some(bytes) = size {
                    u32::try_from(bytes.get())
                        .context("Table-export assignment size does not fit u32")?
                } else if let Some(export) = sleigh.table(*table_id).export {
                    Self::bits_to_bytes(export.len().get())
                        .context("Invalid table-export size from Sleigh metadata")?
                } else {
                    rhs.size
                };

                let rhs = self.normalize_varnode_size(
                    rhs,
                    requested_size,
                    current_address,
                    &mut ops,
                    "ASSIGN_TABLE_EXPORT_RESIZE",
                );

                let out = self.table_export_varnode(*table_id, rhs.size);
                self.table_export_values.insert(*table_id, out.clone());
                ops.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::Copy,
                    address: current_address,
                    output: Some(out),
                    inputs: vec![rhs],
                    asm_mnemonic: Some("ASSIGN_TABLE_EXPORT".to_string()),
                });
                Ok(ops)
            }
        }
    }

    fn resolve_dynamic_varnode_target(
        &self,
        value_id: DynamicValueType,
        attach_id: sleigh_rs::AttachVarnodeId,
        sleigh: &Sleigh,
    ) -> Result<Varnode> {
        let attach = sleigh.attach_varnode(attach_id);
        if attach.0.len() == 1 {
            return self.varnode_from_sleigh(sleigh, attach.0[0].1);
        }

        let dynamic_index = self.resolve_dynamic_value_index(value_id, sleigh)?;
        if let Some(varnode_id) = attach.find_value(dynamic_index) {
            return self.varnode_from_sleigh(sleigh, varnode_id);
        }

        anyhow::bail!(
            "Unable to resolve dynamic varnode assignment target: value={:?}, attach={:?}, index={}",
            value_id,
            attach_id,
            dynamic_index
        )
    }

    fn normalize_varnode_size(
        &mut self,
        input: Varnode,
        out_size: u32,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
        mnemonic: &str,
    ) -> Varnode {
        if input.size == out_size {
            return input;
        }

        let out = self.make_temp_varnode(self.next_seq, out_size);
        let (opcode, inputs) = if input.size < out_size {
            (PcodeOpcode::IntZExt, vec![input])
        } else {
            (PcodeOpcode::SubPiece, vec![input, Varnode::constant(0, 4)])
        };

        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode,
            address: current_address,
            output: Some(out.clone()),
            inputs,
            asm_mnemonic: Some(mnemonic.to_string()),
        });
        out
    }

    fn assign_to_bitrange_target(
        &mut self,
        container: Varnode,
        bit_start: u64,
        bit_len: u64,
        rhs: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<()> {
        if bit_len == 0 {
            anyhow::bail!("Bitrange assignment target cannot have zero width");
        }

        let container_bits = u64::from(container.size) * 8;
        if bit_start.saturating_add(bit_len) > container_bits {
            anyhow::bail!(
                "Bitrange assignment exceeds container width: start={}, len={}, container_bits={}",
                bit_start,
                bit_len,
                container_bits
            );
        }

        if container.size > 8 {
            anyhow::bail!(
                "Bitrange assignment over varnodes larger than 8 bytes is unsupported"
            );
        }

        if bit_start == 0 && bit_len == container_bits {
            let rhs_full = self.normalize_varnode_size(
                rhs,
                container.size,
                current_address,
                emitted,
                "ASSIGN_BITRANGE_DIRECT_RESIZE",
            );
            emitted.push(PcodeOp {
                seq_num: self.take_seq(),
                opcode: PcodeOpcode::Copy,
                address: current_address,
                output: Some(container),
                inputs: vec![rhs_full],
                asm_mnemonic: Some("ASSIGN_BITRANGE_DIRECT".to_string()),
            });
            return Ok(());
        }

        if bit_len > 63 {
            anyhow::bail!(
                "Bitrange assignment widths above 63 bits are unsupported"
            );
        }

        let target_size = Self::bits_to_bytes(bit_len)
            .context("Invalid bitrange target size")?;

        let mut rhs_bits = self.normalize_varnode_size(
            rhs,
            target_size,
            current_address,
            emitted,
            "ASSIGN_BITRANGE_TARGET_RESIZE",
        );

        if bit_len % 8 != 0 {
            let low_mask = ((1u64 << bit_len) - 1) as i64;
            let masked = self.make_temp_varnode(self.next_seq, target_size);
            emitted.push(PcodeOp {
                seq_num: self.take_seq(),
                opcode: PcodeOpcode::IntAnd,
                address: current_address,
                output: Some(masked.clone()),
                inputs: vec![rhs_bits, Varnode::constant(low_mask, target_size)],
                asm_mnemonic: Some("ASSIGN_BITRANGE_INPUT_MASK".to_string()),
            });
            rhs_bits = masked;
        }

        let mut rhs_container = self.normalize_varnode_size(
            rhs_bits,
            container.size,
            current_address,
            emitted,
            "ASSIGN_BITRANGE_CONTAINER_RESIZE",
        );

        if bit_start != 0 {
            let shifted = self.make_temp_varnode(self.next_seq, container.size);
            emitted.push(PcodeOp {
                seq_num: self.take_seq(),
                opcode: PcodeOpcode::IntLeft,
                address: current_address,
                output: Some(shifted.clone()),
                inputs: vec![rhs_container, Varnode::constant(bit_start as i64, 4)],
                asm_mnemonic: Some("ASSIGN_BITRANGE_SHIFT".to_string()),
            });
            rhs_container = shifted;
        }

        let field_mask = ((1u64 << bit_len) - 1) << bit_start;
        let clear_mask = (!field_mask) as i64;
        let cleared = self.make_temp_varnode(self.next_seq, container.size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::IntAnd,
            address: current_address,
            output: Some(cleared.clone()),
            inputs: vec![container.clone(), Varnode::constant(clear_mask, container.size)],
            asm_mnemonic: Some("ASSIGN_BITRANGE_CLEAR".to_string()),
        });

        let merged = self.make_temp_varnode(self.next_seq, container.size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::IntOr,
            address: current_address,
            output: Some(merged.clone()),
            inputs: vec![cleared, rhs_container],
            asm_mnemonic: Some("ASSIGN_BITRANGE_MERGE".to_string()),
        });

        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::Copy,
            address: current_address,
            output: Some(container),
            inputs: vec![merged],
            asm_mnemonic: Some("ASSIGN_BITRANGE_WRITEBACK".to_string()),
        });

        Ok(())
    }

    pub(super) fn lower_assignment_target(
        &self,
        target: &AssignmentWriteVariable,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Varnode> {
        match target {
            AssignmentWriteVariable::Varnode(id) => self.varnode_from_sleigh(sleigh, *id),
            AssignmentWriteVariable::Variable(id) => {
                let bits = execution.variable(*id).len_bits.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid assignment variable size")?;
                Ok(self.execution_varnode(*id, size))
            }
            _ => anyhow::bail!(
                "Unsupported assignment target in converter MVP: {:?}",
                target
            ),
        }
    }

    pub(super) fn apply_assignment_op(
        &mut self,
        op: &AssignmentOp,
        rhs: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match op {
            AssignmentOp::TakeLsb(len) => {
                let out_size = u32::try_from(len.get())
                    .context("Assignment TakeLsb size does not fit u32")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![rhs, Varnode::constant(0, 4)],
                    asm_mnemonic: Some("ASSIGN_TAKELSB".to_string()),
                });
                Ok(out)
            }
            AssignmentOp::TrunkLsb(trunk) => {
                let trunk_bytes = u32::try_from(*trunk)
                    .context("Assignment TrunkLsb does not fit u32")?;
                let out_size = rhs.size.saturating_sub(trunk_bytes).max(1);
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![rhs, Varnode::constant((*trunk) as i64, 4)],
                    asm_mnemonic: Some("ASSIGN_TRUNKLSB".to_string()),
                });
                Ok(out)
            }
            AssignmentOp::BitRange(range) => {
                let bits = range.end.saturating_sub(range.start);
                if bits == 0 {
                    anyhow::bail!("Assignment BitRange cannot have zero width");
                }
                self.extract_bit_range(
                    rhs,
                    range.clone(),
                    bits,
                    current_address,
                    emitted,
                )
            }
        }
    }
}
