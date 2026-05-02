use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use fission_core::architecture::select_elf_load_spec;

pub mod schema;
use schema::*;

pub struct ElfLoader;

const ET_REL: u16 = 1;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;
const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;
const SHN_UNDEF: u16 = 0;
const STT_NOTYPE: u8 = 0;
const STT_FUNC: u8 = 2;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
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

    fn parse_64(data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        // Read Header
        let header = Elf64Header::parse(bytes, &reader)?;

        let is_64bit = true;
        let entry_point = header.entry;
        let is_relocatable = header.type_ == ET_REL;

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
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
                        endian,
                    );
                }
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
            .build()
    }

    fn parse_32(data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        // Read Header
        let header = Elf32Header::parse(bytes, &reader)?;

        let is_64bit = false;
        let entry_point = header.entry as u64;
        let is_relocatable = header.type_ == ET_REL;

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
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
                        endian,
                    );
                }
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
            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }

            let address = if is_relocatable {
                section_addresses
                    .get(sym.st_shndx as usize)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(sym.st_value)
            } else {
                sym.st_value
            };

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
            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }

            let address = if is_relocatable {
                section_addresses
                    .get(sym.st_shndx as usize)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(sym.st_value as u64)
            } else {
                sym.st_value as u64
            };

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

fn push_unique_function(out: &mut Vec<FunctionInfo>, function: FunctionInfo) {
    if out.iter().any(|existing| {
        existing.address == function.address
            || (existing.is_import && function.is_import && existing.name == function.name)
    }) {
        return;
    }
    out.push(function);
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
