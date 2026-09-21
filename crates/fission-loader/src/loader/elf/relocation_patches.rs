//! Architecture-specific ELF relocation decoding and patch application.

use super::*;

pub(super) fn ppc64_function_descriptor_map_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    section_names: &[String],
    endian: Endian,
) -> HashMap<u64, u64> {
    let mut descriptors = HashMap::new();
    let reader = ByteReader::new(full_data, endian);
    for relocation_section in shdrs
        .iter()
        .filter(|shdr| shdr.sh_type == SHT_RELA)
        .filter(|shdr| {
            section_names
                .get(shdr.sh_info as usize)
                .map(|name| name == ".opd")
                .unwrap_or(false)
        })
    {
        let Some(opd_base) = section_addresses
            .get(relocation_section.sh_info as usize)
            .copied()
        else {
            continue;
        };
        let Some(symtab) = shdrs.get(relocation_section.sh_link as usize) else {
            continue;
        };
        let entry_size = if relocation_section.sh_entsize > 0 {
            relocation_section.sh_entsize as usize
        } else {
            24
        };
        let count = (relocation_section.sh_size as usize)
            .checked_div(entry_size)
            .unwrap_or(0);
        let start = relocation_section.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u64(offset) else {
                break;
            };
            let Ok(r_info) = reader.u64(offset + 8) else {
                break;
            };
            let Ok(addend) = reader.u64(offset + 16) else {
                break;
            };
            let reloc_type = (r_info & 0xffff_ffff) as u32;
            if reloc_type != R_PPC64_ADDR64 {
                continue;
            }
            let symbol_index = (r_info >> 32) as usize;
            let Some(symbol_value) =
                symbol_value_64(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                continue;
            };
            descriptors.insert(
                opd_base.saturating_add(r_offset),
                symbol_value.wrapping_add(addend),
            );
        }
    }
    descriptors
}

pub(super) fn riscv_relocation_patches_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<(usize, u32)> {
    let reader = ByteReader::new(full_data, endian);
    let mut patches = Vec::new();
    for shdr in shdrs.iter().filter(|shdr| shdr.sh_type == SHT_RELA) {
        let Some(target_section) = shdrs.get(shdr.sh_info as usize) else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else {
            24
        };
        let count = (shdr.sh_size as usize).checked_div(entry_size).unwrap_or(0);
        let start = shdr.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u64(offset) else {
                break;
            };
            let Ok(r_info) = reader.u64(offset + 8) else {
                break;
            };
            let Ok(addend) = reader.u64(offset + 16) else {
                break;
            };
            let reloc_type = (r_info & 0xffff_ffff) as u32;
            if reloc_type == R_RISCV_RELAX {
                continue;
            }
            let symbol_index = (r_info >> 32) as usize;
            let Some(symbol_value) =
                symbol_value_64(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                continue;
            };
            let patch_offset = target_section
                .sh_offset
                .checked_add(r_offset)
                .and_then(|value| usize::try_from(value).ok());
            let Some(patch_offset) = patch_offset else {
                continue;
            };
            let Ok(word) = reader.u32(patch_offset) else {
                continue;
            };
            if let Some(patched) =
                apply_riscv_relocation_to_word(word, reloc_type, symbol_value, addend as i64)
            {
                patches.push((patch_offset, patched));
            }
        }
    }
    patches
}

fn symbol_value_64(
    full_data: &[u8],
    symtab: &Elf64Shdr,
    section_addresses: &[u64],
    symbol_index: usize,
    endian: Endian,
) -> Option<u64> {
    let entry_size = if symtab.sh_entsize > 0 {
        symtab.sh_entsize as usize
    } else {
        std::mem::size_of::<Elf64Sym>()
    };
    let offset = (symtab.sh_offset as usize).checked_add(symbol_index.checked_mul(entry_size)?)?;
    if offset + entry_size > full_data.len() {
        return None;
    }
    let reader = ByteReader::new(full_data, endian);
    let symbol = Elf64Sym::parse(&reader, offset).ok()?;
    if symbol.st_shndx == SHN_UNDEF {
        return None;
    }
    let base = section_addresses.get(symbol.st_shndx as usize).copied()?;
    Some(base.saturating_add(symbol.st_value))
}

