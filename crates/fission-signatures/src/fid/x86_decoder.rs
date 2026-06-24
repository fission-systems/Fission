//! Pure-Rust lightweight x86/x64 instruction-level masking decoder.
//!
//! Parses instruction prefixes, opcodes, ModR/M, SIB, displacements, and immediates
//! to compute Ghidra-compatible opcode-only masks and operands deterministically.

use super::hash::{FidHashUnit, FidInstructionOperand, FidOperandValue};
use std::collections::HashMap;

/// Dissect raw x86/x64 function bytes into deterministic `FidHashUnit` structures.
///
/// Parses prefixes, ModR/M, SIB, displacement, and immediate operands to mask them out
/// exactly matching Ghidra's opcode-only instruction masking.
pub fn dissect_x86_function_to_fid_units(
    bytes: &[u8],
    start_addr: u64,
    relocations: &HashMap<u64, String>,
) -> Vec<FidHashUnit> {
    let mut units = Vec::new();
    let mut offset = 0;

    while offset < bytes.len() {
        let pc = start_addr + offset as u64;
        let remaining = &bytes[offset..];
        if remaining.is_empty() {
            break;
        }

        // Decode instruction details
        let (inst_len, mask, operands, is_call) =
            decode_x86_instruction_details(remaining, pc, relocations);
        if inst_len == 0 {
            break; // Stop decoding on invalid/malformed bytes
        }

        let inst_bytes = remaining[..inst_len].to_vec();

        // Adjust mask with relocation entries: any byte overlapped by a relocation is wildcarded (0x00)
        let mut final_mask = mask;
        let mut has_relocation = false;
        for i in 0..inst_len {
            if relocations.contains_key(&(pc + i as u64)) {
                final_mask[i] = 0x00;
                has_relocation = true;
            }
        }

        units.push(FidHashUnit {
            bytes: inst_bytes,
            instruction_mask: Some(final_mask),
            operands,
            is_call,
            has_relocation,
        });

        offset += inst_len;
    }

    units
}

