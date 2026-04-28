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

pub(super) fn packed_context_word(context_register: u64, index: u32) -> Result<u32> {
    match index {
        0 => Ok(context_register as u32),
        1 => Ok((context_register >> 32) as u32),
        _ => bail!("packed context word index {index} is out of range"),
    }
}

pub(super) fn set_packed_context_word(
    context_register: &mut u64,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    let shift = match index {
        0 => 0,
        1 => 32,
        _ => bail!("packed context word index {index} is out of range"),
    };
    let shifted_mask = u64::from(mask) << shift;
    let shifted_value = u64::from(value & mask) << shift;
    *context_register &= !shifted_mask;
    *context_register |= shifted_value;
    Ok(())
}

pub(super) fn set_packed_context_bits(
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

pub(super) fn packed_context_bytes(context_register: u64, bytestart: u32, bytesize: u32) -> Result<u32> {
    if bytesize == 0 || bytesize > 4 {
        bail!("packed context byte read must be 1..=4 bytes, got {bytesize}");
    }
    let mut intstart = bytestart / 4;
    let mut res = packed_context_word(context_register, intstart)?;
    let byte_offset = bytestart % 4;
    let mut unused_bytes = 4 - bytesize;
    res <<= byte_offset * 8;
    res >>= unused_bytes * 8;
    let remaining = bytesize as i32 - 4 + byte_offset as i32;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bytes = 4 - remaining as u32;
        res2 >>= unused_bytes * 8;
        res |= res2;
    }
    Ok(res)
}

pub(super) fn packed_context_bits(context_register: u64, startbit: u32, bitsize: u32) -> Result<u32> {
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
    let remaining = bitsize as i32 - 32 + bit_offset as i32;
    if remaining > 0 {
        intstart += 1;
        let mut res2 = packed_context_word(context_register, intstart)?;
        unused_bits = 32 - remaining as u32;
        res2 >>= unused_bits;
        res |= res2;
    }
    Ok(res)
}
