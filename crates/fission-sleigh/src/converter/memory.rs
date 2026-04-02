use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::MemoryLocation;

use super::IRConverter;

impl IRConverter {
    pub(super) fn make_space_varnode(&self, space_id: usize) -> Varnode {
        Varnode::constant(i64::try_from(space_id).unwrap_or(i64::MAX), 8)
    }

    pub(super) fn emit_store(
        &mut self,
        mem: &MemoryLocation,
        address: Varnode,
        value: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) {
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::Store,
            address: current_address,
            output: None,
            inputs: vec![self.make_space_varnode(mem.space.0), address, value],
            asm_mnemonic: Some("STORE".to_string()),
        });
    }

    pub(super) fn lower_dereference(
        &mut self,
        mem: &MemoryLocation,
        input: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        let out_size =
            u32::try_from(mem.len_bytes.get()).context("Dereference size does not fit u32")?;
        let out = self.make_temp_varnode(self.next_seq, out_size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::Load,
            address: current_address,
            output: Some(out.clone()),
            inputs: vec![self.make_space_varnode(mem.space.0), input],
            asm_mnemonic: Some("UNARY_DEREF".to_string()),
        });
        Ok(out)
    }
}
