use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{
    extract_cstring, DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::select_elf_load_spec;
use std::collections::HashMap;

pub mod schema;
use schema::*;

pub struct ElfLoader;

const ET_REL: u16 = 1;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;
const SHT_SYMTAB: u32 = 2;
const SHT_RELA: u32 = 4;
const SHT_REL: u32 = 9;
const SHT_DYNSYM: u32 = 11;
const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;
const SHN_UNDEF: u16 = 0;
const STT_NOTYPE: u8 = 0;
const STT_OBJECT: u8 = 1;
const STT_FUNC: u8 = 2;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
const EM_ARM: u16 = 40;
const EM_RISCV: u16 = 243;
const R_ARM_PC24: u32 = 1;
const R_ARM_ABS32: u32 = 2;
const R_ARM_CALL: u32 = 28;
const R_ARM_JUMP24: u32 = 29;
const R_RISCV_HI20: u32 = 26;
const R_RISCV_LO12_I: u32 = 27;
const R_RISCV_LO12_S: u32 = 28;
const R_RISCV_RELAX: u32 = 51;
const RELOCATABLE_IMAGE_BASE: u64 = 0x100000;
const ELF_EXTERNAL_IMAGE_BASE: u64 = 0xffff_2000_0000_0000;

impl ElfLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        // 1. Read Identification (first 16 bytes)
        let ident = ElfIdent::parse(data.as_slice())?;

        let is_64 = ident.class == 2;
        let is_little = ident.endian == 1; // 1=Little, 2=Big

        // Now we can move `data`
        if is_64 {
            Self::parse_64(
                data,
                path,
                if is_little {
                    Endian::Little
                } else {
                    Endian::Big
                },
            )
        } else {
            Self::parse_32(
                data,
                path,
                if is_little {
                    Endian::Little
                } else {
                    Endian::Big
                },
            )
        }
    }

    fn parse_64(mut data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        // Read Header
        let header = Elf64Header::parse(bytes, &reader)?;

        let is_64bit = true;
        let entry_point = header.entry;
        let is_relocatable = header.type_ == ET_REL;

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut global_symbols = HashMap::new();
        let mut relocation_symbols = HashMap::new();
        let phdrs = read_program_headers_64(bytes, &header, endian);
        let mut image_base = if !is_relocatable {
            image_base_from_phdrs_64(&phdrs).unwrap_or(u64::MAX)
        } else {
            u64::MAX
        };

        // Parse Sections
        if header.shoff != 0 && header.shnum > 0 {
            let mut shdrs = Vec::new();
            let section_header_size = if header.shentsize > 0 {
                header.shentsize as usize
            } else {
                Elf64Shdr::SIZE
            };
            for index in 0..header.shnum as usize {
                let offset = header
                    .shoff
                    .saturating_add((index * section_header_size) as u64)
                    as usize;
                shdrs.push(Elf64Shdr::parse(&reader, offset)?);
            }

            let section_addresses = if is_relocatable {
                assign_relocatable_section_addresses_64(&shdrs)
            } else {
                shdrs.iter().map(|shdr| shdr.sh_addr).collect()
            };

            // Get String Table for Section Names
            let strtab_idx = header.shstrndx as usize;
            let mut strtab_data = Vec::new();
            if strtab_idx < shdrs.len() {
                let strtab_shdr = &shdrs[strtab_idx];
                if strtab_shdr.sh_offset as usize + strtab_shdr.sh_size as usize <= bytes.len() {
                    strtab_data = bytes[strtab_shdr.sh_offset as usize
                        ..(strtab_shdr.sh_offset + strtab_shdr.sh_size) as usize]
                        .to_vec();
                }
            }
            let section_names: Vec<String> = shdrs
                .iter()
                .map(|shdr| extract_cstring(&strtab_data, shdr.sh_name as usize))
                .collect();

            for (index, shdr) in shdrs.iter().enumerate() {
                let virtual_address = section_addresses
                    .get(index)
                    .copied()
                    .unwrap_or(shdr.sh_addr);
                // Calculate simplified Image Base (lowest VA of loadable section)
                if (shdr.sh_flags & SHF_ALLOC) != 0
                    && virtual_address < image_base
                    && virtual_address != 0
                {
                    image_base = virtual_address;
                }

                let name = section_names.get(index).cloned().unwrap_or_default();

                sections_info.push(SectionInfo {
                    name: name.clone(),
                    virtual_address,
                    virtual_size: shdr.sh_size, // ELF does not distinguish VSize/RawSize clearly in SH, mostly same
                    file_offset: shdr.sh_offset,
                    file_size: shdr.sh_size, // except NOBITS
                    is_executable: (shdr.sh_flags & SHF_EXECINSTR) != 0,
                    is_readable: (shdr.sh_flags & SHF_ALLOC) != 0,
                    is_writable: (shdr.sh_flags & SHF_WRITE) != 0,
                });

                // If this is a symbol table, read functions
                if shdr.sh_type == SHT_SYMTAB || shdr.sh_type == SHT_DYNSYM {
                    // SYMTAB or DYNSYM
                    Self::parse_symbols_64(
                        bytes,
                        shdr.sh_offset,
                        shdr.sh_size,
                        shdr.sh_entsize,
                        shdr.sh_link as usize, // Link to String Table
                        &shdrs,
                        &section_addresses,
                        &section_names,
                        is_relocatable,
                        shdr.sh_type == SHT_DYNSYM,
                        &mut functions_info,
                        &mut global_symbols,
                        endian,
                    );
                }
            }

            parse_relocation_symbols_64(
                bytes,
                &shdrs,
                &section_addresses,
                &mut relocation_symbols,
                endian,
            );
            if is_relocatable && header.machine == EM_RISCV {
                let patches =
                    riscv_relocation_patches_64(bytes, &shdrs, &section_addresses, endian);
                apply_u32_relocation_patches(&mut data, patches, endian);
            }
        } else if !is_relocatable {
            sections_info.extend(load_segments_as_sections_64(&phdrs));
        }

        if image_base == u64::MAX {
            image_base = 0;
        }
        let (architecture, load_spec) = select_elf_load_spec(
            header.machine,
            header.ident.class,
            header.ident.endian,
            header.flags,
            image_base,
        )
        .map_err(|e| err!(loader, "{}", e))?;

        // Entry point fallback
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
                origin: Some("elf-entry".to_string()),
                kind: Some("entry".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
                thunk_target: None,
            });
        }

        LoadedBinaryBuilder::new(path, data)
            .format("ELF64")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_global_symbols(global_symbols)
            .add_relocation_symbols(relocation_symbols)
            .build()
    }

    fn parse_32(mut data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        // Read Header
        let header = Elf32Header::parse(bytes, &reader)?;

        let is_64bit = false;
        let entry_point = header.entry as u64;
        let is_relocatable = header.type_ == ET_REL;

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut global_symbols = HashMap::new();
        let mut relocation_symbols = HashMap::new();
        let phdrs = read_program_headers_32(bytes, &header, endian);
        let mut image_base = if !is_relocatable {
            image_base_from_phdrs_32(&phdrs).unwrap_or(u64::MAX)
        } else {
            u64::MAX
        };

        // Parse Sections
        if header.shoff != 0 && header.shnum > 0 {
            let mut shdrs = Vec::new();
            let section_header_size = if header.shentsize > 0 {
                header.shentsize as usize
            } else {
                Elf32Shdr::SIZE
            };
            for index in 0..header.shnum as usize {
                let offset = header
                    .shoff
                    .saturating_add((index * section_header_size) as u32)
                    as usize;
                shdrs.push(Elf32Shdr::parse(&reader, offset)?);
            }

            let section_addresses = if is_relocatable {
                assign_relocatable_section_addresses_32(&shdrs)
            } else {
                shdrs.iter().map(|shdr| shdr.sh_addr as u64).collect()
            };

            // Get String Table for Section Names
            let strtab_idx = header.shstrndx as usize;
            let mut strtab_data = Vec::new();
            if strtab_idx < shdrs.len() {
                let strtab_shdr = &shdrs[strtab_idx];
                if strtab_shdr.sh_offset as usize + strtab_shdr.sh_size as usize <= bytes.len() {
                    strtab_data = bytes[strtab_shdr.sh_offset as usize
                        ..(strtab_shdr.sh_offset + strtab_shdr.sh_size) as usize]
                        .to_vec();
                }
            }
            let section_names: Vec<String> = shdrs
                .iter()
                .map(|shdr| extract_cstring(&strtab_data, shdr.sh_name as usize))
                .collect();

            for (index, shdr) in shdrs.iter().enumerate() {
                let virtual_address = section_addresses
                    .get(index)
                    .copied()
                    .unwrap_or(shdr.sh_addr as u64);
                // Calculate simplified Image Base
                if (shdr.sh_flags as u64 & SHF_ALLOC) != 0
                    && virtual_address < image_base
                    && virtual_address != 0
                {
                    image_base = virtual_address;
                }

                let name = section_names.get(index).cloned().unwrap_or_default();

                sections_info.push(SectionInfo {
                    name,
                    virtual_address,
                    virtual_size: shdr.sh_size as u64,
                    file_offset: shdr.sh_offset as u64,
                    file_size: shdr.sh_size as u64,
                    is_executable: (shdr.sh_flags as u64 & SHF_EXECINSTR) != 0,
                    is_readable: (shdr.sh_flags as u64 & SHF_ALLOC) != 0,
                    is_writable: (shdr.sh_flags as u64 & SHF_WRITE) != 0,
                });

                // If this is a symbol table, read functions
                if shdr.sh_type == SHT_SYMTAB || shdr.sh_type == SHT_DYNSYM {
                    Self::parse_symbols_32(
                        bytes,
                        shdr.sh_offset as u64,
                        shdr.sh_size as u64,
                        shdr.sh_entsize as u64,
                        shdr.sh_link as usize,
                        &shdrs,
                        &section_addresses,
                        &section_names,
                        is_relocatable,
                        shdr.sh_type == SHT_DYNSYM,
                        &mut functions_info,
                        &mut global_symbols,
                        endian,
                    );
                }
            }

            parse_relocation_symbols_32(
                bytes,
                &shdrs,
                &section_addresses,
                &mut relocation_symbols,
                endian,
            );
            if is_relocatable && header.machine == EM_ARM {
                let patches = arm_relocation_patches_32(bytes, &shdrs, &section_addresses, endian);
                apply_u32_relocation_patches(&mut data, patches, endian);
            }
        } else if !is_relocatable {
            sections_info.extend(load_segments_as_sections_32(&phdrs));
        }

        if image_base == u64::MAX {
            image_base = 0;
        }
        let (architecture, load_spec) = select_elf_load_spec(
            header.machine,
            header.ident.class,
            header.ident.endian,
            header.flags,
            image_base,
        )
        .map_err(|e| err!(loader, "{}", e))?;

        // Entry point fallback
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
                origin: Some("elf-entry".to_string()),
                kind: Some("entry".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
                thunk_target: None,
            });
        }

        LoadedBinaryBuilder::new(path, data)
            .format("ELF32")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_global_symbols(global_symbols)
            .add_relocation_symbols(relocation_symbols)
            .build()
    }

    fn parse_symbols_64(
        full_data: &[u8],
        offset: u64,
        size: u64,
        entsize: u64,
        strtab_shndx: usize,
        shdrs: &[Elf64Shdr],
        section_addresses: &[u64],
        section_names: &[String],
        is_relocatable: bool,
        is_dynamic_table: bool,
        out_funcs: &mut Vec<FunctionInfo>,
        out_globals: &mut HashMap<u64, String>,
        endian: Endian,
    ) {
        // Resolve the symbol string table from the linked section header
        let strtab = if strtab_shndx < shdrs.len() {
            let sh = &shdrs[strtab_shndx];
            let start = sh.sh_offset as usize;
            let end = start + sh.sh_size as usize;
            if end <= full_data.len() {
                &full_data[start..end]
            } else {
                return;
            }
        } else {
            return;
        };

        let entry_size = if entsize > 0 {
            entsize as usize
        } else {
            std::mem::size_of::<Elf64Sym>()
        };
        let count = (size as usize).checked_div(entry_size).unwrap_or(0);

        let sym_start = offset as usize;
        let sym_end = sym_start + size as usize;
        if sym_end > full_data.len() {
            return;
        }

        let reader = ByteReader::new(full_data, endian);
        for index in 0..count {
            let offset = sym_start + index * entry_size;
            let sym = match Elf64Sym::parse(&reader, offset) {
                Ok(sym) => sym,
                Err(_) => break,
            };

            let sym_type = sym.st_info & 0xf;
            let name = extract_cstring(strtab, sym.st_name as usize);
            if name.is_empty() {
                continue;
            }
            if is_elf_mapping_symbol(&name) {
                continue;
            }
            let binding = sym.st_info >> 4;
            let externally_visible = binding == STB_GLOBAL || binding == STB_WEAK;

            if sym.st_shndx == SHN_UNDEF {
                if is_dynamic_table
                    && externally_visible
                    && (sym_type == STT_FUNC || sym_type == STT_NOTYPE)
                {
                    push_unique_function(
                        out_funcs,
                        FunctionInfo {
                            name,
                            address: ELF_EXTERNAL_IMAGE_BASE
                                + out_funcs.iter().filter(|f| f.is_import).count() as u64 * 8,
                            size: 0,
                            is_export: false,
                            is_import: true,
                            origin: Some("elf-dynsym".to_string()),
                            kind: Some("undefined_external".to_string()),
                            source_section: None,
                            external_library: None,
                            is_thunk_like: false,
                            thunk_target: None,
                        },
                    );
                }
                continue;
            }

            let section_index = sym.st_shndx as usize;
            let section_is_exec = shdrs
                .get(section_index)
                .map(|shdr| (shdr.sh_flags & SHF_EXECINSTR) != 0)
                .unwrap_or(false);

            let address = if is_relocatable {
                section_addresses
                    .get(sym.st_shndx as usize)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(sym.st_value)
            } else {
                sym.st_value
            };

            if sym_type == STT_OBJECT {
                out_globals.entry(address).or_insert(name);
                continue;
            }

            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }

            push_unique_function(
                out_funcs,
                FunctionInfo {
                    name,
                    address,
                    size: sym.st_size,
                    is_export: externally_visible,
                    is_import: false,
                    origin: Some(
                        if is_dynamic_table {
                            "elf-dynsym"
                        } else {
                            "elf-symtab"
                        }
                        .to_string(),
                    ),
                    kind: Some("code".to_string()),
                    source_section: section_names.get(section_index).cloned(),
                    external_library: None,
                    is_thunk_like: false,
                    thunk_target: None,
                },
            );
        }
    }

    fn parse_symbols_32(
        full_data: &[u8],
        offset: u64,
        size: u64,
        entsize: u64,
        strtab_shndx: usize,
        shdrs: &[Elf32Shdr],
        section_addresses: &[u64],
        section_names: &[String],
        is_relocatable: bool,
        is_dynamic_table: bool,
        out_funcs: &mut Vec<FunctionInfo>,
        out_globals: &mut HashMap<u64, String>,
        endian: Endian,
    ) {
        let strtab = if strtab_shndx < shdrs.len() {
            let sh = &shdrs[strtab_shndx];
            let start = sh.sh_offset as usize;
            let end = start + sh.sh_size as usize;
            if end <= full_data.len() {
                &full_data[start..end]
            } else {
                return;
            }
        } else {
            return;
        };

        let entry_size = if entsize > 0 {
            entsize as usize
        } else {
            std::mem::size_of::<Elf32Sym>()
        };
        let count = (size as usize).checked_div(entry_size).unwrap_or(0);

        let sym_start = offset as usize;
        let sym_end = sym_start + size as usize;
        if sym_end > full_data.len() {
            return;
        }

        let reader = ByteReader::new(full_data, endian);
        for index in 0..count {
            let offset = sym_start + index * entry_size;
            let sym = match Elf32Sym::parse(&reader, offset) {
                Ok(sym) => sym,
                Err(_) => break,
            };

            let sym_type = sym.st_info & 0xf;
            let name = extract_cstring(strtab, sym.st_name as usize);
            if name.is_empty() {
                continue;
            }
            if is_elf_mapping_symbol(&name) {
                continue;
            }

            let binding = sym.st_info >> 4;
            let externally_visible = binding == STB_GLOBAL || binding == STB_WEAK;

            if sym.st_shndx == SHN_UNDEF {
                if is_dynamic_table
                    && externally_visible
                    && (sym_type == STT_FUNC || sym_type == STT_NOTYPE)
                {
                    push_unique_function(
                        out_funcs,
                        FunctionInfo {
                            name,
                            address: ELF_EXTERNAL_IMAGE_BASE
                                + out_funcs.iter().filter(|f| f.is_import).count() as u64 * 8,
                            size: 0,
                            is_export: false,
                            is_import: true,
                            origin: Some("elf-dynsym".to_string()),
                            kind: Some("undefined_external".to_string()),
                            source_section: None,
                            external_library: None,
                            is_thunk_like: false,
                            thunk_target: None,
                        },
                    );
                }
                continue;
            }

            let section_index = sym.st_shndx as usize;
            let section_is_exec = shdrs
                .get(section_index)
                .map(|shdr| (shdr.sh_flags as u64 & SHF_EXECINSTR) != 0)
                .unwrap_or(false);

            let address = if is_relocatable {
                section_addresses
                    .get(sym.st_shndx as usize)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(sym.st_value as u64)
            } else {
                sym.st_value as u64
            };

            if sym_type == STT_OBJECT {
                out_globals.entry(address).or_insert(name);
                continue;
            }

            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }

            push_unique_function(
                out_funcs,
                FunctionInfo {
                    name,
                    address,
                    size: sym.st_size as u64,
                    is_export: externally_visible,
                    is_import: false,
                    origin: Some(
                        if is_dynamic_table {
                            "elf-dynsym"
                        } else {
                            "elf-symtab"
                        }
                        .to_string(),
                    ),
                    kind: Some("code".to_string()),
                    source_section: section_names.get(section_index).cloned(),
                    external_library: None,
                    is_thunk_like: false,
                    thunk_target: None,
                },
            );
        }
    }
}

