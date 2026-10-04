use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PackedContext {
    /// Ghidra context register words 0..=3, packed in native word order.
    bits: u128,
}

impl PackedContext {
    pub const fn new(bits: u64) -> Self {
        Self { bits: bits as u128 }
    }

    pub const fn bits(self) -> u64 {
        // Preserve the original low-64-bit accessor for existing callers.
        self.bits as u64
    }

    /// Return all four 32-bit context words.
    pub const fn wide_bits(self) -> u128 {
        self.bits
    }

    pub const fn new_wide(bits: u128) -> Self {
        Self { bits }
    }

    pub fn set_bits(&mut self, startbit: u32, bitsize: u32, value: u64) -> Result<()> {
        set_packed_context_bits(&mut self.bits, startbit, bitsize, value)
    }

    pub fn set_word(&mut self, index: u32, value: u32, mask: u32) -> Result<()> {
        set_packed_context_word(&mut self.bits, index, value, mask)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PackedContextOverride {
    context: PackedContext,
    mask: PackedContext,
}

impl PackedContextOverride {
    pub const fn new(context_bits: u64, mask_bits: u64) -> Self {
        Self {
            context: PackedContext::new(context_bits),
            mask: PackedContext::new(mask_bits),
        }
    }

    pub const fn context_bits(self) -> u64 {
        // Preserve the original low-64-bit accessor for existing callers.
        self.context.bits()
    }

    pub const fn mask_bits(self) -> u64 {
        self.mask.bits()
    }

    /// Return the full 128-bit context value, including words 2 and 3.
    pub const fn context_bits_wide(self) -> u128 {
        self.context.wide_bits()
    }

    /// Return the full 128-bit known-bit mask, including words 2 and 3.
    pub const fn mask_bits_wide(self) -> u128 {
        self.mask.wide_bits()
    }

    pub const fn new_wide(context_bits: u128, mask_bits: u128) -> Self {
        Self {
            context: PackedContext::new_wide(context_bits),
            mask: PackedContext::new_wide(mask_bits),
        }
    }

    pub fn set_bits(&mut self, startbit: u32, bitsize: u32, value: u64) -> Result<()> {
        self.context.set_bits(startbit, bitsize, value)?;
        let known_value = if bitsize >= 64 {
            u64::MAX
        } else if bitsize == 0 {
            0
        } else {
            (1u64 << bitsize) - 1
        };
        self.mask.set_bits(startbit, bitsize, known_value)
    }

    pub fn merge_commit_word(&mut self, word_index: u32, mask: u32, value: u32) -> Result<()> {
        let mask_wide = packed_context_word_to_u128(word_index, mask)?;
        let value_wide = packed_context_word_to_u128(word_index, value)?;
        let context_bits = (self.context_bits_wide() & !mask_wide) | (value_wide & mask_wide);
        let mask_bits = self.mask_bits_wide() | mask_wide;
        *self = Self::new_wide(context_bits, mask_bits);
        Ok(())
    }

    pub const fn merge_override(self, pending: Self) -> Self {
        let pending_mask = pending.mask_bits_wide();
        Self::new_wide(
            (self.context_bits_wide() & !pending_mask)
                | (pending.context_bits_wide() & pending_mask),
            self.mask_bits_wide() | pending_mask,
        )
    }

    pub fn apply_to(self, context_register: &mut u64, known_mask: &mut u64) {
        let mask = self.mask_bits();
        *context_register = (*context_register & !mask) | (self.context_bits() & mask);
        *known_mask |= mask;
    }

    /// Apply the complete four-word context override.
    pub fn apply_to_wide(self, context_register: &mut u128, known_mask: &mut u128) {
        let mask = self.mask_bits_wide();
        *context_register = (*context_register & !mask) | (self.context_bits_wide() & mask);
        *known_mask |= mask;
    }
}

pub fn packed_context_word(context_register: u128, index: u32) -> Result<u32> {
    let shift = index
        .checked_mul(32)
        .ok_or_else(|| anyhow!("packed context word index {index} shift overflows"))?;
    let word = context_register
        .checked_shr(shift)
        .ok_or_else(|| anyhow!("packed context word index {index} is out of range"))?;
    Ok(word as u32)
}

fn packed_context_word_to_u128(word_index: u32, value: u32) -> Result<u128> {
    let shift = word_index
        .checked_mul(32)
        .ok_or_else(|| anyhow!("context commit word index {word_index} shift overflows"))?;
    u128::from(value).checked_shl(shift).ok_or_else(|| {
        anyhow!("context commit word index {word_index} exceeds packed u128 context")
    })
}

pub fn set_packed_context_word(
    context_register: &mut u128,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    let shift = index
        .checked_mul(32)
        .ok_or_else(|| anyhow!("packed context word index {index} shift overflows"))?;
    let shifted_mask = u128::from(mask)
        .checked_shl(shift)
        .ok_or_else(|| anyhow!("packed context word index {index} is out of range"))?;
    let shifted_value = u128::from(value & mask)
        .checked_shl(shift)
        .ok_or_else(|| anyhow!("packed context word index {index} is out of range"))?;
    *context_register &= !shifted_mask;
    *context_register |= shifted_value;
    Ok(())
}

pub fn set_packed_context_bits(
    context_register: &mut u128,
    startbit: u32,
    bitsize: u32,
    value: u64,
) -> Result<()> {
    if bitsize == 0 {
        return Ok(());
    }
    if bitsize > 64 {
        bail!("packed context bit write must be 1..=64 bits, got {bitsize}");
    }
    let end_bit = startbit
        .checked_add(bitsize)
        .ok_or_else(|| anyhow!("packed context bit write range overflows"))?;
    if end_bit > 128 {
        bail!("packed context bit write ends at bit {end_bit}, beyond 128-bit context");
    }

    let mut remaining = bitsize;
    let mut word_index = startbit / 32;
    let mut bit_offset = startbit % 32;
    while remaining > 0 {
        let chunk_bits = remaining.min(32 - bit_offset);
        let chunk_mask = if chunk_bits >= 32 {
            u32::MAX
        } else {
            (1u32 << chunk_bits) - 1
        };
        let word_shift = 32 - chunk_bits - bit_offset;
        let value_shift = remaining - chunk_bits;
        let chunk_value = ((value >> value_shift) as u32) & chunk_mask;
        set_packed_context_word(
            context_register,
            word_index,
            chunk_value << word_shift,
            chunk_mask << word_shift,
        )?;
        remaining -= chunk_bits;
        word_index += 1;
        bit_offset = 0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_context_word_write_matches_ghidra_bit_numbering() {
        let mut context = 0;
        set_packed_context_word(&mut context, 0, 1u32 << 31, 1u32 << 31).expect("set context word");
        assert_eq!(context, 0x8000_0000);
    }

    #[test]
    fn packed_context_bit_write_crosses_word_boundaries() {
        let mut context = 0;
        set_packed_context_bits(&mut context, 31, 2, 0b11).expect("set cross-word bits");
        assert_eq!(packed_context_word(context, 0).expect("word 0") & 1, 1);
        assert_eq!(
            packed_context_word(context, 1).expect("word 1") & 0x8000_0000,
            0x8000_0000
        );
    }

    #[test]
    fn packed_context_supports_four_words_and_fails_closed_after_128_bits() {
        let mut context = 0u128;
        for index in 0..4 {
            let value = 1u32 << 31;
            set_packed_context_word(&mut context, index, value, u32::MAX)
                .expect("write supported context word");
            assert_eq!(
                packed_context_word(context, index).expect("read context word"),
                value
            );
        }
        assert!(set_packed_context_word(&mut context, 4, 1, u32::MAX).is_err());
        assert!(packed_context_word(context, 4).is_err());
    }

    #[test]
    fn packed_context_bit_write_handles_jvm_switch_flags_above_bit_64() {
        let mut context = 0u128;
        set_packed_context_bits(&mut context, 96, 4, 0b1010).expect("write JVM context flags");
        assert_eq!(
            packed_context_word(context, 3).expect("JVM context word 3"),
            0xa000_0000
        );

        let mut override_bits = PackedContextOverride::default();
        override_bits
            .set_bits(96, 4, 0b1010)
            .expect("set JVM context override");
        assert_eq!(override_bits.context_bits_wide(), context);
        assert_eq!(
            override_bits.mask_bits_wide(),
            0xf000_0000_0000_0000_0000_0000_0000_0000
        );
    }

    #[test]
    fn packed_context_override_merge_uses_pending_mask() {
        let base = PackedContextOverride::new(0b1010, 0b1111);
        let pending = PackedContextOverride::new(0b0101, 0b0011);
        let merged = base.merge_override(pending);

        assert_eq!(merged.context_bits(), 0b1001);
        assert_eq!(merged.mask_bits(), 0b1111);
    }

    #[test]
    fn packed_context_override_commit_word_merges_checked_word() {
        let mut context_override = PackedContextOverride::new(0, 0);
        context_override
            .merge_commit_word(1, 0x8000_0000, 0x8000_0000)
            .expect("merge high context word");

        assert_eq!(context_override.context_bits(), 0x8000_0000_0000_0000);
        assert_eq!(context_override.mask_bits(), 0x8000_0000_0000_0000);

        context_override
            .merge_commit_word(3, 0x0000_0001, 0x0000_0001)
            .expect("merge JVM context word");
        assert_ne!(context_override.context_bits_wide() >> 96, 0);
        assert_ne!(context_override.mask_bits_wide() >> 96, 0);
        assert!(context_override.merge_commit_word(4, 1, 1).is_err());
    }
}
