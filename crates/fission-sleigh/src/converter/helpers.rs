use anyhow::{Context, Result};
use fission_pcode::Varnode;
use sleigh_rs::{Number, Sleigh};

use super::IRConverter;

impl IRConverter {
    pub(super) fn varnode_from_sleigh(
        &self,
        sleigh: &Sleigh,
        id: sleigh_rs::VarnodeId,
    ) -> Result<Varnode> {
        let vn = sleigh.varnode(id);
        let size = u32::try_from(vn.len_bytes.get())
            .context("Sleigh varnode size does not fit u32")?;
        Ok(Varnode {
            space_id: vn.space.0 as u64,
            offset: vn.address,
            size,
            is_constant: false,
            constant_val: 0,
        })
    }

    pub(super) fn execution_varnode(
        &self,
        id: sleigh_rs::execution::VariableId,
        size: u32,
    ) -> Varnode {
        Varnode {
            space_id: 1,
            offset: 0x1_0000_0000u64 + (id.0 as u64),
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    pub(super) fn make_temp_varnode(&self, seq: u32, size: u32) -> Varnode {
        Varnode {
            space_id: 1,
            offset: u64::from(seq),
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    pub(super) fn take_seq(&mut self) -> u32 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }

    pub(super) fn bits_to_bytes(bits: u64) -> Result<u32> {
        if bits == 0 {
            return Ok(1);
        }
        let bytes = bits.div_ceil(8);
        u32::try_from(bytes).context("Byte width does not fit u32")
    }

    pub(super) fn number_to_i64(number: Number) -> i64 {
        let signed = number.signed_super();
        if signed > i64::MAX as i128 {
            i64::MAX
        } else if signed < i64::MIN as i128 {
            i64::MIN
        } else {
            signed as i64
        }
    }

}
