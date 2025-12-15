use crate::analysis::dotnet::{DotNetError, DotNetResult};

/// A single IL instruction with decoded opcode and operand text.
#[derive(Debug, Clone)]
pub struct ILInstruction {
    pub offset: u32,
    pub opcode: String,
    pub operand: Option<String>,
    pub size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperandType {
    InlineNone,
    InlineBrTarget,
    InlineField,
    InlineI,
    InlineI8,
    InlineMethod,
    InlineR,
    InlineSig,
    InlineString,
    InlineSwitch,
    InlineTok,
    InlineType,
    InlineVar,
    ShortInlineBrTarget,
    ShortInlineI,
    ShortInlineR,
    ShortInlineVar,
}

#[derive(Clone, Copy, Debug)]
struct OpCodeDef {
    code: u16,
    name: &'static str,
    operand: OperandType,
    size: u8,
}

/// Simple IL disassembler aimed at readability and compatibility with ildasm-like output.
pub struct IlDisassembler;

impl IlDisassembler {
    pub fn new() -> Self {
        Self
    }

    /// Disassemble a method body starting at the supplied byte slice.
    /// The slice should begin at the method header (tiny or fat format).
    pub fn disassemble(&self, data: &[u8]) -> DotNetResult<Vec<ILInstruction>> {
        let (code_start, code_size) = parse_body_header(data)?;
        let code = data
            .get(code_start..code_start + code_size)
            .ok_or_else(|| DotNetError::Malformed("Method body truncated".into()))?;

        let mut cursor = 0usize;
        let mut result = Vec::new();
        while cursor < code.len() {
            let instr_offset = cursor;
            let opcode_byte = *code
                .get(cursor)
                .ok_or_else(|| DotNetError::Malformed("Unexpected end of IL stream".into()))?;
            cursor += 1;

            let opcode = if opcode_byte == 0xFE {
                let next = *code
                    .get(cursor)
                    .ok_or_else(|| DotNetError::Malformed("Missing two-byte opcode suffix".into()))?;
                cursor += 1;
                0xFE00 | next as u16
            } else {
                opcode_byte as u16
            };

            let op_def = lookup_opcode(opcode).unwrap_or(&UNKNOWN_OPCODE);
            let operand = decode_operand(op_def, code, &mut cursor, instr_offset)?;
            result.push(ILInstruction {
                offset: instr_offset as u32,
                opcode: op_def.name.to_string(),
                operand,
                size: cursor - instr_offset,
            });
        }

        Ok(result)
    }
}

fn parse_body_header(data: &[u8]) -> DotNetResult<(usize, usize)> {
    let first = *data
        .get(0)
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
            let code_size =
                u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
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

fn decode_operand(
    op: &OpCodeDef,
    code: &[u8],
    cursor: &mut usize,
    instr_start: usize,
) -> DotNetResult<Option<String>> {
    let read_u8 = |buf: &[u8], offset: &mut usize| -> DotNetResult<u8> {
        let byte = *buf
            .get(*offset)
            .ok_or_else(|| DotNetError::Malformed("Unexpected end of IL stream".into()))?;
        *offset += 1;
        Ok(byte)
    };

    let read_i8 = |buf: &[u8], offset: &mut usize| -> DotNetResult<i8> {
        Ok(read_u8(buf, offset)? as i8)
    };

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

    let read_i32 = |buf: &[u8], offset: &mut usize| -> DotNetResult<i32> {
        Ok(read_u32(buf, offset)? as i32)
    };

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

fn lookup_opcode(code: u16) -> Option<&'static OpCodeDef> {
    OPCODES.iter().find(|op| op.code == code)
}

const UNKNOWN_OPCODE: OpCodeDef = OpCodeDef {
    code: 0xFFFF,
    name: "???",
    operand: OperandType::InlineNone,
    size: 1,
};

const OPCODES: &[OpCodeDef] = &[
    OpCodeDef { code: 0x0, name: "nop", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1, name: "break", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x2, name: "ldarg.0", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x3, name: "ldarg.1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4, name: "ldarg.2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5, name: "ldarg.3", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6, name: "ldloc.0", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x7, name: "ldloc.1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x8, name: "ldloc.2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9, name: "ldloc.3", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xA, name: "stloc.0", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB, name: "stloc.1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xC, name: "stloc.2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD, name: "stloc.3", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xE, name: "ldarg.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0xF, name: "ldarga.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0x10, name: "starg.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0x11, name: "ldloc.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0x12, name: "ldloca.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0x13, name: "stloc.s", operand: OperandType::ShortInlineVar, size: 1 },
    OpCodeDef { code: 0x14, name: "ldnull", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x15, name: "ldc.i4.m1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x16, name: "ldc.i4.0", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x17, name: "ldc.i4.1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x18, name: "ldc.i4.2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x19, name: "ldc.i4.3", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1A, name: "ldc.i4.4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1B, name: "ldc.i4.5", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1C, name: "ldc.i4.6", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1D, name: "ldc.i4.7", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1E, name: "ldc.i4.8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x1F, name: "ldc.i4.s", operand: OperandType::ShortInlineI, size: 1 },
    OpCodeDef { code: 0x20, name: "ldc.i4", operand: OperandType::InlineI, size: 1 },
    OpCodeDef { code: 0x21, name: "ldc.i8", operand: OperandType::InlineI8, size: 1 },
    OpCodeDef { code: 0x22, name: "ldc.r4", operand: OperandType::ShortInlineR, size: 1 },
    OpCodeDef { code: 0x23, name: "ldc.r8", operand: OperandType::InlineR, size: 1 },
    OpCodeDef { code: 0x25, name: "dup", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x26, name: "pop", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x27, name: "jmp", operand: OperandType::InlineMethod, size: 1 },
    OpCodeDef { code: 0x28, name: "call", operand: OperandType::InlineMethod, size: 1 },
    OpCodeDef { code: 0x29, name: "calli", operand: OperandType::InlineSig, size: 1 },
    OpCodeDef { code: 0x2A, name: "ret", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x2B, name: "br.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x2C, name: "brfalse.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x2D, name: "brtrue.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x2E, name: "beq.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x2F, name: "bge.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x30, name: "bgt.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x31, name: "ble.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x32, name: "blt.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x33, name: "bne.un.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x34, name: "bge.un.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x35, name: "bgt.un.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x36, name: "ble.un.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x37, name: "blt.un.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0x38, name: "br", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x39, name: "brfalse", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3A, name: "brtrue", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3B, name: "beq", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3C, name: "bge", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3D, name: "bgt", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3E, name: "ble", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x3F, name: "blt", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x40, name: "bne.un", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x41, name: "bge.un", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x42, name: "bgt.un", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x43, name: "ble.un", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x44, name: "blt.un", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0x45, name: "switch", operand: OperandType::InlineSwitch, size: 1 },
    OpCodeDef { code: 0x46, name: "ldind.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x47, name: "ldind.u1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x48, name: "ldind.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x49, name: "ldind.u2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4A, name: "ldind.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4B, name: "ldind.u4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4C, name: "ldind.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4D, name: "ldind.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4E, name: "ldind.r4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x4F, name: "ldind.r8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x50, name: "ldind.ref", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x51, name: "stind.ref", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x52, name: "stind.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x53, name: "stind.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x54, name: "stind.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x55, name: "stind.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x56, name: "stind.r4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x57, name: "stind.r8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x58, name: "add", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x59, name: "sub", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5A, name: "mul", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5B, name: "div", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5C, name: "div.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5D, name: "rem", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5E, name: "rem.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x5F, name: "and", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x60, name: "or", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x61, name: "xor", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x62, name: "shl", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x63, name: "shr", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x64, name: "shr.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x65, name: "neg", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x66, name: "not", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x67, name: "conv.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x68, name: "conv.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x69, name: "conv.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6A, name: "conv.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6B, name: "conv.r4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6C, name: "conv.r8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6D, name: "conv.u4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6E, name: "conv.u8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x6F, name: "callvirt", operand: OperandType::InlineMethod, size: 1 },
    OpCodeDef { code: 0x70, name: "cpobj", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x71, name: "ldobj", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x72, name: "ldstr", operand: OperandType::InlineString, size: 1 },
    OpCodeDef { code: 0x73, name: "newobj", operand: OperandType::InlineMethod, size: 1 },
    OpCodeDef { code: 0x74, name: "castclass", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x75, name: "isinst", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x76, name: "conv.r.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x79, name: "unbox", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x7A, name: "throw", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x7B, name: "ldfld", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x7C, name: "ldflda", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x7D, name: "stfld", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x7E, name: "ldsfld", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x7F, name: "ldsflda", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x80, name: "stsfld", operand: OperandType::InlineField, size: 1 },
    OpCodeDef { code: 0x81, name: "stobj", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x82, name: "conv.ovf.i1.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x83, name: "conv.ovf.i2.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x84, name: "conv.ovf.i4.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x85, name: "conv.ovf.i8.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x86, name: "conv.ovf.u1.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x87, name: "conv.ovf.u2.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x88, name: "conv.ovf.u4.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x89, name: "conv.ovf.u8.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x8A, name: "conv.ovf.i.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x8B, name: "conv.ovf.u.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x8C, name: "box", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x8D, name: "newarr", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x8E, name: "ldlen", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x8F, name: "ldelema", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0x90, name: "ldelem.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x91, name: "ldelem.u1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x92, name: "ldelem.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x93, name: "ldelem.u2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x94, name: "ldelem.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x95, name: "ldelem.u4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x96, name: "ldelem.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x97, name: "ldelem.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x98, name: "ldelem.r4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x99, name: "ldelem.r8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9A, name: "ldelem.ref", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9B, name: "stelem.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9C, name: "stelem.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9D, name: "stelem.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9E, name: "stelem.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0x9F, name: "stelem.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xA0, name: "stelem.r4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xA1, name: "stelem.r8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xA2, name: "stelem.ref", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xA3, name: "ldelem", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0xA4, name: "stelem", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0xA5, name: "unbox.any", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0xB3, name: "conv.ovf.i1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB4, name: "conv.ovf.u1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB5, name: "conv.ovf.i2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB6, name: "conv.ovf.u2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB7, name: "conv.ovf.i4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB8, name: "conv.ovf.u4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xB9, name: "conv.ovf.i8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xBA, name: "conv.ovf.u8", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xC2, name: "refanyval", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0xC3, name: "ckfinite", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xC6, name: "mkrefany", operand: OperandType::InlineType, size: 1 },
    OpCodeDef { code: 0xD0, name: "ldtoken", operand: OperandType::InlineTok, size: 1 },
    OpCodeDef { code: 0xD1, name: "conv.u2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD2, name: "conv.u1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD3, name: "conv.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD4, name: "conv.ovf.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD5, name: "conv.ovf.u", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD6, name: "add.ovf", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD7, name: "add.ovf.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD8, name: "mul.ovf", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xD9, name: "mul.ovf.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xDA, name: "sub.ovf", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xDB, name: "sub.ovf.un", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xDC, name: "endfinally", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xDD, name: "leave", operand: OperandType::InlineBrTarget, size: 1 },
    OpCodeDef { code: 0xDE, name: "leave.s", operand: OperandType::ShortInlineBrTarget, size: 1 },
    OpCodeDef { code: 0xDF, name: "stind.i", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xE0, name: "conv.u", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xF8, name: "prefix7", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xF9, name: "prefix6", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFA, name: "prefix5", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFB, name: "prefix4", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFC, name: "prefix3", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFD, name: "prefix2", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFE, name: "prefix1", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFF, name: "prefixref", operand: OperandType::InlineNone, size: 1 },
    OpCodeDef { code: 0xFE00, name: "arglist", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE01, name: "ceq", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE02, name: "cgt", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE03, name: "cgt.un", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE04, name: "clt", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE05, name: "clt.un", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE06, name: "ldftn", operand: OperandType::InlineMethod, size: 2 },
    OpCodeDef { code: 0xFE07, name: "ldvirtftn", operand: OperandType::InlineMethod, size: 2 },
    OpCodeDef { code: 0xFE09, name: "ldarg", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0A, name: "ldarga", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0B, name: "starg", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0C, name: "ldloc", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0D, name: "ldloca", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0E, name: "stloc", operand: OperandType::InlineVar, size: 2 },
    OpCodeDef { code: 0xFE0F, name: "localloc", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE11, name: "endfilter", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE12, name: "unaligned.", operand: OperandType::ShortInlineI, size: 2 },
    OpCodeDef { code: 0xFE13, name: "volatile.", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE14, name: "tail.", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE15, name: "initobj", operand: OperandType::InlineType, size: 2 },
    OpCodeDef { code: 0xFE16, name: "constrained.", operand: OperandType::InlineType, size: 2 },
    OpCodeDef { code: 0xFE17, name: "cpblk", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE18, name: "initblk", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE1A, name: "rethrow", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE1C, name: "sizeof", operand: OperandType::InlineType, size: 2 },
    OpCodeDef { code: 0xFE1D, name: "refanytype", operand: OperandType::InlineNone, size: 2 },
    OpCodeDef { code: 0xFE1E, name: "readonly.", operand: OperandType::InlineNone, size: 2 },
];