fn parse_relocation_symbols_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    out: &mut HashMap<u64, String>,
    endian: Endian,
) {
    let reader = ByteReader::new(full_data, endian);
    for shdr in shdrs
        .iter()
        .filter(|shdr| matches!(shdr.sh_type, SHT_RELA | SHT_REL))
    {
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let Some(strtab) = symbol_string_table_64(full_data, shdrs, symtab) else {
            continue;
        };
        let entry_size = if shdr.sh_entsize > 0 {
            shdr.sh_entsize as usize
        } else if shdr.sh_type == SHT_RELA {
            24
        } else {
            16
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
            let sym_index = (r_info >> 32) as usize;
            let Some(name) = symbol_name_64(full_data, symtab, strtab, sym_index, endian) else {
                continue;
            };
            out.entry(target_base.saturating_add(r_offset))
                .or_insert(name);
        }
    }
}

fn riscv_relocation_patches_64(
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

fn apply_riscv_relocation_to_word(
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

fn arm_relocation_patches_32(
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
            if let Some(patched) =
                apply_arm_relocation_to_word(word, reloc_type, symbol_value, place, explicit_addend)
            {
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

fn apply_arm_relocation_to_word(
    word: u32,
    reloc_type: u32,
    symbol_value: u64,
    place: u64,
    explicit_addend: Option<i64>,
) -> Option<u32> {
    match reloc_type {
        R_ARM_ABS32 => {
            let addend = explicit_addend.unwrap_or(word as i32 as i64);
            let value = (symbol_value as i128).wrapping_add(addend as i128) as u32;
            Some(value)
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

fn arm_branch_addend(word: u32) -> i64 {
    let imm24 = word & 0x00ff_ffff;
    let signed = ((imm24 << 8) as i32) >> 6;
    i64::from(signed)
}

fn apply_u32_relocation_patches(
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

fn parse_relocation_symbols_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    section_addresses: &[u64],
    out: &mut HashMap<u64, String>,
    endian: Endian,
) {
    let reader = ByteReader::new(full_data, endian);
    for shdr in shdrs
        .iter()
        .filter(|shdr| matches!(shdr.sh_type, SHT_RELA | SHT_REL))
    {
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let Some(symtab) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let Some(strtab) = symbol_string_table_32(full_data, shdrs, symtab) else {
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
            let sym_index = (r_info >> 8) as usize;
            let Some(name) = symbol_name_32(full_data, symtab, strtab, sym_index, endian) else {
                continue;
            };
            out.entry(target_base.saturating_add(r_offset as u64))
                .or_insert(name);
        }
    }
}

fn symbol_string_table_64<'a>(
    full_data: &'a [u8],
    shdrs: &[Elf64Shdr],
    symtab: &Elf64Shdr,
) -> Option<&'a [u8]> {
    let strtab = shdrs.get(symtab.sh_link as usize)?;
    let start = strtab.sh_offset as usize;
    let end = start.checked_add(strtab.sh_size as usize)?;
    (end <= full_data.len()).then_some(&full_data[start..end])
}

fn symbol_string_table_32<'a>(
    full_data: &'a [u8],
    shdrs: &[Elf32Shdr],
    symtab: &Elf32Shdr,
) -> Option<&'a [u8]> {
    let strtab = shdrs.get(symtab.sh_link as usize)?;
    let start = strtab.sh_offset as usize;
    let end = start.checked_add(strtab.sh_size as usize)?;
    (end <= full_data.len()).then_some(&full_data[start..end])
}

fn symbol_name_64(
    full_data: &[u8],
    symtab: &Elf64Shdr,
    strtab: &[u8],
    sym_index: usize,
    endian: Endian,
) -> Option<String> {
    let entry_size = if symtab.sh_entsize > 0 {
        symtab.sh_entsize as usize
    } else {
        Elf64Sym::SIZE
    };
    let offset = (symtab.sh_offset as usize).checked_add(sym_index.checked_mul(entry_size)?)?;
    if offset + entry_size > full_data.len() {
        return None;
    }
    let reader = ByteReader::new(full_data, endian);
    let sym = Elf64Sym::parse(&reader, offset).ok()?;
    let name = extract_cstring(strtab, sym.st_name as usize);
    (!name.is_empty() && !is_elf_mapping_symbol(&name)).then_some(name)
}

fn symbol_name_32(
    full_data: &[u8],
    symtab: &Elf32Shdr,
    strtab: &[u8],
    sym_index: usize,
    endian: Endian,
) -> Option<String> {
    let entry_size = if symtab.sh_entsize > 0 {
        symtab.sh_entsize as usize
    } else {
        Elf32Sym::SIZE
    };
    let offset = (symtab.sh_offset as usize).checked_add(sym_index.checked_mul(entry_size)?)?;
    if offset + entry_size > full_data.len() {
        return None;
    }
    let reader = ByteReader::new(full_data, endian);
    let sym = Elf32Sym::parse(&reader, offset).ok()?;
    let name = extract_cstring(strtab, sym.st_name as usize);
    (!name.is_empty() && !is_elf_mapping_symbol(&name)).then_some(name)
}

fn push_unique_function(out: &mut Vec<FunctionInfo>, function: FunctionInfo) {
    if out.iter().any(|existing| {
        existing.address == function.address
            || (existing.is_import && function.is_import && existing.name == function.name)
    }) {
        return;
    }
    out.push(function);
}

fn is_elf_mapping_symbol(name: &str) -> bool {
    matches!(name, "$a" | "$d" | "$t" | "$x")
        || name
            .strip_prefix("$a.")
            .or_else(|| name.strip_prefix("$d."))
            .or_else(|| name.strip_prefix("$t."))
            .or_else(|| name.strip_prefix("$x."))
            .is_some()
}

fn read_program_headers_64(bytes: &[u8], header: &Elf64Header, endian: Endian) -> Vec<Elf64Phdr> {
    if header.phoff == 0 || header.phnum == 0 {
        return Vec::new();
    }
    let reader = ByteReader::new(bytes, endian);
    let entry_size = if header.phentsize > 0 {
        header.phentsize as usize
    } else {
        Elf64Phdr::SIZE
    };
    (0..header.phnum)
        .filter_map(|index| {
            let offset = header
                .phoff
                .saturating_add(index as u64 * entry_size as u64) as usize;
            Elf64Phdr::parse(&reader, offset).ok()
        })
        .collect()
}

fn read_program_headers_32(bytes: &[u8], header: &Elf32Header, endian: Endian) -> Vec<Elf32Phdr> {
    if header.phoff == 0 || header.phnum == 0 {
        return Vec::new();
    }
    let reader = ByteReader::new(bytes, endian);
    let entry_size = if header.phentsize > 0 {
        header.phentsize as usize
    } else {
        Elf32Phdr::SIZE
    };
    (0..header.phnum)
        .filter_map(|index| {
            let offset = header
                .phoff
                .saturating_add(index as u32 * entry_size as u32) as usize;
            Elf32Phdr::parse(&reader, offset).ok()
        })
        .collect()
}

fn image_base_from_phdrs_64(phdrs: &[Elf64Phdr]) -> Option<u64> {
    phdrs
        .iter()
        .filter(|phdr| phdr.p_type == PT_LOAD && phdr.p_memsz > 0)
        .map(|phdr| phdr.p_vaddr)
        .min()
}

fn image_base_from_phdrs_32(phdrs: &[Elf32Phdr]) -> Option<u64> {
    phdrs
        .iter()
        .filter(|phdr| phdr.p_type == PT_LOAD && phdr.p_memsz > 0)
        .map(|phdr| phdr.p_vaddr as u64)
        .min()
}

fn load_segments_as_sections_64(phdrs: &[Elf64Phdr]) -> Vec<SectionInfo> {
    phdrs
        .iter()
        .enumerate()
        .filter(|(_, phdr)| phdr.p_type == PT_LOAD && phdr.p_memsz > 0)
        .map(|(index, phdr)| SectionInfo {
            name: format!("PT_LOAD_{index}"),
            virtual_address: phdr.p_vaddr,
            virtual_size: phdr.p_memsz,
            file_offset: phdr.p_offset,
            file_size: phdr.p_filesz,
            is_executable: (phdr.p_flags & PF_X) != 0,
            is_readable: (phdr.p_flags & PF_R) != 0,
            is_writable: (phdr.p_flags & PF_W) != 0,
        })
        .collect()
}

fn load_segments_as_sections_32(phdrs: &[Elf32Phdr]) -> Vec<SectionInfo> {
    phdrs
        .iter()
        .enumerate()
        .filter(|(_, phdr)| phdr.p_type == PT_LOAD && phdr.p_memsz > 0)
        .map(|(index, phdr)| SectionInfo {
            name: format!("PT_LOAD_{index}"),
            virtual_address: phdr.p_vaddr as u64,
            virtual_size: phdr.p_memsz as u64,
            file_offset: phdr.p_offset as u64,
            file_size: phdr.p_filesz as u64,
            is_executable: (phdr.p_flags & PF_X) != 0,
            is_readable: (phdr.p_flags & PF_R) != 0,
            is_writable: (phdr.p_flags & PF_W) != 0,
        })
        .collect()
}

fn assign_relocatable_section_addresses_64(shdrs: &[Elf64Shdr]) -> Vec<u64> {
    let mut next = RELOCATABLE_IMAGE_BASE;
    let mut addresses = vec![0; shdrs.len()];
    for (index, shdr) in shdrs.iter().enumerate() {
        if (shdr.sh_flags & SHF_ALLOC) == 0 || shdr.sh_size == 0 {
            continue;
        }
        next = align_up(next, shdr.sh_addralign);
        addresses[index] = next;
        next = next.saturating_add(shdr.sh_size);
    }
    addresses
}

fn assign_relocatable_section_addresses_32(shdrs: &[Elf32Shdr]) -> Vec<u64> {
    let mut next = RELOCATABLE_IMAGE_BASE;
    let mut addresses = vec![0; shdrs.len()];
    for (index, shdr) in shdrs.iter().enumerate() {
        if (shdr.sh_flags as u64 & SHF_ALLOC) == 0 || shdr.sh_size == 0 {
            continue;
        }
        next = align_up(next, shdr.sh_addralign as u64);
        addresses[index] = next;
        next = next.saturating_add(shdr.sh_size as u64);
    }
    addresses
}

fn align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    if !alignment.is_power_of_two() {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riscv_relocation_patch_encodes_hi20_and_lo12_fields() {
        let lui_a4_zero = 0x0000_0737;
        let ld_a4_a4_zero = 0x0007_3703;
        let sd_a4_a5_zero = 0x00e7_b023;

        assert_eq!(
            apply_riscv_relocation_to_word(lui_a4_zero, R_RISCV_HI20, 0x100098, 0)
                .expect("HI20 patch"),
            0x0010_0737
        );
        assert_eq!(
            apply_riscv_relocation_to_word(ld_a4_a4_zero, R_RISCV_LO12_I, 0x100098, 0)
                .expect("LO12_I patch"),
            0x0987_3703
        );
        assert_eq!(
            apply_riscv_relocation_to_word(sd_a4_a5_zero, R_RISCV_LO12_S, 0x1000a0, 0)
                .expect("LO12_S patch"),
            0x0ae7_b023
        );
    }

    #[test]
    fn arm_call_relocation_patch_encodes_pc_relative_branch_field() {
        let bl_self_loop = 0xebff_fffe;

        assert_eq!(
            arm_branch_addend(bl_self_loop),
            -8,
            "REL addend is encoded in the ARM branch immediate"
        );
        assert_eq!(
            apply_arm_relocation_to_word(bl_self_loop, R_ARM_CALL, 0x100000, 0x100018, None)
                .expect("ARM CALL patch"),
            0xebff_fff8
        );
    }

    #[test]
    fn elf32_arm_relocatable_call_relocations_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark/binary/ARM4t_be/baremetal/small/binary/c/function_calls.o");
        if !fixture.exists() {
            eprintln!("skip: ARM4t function_calls fixture missing");
            return;
        }

        let binary = LoadedBinary::from_file(&fixture).expect("load ARM4t relocatable object");
        let call = binary
            .view_bytes(0x100018, 4)
            .expect("recursive_fib call bytes");
        assert_eq!(
            call,
            [0xeb, 0xff, 0xff, 0xf8],
            "R_ARM_CALL should retarget recursive_fib BL to 0x100000"
        );
    }
}
