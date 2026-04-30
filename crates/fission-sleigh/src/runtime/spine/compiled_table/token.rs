pub(super) fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
    let end = offset + size as usize;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow!("missing immediate bytes"))?;
    let mut value = 0u64;
    for (index, byte) in slice.iter().enumerate() {
        value |= u64::from(*byte) << (index * 8);
    }
    Ok(value)
}

pub(super) fn read_sint(bytes: &[u8], offset: usize, size: u32) -> Result<i64> {
    let value = read_uint(bytes, offset, size)?;
    let bits = size * 8;
    if bits == 64 {
        Ok(i64::from_ne_bytes(value.to_ne_bytes()))
    } else {
        let shift = 64 - bits;
        Ok(((value << shift) as i64) >> shift)
    }
}

pub(super) fn constructor_replaces_current(constructor: &CompiledExecutableConstructor) -> bool {
    constructor
        .constructor_template
        .decode_steps
        .iter()
        .any(|step| {
            matches!(
                step,
                CompiledOperandDecodeStep::DescendSubtable {
                    replace_current: true,
                    ..
                }
            )
        })
}

pub(super) fn read_sla_token_field(
    ctx: &CompiledInstructionContext<'_>,
    big_endian: bool,
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
) -> Result<u64> {
    read_sla_token_field_at(
        ctx, ctx.cursor, big_endian, sign_bit, bit_start, bit_end, byte_start, byte_end, shift,
    )
}

pub(super) fn read_sla_token_field_at(
    ctx: &CompiledInstructionContext<'_>,
    base_cursor: usize,
    big_endian: bool,
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
) -> Result<u64> {
    let size = byte_end.saturating_sub(byte_start) + 1;
    let mut res = 0u64;
    for idx in 0..size {
        let off = if big_endian {
            byte_start + idx
        } else {
            byte_end.saturating_sub(idx)
        } as usize;
        let byte = *ctx
            .bytes
            .get(base_cursor + off)
            .ok_or_else(|| anyhow!("tokenfield byte {} out of range", off))?;
        res = (res << 8) | u64::from(byte);
    }
    let shifted = if shift >= 0 {
        res >> (shift as u32)
    } else {
        res << ((-shift) as u32)
    };
    let width = bit_end.saturating_sub(bit_start) + 1;
    Ok(if sign_bit {
        sign_extend_bits(shifted, width)
    } else {
        zero_extend_bits(shifted, width)
    })
}

pub(super) fn matcher_instruction_length(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|c| match c {
                crate::compiler::PatternConstraint::Instruction { offset, .. } => {
                    Some(*offset as usize + 1)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0),
    }
}

pub(super) fn zero_extend_bits(value: u64, bit: u32) -> u64 {
    if bit >= 64 {
        value
    } else {
        let mask = (1u64 << bit) - 1;
        value & mask
    }
}

pub(super) fn sign_extend_bits(value: u64, bit: u32) -> u64 {
    if bit == 0 || bit >= 64 {
        return value;
    }
    let sign_mask = 1u64 << (bit - 1);
    let value = zero_extend_bits(value, bit);
    if value & sign_mask != 0 {
        value | (!0u64 << bit)
    } else {
        value
    }
}