pub(super) fn apply_riscv_relocation_to_word(
    word: u32,
    reloc_type: u32,
    symbol_value: u64,
    addend: i64,
) -> Option<u32> {
    let value = (symbol_value as i128).wrapping_add(addend as i128);
    match reloc_type {
        R_RISCV_HI20 => {
            let imm20 = ((value + 0x800) >> 12) as u32 & 0x000f_ffff;
            Some((word & 0x0000_0fff) | (imm20 << 12))
        }
        R_RISCV_LO12_I => {
            let imm12 = value as u32 & 0x0000_0fff;
            Some((word & !(0x0000_0fff << 20)) | (imm12 << 20))
        }
        R_RISCV_LO12_S => {
            let imm12 = value as u32 & 0x0000_0fff;
            Some(
                (word & !((0x7f << 25) | (0x1f << 7)))
                    | ((imm12 >> 5) << 25)
                    | ((imm12 & 0x1f) << 7),
            )
        }
        _ => None,
    }
}

pub(super) fn loongarch_relocation_patches_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<(usize, u32)> {
    let reader = ByteReader::new(full_data, endian);
    let mut patches = Vec::new();
    for shdr in shdrs.iter().filter(|shdr| shdr.sh_type == SHT_RELA) {
        let Some(target_section) = shdrs.get(shdr.sh_info as usize) else {
            continue;
        };
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else {
            24
        };
        let count = (shdr.sh_size as usize).checked_div(entry_size).unwrap_or(0);
        let start = shdr.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u64(offset) else {
                break;
            };
            let Ok(r_info) = reader.u64(offset + 8) else {
                break;
            };
            let Ok(addend_raw) = reader.u64(offset + 16) else {
                break;
            };
            let addend = addend_raw as i64;
            let reloc_type = (r_info & 0xffff_ffff) as u32;
            let symbol_index = (r_info >> 32) as usize;
            let Some(symbol_value) =
                symbol_value_64(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                continue;
            };
            let patch_offset = target_section
                .sh_offset
                .checked_add(r_offset)
                .and_then(|value| usize::try_from(value).ok());
            let Some(patch_offset) = patch_offset else {
                continue;
            };
            let Ok(word) = reader.u32(patch_offset) else {
                continue;
            };
            let place = target_base.saturating_add(r_offset);
            if let Some(patched) =
                apply_loongarch_relocation_to_word(word, reloc_type, symbol_value, place, addend)
            {
                patches.push((patch_offset, patched));
            }
        }
    }
    patches
}

pub(super) fn loongarch_relocation_patches_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<(usize, u32)> {
    let reader = ByteReader::new(full_data, endian);
    let mut patches = Vec::new();
    for shdr in shdrs.iter().filter(|shdr| shdr.sh_type == SHT_RELA) {
        let Some(target_section) = shdrs.get(shdr.sh_info as usize) else {
            continue;
        };
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else {
            12
        };
        let count = (shdr.sh_size as usize).checked_div(entry_size).unwrap_or(0);
        let start = shdr.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u32(offset) else {
                break;
            };
            let Ok(r_info) = reader.u32(offset + 4) else {
                break;
            };
            let Ok(addend) = reader.i32(offset + 8) else {
                break;
            };
            let reloc_type = r_info & 0xff;
            let symbol_index = (r_info >> 8) as usize;
            let Some(symbol_value) =
                symbol_value_32(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                continue;
            };
            let patch_offset = target_section
                .sh_offset
                .checked_add(r_offset)
                .and_then(|value| usize::try_from(value).ok());
            let Some(patch_offset) = patch_offset else {
                continue;
            };
            let Ok(word) = reader.u32(patch_offset) else {
                continue;
            };
            let place = target_base.saturating_add(u64::from(r_offset));
            if let Some(patched) = apply_loongarch_relocation_to_word(
                word,
                reloc_type,
                symbol_value,
                place,
                i64::from(addend),
            ) {
                patches.push((patch_offset, patched));
            }
        }
    }
    patches
}

