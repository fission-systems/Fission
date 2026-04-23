use anyhow::{anyhow, bail, Result};

use crate::runtime::spine::RuntimeInstructionContext;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RexPrefix {
    pub(crate) w: bool,
    pub(crate) r: bool,
    pub(crate) x: bool,
    pub(crate) b: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct X86InstructionContext<'a> {
    inner: RuntimeInstructionContext<'a>,
    pub(crate) rex: RexPrefix,
}

impl<'a> std::ops::Deref for X86InstructionContext<'a> {
    type Target = RuntimeInstructionContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> X86InstructionContext<'a> {
    pub(crate) fn parse(bytes: &'a [u8], address: u64) -> Result<Self> {
        if bytes.is_empty() {
            bail!("empty x86-64 decode buffer");
        }
        let mut cursor = 0usize;
        let mut operand_size_override = false;
        let mut rex = RexPrefix {
            w: false,
            r: false,
            x: false,
            b: false,
        };
        while cursor < bytes.len() {
            match bytes[cursor] {
                0x66 => {
                    operand_size_override = true;
                    cursor += 1;
                }
                0x67 | 0xF0 | 0xF2 | 0xF3 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => {
                    cursor += 1;
                }
                value @ 0x40..=0x4F => {
                    rex = RexPrefix {
                        w: value & 0x08 != 0,
                        r: value & 0x04 != 0,
                        x: value & 0x02 != 0,
                        b: value & 0x01 != 0,
                    };
                    cursor += 1;
                }
                _ => break,
            }
        }
        let operand_size_code = if rex.w {
            2
        } else if operand_size_override {
            0
        } else {
            1
        };
        Ok(Self {
            inner: RuntimeInstructionContext::new(bytes, address, cursor, operand_size_code),
            rex,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModRm {
    pub(crate) mod_bits: u8,
    pub(crate) reg: u8,
    pub(crate) rm: u8,
    pub(crate) base: Option<u8>,
    pub(crate) index: Option<u8>,
    pub(crate) scale: u8,
    pub(crate) displacement: i64,
    pub(crate) rip_relative: bool,
    pub(crate) length: usize,
}

pub(crate) fn candidate_bucket_keys(ctx: &X86InstructionContext<'_>) -> Option<Vec<String>> {
    let first = *ctx.bytes.get(ctx.cursor)?;
    let mut keys = vec![format!("byte_{first:02x}")];
    if first == 0x0f {
        if let Some(second) = ctx.bytes.get(ctx.cursor + 1) {
            keys.push(format!("row_{}_after_0f", second >> 4));
        }
    }
    keys.push(format!("row_{}_page_{}", first >> 4, (first >> 3) & 0x1));
    keys.push(format!("row_{}", first >> 4));
    Some(keys)
}

pub(crate) fn ensure_modrm<'a>(
    ctx: &X86InstructionContext<'_>,
    cached_modrm: &'a mut Option<ModRm>,
) -> Result<&'a ModRm> {
    if cached_modrm.is_none() {
        *cached_modrm = Some(parse_modrm(
            ctx,
            ctx.cursor + opcode_len_from_context(ctx)?,
        )?);
    }
    cached_modrm
        .as_ref()
        .ok_or_else(|| anyhow!("missing cached modrm"))
}

pub(crate) fn opcode_len_from_context(ctx: &X86InstructionContext<'_>) -> Result<usize> {
    let opcode = *ctx
        .bytes
        .get(ctx.cursor)
        .ok_or_else(|| anyhow!("missing opcode byte"))?;
    Ok(if opcode == 0x0f { 2 } else { 1 })
}

pub(crate) fn parse_modrm(ctx: &X86InstructionContext<'_>, offset: usize) -> Result<ModRm> {
    let byte = *ctx
        .bytes
        .get(offset)
        .ok_or_else(|| anyhow!("missing modrm at {offset}"))?;
    let mod_bits = byte >> 6;
    let reg = ((byte >> 3) & 0x7) | ((ctx.rex.r as u8) << 3);
    let rm_low = byte & 0x7;
    let rm = rm_low | ((ctx.rex.b as u8) << 3);
    if mod_bits == 3 {
        return Ok(ModRm {
            mod_bits,
            reg,
            rm,
            base: Some(rm),
            index: None,
            scale: 1,
            displacement: 0,
            rip_relative: false,
            length: 1,
        });
    }

    let mut length = 1usize;
    let mut displacement = 0i64;
    let mut rip_relative = false;
    let mut base = Some(rm);
    let mut index = None;
    let mut scale = 1u8;

    if rm_low == 4 {
        let sib = *ctx
            .bytes
            .get(offset + length)
            .ok_or_else(|| anyhow!("missing sib"))?;
        length += 1;
        scale = 1u8 << (sib >> 6);
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;
        if index_low != 4 {
            index = Some(index_low | ((ctx.rex.x as u8) << 3));
        }
        if mod_bits == 0 && base_low == 5 {
            base = None;
            displacement = read_sint(ctx.bytes, offset + length, 4)?;
            length += 4;
        } else {
            base = Some(base_low | ((ctx.rex.b as u8) << 3));
        }
    } else if mod_bits == 0 && rm_low == 5 {
        base = None;
        rip_relative = true;
        displacement = read_sint(ctx.bytes, offset + length, 4)?;
        length += 4;
    }

    match mod_bits {
        1 => {
            displacement = displacement.wrapping_add(read_sint(ctx.bytes, offset + length, 1)?);
            length += 1;
        }
        2 => {
            displacement = displacement.wrapping_add(read_sint(ctx.bytes, offset + length, 4)?);
            length += 4;
        }
        _ => {}
    }

    Ok(ModRm {
        mod_bits,
        reg,
        rm,
        base,
        index,
        scale,
        displacement,
        rip_relative,
        length,
    })
}

pub(crate) fn read_uint(bytes: &[u8], offset: usize, size: u32) -> Result<u64> {
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

pub(crate) fn read_sint(bytes: &[u8], offset: usize, size: u32) -> Result<i64> {
    let value = read_uint(bytes, offset, size)?;
    let bits = size * 8;
    if bits == 64 {
        Ok(i64::from_ne_bytes(value.to_ne_bytes()))
    } else {
        let shift = 64 - bits;
        Ok(((value << shift) as i64) >> shift)
    }
}
