use anyhow::{bail, Context, Result};

#[derive(Clone, Copy)]
enum OpcodeMap {
    Primary,
    Map0F,
    Map0F38,
    Map0F3A,
    VexUnknown,
}

pub(crate) fn decode_len(bytes: &[u8]) -> Result<u64> {
    if bytes.is_empty() {
        bail!("x86 decode received empty bytes");
    }

    let mut i = 0usize;
    let mut operand_size_override = false;
    let mut rex = 0u8;
    while i < bytes.len() && is_prefix(bytes[i]) {
        if bytes[i] == 0x66 {
            operand_size_override = true;
        }
        if (0x40..=0x4F).contains(&bytes[i]) {
            rex = bytes[i];
        }
        i += 1;
    }
    if i >= bytes.len() {
        bail!("x86 decode found only prefixes");
    }

    let mut opcode = bytes[i];
    i += 1;
    let mut opcode_map = OpcodeMap::Primary;

    if opcode == 0xC5 {
        if i >= bytes.len() {
            bail!("x86 truncated 2-byte VEX prefix");
        }
        // Consume VEX.vvvv/L/pp.
        i += 1;
        opcode_map = OpcodeMap::Map0F;
        if i >= bytes.len() {
            bail!("x86 missing opcode after 2-byte VEX prefix");
        }
        opcode = bytes[i];
        i += 1;
    } else if opcode == 0xC4 {
        if i + 1 >= bytes.len() {
            bail!("x86 truncated 3-byte VEX prefix");
        }
        let vex_map = bytes[i] & 0x1F;
        // Consume VEX m-mmmm and W/vvvv/L/pp bytes.
        i += 2;
        opcode_map = match vex_map {
            0x01 => OpcodeMap::Map0F,
            0x02 => OpcodeMap::Map0F38,
            0x03 => OpcodeMap::Map0F3A,
            _ => OpcodeMap::VexUnknown,
        };
        if i >= bytes.len() {
            bail!("x86 missing opcode after 3-byte VEX prefix");
        }
        opcode = bytes[i];
        i += 1;
    } else if opcode == 0x0F {
        if i >= bytes.len() {
            bail!("x86 truncated 0x0F escape opcode");
        }
        let second = bytes[i];
        i += 1;

        if matches!(second, 0x38 | 0x3A) {
            opcode_map = if second == 0x38 {
                OpcodeMap::Map0F38
            } else {
                OpcodeMap::Map0F3A
            };
            if i >= bytes.len() {
                bail!("x86 truncated 0x0F {:02X} escape opcode", second);
            }
            opcode = bytes[i];
            i += 1;
        } else {
            opcode_map = OpcodeMap::Map0F;
            opcode = second;
        }
    }

    let needs_modrm = needs_modrm(opcode, opcode_map);
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

    i = i.saturating_add(imm_len(
        opcode,
        opcode_map,
        modrm,
        operand_size_override,
        (rex & 0x08) != 0,
    ));
    if i > bytes.len() {
        bail!(
            "x86 instruction truncated: need {} bytes, have {}",
            i,
            bytes.len()
        );
    }

    u64::try_from(i).context("x86 decoded length does not fit u64")
}