pub(super) fn apply_loongarch_relocation_to_word(
    word: u32,
    reloc_type: u32,
    symbol_value: u64,
    place: u64,
    addend: i64,
) -> Option<u32> {
    let value = (symbol_value as i128)
        .wrapping_add(addend as i128)
        .wrapping_sub(place as i128);
    match reloc_type {
        R_LARCH_B16 => {
            let imm16 = ((value >> 2) as u32) & 0xffff;
            Some((word & !(0xffff << 10)) | (imm16 << 10))
        }
        R_LARCH_B21 => {
            let imm21 = ((value >> 2) as u32) & 0x1f_ffff;
            Some(
                (word & !(0x1f | (0xffff << 10)))
                    | ((imm21 >> 16) & 0x1f)
                    | ((imm21 & 0xffff) << 10),
            )
        }
        R_LARCH_B26 => {
            let imm26 = ((value >> 2) as u32) & 0x03ff_ffff;
            Some(
                (word & !(0x03ff | (0xffff << 10)))
                    | ((imm26 >> 16) & 0x03ff)
                    | ((imm26 & 0xffff) << 10),
            )
        }
        _ => None,
    }
}

pub(super) fn arm_relocation_patches_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<(usize, u32)> {
    let reader = ByteReader::new(full_data, endian);
    let mut patches = Vec::new();
    for shdr in shdrs
        .iter()
        .filter(|shdr| matches!(shdr.sh_type, SHT_REL | SHT_RELA))
    {
        let Some(target_section) = shdrs.get(shdr.sh_info as usize) else {
            continue;
        };
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else if shdr.sh_type == SHT_RELA {
            12
        } else {
            8
        };
        let count = (shdr.sh_size as usize).checked_div(entry_size).unwrap_or(0);
        let start = shdr.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u32(offset) else {
                break;
            };
            let Ok(r_info) = reader.u32(offset + 4) else {
                break;
            };
            let reloc_type = r_info & 0xff;
            let symbol_index = (r_info >> 8) as usize;
            let Some(symbol_value) =
                symbol_value_32(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                continue;
            };
            let patch_offset = target_section
                .sh_offset
                .checked_add(r_offset)
                .and_then(|value| usize::try_from(value).ok());
            let Some(patch_offset) = patch_offset else {
                continue;
            };
            let Ok(word) = reader.u32(patch_offset) else {
                continue;
            };
            let place = target_base.saturating_add(r_offset as u64);
            let explicit_addend = if shdr.sh_type == SHT_RELA {
                reader.i32(offset + 8).ok().map(i64::from)
            } else {
                None
            };
            if let Some(patched) = apply_arm_relocation_to_word(
                word,
                reloc_type,
                symbol_value,
                place,
                explicit_addend,
                endian,
            ) {
                patches.push((patch_offset, patched));
            }
        }
    }
    patches
}

fn symbol_value_32(
    full_data: &[u8],
    symtab: &Elf32Shdr,
    section_addresses: &[u64],
    symbol_index: usize,
    endian: Endian,
) -> Option<u64> {
    let entry_size = if symtab.sh_entsize > 0 {
        symtab.sh_entsize as usize
    } else {
        Elf32Sym::SIZE
    };
    let offset = (symtab.sh_offset as usize).checked_add(symbol_index.checked_mul(entry_size)?)?;
    if offset + entry_size > full_data.len() {
        return None;
    }
    let reader = ByteReader::new(full_data, endian);
    let symbol = Elf32Sym::parse(&reader, offset).ok()?;
    if symbol.st_shndx == SHN_UNDEF {
        return None;
    }
    let base = section_addresses.get(symbol.st_shndx as usize).copied()?;
    Some(base.saturating_add(symbol.st_value as u64))
}

