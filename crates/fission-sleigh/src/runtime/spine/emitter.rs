use anyhow::{anyhow, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::runtime::RuntimeSleighError;

#[derive(Debug, Clone)]
pub struct RuntimePcodeEmitter {
    address: u64,
    seq: u32,
    next_tmp: u64,
    ops: Vec<PcodeOp>,
}

impl RuntimePcodeEmitter {
    pub fn new(address: u64, unique_seed: u64) -> Self {
        Self {
            address,
            seq: 0,
            next_tmp: unique_seed,
            ops: Vec::new(),
        }
    }

    pub fn finish(self) -> Vec<PcodeOp> {
        self.ops
    }

    pub fn tmp(&mut self, space_id: u64, size: u32) -> Varnode {
        let vn = Varnode {
            space_id,
            offset: self.next_tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next_tmp = self.next_tmp.wrapping_add(0x200);
        vn
    }

    pub fn append_checked(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) -> Result<()> {
        let op = PcodeOp {
            seq_num: self.seq,
            opcode,
            address: self.address,
            output,
            inputs,
            asm_mnemonic: Some(mnemonic.to_string()),
        };
        op.validate_shape().map_err(|err| {
            anyhow!(RuntimeSleighError::InvalidPcodeShape {
                language: "compiled-table".to_string(),
                reason: err.to_string(),
            })
        })?;
        self.ops.push(op);
        self.seq = self.seq.saturating_add(1);
        Ok(())
    }

    pub fn emit_copy(&mut self, out: Varnode, input: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::Copy, Some(out), vec![input], mnemonic)
    }

    pub fn emit_load(
        &mut self,
        out: Varnode,
        space: Varnode,
        ptr: Varnode,
        mnemonic: &str,
    ) -> Result<()> {
        self.append_checked(PcodeOpcode::Load, Some(out), vec![space, ptr], mnemonic)
    }

    pub fn emit_store(
        &mut self,
        space: Varnode,
        ptr: Varnode,
        value: Varnode,
        mnemonic: &str,
    ) -> Result<()> {
        self.append_checked(PcodeOpcode::Store, None, vec![space, ptr, value], mnemonic)
    }

    pub fn emit_branch(&mut self, target: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::Branch, None, vec![target], mnemonic)
    }

    pub fn emit_cbranch(&mut self, target: Varnode, cond: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::CBranch, None, vec![target, cond], mnemonic)
    }

    pub fn emit_branch_ind(&mut self, target: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::BranchInd, None, vec![target], mnemonic)
    }

    pub fn emit_call(&mut self, target: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::Call, None, vec![target], mnemonic)
    }

    pub fn emit_return(&mut self, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::Return, None, Vec::new(), mnemonic)
    }

    pub fn emit_return_target(&mut self, target: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::Return, None, vec![target], mnemonic)
    }

    pub fn emit_int_unop(
        &mut self,
        opcode: PcodeOpcode,
        out: Varnode,
        input: Varnode,
        mnemonic: &str,
    ) -> Result<()> {
        self.append_checked(opcode, Some(out), vec![input], mnemonic)
    }

    pub fn emit_int_binop(
        &mut self,
        opcode: PcodeOpcode,
        out: Varnode,
        left: Varnode,
        right: Varnode,
        mnemonic: &str,
    ) -> Result<()> {
        self.append_checked(opcode, Some(out), vec![left, right], mnemonic)
    }

    pub fn emit_callother(
        &mut self,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) -> Result<()> {
        self.append_checked(PcodeOpcode::CallOther, output, inputs, mnemonic)
    }
}
