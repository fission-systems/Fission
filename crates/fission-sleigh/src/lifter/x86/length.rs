use anyhow::{bail, Context, Result};

pub(crate) fn decode_len(bytes: &[u8]) -> Result<u64> {
    if bytes.is_empty() {
        bail!("x86 decode received empty bytes");
    }

    let mut i = 0usize;
    let mut operand_size_override = false;
    while i < bytes.len() && is_prefix(bytes[i]) {
        if bytes[i] == 0x66 {
            operand_size_override = true;
        }
        i += 1;
    }
    if i >= bytes.len() {
        bail!("x86 decode found only prefixes");
    }

    let mut opcode = bytes[i];
    i += 1;
    let mut ext: Option<u8> = None;

    if opcode == 0x0F {
        if i >= bytes.len() {
            bail!("x86 truncated 0x0F escape opcode");
        }
        opcode = bytes[i];
        ext = Some(opcode);
        i += 1;
    }

    let needs_modrm = needs_modrm(opcode, ext);
    let mut modrm: Option<u8> = None;
    if needs_modrm {
        if i >= bytes.len() {
            bail!("x86 missing ModRM byte");
        }
        let modrm_byte = bytes[i];
        modrm = Some(modrm_byte);
        i += 1;

        let mode = (modrm_byte >> 6) & 0x3;
        let rm = modrm_byte & 0x7;

        if mode != 3 && rm == 4 {
            if i >= bytes.len() {
                bail!("x86 missing SIB byte");
            }
            let sib = bytes[i];
            i += 1;
            let base = sib & 0x7;
            if mode == 0 && base == 5 {
                i = i.saturating_add(4);
            }
        }

        match mode {
            0 => {
                if rm == 5 {
                    i = i.saturating_add(4);
                }
            }
            1 => i = i.saturating_add(1),
            2 => i = i.saturating_add(4),
            _ => {}
        }
    }

    i = i.saturating_add(imm_len(opcode, ext, modrm, operand_size_override));
    if i > bytes.len() {
        bail!(
            "x86 instruction truncated: need {} bytes, have {}",
            i,
            bytes.len()
        );
    }

    u64::try_from(i).context("x86 decoded length does not fit u64")
}

fn needs_modrm(opcode: u8, ext: Option<u8>) -> bool {
    if let Some(second) = ext {
        return !matches!(second, 0x80..=0x8F | 0x05 | 0x34 | 0x35);
    }

    !matches!(
        opcode,
        0x6A
            | 0x68
            | 0x90
            | 0xC3
            | 0xCB
            | 0xC2
            | 0xCA
            | 0xE8
            | 0xE9
            | 0xEB
            | 0x70..=0x7F
            | 0xA0
            | 0xA1
            | 0xA2
            | 0xA3
            | 0xA8
            | 0xA9
            | 0xB0..=0xBF
    )
}

fn imm_len(opcode: u8, ext: Option<u8>, modrm: Option<u8>, operand_size_override: bool) -> usize {
    let full_operand_imm = if operand_size_override { 2 } else { 4 };

    if let Some(second) = ext {
        if (0x80..=0x8F).contains(&second) {
            return 4;
        }
        return 0;
    }

    match opcode {
        0x81 => full_operand_imm,
        0x83 => 1,
        0xC0 => 1,
        0xC1 => 1,
        0xF7 => {
            if modrm.map(|m| ((m >> 3) & 0x7) == 0).unwrap_or(false) {
                full_operand_imm
            } else {
                0
            }
        }
        0xA8 => 1,
        0xA9 => full_operand_imm,
        0xC2 | 0xCA => 2,
        0x6A | 0xEB | 0x70..=0x7F | 0xCD => 1,
        0x68 | 0xE8 | 0xE9 | 0xA0 | 0xA1 | 0xA2 | 0xA3 => 4,
        0xB8..=0xBF => 4,
        _ => 0,
    }
}

fn is_prefix(byte: u8) -> bool {
    matches!(
        byte,
        0xF0
            | 0xF2
            | 0xF3
            | 0x2E
            | 0x36
            | 0x3E
            | 0x26
            | 0x64
            | 0x65
            | 0x66
            | 0x67
            | 0x40..=0x4F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_len_handles_81_83_immediates() {
        assert_eq!(decode_len(&[0x81, 0xF8, 0x34, 0x12, 0x00, 0x00]).unwrap(), 6);
        assert_eq!(decode_len(&[0x83, 0xE8, 0xFF]).unwrap(), 3);
    }

    #[test]
    fn decode_len_handles_f7_test_immediate_only_for_group0() {
        assert_eq!(decode_len(&[0xF7, 0xC0, 0x78, 0x56, 0x34, 0x12]).unwrap(), 6);
        assert_eq!(decode_len(&[0xF7, 0xD0]).unwrap(), 2);
    }

    #[test]
    fn decode_len_handles_a9_and_operand_override() {
        assert_eq!(decode_len(&[0xA9, 0x01, 0x00, 0x00, 0x00]).unwrap(), 5);
        assert_eq!(decode_len(&[0x66, 0xA9, 0x34, 0x12]).unwrap(), 4);
    }

    #[test]
    fn decode_len_handles_modrm_memory_disp_and_imm8() {
        assert_eq!(decode_len(&[0x83, 0x68, 0x04, 0x7F]).unwrap(), 4);
        assert_eq!(decode_len(&[0xC1, 0xE8, 0x03]).unwrap(), 3);
    }

    #[test]
    fn decode_len_handles_d3_and_address_size_override() {
        assert_eq!(decode_len(&[0xD3, 0xE0]).unwrap(), 2);
        assert_eq!(decode_len(&[0x67, 0x01, 0x18]).unwrap(), 3);
    }

    #[test]
    fn decode_len_handles_byte_shift_group2() {
        assert_eq!(decode_len(&[0xD0, 0xE0]).unwrap(), 2);
        assert_eq!(decode_len(&[0xD2, 0xE0]).unwrap(), 2);
        assert_eq!(decode_len(&[0xC0, 0xE8, 0x03]).unwrap(), 3);
    }
}
