//! IL Disassembler
//!
//! Main disassembler implementation

use super::decoder::{decode_operand, parse_body_header};
use super::opcodes::{OPCODE_MAP, UNKNOWN_OPCODE};
use super::types::ILInstruction;
use crate::dotnet::{DotNetError, DotNetResult};

/// O(1) opcode lookup using pre-built HashMap.
///
/// Performance: Reduced from O(n) linear search (~220 opcodes) to O(1) hash lookup.
fn lookup_opcode(code: u16) -> Option<&'static super::types::OpCodeDef> {
    OPCODE_MAP.get(&code).copied()
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
                let next = *code.get(cursor).ok_or_else(|| {
                    DotNetError::Malformed("Missing two-byte opcode suffix".into())
                })?;
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

impl Default for IlDisassembler {
    fn default() -> Self {
        Self::new()
    }
}
