use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{Execution, Export};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_export(
        &mut self,
        export: &Export,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut emitted = Vec::new();
        match export {
            Export::Value(expr) => {
                let _ = self.lower_expr(
                    expr,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    &mut emitted,
                )?;
            }
            Export::Reference { addr, memory } => {
                let _ = self.lower_expr(
                    addr,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    &mut emitted,
                )?;
                let _ = memory;
            }
            Export::AttachVarnode {
                location,
                attach_value,
                attach_id,
            } => {
                let _ = location;
                let attach = sleigh.attach_varnode(*attach_id);
                if attach.0.len() == 1 {
                    let _ = self.varnode_from_sleigh(sleigh, attach.0[0].1)?;
                } else {
                    let dynamic_index =
                        self.resolve_dynamic_value_index(*attach_value, sleigh)?;
                    let varnode_id = attach
                        .find_value(dynamic_index)
                        .context("Missing attach-varnode value for export dynamic index")?;
                    let _ = self.varnode_from_sleigh(sleigh, varnode_id)?;
                }
            }
            Export::Table { location, table_id } => {
                let _ = location;
                let _ = self.ensure_table_export_slot(
                    *table_id,
                    current_address,
                    sleigh,
                    &mut emitted,
                )?;
            }
        }
        Ok(emitted)
    }

    pub(super) fn ensure_table_export_slot(
        &mut self,
        table_id: sleigh_rs::TableId,
        current_address: u64,
        sleigh: &Sleigh,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        if let Some(cached) = self.table_export_values.get(&table_id) {
            return Ok(cached.clone());
        }

        let export_size = sleigh
            .table(table_id)
            .export
            .map(|export| Self::bits_to_bytes(export.len().get()))
            .transpose()
            .context("Invalid table export length")?
            .unwrap_or(1);

        let slot = self.table_export_varnode(table_id, export_size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::Copy,
            address: current_address,
            output: Some(slot.clone()),
            inputs: vec![Varnode::constant(0, export_size)],
            asm_mnemonic: Some("TABLE_EXPORT_INIT".to_string()),
        });
        self.table_export_values.insert(table_id, slot.clone());
        Ok(slot)
    }
}
