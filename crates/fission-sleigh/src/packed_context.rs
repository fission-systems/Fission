//! SLEIGH context registers.
//!
//! A SLEIGH language declares its context register as an array of 32-bit words
//! (`ContextDatabase` sizes it from the largest context field). Word `i` holds
//! bits `32 * i ..= 32 * i + 31`, numbered from the most significant bit of
//! the word, the way Ghidra numbers them. Languages with vector or mode state
//! (JVM, ARM, MIPS, PowerPC, RISC-V) use more than two words.
//!
//! [`ContextWords`] holds [`CONTEXT_WORD_CAP`] words. Every access outside
//! that range is an error, never a silent no-op, so a language that needs
//! more words fails loudly instead of decoding with a truncated register.

use anyhow::{anyhow, bail, Result};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Number of 32-bit context words a [`ContextWords`] holds (256 bits).
pub const CONTEXT_WORD_CAP: usize = 8;

/// A context register, or a mask over one, as 32-bit words.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub struct ContextWords([u32; CONTEXT_WORD_CAP]);

impl ContextWords {
    pub const ZERO: Self = Self([0; CONTEXT_WORD_CAP]);

    /// Words 0 and 1 from `bits` (word 0 is the low half); the rest are zero.
    pub const fn from_low_u64(bits: u64) -> Self {
        let mut words = [0; CONTEXT_WORD_CAP];
        words[0] = bits as u32;
        words[1] = (bits >> 32) as u32;
        Self(words)
    }

    /// Words 0 and 1 as one integer. Lossy for wider registers: it is only for
    /// diagnostics and for callers that have checked [`Self::fits_u64`].
    pub const fn low_u64(&self) -> u64 {
        (self.0[0] as u64) | ((self.0[1] as u64) << 32)
    }

    /// Whether every word above word 1 is zero.
    pub fn fits_u64(&self) -> bool {
        self.0[2..].iter().all(|word| *word == 0)
    }

    /// Bits `offset..offset + 64` of the register read as one little-endian
    /// integer (word 0 least significant), zero-filled past the last word.
    /// Used by legacy constraints that address context by bit position from
    /// the low end.
    pub fn shr_low_u64(&self, offset: u32) -> Result<u64> {
        let total_bits = (CONTEXT_WORD_CAP * 32) as u32;
        if offset >= total_bits {
            bail!("context shift {offset} exceeds the {total_bits}-bit context register");
        }
        let word = (offset / 32) as usize;
        let bit = offset % 32;
        let at = |index: usize| u64::from(self.0.get(index).copied().unwrap_or(0));
        let low = at(word) | (at(word + 1) << 32);
        let value = low >> bit;
        Ok(if bit == 0 {
            value
        } else {
            value | (at(word + 2) << (64 - bit))
        })
    }

    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    pub const fn words(&self) -> &[u32; CONTEXT_WORD_CAP] {
        &self.0
    }

    pub fn word(&self, index: u32) -> Result<u32> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.0.get(index))
            .copied()
            .ok_or_else(|| out_of_range(index))
    }

    /// Replace the bits of word `index` selected by `mask` with those of `value`.
    pub fn set_word(&mut self, index: u32, value: u32, mask: u32) -> Result<()> {
        let word = usize::try_from(index)
            .ok()
            .and_then(|index| self.0.get_mut(index))
            .ok_or_else(|| out_of_range(index))?;
        *word = (*word & !mask) | (value & mask);
        Ok(())
    }

    /// Write `bitsize` (1..=64) bits of `value` at `startbit`, which may span words.
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
            word_index = word_index
                .checked_add(1)
                .ok_or_else(|| anyhow!("packed context word index overflows"))?;
            bit_offset = 0;
        }
        Ok(())
    }
}

fn out_of_range(index: u32) -> anyhow::Error {
    anyhow!("packed context word index {index} is out of range (this runtime keeps {CONTEXT_WORD_CAP} 32-bit context words)")
}

/// `{:x}` shows every word, most significant first, so wide registers are not truncated in traces.
impl std::fmt::LowerHex for ContextWords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, word) in self.0.iter().rev().enumerate() {
            if i > 0 {
                f.write_str("_")?;
            }
            write!(f, "{word:08x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PackedContextOverride {
    context: ContextWords,
    mask: ContextWords,
}

impl PackedContextOverride {
    /// An override of the low 64 context bits.
    pub const fn new(context_bits: u64, mask_bits: u64) -> Self {
        Self {
            context: ContextWords::from_low_u64(context_bits),
            mask: ContextWords::from_low_u64(mask_bits),
        }
    }

    pub const fn from_words(context: ContextWords, mask: ContextWords) -> Self {
        Self { context, mask }
    }

    pub const fn context(&self) -> ContextWords {
        self.context
    }

    pub const fn mask(&self) -> ContextWords {
        self.mask
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
        self.mask.set_word(word_index, mask, mask)
    }

    pub fn merge_override(self, pending: Self) -> Self {
        let mut context = self.context;
        let mut mask = self.mask;
        for index in 0..CONTEXT_WORD_CAP {
            let pending_mask = pending.mask.0[index];
            context.0[index] =
                (context.0[index] & !pending_mask) | (pending.context.0[index] & pending_mask);
            mask.0[index] |= pending_mask;
        }
        Self { context, mask }
    }