pub(super) fn apply_arm_relocation_to_word(
    word: u32,
    reloc_type: u32,
    symbol_value: u64,
    place: u64,
    explicit_addend: Option<i64>,
    endian: Endian,
) -> Option<u32> {
    match reloc_type {
        R_ARM_ABS32 => {
            let addend = explicit_addend.unwrap_or(word as i32 as i64);
            let value = (symbol_value as i128).wrapping_add(addend as i128) as u32;
            Some(value)
        }
        R_ARM_MOVW_ABS_NC | R_ARM_MOVT_ABS => {
            let encoded_addend = (((word & 0x000f_0000) >> 4) | (word & 0x0000_0fff)) as u16;
            let addend = explicit_addend.unwrap_or_else(|| sign_extend_i16(encoded_addend));
            let mut value = (symbol_value as i128).wrapping_add(addend as i128) as u32;
            if reloc_type == R_ARM_MOVT_ABS {
                value >>= 16;
            }
            let patched =
                (word & 0xfff0_f000) | ((value & 0x0000_f000) << 4) | (value & 0x0000_0fff);
            Some(patched)
        }
        R_ARM_THM_MOVW_ABS_NC | R_ARM_THM_MOVT_ABS => {
            let old_value = thumb_relocation_word(word, endian);
            let encoded_addend = (((old_value >> 4) & 0x0000_f000)
                | ((old_value >> 15) & 0x0000_0800)
                | ((old_value >> 4) & 0x0000_0700)
                | (old_value & 0x0000_00ff)) as u16;
            let addend = explicit_addend.unwrap_or_else(|| sign_extend_i16(encoded_addend));
            let mut value = (symbol_value as i128).wrapping_add(addend as i128) as u32;
            if reloc_type == R_ARM_THM_MOVT_ABS {
                value >>= 16;
            }
            let patched = (old_value & 0xfbf0_8f00)
                | ((value & 0x0000_f000) << 4)
                | ((value & 0x0000_0800) << 15)
                | ((value & 0x0000_0700) << 4)
                | (value & 0x0000_00ff);
            Some(thumb_relocation_word_to_file_word(patched, endian))
        }
        R_ARM_PC24 | R_ARM_CALL | R_ARM_JUMP24 => {
            let addend = explicit_addend.unwrap_or_else(|| arm_branch_addend(word));
            let value = (symbol_value as i128)
                .wrapping_add(addend as i128)
                .wrapping_sub(place as i128);
            let encoded = ((value >> 2) as i32 as u32) & 0x00ff_ffff;
            Some((word & 0xff00_0000) | encoded)
        }
        _ => None,
    }
}

fn sign_extend_i16(value: u16) -> i64 {
    i64::from(i16::from_ne_bytes(value.to_ne_bytes()))
}

fn thumb_relocation_word(word: u32, endian: Endian) -> u32 {
    match endian {
        Endian::Little => ((word & 0x0000_ffff) << 16) | (word >> 16),
        Endian::Big => word,
    }
}

fn thumb_relocation_word_to_file_word(value: u32, endian: Endian) -> u32 {
    match endian {
        Endian::Little => ((value & 0x0000_ffff) << 16) | (value >> 16),
        Endian::Big => value,
    }
}

pub(super) fn arm_branch_addend(word: u32) -> i64 {
    let imm24 = word & 0x00ff_ffff;
    let signed = ((imm24 << 8) as i32) >> 6;
    i64::from(signed)
}

pub(super) fn apply_u32_relocation_patches(
    data: &mut DataBuffer,
    patches: impl IntoIterator<Item = (usize, u32)>,
    endian: Endian,
) {
    let bytes = data.to_mut_vec();
    for (offset, value) in patches {
        let Some(dst) = bytes.get_mut(offset..offset.saturating_add(4)) else {
            continue;
        };
        let raw = match endian {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        };
        dst.copy_from_slice(&raw);
    }
}