fn needs_modrm(opcode: u8, map: OpcodeMap) -> bool {
    match map {
        OpcodeMap::Map0F => {
            !matches!(
                opcode,
            0x05
                | 0x06
                | 0x07
                | 0x08
                | 0x09
                | 0x0B
                | 0x30
                | 0x31
                | 0x32
                | 0x34
                | 0x35
                | 0x77
                | 0x80..=0x8F
                | 0xA0
                | 0xA1
                | 0xA2
                | 0xA8
                | 0xA9
                | 0xAA
                | 0xC8..=0xCF
            )
        }
        OpcodeMap::Map0F38 | OpcodeMap::Map0F3A | OpcodeMap::VexUnknown => true,
        OpcodeMap::Primary => {
            !matches!(
                opcode,
                0x6A
                    | 0x68
                    | 0x50..=0x5F
                    | 0x90
                    | 0xCC
                    | 0xCD
                    | 0xC3
                    | 0xCB
                    | 0xC2
                    | 0xCA
                    | 0xE8
                    | 0xE9
                    | 0xEB
                    | 0x70..=0x7F
                    | 0x98
                    | 0x99
                    | 0xA0
                    | 0xA1
                    | 0xA2
                    | 0xA3
                    | 0xA4
                    | 0xA5
                    | 0xA6
                    | 0xA7
                    | 0xA8
                    | 0xA9
                    | 0xAA
                    | 0xAB
                    | 0xAC
                    | 0xAD
                    | 0xAE
                    | 0xAF
                    | 0xB0..=0xBF
            )
        }
    }
}

