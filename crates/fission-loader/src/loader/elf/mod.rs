use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use fission_core::architecture::select_elf_load_spec;
use std::collections::{HashMap, HashSet};

pub mod eh_frame;
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
const EM_PPC64: u16 = 21;
const EM_ARM: u16 = 40;
const EM_RISCV: u16 = 243;
const EM_LOONGARCH: u16 = 258;
const EM_X86_64: u16 = 62;
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_32: u32 = 10;
const R_X86_64_32S: u32 = 11;
const R_ARM_PC24: u32 = 1;
const R_ARM_ABS32: u32 = 2;
const R_ARM_CALL: u32 = 28;
const R_ARM_JUMP24: u32 = 29;
const R_ARM_MOVW_ABS_NC: u32 = 43;
const R_ARM_MOVT_ABS: u32 = 44;
const R_ARM_THM_MOVW_ABS_NC: u32 = 47;
const R_ARM_THM_MOVT_ABS: u32 = 48;
const R_PPC64_ADDR64: u32 = 38;
const R_RISCV_HI20: u32 = 26;
const R_RISCV_LO12_I: u32 = 27;
const R_RISCV_LO12_S: u32 = 28;
const R_RISCV_RELAX: u32 = 51;
const R_LARCH_B16: u32 = 64;
const R_LARCH_B21: u32 = 65;
const R_LARCH_B26: u32 = 66;
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
        let mut existing_addresses = HashSet::new();
        let mut existing_imports = HashSet::new();
        let mut global_symbols = HashMap::new();
        let mut global_symbol_sizes = HashMap::new();
        let mut relocation_symbols = HashMap::new();
        let mut relocs = Vec::new();
        let mut symbol_versions = HashMap::new();
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
            let ppc64_function_descriptors = if is_relocatable && header.machine == EM_PPC64 {
                ppc64_function_descriptor_map_64(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    &section_names,
                    endian,
                )
            } else {
                HashMap::new()
            };

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
                        &mut existing_addresses,
                        &mut existing_imports,
                        &mut global_symbols,
                        &mut global_symbol_sizes,
                        &ppc64_function_descriptors,
                        endian,
                    );
                }
            }

            // eh_frame parsing for 64-bit
            if let Some(eh_frame_sec) = sections_info.iter().find(|s| s.name == ".eh_frame") {
                let text_addr = sections_info
                    .iter()
                    .find(|s| s.name == ".text")
                    .map(|s| s.virtual_address)
                    .unwrap_or(0);
                let data_addr = sections_info
                    .iter()
                    .find(|s| s.name == ".data")
                    .map(|s| s.virtual_address)
                    .unwrap_or(0);
                let start = eh_frame_sec.file_offset as usize;
                let end = start.saturating_add(eh_frame_sec.file_size as usize);
                if let Some(eh_frame_data) = bytes.get(start..end) {
                    let eh_funcs = eh_frame::parse_eh_frame(
                        eh_frame_data,
                        eh_frame_sec.virtual_address,
                        text_addr,
                        data_addr,
                        endian == Endian::Little,
                        true,
                    );
                    functions_info.extend(eh_funcs);
                }
            }

            parse_relocation_symbols_64(
                bytes,
                &shdrs,
                &section_addresses,
                &mut relocation_symbols,
                endian,
            );
            let patches_riscv = if is_relocatable && header.machine == EM_RISCV {
                Some(riscv_relocation_patches_64(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    endian,
                ))
            } else {
                None
            };
            let patches_loong = if is_relocatable && header.machine == EM_LOONGARCH {
                Some(loongarch_relocation_patches_64(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    endian,
                ))
            } else {
                None
            };
            let patches_x86 = if is_relocatable && header.machine == EM_X86_64 {
                Some(x86_64_relocation_patches_64(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    endian,
                ))
            } else {
                None
            };

            if let Some(patches) = patches_riscv {
                apply_u32_relocation_patches(&mut data, patches, endian);
            }
            if let Some(patches) = patches_loong {
                apply_u32_relocation_patches(&mut data, patches, endian);
            }
            if let Some((patches_64, patches_32)) = patches_x86 {
                apply_u64_relocation_patches(&mut data, patches_64, endian);
                apply_u32_relocation_patches(&mut data, patches_32, endian);
            }

            let bytes_patched = data.as_slice();
            relocs = parse_relocations_64(bytes_patched, &shdrs, &section_addresses, endian);
            let ver_names = parse_gnu_versions_64(bytes_patched, &shdrs, endian);
            symbol_versions = map_symbol_versions_64(bytes_patched, &shdrs, endian, &ver_names);
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

        // Apply PT_GNU_RELRO protection (Gap 6)
        let pt_gnu_relro = 0x6474e552;
        for phdr in &phdrs {
            if phdr.p_type == pt_gnu_relro {
                let relro_start = phdr.p_vaddr;
                let relro_end = phdr.p_vaddr.saturating_add(phdr.p_memsz);
                for section in &mut sections_info {
                    let sec_start = section.virtual_address;
                    let sec_end = section.virtual_address.saturating_add(section.virtual_size);
                    if sec_start < relro_end && sec_end > relro_start {
                        section.is_writable = false;
                    }
                }
            }
        }

        let (header_types, header_symbols) = generate_elf_header_types(
            true,
            image_base,
            header.phoff,
            header.phnum,
            header.shoff,
            header.shnum,
        );
        global_symbols.extend(header_symbols);

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
            .add_global_symbol_sizes(global_symbol_sizes)
            .add_relocation_symbols(relocation_symbols)
            .add_inferred_types(header_types)
            .add_relocations(relocs)
            .add_symbol_versions(symbol_versions)
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
        let mut existing_addresses = HashSet::new();
        let mut existing_imports = HashSet::new();
        let mut global_symbols = HashMap::new();
        let mut global_symbol_sizes = HashMap::new();
        let mut relocation_symbols = HashMap::new();
        let mut relocs = Vec::new();
        let mut symbol_versions = HashMap::new();
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
                        &mut existing_addresses,
                        &mut existing_imports,
                        &mut global_symbols,
                        &mut global_symbol_sizes,
                        endian,
                    );
                }
            }

            // eh_frame parsing for 32-bit
            if let Some(eh_frame_sec) = sections_info.iter().find(|s| s.name == ".eh_frame") {
                let text_addr = sections_info
                    .iter()
                    .find(|s| s.name == ".text")
                    .map(|s| s.virtual_address)
                    .unwrap_or(0);
                let data_addr = sections_info
                    .iter()
                    .find(|s| s.name == ".data")
                    .map(|s| s.virtual_address)
                    .unwrap_or(0);
                let start = eh_frame_sec.file_offset as usize;
                let end = start.saturating_add(eh_frame_sec.file_size as usize);
                if let Some(eh_frame_data) = bytes.get(start..end) {
                    let eh_funcs = eh_frame::parse_eh_frame(
                        eh_frame_data,
                        eh_frame_sec.virtual_address,
                        text_addr,
                        data_addr,
                        endian == Endian::Little,
                        false,
                    );
                    functions_info.extend(eh_funcs);
                }
            }

            parse_relocation_symbols_32(
                bytes,
                &shdrs,
                &section_addresses,
                &mut relocation_symbols,
                endian,
            );
            let patches_arm = if is_relocatable && header.machine == EM_ARM {
                Some(arm_relocation_patches_32(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    endian,
                ))
            } else {
                None
            };
            let patches_loong = if is_relocatable && header.machine == EM_LOONGARCH {
                Some(loongarch_relocation_patches_32(
                    bytes,
                    &shdrs,
                    &section_addresses,
                    endian,
                ))
            } else {
                None
            };

            if let Some(patches) = patches_arm {
                apply_u32_relocation_patches(&mut data, patches, endian);
            }
            if let Some(patches) = patches_loong {
                apply_u32_relocation_patches(&mut data, patches, endian);
            }

            let bytes_patched = data.as_slice();
            relocs = parse_relocations_32(bytes_patched, &shdrs, &section_addresses, endian);
            let ver_names = parse_gnu_versions_32(bytes_patched, &shdrs, endian);
            symbol_versions = map_symbol_versions_32(bytes_patched, &shdrs, endian, &ver_names);
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

        // Apply PT_GNU_RELRO protection (Gap 6)
        let pt_gnu_relro = 0x6474e552;
        for phdr in &phdrs {
            if phdr.p_type == pt_gnu_relro {
                let relro_start = phdr.p_vaddr as u64;
                let relro_end = (phdr.p_vaddr as u64).saturating_add(phdr.p_memsz as u64);
                for section in &mut sections_info {
                    let sec_start = section.virtual_address;
                    let sec_end = section.virtual_address.saturating_add(section.virtual_size);
                    if sec_start < relro_end && sec_end > relro_start {
                        section.is_writable = false;
                    }
                }
            }
        }

        let (header_types, header_symbols) = generate_elf_header_types(
            false,
            image_base,
            header.phoff as u64,
            header.phnum,
            header.shoff as u64,
            header.shnum,
        );
        global_symbols.extend(header_symbols);

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
            .add_global_symbol_sizes(global_symbol_sizes)
            .add_relocation_symbols(relocation_symbols)
            .add_inferred_types(header_types)
            .add_relocations(relocs)
            .add_symbol_versions(symbol_versions)
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
        existing_addresses: &mut HashSet<u64>,
        existing_imports: &mut HashSet<String>,
        out_globals: &mut HashMap<u64, String>,
        out_global_sizes: &mut HashMap<u64, u64>,
        function_descriptors: &HashMap<u64, u64>,
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
                        existing_addresses,
                        existing_imports,
                        FunctionInfo {
                            name,
                            address: ELF_EXTERNAL_IMAGE_BASE + existing_imports.len() as u64 * 8,
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

            let raw_address = if is_relocatable {
                section_addresses
                    .get(sym.st_shndx as usize)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(sym.st_value)
            } else {
                sym.st_value
            };

            if sym_type == STT_OBJECT {
                out_globals.entry(raw_address).or_insert(name);
                if sym.st_size != 0 {
                    out_global_sizes.entry(raw_address).or_insert(sym.st_size);
                }
                continue;
            }

            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }
            let address = function_descriptors
                .get(&raw_address)
                .copied()
                .unwrap_or(raw_address);

            push_unique_function(
                out_funcs,
                existing_addresses,
                existing_imports,
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
        existing_addresses: &mut HashSet<u64>,
        existing_imports: &mut HashSet<String>,
        out_globals: &mut HashMap<u64, String>,
        out_global_sizes: &mut HashMap<u64, u64>,
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
                        existing_addresses,
                        existing_imports,
                        FunctionInfo {
                            name,
                            address: ELF_EXTERNAL_IMAGE_BASE + existing_imports.len() as u64 * 8,
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
                if sym.st_size != 0 {
                    out_global_sizes
                        .entry(address)
                        .or_insert(u64::from(sym.st_size));
                }
                continue;
            }

            if sym_type != STT_FUNC && !(sym_type == STT_NOTYPE && section_is_exec) {
                continue;
            }

            push_unique_function(
                out_funcs,
                existing_addresses,
                existing_imports,
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

fn ppc64_function_descriptor_map_64(
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

fn loongarch_relocation_patches_64(
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

fn loongarch_relocation_patches_32(
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

fn apply_loongarch_relocation_to_word(
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

fn apply_arm_relocation_to_word(
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

fn apply_u64_relocation_patches(
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

fn x86_64_relocation_patches_64(
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

fn push_unique_function(
    out: &mut Vec<FunctionInfo>,
    existing_addresses: &mut HashSet<u64>,
    existing_imports: &mut HashSet<String>,
    function: FunctionInfo,
) {
    if existing_addresses.contains(&function.address) {
        return;
    }
    if function.is_import && existing_imports.contains(&function.name) {
        return;
    }
    existing_addresses.insert(function.address);
    if function.is_import {
        existing_imports.insert(function.name.clone());
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

fn generate_elf_header_types(
    is_64bit: bool,
    image_base: u64,
    phoff: u64,
    phnum: u16,
    shoff: u64,
    shnum: u16,
) -> (
    Vec<crate::loader::types::InferredTypeInfo>,
    std::collections::HashMap<u64, String>,
) {
    use crate::loader::types::{InferredFieldInfo, InferredTypeInfo};
    let mut types = Vec::new();
    let mut symbols = std::collections::HashMap::new();

    // 1. ELF Header
    let ehdr_name = if is_64bit { "Elf64_Ehdr" } else { "Elf32_Ehdr" };
    let ehdr_size = if is_64bit { 64 } else { 52 };
    let ehdr_fields = if is_64bit {
        vec![
            InferredFieldInfo {
                name: "e_ident".to_string(),
                type_name: "unsigned char[16]".to_string(),
                offset: 0,
                size: 16,
            },
            InferredFieldInfo {
                name: "e_type".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 16,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_machine".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 18,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_version".to_string(),
                type_name: "Elf64_Word".to_string(),
                offset: 20,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_entry".to_string(),
                type_name: "Elf64_Addr".to_string(),
                offset: 24,
                size: 8,
            },
            InferredFieldInfo {
                name: "e_phoff".to_string(),
                type_name: "Elf64_Off".to_string(),
                offset: 32,
                size: 8,
            },
            InferredFieldInfo {
                name: "e_shoff".to_string(),
                type_name: "Elf64_Off".to_string(),
                offset: 40,
                size: 8,
            },
            InferredFieldInfo {
                name: "e_flags".to_string(),
                type_name: "Elf64_Word".to_string(),
                offset: 48,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_ehsize".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 52,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_phentsize".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 54,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_phnum".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 56,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shentsize".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 58,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shnum".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 60,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shstrndx".to_string(),
                type_name: "Elf64_Half".to_string(),
                offset: 62,
                size: 2,
            },
        ]
    } else {
        vec![
            InferredFieldInfo {
                name: "e_ident".to_string(),
                type_name: "unsigned char[16]".to_string(),
                offset: 0,
                size: 16,
            },
            InferredFieldInfo {
                name: "e_type".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 16,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_machine".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 18,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_version".to_string(),
                type_name: "Elf32_Word".to_string(),
                offset: 20,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_entry".to_string(),
                type_name: "Elf32_Addr".to_string(),
                offset: 24,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_phoff".to_string(),
                type_name: "Elf32_Off".to_string(),
                offset: 28,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_shoff".to_string(),
                type_name: "Elf32_Off".to_string(),
                offset: 32,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_flags".to_string(),
                type_name: "Elf32_Word".to_string(),
                offset: 36,
                size: 4,
            },
            InferredFieldInfo {
                name: "e_ehsize".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 40,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_phentsize".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 42,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_phnum".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 44,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shentsize".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 46,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shnum".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 48,
                size: 2,
            },
            InferredFieldInfo {
                name: "e_shstrndx".to_string(),
                type_name: "Elf32_Half".to_string(),
                offset: 50,
                size: 2,
            },
        ]
    };
    types.push(InferredTypeInfo {
        name: ehdr_name.to_string(),
        mangled_name: ehdr_name.to_string(),
        kind: "struct".to_string(),
        fields: ehdr_fields,
        size: ehdr_size,
        metadata_address: image_base,
    });
    symbols.insert(image_base, "ELF_HEADER".to_string());

    // 2. Program Headers
    if phoff != 0 && phnum > 0 {
        let phdr_name = if is_64bit { "Elf64_Phdr" } else { "Elf32_Phdr" };
        let phdr_size = if is_64bit { 56 } else { 32 };
        let phdr_fields = if is_64bit {
            vec![
                InferredFieldInfo {
                    name: "p_type".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 0,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_flags".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 4,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_offset".to_string(),
                    type_name: "Elf64_Off".to_string(),
                    offset: 8,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "p_vaddr".to_string(),
                    type_name: "Elf64_Addr".to_string(),
                    offset: 16,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "p_paddr".to_string(),
                    type_name: "Elf64_Addr".to_string(),
                    offset: 24,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "p_filesz".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 32,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "p_memsz".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 40,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "p_align".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 48,
                    size: 8,
                },
            ]
        } else {
            vec![
                InferredFieldInfo {
                    name: "p_type".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 0,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_offset".to_string(),
                    type_name: "Elf32_Off".to_string(),
                    offset: 4,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_vaddr".to_string(),
                    type_name: "Elf32_Addr".to_string(),
                    offset: 8,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_paddr".to_string(),
                    type_name: "Elf32_Addr".to_string(),
                    offset: 12,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_filesz".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 16,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_memsz".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 20,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_flags".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 24,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "p_align".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 28,
                    size: 4,
                },
            ]
        };
        let phdr_va = image_base + phoff;
        types.push(InferredTypeInfo {
            name: phdr_name.to_string(),
            mangled_name: phdr_name.to_string(),
            kind: "struct".to_string(),
            fields: phdr_fields,
            size: phdr_size * phnum as u32,
            metadata_address: phdr_va,
        });
        symbols.insert(phdr_va, "PROGRAM_HEADERS".to_string());
    }

    // 3. Section Headers
    if shoff != 0 && shnum > 0 {
        let shdr_name = if is_64bit { "Elf64_Shdr" } else { "Elf32_Shdr" };
        let shdr_size = if is_64bit { 64 } else { 40 };
        let shdr_fields = if is_64bit {
            vec![
                InferredFieldInfo {
                    name: "sh_name".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 0,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_type".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 4,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_flags".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 8,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "sh_addr".to_string(),
                    type_name: "Elf64_Addr".to_string(),
                    offset: 16,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "sh_offset".to_string(),
                    type_name: "Elf64_Off".to_string(),
                    offset: 24,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "sh_size".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 32,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "sh_link".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 40,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_info".to_string(),
                    type_name: "Elf64_Word".to_string(),
                    offset: 44,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_addralign".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 48,
                    size: 8,
                },
                InferredFieldInfo {
                    name: "sh_entsize".to_string(),
                    type_name: "Elf64_Xword".to_string(),
                    offset: 56,
                    size: 8,
                },
            ]
        } else {
            vec![
                InferredFieldInfo {
                    name: "sh_name".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 0,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_type".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 4,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_flags".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 8,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_addr".to_string(),
                    type_name: "Elf32_Addr".to_string(),
                    offset: 12,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_offset".to_string(),
                    type_name: "Elf32_Off".to_string(),
                    offset: 16,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_size".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 20,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_link".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 24,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_info".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 28,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_addralign".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 32,
                    size: 4,
                },
                InferredFieldInfo {
                    name: "sh_entsize".to_string(),
                    type_name: "Elf32_Word".to_string(),
                    offset: 36,
                    size: 4,
                },
            ]
        };
        let shdr_va = image_base + shoff;
        types.push(InferredTypeInfo {
            name: shdr_name.to_string(),
            mangled_name: shdr_name.to_string(),
            kind: "struct".to_string(),
            fields: shdr_fields,
            size: shdr_size * shnum as u32,
            metadata_address: shdr_va,
        });
        symbols.insert(shdr_va, "SECTION_HEADERS".to_string());
    }

    (types, symbols)
}

fn parse_relocations_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<crate::loader::types::RelocationEntry> {
    let reader = ByteReader::new(full_data, endian);
    let mut relocs = Vec::new();
    for shdr in shdrs
        .iter()
        .filter(|shdr| matches!(shdr.sh_type, SHT_RELA | SHT_REL))
    {
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let symtab = shdrs.get(shdr.sh_link as usize);
        let strtab = symtab.and_then(|sym| symbol_string_table_64(full_data, shdrs, sym));

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
            let r_type = (r_info & 0xFFFFFFFF) as u32;
            let sym_index = (r_info >> 32) as usize;

            let addend = if shdr.sh_type == SHT_RELA {
                reader.i64(offset + 16).unwrap_or(0)
            } else {
                0
            };

            let symbol_name = symtab.and_then(|sym| {
                strtab
                    .and_then(|str_tab| symbol_name_64(full_data, sym, str_tab, sym_index, endian))
            });

            let address = target_base.saturating_add(r_offset);
            let size = match r_type {
                1 => 8,           // R_X86_64_64
                2 | 10 | 11 => 4, // R_X86_64_PC32 / 32 / 32S
                _ => 8,
            };

            relocs.push(crate::loader::types::RelocationEntry {
                address,
                r_type,
                size,
                addend,
                symbol_name,
            });
        }
    }
    relocs
}

fn parse_relocations_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    section_addresses: &[u64],
    endian: Endian,
) -> Vec<crate::loader::types::RelocationEntry> {
    let reader = ByteReader::new(full_data, endian);
    let mut relocs = Vec::new();
    for shdr in shdrs
        .iter()
        .filter(|shdr| matches!(shdr.sh_type, SHT_RELA | SHT_REL))
    {
        let Some(target_base) = section_addresses.get(shdr.sh_info as usize).copied() else {
            continue;
        };
        let symtab = shdrs.get(shdr.sh_link as usize);
        let strtab = symtab.and_then(|sym| symbol_string_table_32(full_data, shdrs, sym));

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
            let r_type = (r_info & 0xFF) as u32;
            let sym_index = (r_info >> 8) as usize;

            let addend = if shdr.sh_type == SHT_RELA {
                reader.i32(offset + 8).unwrap_or(0) as i64
            } else {
                0
            };

            let symbol_name = symtab.and_then(|sym| {
                strtab
                    .and_then(|str_tab| symbol_name_32(full_data, sym, str_tab, sym_index, endian))
            });

            let address = target_base.saturating_add(r_offset as u64);
            let size = match r_type {
                2 => 4, // R_ARM_ABS32
                _ => 4,
            };

            relocs.push(crate::loader::types::RelocationEntry {
                address,
                r_type,
                size,
                addend,
                symbol_name,
            });
        }
    }
    relocs
}

fn parse_gnu_versions_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    endian: Endian,
) -> HashMap<u32, String> {
    let reader = ByteReader::new(full_data, endian);
    let mut ver_names = HashMap::new();

    // 1. Parse SHT_GNU_verdef
    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6ffffffd) {
        let Some(strtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let strtab_offset = strtab_shdr.sh_offset as usize;
        let strtab_size = strtab_shdr.sh_size as usize;
        if strtab_offset + strtab_size > full_data.len() {
            continue;
        }
        let strtab_data = &full_data[strtab_offset..strtab_offset + strtab_size];

        let mut offset = shdr.sh_offset as usize;
        let end = offset + shdr.sh_size as usize;

        while offset + 20 <= end {
            let Ok(vd_version) = reader.u16(offset) else {
                break;
            };
            if vd_version == 0 {
                break;
            }
            let Ok(vd_ndx) = reader.u16(offset + 4) else {
                break;
            };
            let Ok(vd_aux) = reader.u32(offset + 12) else {
                break;
            };
            let Ok(vd_next) = reader.u32(offset + 16) else {
                break;
            };

            let aux_offset = offset + vd_aux as usize;
            if aux_offset + 8 <= full_data.len() {
                let Ok(vda_name) = reader.u32(aux_offset) else {
                    break;
                };
                let ver_name = extract_cstring(strtab_data, vda_name as usize);
                if !ver_name.is_empty() {
                    ver_names.insert(vd_ndx as u32, ver_name);
                }
            }

            if vd_next == 0 {
                break;
            }
            offset += vd_next as usize;
        }
    }

    // 2. Parse SHT_GNU_verneed
    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6ffffffe) {
        let Some(strtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let strtab_offset = strtab_shdr.sh_offset as usize;
        let strtab_size = strtab_shdr.sh_size as usize;
        if strtab_offset + strtab_size > full_data.len() {
            continue;
        }
        let strtab_data = &full_data[strtab_offset..strtab_offset + strtab_size];

        let mut offset = shdr.sh_offset as usize;
        let end = offset + shdr.sh_size as usize;

        while offset + 16 <= end {
            let Ok(vn_version) = reader.u16(offset) else {
                break;
            };
            if vn_version == 0 {
                break;
            }
            let Ok(vn_cnt) = reader.u16(offset + 2) else {
                break;
            };
            let Ok(vn_aux) = reader.u32(offset + 8) else {
                break;
            };
            let Ok(vn_next) = reader.u32(offset + 12) else {
                break;
            };

            let mut aux_offset = offset + vn_aux as usize;
            for _ in 0..vn_cnt {
                if aux_offset + 16 > full_data.len() {
                    break;
                }
                let Ok(vna_other) = reader.u16(aux_offset + 6) else {
                    break;
                };
                let Ok(vna_name) = reader.u32(aux_offset + 8) else {
                    break;
                };
                let Ok(vna_next) = reader.u32(aux_offset + 12) else {
                    break;
                };

                let ver_name = extract_cstring(strtab_data, vna_name as usize);
                if !ver_name.is_empty() {
                    ver_names.insert(vna_other as u32, ver_name);
                }

                if vna_next == 0 {
                    break;
                }
                aux_offset += vna_next as usize;
            }

            if vn_next == 0 {
                break;
            }
            offset += vn_next as usize;
        }
    }

    ver_names
}

fn map_symbol_versions_64(
    full_data: &[u8],
    shdrs: &[Elf64Shdr],
    endian: Endian,
    ver_names: &HashMap<u32, String>,
) -> HashMap<u64, String> {
    let reader = ByteReader::new(full_data, endian);
    let mut symbol_versions = HashMap::new();

    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6fffffff) {
        let Some(symtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };

        let versym_offset = shdr.sh_offset as usize;
        let versym_count = (shdr.sh_size as usize) / 2;

        let symtab_offset = symtab_shdr.sh_offset as usize;
        let symtab_entry_size = if symtab_shdr.sh_entsize > 0 {
            symtab_shdr.sh_entsize as usize
        } else {
            Elf64Sym::SIZE
        };
        let symtab_count = (symtab_shdr.sh_size as usize) / symtab_entry_size;

        let count = versym_count.min(symtab_count);
        for idx in 0..count {
            let Ok(ver_idx) = reader.u16(versym_offset + idx * 2) else {
                break;
            };
            let clean_ver_idx = (ver_idx & 0x7FFF) as u32;

            if clean_ver_idx >= 2 {
                if let Some(ver_name) = ver_names.get(&clean_ver_idx) {
                    let sym_offset = symtab_offset + idx * symtab_entry_size;
                    if sym_offset + symtab_entry_size <= full_data.len() {
                        if let Ok(sym_val) = reader.u64(sym_offset + 8) {
                            if sym_val != 0 {
                                symbol_versions.insert(sym_val, ver_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    symbol_versions
}

fn parse_gnu_versions_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    endian: Endian,
) -> HashMap<u32, String> {
    let reader = ByteReader::new(full_data, endian);
    let mut ver_names = HashMap::new();

    // 1. Parse SHT_GNU_verdef
    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6ffffffd) {
        let Some(strtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let strtab_offset = strtab_shdr.sh_offset as usize;
        let strtab_size = strtab_shdr.sh_size as usize;
        if strtab_offset + strtab_size > full_data.len() {
            continue;
        }
        let strtab_data = &full_data[strtab_offset..strtab_offset + strtab_size];

        let mut offset = shdr.sh_offset as usize;
        let end = offset + shdr.sh_size as usize;

        while offset + 20 <= end {
            let Ok(vd_version) = reader.u16(offset) else {
                break;
            };
            if vd_version == 0 {
                break;
            }
            let Ok(vd_ndx) = reader.u16(offset + 4) else {
                break;
            };
            let Ok(vd_aux) = reader.u32(offset + 12) else {
                break;
            };
            let Ok(vd_next) = reader.u32(offset + 16) else {
                break;
            };

            let aux_offset = offset + vd_aux as usize;
            if aux_offset + 8 <= full_data.len() {
                let Ok(vda_name) = reader.u32(aux_offset) else {
                    break;
                };
                let ver_name = extract_cstring(strtab_data, vda_name as usize);
                if !ver_name.is_empty() {
                    ver_names.insert(vd_ndx as u32, ver_name);
                }
            }

            if vd_next == 0 {
                break;
            }
            offset += vd_next as usize;
        }
    }

    // 2. Parse SHT_GNU_verneed
    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6ffffffe) {
        let Some(strtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };
        let strtab_offset = strtab_shdr.sh_offset as usize;
        let strtab_size = strtab_shdr.sh_size as usize;
        if strtab_offset + strtab_size > full_data.len() {
            continue;
        }
        let strtab_data = &full_data[strtab_offset..strtab_offset + strtab_size];

        let mut offset = shdr.sh_offset as usize;
        let end = offset + shdr.sh_size as usize;

        while offset + 16 <= end {
            let Ok(vn_version) = reader.u16(offset) else {
                break;
            };
            if vn_version == 0 {
                break;
            }
            let Ok(vn_cnt) = reader.u16(offset + 2) else {
                break;
            };
            let Ok(vn_aux) = reader.u32(offset + 8) else {
                break;
            };
            let Ok(vn_next) = reader.u32(offset + 12) else {
                break;
            };

            let mut aux_offset = offset + vn_aux as usize;
            for _ in 0..vn_cnt {
                if aux_offset + 16 > full_data.len() {
                    break;
                }
                let Ok(vna_other) = reader.u16(aux_offset + 6) else {
                    break;
                };
                let Ok(vna_name) = reader.u32(aux_offset + 8) else {
                    break;
                };
                let Ok(vna_next) = reader.u32(aux_offset + 12) else {
                    break;
                };

                let ver_name = extract_cstring(strtab_data, vna_name as usize);
                if !ver_name.is_empty() {
                    ver_names.insert(vna_other as u32, ver_name);
                }

                if vna_next == 0 {
                    break;
                }
                aux_offset += vna_next as usize;
            }

            if vn_next == 0 {
                break;
            }
            offset += vn_next as usize;
        }
    }

    ver_names
}

fn map_symbol_versions_32(
    full_data: &[u8],
    shdrs: &[Elf32Shdr],
    endian: Endian,
    ver_names: &HashMap<u32, String>,
) -> HashMap<u64, String> {
    let reader = ByteReader::new(full_data, endian);
    let mut symbol_versions = HashMap::new();

    for shdr in shdrs.iter().filter(|s| s.sh_type == 0x6fffffff) {
        let Some(symtab_shdr) = shdrs.get(shdr.sh_link as usize) else {
            continue;
        };

        let versym_offset = shdr.sh_offset as usize;
        let versym_count = (shdr.sh_size as usize) / 2;

        let symtab_offset = symtab_shdr.sh_offset as usize;
        let symtab_entry_size = if symtab_shdr.sh_entsize > 0 {
            symtab_shdr.sh_entsize as usize
        } else {
            Elf32Sym::SIZE
        };
        let symtab_count = (symtab_shdr.sh_size as usize) / symtab_entry_size;

        let count = versym_count.min(symtab_count);
        for idx in 0..count {
            let Ok(ver_idx) = reader.u16(versym_offset + idx * 2) else {
                break;
            };
            let clean_ver_idx = (ver_idx & 0x7FFF) as u32;

            if clean_ver_idx >= 2 {
                if let Some(ver_name) = ver_names.get(&clean_ver_idx) {
                    let sym_offset = symtab_offset + idx * symtab_entry_size;
                    if sym_offset + symtab_entry_size <= full_data.len() {
                        if let Ok(sym_val) = reader.u32(sym_offset + 4) {
                            if sym_val != 0 {
                                symbol_versions.insert(sym_val as u64, ver_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    symbol_versions
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
            apply_arm_relocation_to_word(
                bl_self_loop,
                R_ARM_CALL,
                0x100000,
                0x100018,
                None,
                Endian::Big,
            )
            .expect("ARM CALL patch"),
            0xebff_fff8
        );
    }

    #[test]
    fn arm_thumb_movw_movt_relocations_patch_symbol_address() {
        let movw_r1_zero_le_file_word = 0x0100_f240;
        let movt_r1_zero_le_file_word = 0x0100_f2c0;

        assert_eq!(
            apply_arm_relocation_to_word(
                movw_r1_zero_le_file_word,
                R_ARM_THM_MOVW_ABS_NC,
                0x100054,
                0x100044,
                None,
                Endian::Little,
            )
            .expect("THM MOVW patch"),
            0x0154_f240
        );
        assert_eq!(
            apply_arm_relocation_to_word(
                movt_r1_zero_le_file_word,
                R_ARM_THM_MOVT_ABS,
                0x100054,
                0x100048,
                None,
                Endian::Little,
            )
            .expect("THM MOVT patch"),
            0x0110_f2c0
        );
    }

    #[test]
    fn elf32_arm_relocatable_call_relocations_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark")
            .join("binary/ARM4t_be/baremetal/small/binary/c/function_calls.o");
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

    #[test]
    fn elf32_arm_thumb_movw_movt_relocations_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark")
            .join("binary/ARM8m_le/baremetal/small/binary/c/mathematics.o");
        if !fixture.exists() {
            eprintln!("skip: ARM8m mathematics fixture missing");
            return;
        }

        let binary = LoadedBinary::from_file(&fixture).expect("load ARM8m relocatable object");
        assert_eq!(
            binary.inner().global_symbols.get(&0x100058),
            Some(&"math_sink".to_string())
        );
        let movw = binary
            .view_bytes(0x100044, 4)
            .expect("math_sink MOVW bytes");
        let movt = binary
            .view_bytes(0x100048, 4)
            .expect("math_sink MOVT bytes");
        assert_eq!(
            movw,
            [0x40, 0xf2, 0x58, 0x01],
            "R_ARM_THM_MOVW_ABS_NC should materialize math_sink low half"
        );
        assert_eq!(
            movt,
            [0xc0, 0xf2, 0x10, 0x01],
            "R_ARM_THM_MOVT_ABS should materialize math_sink high half"
        );
    }

    #[test]
    fn loongarch_branch_relocation_patch_encodes_split_immediates() {
        let bltu_a0_a1_self = 0x6800_0085;
        let bl_self = 0x5400_0000;

        assert_eq!(
            apply_loongarch_relocation_to_word(bltu_a0_a1_self, R_LARCH_B16, 0x100048, 0x100020, 0)
                .expect("B16 patch"),
            0x6800_2885
        );
        assert_eq!(
            apply_loongarch_relocation_to_word(bl_self, R_LARCH_B26, 0x100000, 0x100034, 0)
                .expect("B26 patch"),
            0x57ff_cfff
        );
    }

    #[test]
    fn elf64_loongarch_relocatable_branch_relocations_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            Path::new("../../benchmark")
                .join("binary/loongarch64_f64/baremetal/small/binary/c/function_calls.o"),
        );
        if !fixture.exists() {
            eprintln!("skip: LoongArch64 function_calls fixture missing");
            return;
        }

        let binary =
            LoadedBinary::from_file(&fixture).expect("load LoongArch64 relocatable object");
        let first_branch = binary
            .view_bytes(0x100020, 4)
            .expect("recursive_fib first branch bytes");
        let loop_branch = binary
            .view_bytes(0x100044, 4)
            .expect("recursive_fib loop branch bytes");
        assert_eq!(
            first_branch,
            [0x85, 0x28, 0x00, 0x68],
            "R_LARCH_B16 should retarget the base-case branch to 0x100048"
        );
        assert_eq!(
            loop_branch,
            [0x16, 0xef, 0xff, 0x6b],
            "R_LARCH_B16 should retarget the loop branch to 0x100030"
        );
    }

    #[test]
    fn elf32_loongarch_relocatable_branch_relocations_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            Path::new("../../benchmark")
                .join("binary/loongarch32_f64/baremetal/small/binary/c/function_calls.o"),
        );
        if !fixture.exists() {
            eprintln!("skip: LoongArch32 function_calls fixture missing");
            return;
        }

        let binary =
            LoadedBinary::from_file(&fixture).expect("load LoongArch32 relocatable object");
        let recursive_call = binary
            .view_bytes(0x100034, 4)
            .expect("recursive_fib call bytes");
        assert_eq!(
            recursive_call,
            [0xff, 0xcf, 0xff, 0x57],
            "R_LARCH_B26 should retarget recursive_fib BL to 0x100000"
        );
    }

    #[test]
    fn elf64_ppc_be_relocatable_opd_symbols_resolve_to_text_addresses() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark")
            .join("binary/ppc_64_be/baremetal/small/binary/c/function_calls.o");
        if !fixture.exists() {
            eprintln!("skip: PPC64 BE function_calls fixture missing");
            return;
        }

        let binary = LoadedBinary::from_file(&fixture).expect("load PPC64 BE relocatable object");
        let op_add = binary
            .functions
            .iter()
            .find(|function| function.name == "op_add")
            .expect("op_add symbol");
        assert_eq!(
            op_add.address, 0x100084,
            "ELFv1 .opd function descriptor must resolve to relocated .text entry"
        );
        assert_eq!(
            binary.view_bytes(op_add.address, 4).expect("op_add bytes"),
            [0x7c, 0x64, 0x1a, 0x14]
        );
    }

    #[test]
    fn elf64_x86_64_relocatable_jump_table_patch_loaded_image() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark")
            .join("binary/x86-64/baremetal/small/binary/c/control_flow.o");
        if !fixture.exists() {
            eprintln!("skip: x86-64 control_flow fixture missing");
            return;
        }

        let binary = LoadedBinary::from_file(&fixture).expect("load x86-64 relocatable object");
        let entry0 = binary.view_bytes(0x100460, 8).expect("rodata entry 0");
        let entry1 = binary.view_bytes(0x100468, 8).expect("rodata entry 1");
        let entry2 = binary.view_bytes(0x100470, 8).expect("rodata entry 2");
        let entry3 = binary.view_bytes(0x100478, 8).expect("rodata entry 3");

        assert_eq!(entry0, [0x28, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(entry1, [0x38, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(entry2, [0x2c, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(entry3, [0x33, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_parse_elf_relro_and_symbol_versions() {
        let mut data = vec![0u8; 4096];

        // 1. ELF Identification
        data[0..4].copy_from_slice(&[0x7f, 0x45, 0x4c, 0x46]); // \x7fELF
        data[4] = 2; // Class 64-bit
        data[5] = 1; // Little Endian
        data[6] = 1; // Version 1

        // 2. ELF Header
        // e_type = ET_EXEC (2)
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        // e_machine = EM_X86_64 (62)
        data[18..20].copy_from_slice(&62u16.to_le_bytes());
        // e_version = 1
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        // e_entry = 0x1000
        data[24..32].copy_from_slice(&0x1000u64.to_le_bytes());
        // e_phoff = 64
        data[32..40].copy_from_slice(&64u64.to_le_bytes());
        // e_shoff = 176 (64 + 2 * 56)
        data[40..48].copy_from_slice(&176u64.to_le_bytes());
        // e_ehsize = 64
        data[52..54].copy_from_slice(&64u16.to_le_bytes());
        // e_phentsize = 56
        data[54..56].copy_from_slice(&56u16.to_le_bytes());
        // e_phnum = 2
        data[56..58].copy_from_slice(&2u16.to_le_bytes());
        // e_shentsize = 64
        data[58..60].copy_from_slice(&64u16.to_le_bytes());
        // e_shnum = 7
        data[60..62].copy_from_slice(&7u16.to_le_bytes());
        // e_shstrndx = 6
        data[62..64].copy_from_slice(&6u16.to_le_bytes());

        // 3. Program Headers (at 64, size 56)
        // Program Header 0 (PT_LOAD)
        // p_type = 1
        data[64..68].copy_from_slice(&1u32.to_le_bytes());
        // p_flags = PF_R | PF_X (5)
        data[68..72].copy_from_slice(&5u32.to_le_bytes());
        // p_offset = 0
        data[72..80].copy_from_slice(&0u64.to_le_bytes());
        // p_vaddr = 0x1000
        data[80..88].copy_from_slice(&0x1000u64.to_le_bytes());
        // p_filesz = 1024
        data[96..104].copy_from_slice(&1024u64.to_le_bytes());
        // p_memsz = 1024
        data[104..112].copy_from_slice(&1024u64.to_le_bytes());

        // Program Header 1 (PT_GNU_RELRO)
        // p_type = 0x6474e552
        data[120..124].copy_from_slice(&0x6474e552u32.to_le_bytes());
        // p_flags = 4 (R)
        data[124..128].copy_from_slice(&4u32.to_le_bytes());
        // p_offset = 0x300
        data[128..136].copy_from_slice(&0x300u64.to_le_bytes());
        // p_vaddr = 0x2000
        data[136..144].copy_from_slice(&0x2000u64.to_le_bytes());
        // p_filesz = 0x100
        data[152..160].copy_from_slice(&0x100u64.to_le_bytes());
        // p_memsz = 0x100
        data[160..168].copy_from_slice(&0x100u64.to_le_bytes());

        // 4. Section Headers (at 176, size 64 each)
        // Section 0 (NULL) - stays all 0s

        // Section 1 (.data.rel.ro) - at 240
        // sh_name = 1 (offset into shstrtab)
        data[240..244].copy_from_slice(&1u32.to_le_bytes());
        // sh_type = 1 (SHT_PROGBITS)
        data[244..248].copy_from_slice(&1u32.to_le_bytes());
        // sh_flags = SHF_WRITE | SHF_ALLOC (3)
        data[248..256].copy_from_slice(&3u64.to_le_bytes());
        // sh_addr = 0x2000
        data[256..264].copy_from_slice(&0x2000u64.to_le_bytes());
        // sh_offset = 0x300
        data[264..272].copy_from_slice(&0x300u64.to_le_bytes());
        // sh_size = 0x100
        data[272..280].copy_from_slice(&0x100u64.to_le_bytes());

        // Section 2 (.dynsym) - at 304
        // sh_name = 14
        data[304..308].copy_from_slice(&14u32.to_le_bytes());
        // sh_type = 11 (SHT_DYNSYM)
        data[308..312].copy_from_slice(&11u32.to_le_bytes());
        // sh_flags = SHF_ALLOC (2)
        data[312..320].copy_from_slice(&2u64.to_le_bytes());
        // sh_addr = 0x2100
        data[320..328].copy_from_slice(&0x2100u64.to_le_bytes());
        // sh_offset = 0x400
        data[328..336].copy_from_slice(&0x400u64.to_le_bytes());
        // sh_size = 48 (2 entries of 24 bytes)
        data[336..344].copy_from_slice(&48u64.to_le_bytes());
        // sh_link = 3 (link to .dynstr)
        data[344..348].copy_from_slice(&3u32.to_le_bytes());
        // sh_entsize = 24
        data[360..368].copy_from_slice(&24u64.to_le_bytes());

        // Section 3 (.dynstr) - at 368
        // sh_name = 23
        data[368..372].copy_from_slice(&23u32.to_le_bytes());
        // sh_type = 3 (SHT_STRTAB)
        data[372..376].copy_from_slice(&3u32.to_le_bytes());
        // sh_flags = SHF_ALLOC (2)
        data[376..384].copy_from_slice(&2u64.to_le_bytes());
        // sh_addr = 0x2200
        data[384..392].copy_from_slice(&0x2200u64.to_le_bytes());
        // sh_offset = 0x500
        data[392..400].copy_from_slice(&0x500u64.to_le_bytes());
        // sh_size = 100
        data[400..408].copy_from_slice(&100u64.to_le_bytes());

        // Section 4 (.gnu.version) - at 432
        // sh_name = 31
        data[432..436].copy_from_slice(&31u32.to_le_bytes());
        // sh_type = 0x6fffffff (SHT_GNU_versym)
        data[436..440].copy_from_slice(&0x6fffffffu32.to_le_bytes());
        // sh_flags = SHF_ALLOC (2)
        data[440..448].copy_from_slice(&2u64.to_le_bytes());
        // sh_addr = 0x2300
        data[448..456].copy_from_slice(&0x2300u64.to_le_bytes());
        // sh_offset = 0x600
        data[456..464].copy_from_slice(&0x600u64.to_le_bytes());
        // sh_size = 4 (2 entries of 2 bytes)
        data[464..472].copy_from_slice(&4u64.to_le_bytes());
        // sh_link = 2 (link to .dynsym)
        data[472..476].copy_from_slice(&2u32.to_le_bytes());

        // Section 5 (.gnu.version_d) - at 496
        // sh_name = 45
        data[496..500].copy_from_slice(&45u32.to_le_bytes());
        // sh_type = 0x6ffffffd (SHT_GNU_verdef)
        data[500..504].copy_from_slice(&0x6ffffffdu32.to_le_bytes());
        // sh_flags = SHF_ALLOC (2)
        data[504..512].copy_from_slice(&2u64.to_le_bytes());
        // sh_addr = 0x2400
        data[512..520].copy_from_slice(&0x2400u64.to_le_bytes());
        // sh_offset = 0x700
        data[520..528].copy_from_slice(&0x700u64.to_le_bytes());
        // sh_size = 32
        data[528..536].copy_from_slice(&32u64.to_le_bytes());
        // sh_link = 3 (link to .dynstr)
        data[536..540].copy_from_slice(&3u32.to_le_bytes());

        // Section 6 (.shstrtab) - at 560
        // sh_name = 60
        data[560..564].copy_from_slice(&60u32.to_le_bytes());
        // sh_type = 3 (SHT_STRTAB)
        data[564..568].copy_from_slice(&3u32.to_le_bytes());
        // sh_offset = 0x800
        data[584..592].copy_from_slice(&0x800u64.to_le_bytes());
        // sh_size = 100
        data[592..600].copy_from_slice(&100u64.to_le_bytes());

        // 5. Dynsym content (at 0x400)
        // Entry 0: all zeroes
        // Entry 1: symbol name = 0, address = 0x2020 (st_value is at offset 8, size 8)
        data[0x400 + 24 + 8..0x400 + 24 + 16].copy_from_slice(&0x2020u64.to_le_bytes());
        // shndx = 1 (offset 6, 2 bytes)
        data[0x400 + 24 + 6..0x400 + 24 + 8].copy_from_slice(&1u16.to_le_bytes());

        // 6. Dynstr content (at 0x500)
        // Strtab index 0 starts with name: "GLIBC_2.2.5\0"
        data[0x500..0x50c].copy_from_slice(b"GLIBC_2.2.5\0");

        // 7. Versym content (at 0x600)
        // Entry 0: 0
        // Entry 1: 2
        data[0x600..0x602].copy_from_slice(&0u16.to_le_bytes());
        data[0x602..0x604].copy_from_slice(&2u16.to_le_bytes());

        // 8. Verdef content (at 0x700)
        // vd_version = 1
        data[0x700..0x702].copy_from_slice(&1u16.to_le_bytes());
        // vd_ndx = 2
        data[0x704..0x706].copy_from_slice(&2u16.to_le_bytes());
        // vd_cnt = 1
        data[0x706..0x708].copy_from_slice(&1u16.to_le_bytes());
        // vd_aux = 20
        data[0x70C..0x710].copy_from_slice(&20u32.to_le_bytes());
        // Aux structure (at 0x714): vda_name = 0
        data[0x714..0x718].copy_from_slice(&0u32.to_le_bytes());

        // 9. Shstrtab content (at 0x800)
        // Section names
        let shstr_data =
            b"\0.data.rel.ro\0.dynsym\0.dynstr\0.gnu.version\0.gnu.version_d\0.shstrtab\0";
        data[0x800..0x800 + shstr_data.len()].copy_from_slice(shstr_data);

        let result = ElfLoader::parse(DataBuffer::Heap(data), "synthetic_elf".to_string());
        assert!(result.is_ok());
        let bin = result.expect("synthetic elf should parse");

        // Verify PT_GNU_RELRO marked writable = false
        let relro_sec = bin
            .sections
            .iter()
            .find(|s| s.name == ".data.rel.ro")
            .expect("find .data.rel.ro");
        assert_eq!(
            relro_sec.is_writable, false,
            ".data.rel.ro should be marked read-only by RELRO"
        );

        // Verify GNU Version parsed
        let ver = bin
            .symbol_versions
            .get(&0x2020)
            .expect("should find version for address 0x2020");
        assert_eq!(ver, "GLIBC_2.2.5");
    }
}