pub(super) fn apply_u64_relocation_patches(
    data: &mut DataBuffer,
    patches: impl IntoIterator<Item = (usize, u64)>,
    endian: Endian,
) {
    let bytes = data.to_mut_vec();
    for (offset, value) in patches {
        let Some(dst) = bytes.get_mut(offset..offset.saturating_add(8)) else {
            continue;
        };
        let raw = match endian {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        };
        dst.copy_from_slice(&raw);
    }
}

pub(super) fn x86_64_relocation_patches_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> (Vec<(usize, u64)>, Vec<(usize, u32)>) {
    let reader = ByteReader::new(full_data, endian);
    let mut patches_64 = Vec::new();
    let mut patches_32 = Vec::new();
    for shdr in shdrs.iter().filter(|shdr| shdr.sh_type == SHT_RELA) {
        let Some(target_section) = shdrs.get(shdr.sh_info as usize) else {
            continue;
        };
        if (target_section.sh_flags & SHF_ALLOC) == 0 || target_section.sh_size == 0 {
            continue;
        }
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else {
            24
        };
        let count = (shdr.sh_size as usize).checked_div(entry_size).unwrap_or(0);
        let start = shdr.sh_offset as usize;
        for index in 0..count {
            let offset = start + index * entry_size;
            if offset + entry_size > full_data.len() {
                break;
            }
            let Ok(r_offset) = reader.u64(offset) else {
                break;
            };
            let Ok(r_info) = reader.u64(offset + 8) else {
                break;
            };
            let Ok(addend_raw) = reader.u64(offset + 16) else {
                break;
            };
            let addend = addend_raw as i64;
            let reloc_type = (r_info & 0xffff_ffff) as u32;
            let symbol_index = (r_info >> 32) as usize;

            // if reloc_type == R_X86_64_64 || reloc_type == R_X86_64_32 || reloc_type == R_X86_64_32S || reloc_type == R_X86_64_PC32 {
            //     println!(
            //         "[reloc-all] index: {}, type: {}, symbol_idx: {}, r_offset: 0x{:x}, addend: {}",
            //         index, reloc_type, symbol_index, r_offset, addend
            //     );
            // }

            let Some(symbol_value) =
                symbol_value_64(full_data, symtab, section_addresses, symbol_index, endian)
            else {
                // if reloc_type == R_X86_64_64 || reloc_type == R_X86_64_32 || reloc_type == R_X86_64_32S || reloc_type == R_X86_64_PC32 {
                //     println!("[reloc-fail] symbol_value_64 returned None for symbol_idx: {}", symbol_index);
                // }
                continue;
            };

            let patch_offset = target_section
                .sh_offset
                .checked_add(r_offset)
                .and_then(|val| usize::try_from(val).ok());
            let Some(patch_offset) = patch_offset else {
                continue;
            };

            // if reloc_type == R_X86_64_64 || reloc_type == R_X86_64_32 || reloc_type == R_X86_64_32S || reloc_type == R_X86_64_PC32 {
            //     let value = (symbol_value as i128).wrapping_add(addend as i128);
            //     println!(
            //         "[reloc-success] final_val: 0x{:x}, patch_offset: 0x{:x}",
            //         value, patch_offset
            //     );
            // }

            match reloc_type {
                R_X86_64_64 => {
                    let value = (symbol_value as i128).wrapping_add(addend as i128) as u64;
                    patches_64.push((patch_offset, value));
                }
                R_X86_64_32 | R_X86_64_32S => {
                    let value = (symbol_value as i128).wrapping_add(addend as i128) as u32;
                    patches_32.push((patch_offset, value));
                }
                R_X86_64_PC32 => {
                    let target_base = section_addresses[shdr.sh_info as usize];
                    let place = target_base.saturating_add(r_offset);
                    let value = (symbol_value as i128)
                        .wrapping_add(addend as i128)
                        .wrapping_sub(place as i128) as u32;
                    patches_32.push((patch_offset, value));
                }
                _ => {}
            }
        }
    }
    (patches_64, patches_32)
}
