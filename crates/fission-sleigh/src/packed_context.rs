use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackedContext {
    words: [u32; 8],
}

impl Default for PackedContext {
    fn default() -> Self {
        Self { words: [0; 8] }
    }
}

impl PackedContext {
    pub const fn new(bits: u64) -> Self {
        Self {
            words: [bits as u32, (bits >> 32) as u32, 0, 0, 0, 0, 0, 0],
        }
    }

    pub const fn from_words(words: [u32; 8]) -> Self {
        Self { words }
    }

    pub const fn words(&self) -> &[u32; 8] {
        &self.words
    }

    pub const fn bits(self) -> u64 {
        (self.words[0] as u64) | ((self.words[1] as u64) << 32)
    }

    pub fn word(&self, index: u32) -> Result<u32> {
        let idx = index as usize;
        if idx < self.words.len() {
            Ok(self.words[idx])
        } else {
            Ok(0)
        }
    }

    pub fn set_bits(&mut self, startbit: u32, bitsize: u32, value: u64) -> Result<()> {
        if bitsize == 0 {
            return Ok(());
        }
        if bitsize > 64 {
            bail!("packed context bit write must be 1..=64 bits, got {bitsize}");
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
            self.set_word(
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

    pub fn set_word(&mut self, index: u32, value: u32, mask: u32) -> Result<()> {
        let idx = index as usize;
        if idx < self.words.len() {
            self.words[idx] = (self.words[idx] & !mask) | (value & mask);
        }
        Ok(())
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

    pub const fn from_contexts(context: PackedContext, mask: PackedContext) -> Self {
        Self { context, mask }
    }

    pub const fn context_bits(self) -> u64 {
        self.context.bits()
    }

    pub const fn mask_bits(self) -> u64 {
        self.mask.bits()
    }

    pub fn context(&self) -> &PackedContext {
        &self.context
    }

    pub fn mask(&self) -> &PackedContext {
        &self.mask
    }

    pub fn word(&self, index: u32) -> Result<u32> {
        self.context.word(index)
    }

    pub fn mask_word(&self, index: u32) -> Result<u32> {
        self.mask.word(index)
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
        self.context.set_word(word_index, value, mask)?;
        self.mask.set_word(word_index, mask, mask)?;
        Ok(())
    }

    pub fn merge_override(self, pending: Self) -> Self {
        let mut context = self.context;
        let mut mask = self.mask;
        for i in 0..8 {
            let p_mask = pending.mask.words[i];
            let p_val = pending.context.words[i];
            context.words[i] = (context.words[i] & !p_mask) | (p_val & p_mask);
            mask.words[i] |= p_mask;
        }
        Self { context, mask }
    }

    pub fn apply_to(self, context_register: &mut u64, known_mask: &mut u64) {
        let mask = self.mask_bits();
        *context_register = (*context_register & !mask) | (self.context_bits() & mask);
        *known_mask |= mask;
    }

    pub fn apply_to_words(self, context_register: &mut [u32; 8], known_mask: &mut [u32; 8]) {
        for i in 0..8 {
            let m = self.mask.words[i];
            context_register[i] = (context_register[i] & !m) | (self.context.words[i] & m);
            known_mask[i] |= m;
        }
    }
}

pub fn packed_context_word(context_register: u64, index: u32) -> Result<u32> {
    match index {
        0 => Ok(context_register as u32),
        1 => Ok((context_register >> 32) as u32),
        _ => Ok(0),
    }
}

pub fn packed_context_word_to_u64(word_index: u32, value: u32) -> Result<u64> {
    let shift = word_index
        .checked_mul(32)
        .ok_or_else(|| anyhow!("context commit word index {word_index} shift overflows"))?;
    u64::from(value)
        .checked_shl(shift)
        .ok_or_else(|| anyhow!("context commit word index {word_index} exceeds packed u64 context"))
}

pub fn set_packed_context_word(
    context_register: &mut u64,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    match index {
        0 => {
            let shifted_mask = u64::from(mask);
            let shifted_value = u64::from(value & mask);
            *context_register &= !shifted_mask;
            *context_register |= shifted_value;
            Ok(())
        }
        1 => {
            let shifted_mask = u64::from(mask) << 32;
            let shifted_value = u64::from(value & mask) << 32;
            *context_register &= !shifted_mask;
            *context_register |= shifted_value;
            Ok(())
        }
        _ => {
            // Words beyond 64-bit cannot be represented in a u64, but must not fail-closed
            // or panic the runtime when multi-word specs execute.
            Ok(())
        }
    }
}

pub fn set_packed_context_bits(
    context_register: &mut u64,
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
    fn packed_context_word_to_u64_fails_closed_above_two_words() {
        assert_eq!(
            packed_context_word_to_u64(1, 0x8000_0000).expect("word 1"),
            0x8000_0000_0000_0000
        );
        assert!(packed_context_word_to_u64(2, 1).is_err());
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
    }

    #[test]
    fn packed_context_override_commit_word_supports_multi_word() {
        let mut context_override = PackedContextOverride::new(0, 0);
        context_override
            .merge_commit_word(2, 0xffff_ffff, 0x1234_5678)
            .expect("merge word index 2");
        assert_eq!(context_override.word(2).expect("word 2"), 0x1234_5678);
        assert_eq!(
            context_override.mask_word(2).expect("mask word 2"),
            0xffff_ffff
        );
        assert_eq!(context_override.word(0).expect("word 0"), 0);
    }

    #[test]
    fn packed_context_word_returns_zero_above_two_words() {
        assert_eq!(packed_context_word(0x1234, 2).expect("word 2"), 0);
        assert_eq!(packed_context_word(0x1234, 3).expect("word 3"), 0);
    }

    #[test]
    fn set_packed_context_word_does_not_fail_closed_above_two_words() {
        let mut ctx = 0u64;
        assert!(set_packed_context_word(&mut ctx, 2, 0x1234, 0xffff).is_ok());
        assert_eq!(ctx, 0);
    }
}
