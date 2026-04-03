use anyhow::{Context, Result};
use fission_pcode::Varnode;
use sleigh_rs::execution::DynamicValueType;
use sleigh_rs::{ContextId, TokenFieldId};
use sleigh_rs::{Number, Sleigh};

use super::IRConverter;

impl IRConverter {
    fn bit_from_storage(storage: &[u8], bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *storage.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }

    pub(super) fn read_lsb_bits(
        &self,
        storage: &[u8],
        bit_start: u64,
        bit_len: u64,
        source_name: &str,
    ) -> Result<u64> {
        if bit_len > 64 {
            anyhow::bail!(
                "Dynamic value extraction above 64 bits is unsupported: source={}, len={}",
                source_name,
                bit_len
            );
        }

        let storage_bits = (storage.len() as u64)
            .checked_mul(8)
            .context("Storage bit size overflow")?;
        let end = bit_start
            .checked_add(bit_len)
            .context("Dynamic value bit range overflow")?;
        if end > storage_bits {
            anyhow::bail!(
                "Dynamic value out of bounds: source={}, start={}, len={}, storage_bits={}",
                source_name,
                bit_start,
                bit_len,
                storage_bits
            );
        }

        let mut out = 0u64;
        for i in 0..bit_len {
            let bit_index = usize::try_from(bit_start + i)
                .context("Bit index does not fit usize")?;
            let bit = Self::bit_from_storage(storage, bit_index)
                .context("Missing bit while extracting dynamic value")?;
            if bit {
                out |= 1u64 << i;
            }
        }
        Ok(out)
    }

    fn token_field_bit_start(
        &self,
        sleigh: &Sleigh,
        token_field_id: TokenFieldId,
    ) -> Result<u64> {
        let token_field = sleigh.token_field(token_field_id);
        let mut offset_bits = 0u64;
        for token in sleigh.tokens().iter().take(token_field.token.0) {
            let token_bits = token
                .len_bytes
                .get()
                .checked_mul(8)
                .context("Token bit size overflow")?;
            offset_bits = offset_bits
                .checked_add(token_bits)
                .context("Token offset overflow")?;
        }
        offset_bits
            .checked_add(token_field.bits.start())
            .context("Token field bit start overflow")
    }

    pub(super) fn token_field_raw_value(
        &self,
        sleigh: &Sleigh,
        token_field_id: TokenFieldId,
    ) -> Result<u64> {
        let token_field = sleigh.token_field(token_field_id);
        let bit_start = self.token_field_bit_start(sleigh, token_field_id)?;
        let bit_len = token_field.bits.len().get();
        self.read_lsb_bits(
            &self.instruction_bytes,
            bit_start,
            bit_len,
            "token-field",
        )
    }

    pub(super) fn context_raw_value(
        &self,
        sleigh: &Sleigh,
        context_id: ContextId,
    ) -> Result<u64> {
        let mapped_bits = sleigh.context_memory().context(context_id);
        self.read_lsb_bits(
            &self.context_bits,
            mapped_bits.start(),
            mapped_bits.len().get(),
            "context",
        )
    }

    pub(super) fn resolve_dynamic_value_index(
        &self,
        value_id: DynamicValueType,
        sleigh: &Sleigh,
    ) -> Result<usize> {
        let raw = match value_id {
            DynamicValueType::TokenField(token_id) => {
                self.token_field_raw_value(sleigh, token_id)?
            }
            DynamicValueType::Context(context_id) => {
                self.context_raw_value(sleigh, context_id)?
            }
        };
        usize::try_from(raw)
            .context("Dynamic attach index does not fit usize")
    }

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

    pub(super) fn table_export_varnode(
        &self,
        id: sleigh_rs::TableId,
        size: u32,
    ) -> Varnode {
        Varnode {
            space_id: 1,
            offset: 0x2_0000_0000u64 + (id.0 as u64),
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
