use super::*;
pub(super) use crate::packed_context::{
    packed_context_word, set_packed_context_bits, set_packed_context_word,
};

#[derive(Debug, Clone)]
pub(super) struct CompiledInstructionContext<'a> {
    pub(super) inner: RuntimeInstructionContext<'a>,
    pub(super) instruction_cursor: usize,
    pub(super) context_register: u64,
    pub(super) context_known_mask: u64,
}

impl<'a> std::ops::Deref for CompiledInstructionContext<'a> {
    type Target = RuntimeInstructionContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> std::ops::DerefMut for CompiledInstructionContext<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> CompiledInstructionContext<'a> {
    pub(super) fn parse(bytes: &'a [u8], address: u64) -> Result<Self> {
        if bytes.is_empty() {
            bail!("empty compiled-table decode buffer");
        }
        let cursor = 0usize;
        let instruction_width_profile = 1;
        Ok(Self {
            inner: RuntimeInstructionContext::new(
                bytes,
                address,
                cursor,
                instruction_width_profile,
            ),
            instruction_cursor: cursor,
            context_register: 0,
            context_known_mask: 0,
        })
    }
}

pub(super) fn packed_context_bytes(
    context_register: u64,
    bytestart: u32,
    bytesize: u32,
) -> Result<u32> {
    if bytesize == 0 || bytesize > 4 {
        bail!("packed context byte read must be 1..=4 bytes, got {bytesize}");
    }
    let mut intstart = bytestart / 4;
    let mut res = packed_context_word(context_register, intstart)?;
    let byte_offset = bytestart % 4;
    let mut unused_bytes = 4 - bytesize;
    res <<= byte_offset * 8;
    res >>= unused_bytes * 8;
    let remaining = cross_word_remainder(bytesize, 4, byte_offset, "packed context byte read")?;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bytes = checked_remainder_u32(remaining, "packed context byte read")?;
        unused_bytes = 4 - unused_bytes;
        res2 >>= unused_bytes * 8;
        res |= res2;
    }
    Ok(res)
}

pub(super) fn packed_context_bits(
    context_register: u64,
    startbit: u32,
    bitsize: u32,
) -> Result<u32> {
    if bitsize == 0 {
        return Ok(0);
    }
    if bitsize > 32 {
        bail!("packed context bit read must be 1..=32 bits, got {bitsize}");
    }
    let mut intstart = startbit / 32;
    let mut res = packed_context_word(context_register, intstart)?;
    let bit_offset = startbit % 32;
    let mut unused_bits = 32 - bitsize;
    res <<= bit_offset;
    res >>= unused_bits;
    let remaining = cross_word_remainder(bitsize, 32, bit_offset, "packed context bit read")?;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bits = checked_remainder_u32(remaining, "packed context bit read")?;
        unused_bits = 32 - unused_bits;
        res2 >>= unused_bits;
        res |= res2;
    }
    Ok(res)
}

fn cross_word_remainder(width: u32, word_width: u32, offset: u32, role: &str) -> Result<i32> {
    let width = i32::try_from(width).map_err(|_| anyhow!("{role} width exceeds i32"))?;
    let word_width =
        i32::try_from(word_width).map_err(|_| anyhow!("{role} word width exceeds i32"))?;
    let offset = i32::try_from(offset).map_err(|_| anyhow!("{role} offset exceeds i32"))?;
    width
        .checked_sub(word_width)
        .and_then(|value| value.checked_add(offset))
        .ok_or_else(|| anyhow!("{role} cross-word remainder overflowed"))
}

fn checked_remainder_u32(remaining: i32, role: &str) -> Result<u32> {
    u32::try_from(remaining).map_err(|_| anyhow!("{role} remainder {remaining} is negative"))
}

#[cfg(test)]
mod tests {
    use super::{checked_remainder_u32, cross_word_remainder};

    #[test]
    fn packed_context_remainder_helpers_are_checked() {
        assert_eq!(cross_word_remainder(2, 4, 3, "test").unwrap(), 1);
        assert_eq!(cross_word_remainder(1, 4, 0, "test").unwrap(), -3);
        assert_eq!(checked_remainder_u32(1, "test").unwrap(), 1);
        assert!(checked_remainder_u32(-1, "test").is_err());
    }
}
