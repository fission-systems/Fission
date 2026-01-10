//! IL Decoder
//!
//! Functions for parsing method headers and decoding operands

use super::types::{OpCodeDef, OperandType};
use crate::dotnet::{DotNetError, DotNetResult};

/// Parse method body header (tiny or fat format) and return (code_start, code_size).
pub(super) fn parse_body_header(data: &[u8]) -> DotNetResult<(usize, usize)> {
    let first = *data
        .first()
        .ok_or_else(|| DotNetError::Malformed("Empty method body".into()))?;
    match first & 0x3 {
        0x2 => {
            // Tiny format: 1-byte header, code size encoded in upper 6 bits.
            let code_size = (first >> 2) as usize;
            Ok((1, code_size))
        }
        0x3 => {
            // Fat format
            if data.len() < 12 {
                return Err(DotNetError::Malformed(
                    "Fat method header shorter than 12 bytes".into(),
                ));
            }
            let flags_and_size = u16::from_le_bytes([data[0], data[1]]);
            let header_size_dwords = (flags_and_size >> 12) as usize;
            if header_size_dwords < 3 {
                return Err(DotNetError::Malformed("Invalid fat header size".into()));
            }
            let code_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
            let code_start = header_size_dwords * 4;
            if code_start + code_size > data.len() {
                return Err(DotNetError::Malformed(
                    "Method body extends past available data".into(),
                ));
            }
            Ok((code_start, code_size))
        }
        _ => Err(DotNetError::Malformed(
            "Unknown method header format".into(),
        )),
    }
}

/// Decode instruction operand based on opcode definition.
pub(super) fn decode_operand(
    op: &OpCodeDef,
    code: &[u8],
    cursor: &mut usize,
    _instr_start: usize,
) -> DotNetResult<Option<String>> {
    let read_u8 = |buf: &[u8], offset: &mut usize| -> DotNetResult<u8> {
        let byte = *buf
            .get(*offset)
            .ok_or_else(|| DotNetError::Malformed("Unexpected end of IL stream".into()))?;
        *offset += 1;
        Ok(byte)
    };

    let read_i8 =
        |buf: &[u8], offset: &mut usize| -> DotNetResult<i8> { Ok(read_u8(buf, offset)? as i8) };

    let read_u16 = |buf: &[u8], offset: &mut usize| -> DotNetResult<u16> {
        let b0 = read_u8(buf, offset)? as u16;
        let b1 = read_u8(buf, offset)? as u16;
        Ok(b0 | (b1 << 8))
    };

    let read_u32 = |buf: &[u8], offset: &mut usize| -> DotNetResult<u32> {
        let b0 = read_u8(buf, offset)? as u32;
        let b1 = read_u8(buf, offset)? as u32;
        let b2 = read_u8(buf, offset)? as u32;
        let b3 = read_u8(buf, offset)? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    };

    let read_i32 =
        |buf: &[u8], offset: &mut usize| -> DotNetResult<i32> { Ok(read_u32(buf, offset)? as i32) };

    let read_f32 = |buf: &[u8], offset: &mut usize| -> DotNetResult<f32> {
        let bits = read_u32(buf, offset)?;
        Ok(f32::from_bits(bits))
    };

    let read_f64 = |buf: &[u8], offset: &mut usize| -> DotNetResult<f64> {
        let lo = read_u32(buf, offset)? as u64;
        let hi = read_u32(buf, offset)? as u64;
        Ok(f64::from_bits(lo | (hi << 32)))
    };

    let operand = match op.operand {
        OperandType::InlineNone => None,
        OperandType::ShortInlineI => Some((read_i8(code, cursor)? as i32).to_string()),
        OperandType::InlineI => Some(read_i32(code, cursor)?.to_string()),
        OperandType::InlineI8 => {
            let low = read_u32(code, cursor)? as u64;
            let high = read_u32(code, cursor)? as u64;
            let val = (high << 32) | low;
            Some((val as i64).to_string())
        }
        OperandType::ShortInlineR => Some(format!("{:.6}", read_f32(code, cursor)?)),
        OperandType::InlineR => Some(format!("{:.6}", read_f64(code, cursor)?)),
        OperandType::InlineMethod
        | OperandType::InlineField
        | OperandType::InlineType
        | OperandType::InlineTok
        | OperandType::InlineString
        | OperandType::InlineSig => Some(format!("0x{:08X}", read_u32(code, cursor)?)),
        OperandType::InlineVar => Some(read_u16(code, cursor)?.to_string()),
        OperandType::ShortInlineVar => Some(read_u8(code, cursor)?.to_string()),
        OperandType::ShortInlineBrTarget => {
            let rel = read_i8(code, cursor)? as isize;
            let base = *cursor as isize;
            let target = (base + rel).max(0) as usize;
            Some(format!("IL_{target:04X}"))
        }
        OperandType::InlineBrTarget => {
            let rel = read_i32(code, cursor)? as isize;
            let base = *cursor as isize;
            let target = (base + rel).max(0) as usize;
            Some(format!("IL_{target:04X}"))
        }
        OperandType::InlineSwitch => {
            let count = read_u32(code, cursor)? as usize;
            let mut targets = Vec::new();
            let base = (*cursor + count * 4) as isize;
            for _ in 0..count {
                let delta = read_i32(code, cursor)? as isize;
                let dst = (base + delta).max(0) as usize;
                targets.push(format!("IL_{dst:04X}"));
            }
            Some(format!("[{}]", targets.join(", ")))
        }
    };

    Ok(operand)
}
