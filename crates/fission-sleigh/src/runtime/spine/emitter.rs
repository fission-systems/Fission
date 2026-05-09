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

    /// Ghidra `PcodeEmit.setUniqueOffset(Address)` — seed for unique temporaries at `pcode_address`.
    #[inline]
    pub fn unique_seed_for_address(unique_mask: u64, pcode_address: u64) -> u64 {
        (pcode_address & unique_mask).wrapping_shl(8)
    }

    /// Temporarily switch the pcode instruction address and unique temp allocator (cross-build / delay slot).
    pub fn with_emit_context<R>(
        &mut self,
        pcode_address: u64,
        unique_next_tmp: u64,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved_addr = self.address;
        let saved_tmp = self.next_tmp;
        self.address = pcode_address;
        self.next_tmp = unique_next_tmp;
        let out = body(self);
        self.address = saved_addr;
        self.next_tmp = saved_tmp;
        out
    }

    pub fn emit_context(&self) -> (u64, u64) {
        (self.address, self.next_tmp)
    }

    pub fn set_emit_context(&mut self, pcode_address: u64, next_tmp: u64) {
        self.address = pcode_address;
        self.next_tmp = next_tmp;
    }

    pub fn finish(self) -> Vec<PcodeOp> {
        self.ops
    }

    pub fn op_count(&self) -> Result<u32> {
        u32::try_from(self.ops.len()).map_err(|_| anyhow!("p-code op count overflowed"))
    }

    pub fn tmp(&mut self, space_id: u64, size: u32) -> Result<Varnode> {
        let vn = Varnode {
            space_id,
            offset: self.next_tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next_tmp = self
            .next_tmp
            .checked_add(0x200)
            .ok_or_else(|| anyhow!("unique temporary offset overflowed"))?;
        Ok(vn)
    }

    pub fn append_checked(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) -> Result<()> {
        let next_seq = self
            .seq
            .checked_add(1)
            .ok_or_else(|| anyhow!("p-code seq_num overflowed"))?;
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
        self.seq = next_seq;
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

    pub fn emit_call_ind(&mut self, target: Varnode, mnemonic: &str) -> Result<()> {
        self.append_checked(PcodeOpcode::CallInd, None, vec![target], mnemonic)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_varnode(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 3,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn const_varnode(value: i64, size: u32) -> Varnode {
        Varnode::constant(value, size)
    }

    #[test]
    fn emitter_fails_on_seq_num_overflow() {
        let mut emitter = RuntimePcodeEmitter::new(0x1000, 0);
        emitter.seq = u32::MAX;

        let err = emitter
            .append_checked(
                PcodeOpcode::Copy,
                Some(unique_varnode(1, 1)),
                vec![const_varnode(2, 1)],
                "seq",
            )
            .expect_err("seq overflow must fail closed");

        assert!(err.to_string().contains("p-code seq_num overflowed"));
    }

    #[test]
    fn emitter_fails_on_unique_temp_overflow() {
        let mut emitter = RuntimePcodeEmitter::new(0x1000, u64::MAX - 0xff);
        let err = emitter
            .tmp(3, 8)
            .expect_err("temporary overflow must fail closed");

        assert!(err
            .to_string()
            .contains("unique temporary offset overflowed"));
    }

    #[test]
    fn emitter_source_has_no_wrapping_or_saturating_allocators() {
        let source = include_str!("emitter.rs");
        let tmp_wrap = ["next_tmp", "wrapping_add"].join(".");
        let seq_saturating = ["seq", "saturating_add"].join(".");

        assert!(
            !source.contains(&tmp_wrap) && !source.contains(&seq_saturating),
            "p-code emission must not hide seq or unique temp overflow"
        );
    }
}