    pub fn apply_to(self, context_register: &mut ContextWords, known_mask: &mut ContextWords) {
        for index in 0..CONTEXT_WORD_CAP {
            let mask = self.mask.0[index];
            context_register.0[index] =
                (context_register.0[index] & !mask) | (self.context.0[index] & mask);
            known_mask.0[index] |= mask;
        }
    }
}

pub fn packed_context_word(context_register: ContextWords, index: u32) -> Result<u32> {
    context_register.word(index)
}

pub fn set_packed_context_word(
    context_register: &mut ContextWords,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    context_register.set_word(index, value, mask)
}

pub fn set_packed_context_bits(
    context_register: &mut ContextWords,
    startbit: u32,
    bitsize: u32,
    value: u64,
) -> Result<()> {
    context_register.set_bits(startbit, bitsize, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_context_word_write_matches_ghidra_bit_numbering() {
        let mut context = ContextWords::ZERO;
        set_packed_context_word(&mut context, 0, 1u32 << 31, 1u32 << 31).expect("set context word");
        assert_eq!(context.low_u64(), 0x8000_0000);
    }

    #[test]
    fn packed_context_bit_write_crosses_word_boundaries() {
        let mut context = ContextWords::ZERO;
        set_packed_context_bits(&mut context, 31, 2, 0b11).expect("set cross-word bits");
        assert_eq!(packed_context_word(context, 0).expect("word 0") & 1, 1);
        assert_eq!(
            packed_context_word(context, 1).expect("word 1") & 0x8000_0000,
            0x8000_0000
        );
    }

    #[test]
    fn words_beyond_the_old_u64_limit_are_stored() {
        let mut context = ContextWords::ZERO;
        // JVM's `in_lookup_switch`: 2 bits at bit 98, which is word 3.
        set_packed_context_bits(&mut context, 98, 2, 0b11).expect("set word 3 bits");
        assert_eq!(context.word(3).expect("word 3"), 0x3000_0000);
        assert_eq!(context.word(2).expect("word 2"), 0);
        assert!(!context.fits_u64());
        assert_eq!(context.low_u64(), 0);
    }

    #[test]
    fn a_field_spanning_words_two_and_three_round_trips() {
        let mut context = ContextWords::ZERO;
        set_packed_context_bits(&mut context, 90, 12, 0xabc).expect("set");
        // Bits 90..=101: 6 bits in word 2 (offset 26), 6 bits in word 3.
        assert_eq!(context.word(2).expect("word 2") & 0x3f, 0xabc >> 6);
        assert_eq!(context.word(3).expect("word 3") >> 26, 0xabc & 0x3f);
    }

    #[test]
    fn access_past_the_last_word_is_an_error_not_a_no_op() {
        let mut context = ContextWords::ZERO;
        let past = CONTEXT_WORD_CAP as u32;
        assert!(context.word(past).is_err());
        assert!(context.set_word(past, 1, 1).is_err());
        assert!(context.set_bits(past * 32, 1, 1).is_err());
        assert!(context.is_zero());
    }

    #[test]
    fn packed_context_override_merge_uses_pending_mask() {
        let base = PackedContextOverride::new(0b1010, 0b1111);
        let pending = PackedContextOverride::new(0b0101, 0b0011);
        let merged = base.merge_override(pending);

        assert_eq!(merged.context().low_u64(), 0b1001);
        assert_eq!(merged.mask().low_u64(), 0b1111);
    }

    #[test]
    fn packed_context_override_merge_covers_high_words() {
        let mut base = PackedContextOverride::default();
        base.merge_commit_word(3, 0xf000_0000, 0x5000_0000)
            .expect("base word 3");
        let mut pending = PackedContextOverride::default();
        pending
            .merge_commit_word(3, 0x3000_0000, 0x3000_0000)
            .expect("pending word 3");
        let merged = base.merge_override(pending);
        assert_eq!(merged.context().word(3).expect("word 3"), 0x7000_0000);
        assert_eq!(merged.mask().word(3).expect("mask word 3"), 0xf000_0000);
    }

    #[test]
    fn packed_context_override_commit_word_merges_checked_word() {
        let mut context_override = PackedContextOverride::new(0, 0);
        context_override
            .merge_commit_word(1, 0x8000_0000, 0x8000_0000)
            .expect("merge high context word");

        assert_eq!(context_override.context().low_u64(), 0x8000_0000_0000_0000);
        assert_eq!(context_override.mask().low_u64(), 0x8000_0000_0000_0000);
        assert!(context_override
            .merge_commit_word(CONTEXT_WORD_CAP as u32, 1, 1)
            .is_err());
    }

    #[test]
    fn apply_to_overlays_only_masked_bits_in_every_word() {
        let mut context = ContextWords::ZERO;
        context.set_word(5, 0xffff_ffff, 0xffff_ffff).unwrap();
        let mut known = ContextWords::ZERO;
        let mut over = PackedContextOverride::default();
        over.merge_commit_word(5, 0x0000_00ff, 0x0000_0012).unwrap();
        over.apply_to(&mut context, &mut known);
        assert_eq!(context.word(5).unwrap(), 0xffff_ff12);
        assert_eq!(known.word(5).unwrap(), 0x0000_00ff);
    }
}