/// Helper to decode a single x86/x64 instruction's length, mask, operands, and call type.
fn decode_x86_instruction_details(
    bytes: &[u8],
    _pc: u64,
    _relocations: &HashMap<u64, String>,
) -> (usize, Vec<u8>, Vec<FidInstructionOperand>, bool) {
    let mut offset = 0;
    let mut _legacy_prefixes = 0;
    let mut rex = None;
    let mut _vex_len = 0;
    let mut has_operand_size_prefix = false;

    // 1. Parse legacy prefixes
    while offset < bytes.len() {
        let b = bytes[offset];
        match b {
            0xF0 | 0xF2 | 0xF3 | // Lock / Repeat
            0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 | // Segment overrides
            0x66 | 0x67 => { // Operand/Address size override
                if b == 0x66 {
                    has_operand_size_prefix = true;
                }
                _legacy_prefixes += 1;
                offset += 1;
            }
            _ => break,
        }
    }

    // 2. Parse REX prefix (x86_64)
    if offset < bytes.len() {
        let b = bytes[offset];
        if b >= 0x40 && b <= 0x4F {
            rex = Some(b);
            offset += 1;
        }
    }

    // 3. Parse VEX / EVEX prefixes
    if offset < bytes.len() {
        let b = bytes[offset];
        if b == 0xC5 && offset + 1 < bytes.len() {
            _vex_len = 2;
            offset += 2;
        } else if b == 0xC4 && offset + 2 < bytes.len() {
            _vex_len = 3;
            offset += 3;
        } else if b == 0x62 && offset + 3 < bytes.len() {
            _vex_len = 4;
            offset += 4;
        }
    }

    // 4. Parse Opcode
    if offset >= bytes.len() {
        return (0, Vec::new(), Vec::new(), false);
    }
    let opcode_start = offset;
    let b1 = bytes[offset];
    offset += 1;

    let mut is_two_byte = false;
    let mut is_three_byte = false;

    if b1 == 0x0F && offset < bytes.len() {
        let b2 = bytes[offset];
        offset += 1;
        if (b2 == 0x38 || b2 == 0x3A) && offset < bytes.len() {
            offset += 1;
            is_three_byte = true;
        } else {
            is_two_byte = true;
        }
    }
    let _opcode_len = offset - opcode_start;

    // 5. Parse ModR/M & SIB
    let op_byte1 = b1;
    let op_byte2 = if is_two_byte || is_three_byte {
        Some(bytes[opcode_start + 1])
    } else {
        None
    };

    let has_modrm_byte = opcode_has_modrm(op_byte1, op_byte2);
    let mut disp_size = 0;
    let mut _has_sib = false;
    let mut modrm_reg = None;

    let modrm_start = offset;
    if has_modrm_byte && offset < bytes.len() {
        let modrm = bytes[offset];
        offset += 1;
        let r_mod = (modrm >> 6) & 3;
        let r_reg = (modrm >> 3) & 7;
        let r_rm = modrm & 7;
        modrm_reg = Some(r_reg);

        if r_mod != 3 && r_rm == 4 {
            _has_sib = true;
            if offset < bytes.len() {
                let sib = bytes[offset];
                offset += 1;
                let sib_base = sib & 7;
                if sib_base == 5 && r_mod == 0 {
                    disp_size = 4;
                }
            }
        }

        if disp_size == 0 {
            match r_mod {
                0 => {
                    if r_rm == 5 {
                        disp_size = 4; // Absolute disp32 / RIP-relative
                    }
                }
                1 => {
                    disp_size = 1; // disp8
                }
                2 => {
                    disp_size = 4; // disp32
                }
                _ => {}
            }
        }
    }
    let _modrm_len = offset - modrm_start;

    // 6. Read Displacement
    let disp_start = offset;
    if offset + disp_size > bytes.len() {
        return (0, Vec::new(), Vec::new(), false);
    }
    let disp_val = if disp_size > 0 {
        let val = read_signed_value(&bytes[offset..offset + disp_size], disp_size);
        offset += disp_size;
        Some(val)
    } else {
        None
    };

    // 7. Calculate Immediate Size
    let rex_w = rex.is_some_and(|r| (r & 0x08) != 0);
    let imm_size = get_immediate_size(
        op_byte1,
        op_byte2,
        modrm_reg,
        has_operand_size_prefix,
        rex_w,
        rex.is_some(),
    );

    // 8. Read Immediate
    let imm_start = offset;
    if offset + imm_size > bytes.len() {
        return (0, Vec::new(), Vec::new(), false);
    }
    let imm_val = if imm_size > 0 {
        let val = read_signed_value(&bytes[offset..offset + imm_size], imm_size);
        offset += imm_size;
        Some(val)
    } else {
        None
    };

    let total_len = offset;

    // 9. Construct Mask
    let mut mask = vec![0xFF; total_len];
    // Displacement bytes are wildcards (0x00)
    if disp_size > 0 {
        for i in disp_start..disp_start + disp_size {
            mask[i] = 0x00;
        }
    }
    // Immediate bytes are wildcards (0x00)
    if imm_size > 0 {
        for i in imm_start..imm_start + imm_size {
            mask[i] = 0x00;
        }
    }

    // 10. Construct Operands
    let mut operands = Vec::new();
    if let Some(val) = disp_val {
        operands.push(FidInstructionOperand {
            values: vec![FidOperandValue::Scalar {
                value: val,
                is_address: true,
            }],
        });
    }
    if let Some(val) = imm_val {
        // If it's a direct branch or memory offset, mark it as address reference
        let is_branch_target = op_byte1 == 0xE8
            || op_byte1 == 0xE9
            || (op_byte1 == 0x0F && op_byte2.is_some_and(|o| o >= 0x80 && o <= 0x8F));
        let is_moffset = op_byte1 >= 0xA0 && op_byte1 <= 0xA3;
        operands.push(FidInstructionOperand {
            values: vec![FidOperandValue::Scalar {
                value: val,
                is_address: is_branch_target || is_moffset,
            }],
        });
    }

    // 11. Determine Call Type
    let is_call = op_byte1 == 0xE8 || (op_byte1 == 0xFF && modrm_reg == Some(2));

    (total_len, mask, operands, is_call)
}

/// Check if opcode has a ModR/M byte.
fn opcode_has_modrm(b1: u8, b2: Option<u8>) -> bool {
    if let Some(opcode2) = b2 {
        // 2-byte opcode starting with 0x0F
        if b1 == 0x0F {
            match opcode2 {
                0x05 | 0x07 | 0x31 | 0x32 | 0x34 | 0x35 | 0x77 | 0xA1 | 0xA2 => false,
                op if op >= 0x80 && op <= 0x8F => false, // Jcc near Jumps
                _ => true,
            }
        } else {
            true
        }
    } else {
        // 1-byte opcode
        match b1 {
            // AL/eAX immediate arithmetic
            0x04 | 0x05 | 0x0C | 0x0D | 0x14 | 0x15 | 0x1C | 0x1D |
            0x24 | 0x25 | 0x2C | 0x2D | 0x34 | 0x35 | 0x3C | 0x3D |
            // INC/DEC/PUSH/POP reg
            0x40..=0x5F |
            // Jcc short
            0x70..=0x7F |
            // NOP/XCHG/LAHF/etc.
            0x90..=0x9F |
            // MOV/MOVS/CMPS/TEST/STOS/LODS/SCAS
            0xA0..=0xAF |
            // MOV reg imm
            0xB0..=0xBF |
            // RET/RETF
            0xC2 | 0xC3 | 0xCA | 0xCB |
            // IN/OUT/CALL/JMP
            0xE4..=0xEB | 0xEC..=0xEF |
            // HLT/CLC/STI/etc.
            0xF4 | 0xF5 | 0xF8..=0xFD => false,
            _ => true,
        }
    }
}