fn imm_len(
    opcode: u8,
    map: OpcodeMap,
    modrm: Option<u8>,
    operand_size_override: bool,
    rex_w: bool,
) -> usize {
    let full_operand_imm = if operand_size_override { 2 } else { 4 };

    match map {
        OpcodeMap::Map0F3A => return 1,
        OpcodeMap::Map0F38 => return 0,
        OpcodeMap::Map0F => {
            if (0x80..=0x8F).contains(&opcode) {
                return 4;
            }
            return 0;
        }
        OpcodeMap::VexUnknown => return 0,
        OpcodeMap::Primary => {}
    }

    match opcode {
        0x81 => full_operand_imm,
        0x83 => 1,
        0xC0 => 1,
        0xC1 => 1,
        0xC6 => 1,
        0xC7 => full_operand_imm,
        0xF7 => {
            if modrm.map(|m| ((m >> 3) & 0x7) == 0).unwrap_or(false) {
                full_operand_imm
            } else {
                0
            }
        }
        0xF6 => {
            if modrm.map(|m| ((m >> 3) & 0x7) == 0).unwrap_or(false) {
                1
            } else {
                0
            }
        }
        0xA8 => 1,
        0xA9 => full_operand_imm,
        0xC2 | 0xCA => 2,
        0x6A | 0xEB | 0x70..=0x7F | 0xCD => 1,
        0x68 => full_operand_imm,
        0xE8 | 0xE9 | 0xA0 | 0xA1 | 0xA2 | 0xA3 => 4,
        0xB0..=0xB7 => 1,
        0xB8..=0xBF => {
            if rex_w {
                8
            } else {
                full_operand_imm
            }
        }
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
    fn decode_len_handles_f6_test_immediate_only_for_group0() {
        assert_eq!(decode_len(&[0xF6, 0xC0, 0x7F]).unwrap(), 3);
        assert_eq!(decode_len(&[0xF6, 0xD8]).unwrap(), 2);
    }

    #[test]
    fn decode_len_handles_bit_test_family_extended_opcodes() {
        assert_eq!(decode_len(&[0x0F, 0xA3, 0xC8]).unwrap(), 3);
        assert_eq!(decode_len(&[0x0F, 0xAB, 0xD8]).unwrap(), 3);
        assert_eq!(decode_len(&[0x0F, 0xB3, 0x18]).unwrap(), 3);
        assert_eq!(decode_len(&[0x48, 0x0F, 0xBB, 0x18]).unwrap(), 4);
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

    #[test]
    fn decode_len_handles_mov_imm_opcodes() {
        assert_eq!(decode_len(&[0xB0, 0x7F]).unwrap(), 2);
        assert_eq!(decode_len(&[0xB8, 0x78, 0x56, 0x34, 0x12]).unwrap(), 5);
        assert_eq!(decode_len(&[0x49, 0xB8, 1, 2, 3, 4, 5, 6, 7, 8]).unwrap(), 10);
    }

    #[test]
    fn decode_len_handles_mov_group11_immediates() {
        assert_eq!(decode_len(&[0xC6, 0x00, 0x12]).unwrap(), 3);
        assert_eq!(decode_len(&[0xC7, 0x00, 0x78, 0x56, 0x34, 0x12]).unwrap(), 6);
    }

    #[test]
    fn decode_len_handles_pause_and_nop_extended_forms() {
        assert_eq!(decode_len(&[0x90]).unwrap(), 1);
        assert_eq!(decode_len(&[0xF3, 0x90]).unwrap(), 2);
        assert_eq!(decode_len(&[0x0F, 0x1F, 0x00]).unwrap(), 3);
        assert_eq!(decode_len(&[0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap(), 8);
    }

    #[test]
    fn decode_len_handles_int3_and_int_immediates_without_modrm() {
        assert_eq!(decode_len(&[0xCC]).unwrap(), 1);
        assert_eq!(decode_len(&[0xCD, 0x80]).unwrap(), 2);
    }

    #[test]
    fn decode_len_handles_rdtsc_and_clflush_forms() {
        assert_eq!(decode_len(&[0x0F, 0x31]).unwrap(), 2);
        assert_eq!(decode_len(&[0x0F, 0xAE, 0x38]).unwrap(), 3);
        assert_eq!(decode_len(&[0x0F, 0xAE, 0xBC, 0x00, 0x10, 0x00, 0x00, 0x00]).unwrap(), 8);
    }

    #[test]
    fn decode_len_handles_system_no_modrm_0f_opcodes() {
        assert_eq!(decode_len(&[0x0F, 0x05]).unwrap(), 2); // syscall
        assert_eq!(decode_len(&[0x0F, 0x07]).unwrap(), 2); // sysret
        assert_eq!(decode_len(&[0x0F, 0x06]).unwrap(), 2); // clts
        assert_eq!(decode_len(&[0x0F, 0x08]).unwrap(), 2); // invd
        assert_eq!(decode_len(&[0x0F, 0x09]).unwrap(), 2); // wbinvd
        assert_eq!(decode_len(&[0x0F, 0x0B]).unwrap(), 2); // ud2
        assert_eq!(decode_len(&[0x0F, 0x30]).unwrap(), 2); // wrmsr
        assert_eq!(decode_len(&[0x0F, 0x32]).unwrap(), 2); // rdmsr
        assert_eq!(decode_len(&[0x0F, 0x34]).unwrap(), 2); // sysenter
        assert_eq!(decode_len(&[0x0F, 0x35]).unwrap(), 2); // sysexit
        assert_eq!(decode_len(&[0x0F, 0x77]).unwrap(), 2); // emms
        assert_eq!(decode_len(&[0x0F, 0xA0]).unwrap(), 2); // push fs
        assert_eq!(decode_len(&[0x0F, 0xA1]).unwrap(), 2); // pop fs
        assert_eq!(decode_len(&[0x0F, 0xA2]).unwrap(), 2); // cpuid
        assert_eq!(decode_len(&[0x0F, 0xA8]).unwrap(), 2); // push gs
        assert_eq!(decode_len(&[0x0F, 0xA9]).unwrap(), 2); // pop gs
        assert_eq!(decode_len(&[0x0F, 0xAA]).unwrap(), 2); // rsm
        assert_eq!(decode_len(&[0x0F, 0xC8]).unwrap(), 2); // bswap eax
        assert_eq!(decode_len(&[0x49, 0x0F, 0xC8]).unwrap(), 3); // bswap r8d
    }

    #[test]
    fn decode_len_handles_string_opcodes_without_modrm() {
        assert_eq!(decode_len(&[0xA4]).unwrap(), 1); // movsb
        assert_eq!(decode_len(&[0xA5]).unwrap(), 1); // movsd/movsq (size by prefixes)
        assert_eq!(decode_len(&[0xA6]).unwrap(), 1); // cmpsb
        assert_eq!(decode_len(&[0xA7]).unwrap(), 1); // cmpsd/cmpsq
        assert_eq!(decode_len(&[0xAA]).unwrap(), 1); // stosb
        assert_eq!(decode_len(&[0xAB]).unwrap(), 1); // stosd/stosq
        assert_eq!(decode_len(&[0xAC]).unwrap(), 1); // lodsb
        assert_eq!(decode_len(&[0xAD]).unwrap(), 1); // lodsd/lodsq
        assert_eq!(decode_len(&[0xAE]).unwrap(), 1); // scasb
        assert_eq!(decode_len(&[0xAF]).unwrap(), 1); // scasd/scasq
        assert_eq!(decode_len(&[0xF3, 0xA4]).unwrap(), 2); // rep movsb
        assert_eq!(decode_len(&[0xF2, 0xA7]).unwrap(), 2); // repne cmpsd/cmpsq
    }

    #[test]
    fn decode_len_handles_convert_sign_extension_opcodes_without_modrm() {
        assert_eq!(decode_len(&[0x98]).unwrap(), 1); // cwde
        assert_eq!(decode_len(&[0x99]).unwrap(), 1); // cdq
        assert_eq!(decode_len(&[0x66, 0x98]).unwrap(), 2); // cbw
        assert_eq!(decode_len(&[0x48, 0x99]).unwrap(), 2); // cqo
    }

    #[test]
    fn decode_len_handles_0f_three_byte_escape_maps() {
        assert_eq!(decode_len(&[0x0F, 0x38, 0xF1, 0xC0]).unwrap(), 4); // 0f38 + modrm
        assert_eq!(decode_len(&[0x0F, 0x38, 0x00, 0x44, 0x24, 0x10]).unwrap(), 6); // sib+disp8
        assert_eq!(decode_len(&[0x66, 0x0F, 0x3A, 0x0F, 0xC0, 0x04]).unwrap(), 6); // 0f3a + modrm + imm8
    }

    #[test]
    fn decode_len_handles_push_pop_register_opcodes() {
        assert_eq!(decode_len(&[0x53]).unwrap(), 1);
        assert_eq!(decode_len(&[0x5B]).unwrap(), 1);
        assert_eq!(decode_len(&[0x41, 0x50]).unwrap(), 2);
    }

    #[test]
    fn decode_len_handles_push_immediate_with_operand_override() {
        assert_eq!(decode_len(&[0x68, 0x78, 0x56, 0x34, 0x12]).unwrap(), 5);
        assert_eq!(decode_len(&[0x66, 0x68, 0x34, 0x12]).unwrap(), 4);
    }

    #[test]
    fn decode_len_handles_pop_rm_and_push_rm_groups() {
        assert_eq!(decode_len(&[0x8F, 0x00]).unwrap(), 2);
        assert_eq!(decode_len(&[0xFF, 0x30]).unwrap(), 2);
    }

    #[test]
    fn decode_len_handles_vex_two_byte_prefix_variants() {
        assert_eq!(decode_len(&[0xC5, 0xF8, 0x77]).unwrap(), 3); // vzeroupper
        assert_eq!(decode_len(&[0xC5, 0xF9, 0x6E, 0xC0]).unwrap(), 4); // vmovd xmm0, eax
    }

    #[test]
    fn decode_len_handles_vex_three_byte_map_variants() {
        assert_eq!(decode_len(&[0xC4, 0xE2, 0x79, 0x00, 0x44, 0x24, 0x10]).unwrap(), 7); // 0f38 map
        assert_eq!(decode_len(&[0xC4, 0xE3, 0x79, 0x0F, 0xC0, 0x04]).unwrap(), 6); // 0f3a map + imm8
    }

    #[test]
    fn decode_len_rejects_truncated_vex_prefixes() {
        assert!(decode_len(&[0xC5]).is_err());
        assert!(decode_len(&[0xC5, 0xF8]).is_err());
        assert!(decode_len(&[0xC4, 0xE3]).is_err());
        assert!(decode_len(&[0xC4, 0xE3, 0x79]).is_err());
    }
}