/// Calculate immediate size in bytes.
fn get_immediate_size(
    b1: u8,
    b2: Option<u8>,
    modrm_reg: Option<u8>,
    has_operand_size_prefix: bool,
    rex_w: bool,
    is_64bit: bool,
) -> usize {
    if let Some(opcode2) = b2 {
        // 2-byte or 3-byte opcodes
        if b1 == 0x0F {
            match opcode2 {
                0x80..=0x8F => 4, // Jcc near (treated as immediate/offset)
                0x70 | 0xC2 => 1, // cmpps / pshufw
                0x3A => 1,        // 0x0F 0x3A instructions always have 1-byte immediate
                _ => 0,
            }
        } else {
            0
        }
    } else {
        // 1-byte opcode
        let default_word = if has_operand_size_prefix { 2 } else { 4 };
        match b1 {
            0x04
            | 0x0C
            | 0x14
            | 0x1C
            | 0x24
            | 0x2C
            | 0x34
            | 0x3C
            | 0x6A
            | 0x70..=0x7F
            | 0x80
            | 0x82
            | 0x83
            | 0xA8
            | 0xB0..=0xB7
            | 0xC0
            | 0xC1
            | 0xEB => 1,

            0xC2 | 0xCA => 2,

            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D | 0x68 | 0x81 | 0xA9 | 0xC7 => {
                default_word
            }

            0xA0..=0xA3 => {
                if is_64bit {
                    8
                } else {
                    4
                }
            }

            0xB8..=0xBF => {
                if rex_w {
                    8
                } else {
                    default_word
                }
            }

            0xC6 => 1,

            0xE8 | 0xE9 => 4, // displacement treated as immediate

            0xF6 => {
                if let Some(reg) = modrm_reg {
                    if reg == 0 || reg == 1 { 1 } else { 0 }
                } else {
                    0
                }
            }

            0xF7 => {
                if let Some(reg) = modrm_reg {
                    if reg == 0 || reg == 1 {
                        default_word
                    } else {
                        0
                    }
                } else {
                    0
                }
            }

            _ => 0,
        }
    }
}

/// Helper to read a signed value with endianness safety.
fn read_signed_value(slice: &[u8], size: usize) -> i64 {
    match size {
        1 => slice[0] as i8 as i64,
        2 => {
            let mut arr = [0; 2];
            arr.copy_from_slice(&slice[..2]);
            i16::from_le_bytes(arr) as i64
        }
        4 => {
            let mut arr = [0; 4];
            arr.copy_from_slice(&slice[..4]);
            i32::from_le_bytes(arr) as i64
        }
        8 => {
            let mut arr = [0; 8];
            arr.copy_from_slice(&slice[..8]);
            i64::from_le_bytes(arr)
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x86_decoder_push_pop_ebp() {
        let relocs = HashMap::new();
        // push ebp (0x55)
        let units = dissect_x86_function_to_fid_units(&[0x55], 0x1000, &relocs);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].bytes, vec![0x55]);
        assert_eq!(units[0].instruction_mask, Some(vec![0xFF]));
        assert!(units[0].operands.is_empty());
        assert!(!units[0].is_call);
        assert!(!units[0].has_relocation);
    }

    #[test]
    fn test_x86_decoder_mov_eax_imm() {
        let relocs = HashMap::new();
        // mov eax, 0x12345678 (0xB8 0x78 0x56 0x34 0x12)
        let bytes = [0xB8, 0x78, 0x56, 0x34, 0x12];
        let units = dissect_x86_function_to_fid_units(&bytes, 0x1000, &relocs);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].bytes, bytes.to_vec());
        assert_eq!(
            units[0].instruction_mask,
            Some(vec![0xFF, 0x00, 0x00, 0x00, 0x00])
        );
        assert_eq!(units[0].operands.len(), 1);
        assert_eq!(
            units[0].operands[0].values[0],
            FidOperandValue::Scalar {
                value: 0x12345678,
                is_address: false
            }
        );
    }

    #[test]
    fn test_x86_decoder_call_relative() {
        let relocs = HashMap::new();
        // call 0x1005 (0xE8 0x00 0x00 0x00 0x00)
        let bytes = [0xE8, 0x00, 0x00, 0x00, 0x00];
        let units = dissect_x86_function_to_fid_units(&bytes, 0x1000, &relocs);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].bytes, bytes.to_vec());
        assert_eq!(
            units[0].instruction_mask,
            Some(vec![0xFF, 0x00, 0x00, 0x00, 0x00])
        );
        assert!(units[0].is_call);
    }

    #[test]
    fn test_x86_decoder_with_relocation() {
        let mut relocs = HashMap::new();
        relocs.insert(0x1001, "some_symbol".to_string());
        // mov eax, [0x2000] (0xA1 0x00 0x20 0x00 0x00)
        let bytes = [0xA1, 0x00, 0x20, 0x00, 0x00];
        let units = dissect_x86_function_to_fid_units(&bytes, 0x1000, &relocs);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].bytes, bytes.to_vec());
        assert!(units[0].has_relocation);
        // The relocated byte at 0x1001 is wildcarded to 0x00 in final mask
        assert_eq!(units[0].instruction_mask.as_ref().unwrap()[1], 0x00);
    }
}
