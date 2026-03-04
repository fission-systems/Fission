use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use binrw::BinRead;
use std::io::{Cursor, Seek, SeekFrom};

pub mod schema;
use schema::*;

pub struct ElfLoader;

impl ElfLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        // 1. Read Identification (first 16 bytes)
        // We use a temporary cursor here so the borrow of `data` ends immediately
        let ident = {
            let bytes = data.as_slice();
            let mut cursor = Cursor::new(bytes);
            ElfIdent::read_le(&mut cursor).map_err(|e| err!(loader, "Invalid ELF Ident: {}", e))?
        }; // cursor dropped here

        let is_64 = ident.class == 2;
        let is_little = ident.endian == 1; // 1=Little, 2=Big

        // Now we can move `data`
        if is_64 {
            Self::parse_64(
                data,
                path,
                if is_little {
                    binrw::Endian::Little
                } else {
                    binrw::Endian::Big
                },
            )
        } else {
            Self::parse_32(
                data,
                path,
                if is_little {
                    binrw::Endian::Little
                } else {
                    binrw::Endian::Big
                },
            )
        }
    }

    fn parse_64(data: DataBuffer, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let mut reader = Cursor::new(bytes);
        // Read Header
        let header = Elf64Header::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "ELF64 Header: {}", e))?;

        let is_64bit = true;
        let entry_point = header.entry;

        // Machine Arch
        let arch_spec = match header.machine {
            0x3E => "x86:LE:64:default", // AMD64
            0xB7 => "AARCH64:LE:64:v8A", // AArch64
            _ => "x86:LE:64:default",
        };

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;

        // Parse Sections
        if header.shoff != 0 && header.shnum > 0 {
            reader
                .seek(SeekFrom::Start(header.shoff))
                .map_err(|_| err!(loader, "Seek error"))?;

            let mut shdrs = Vec::new();
            for _ in 0..header.shnum {
                shdrs.push(
                    Elf64Shdr::read_options(&mut reader, endian, ())
                        .map_err(|_| err!(loader, "Failed to read ELF64 section header"))?,
                );
            }

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

            for shdr in &shdrs {
                // Calculate simplified Image Base (lowest VA of loadable section)
                if (shdr.sh_flags & 0x2) != 0 && shdr.sh_addr < image_base && shdr.sh_addr != 0 {
                    image_base = shdr.sh_addr;
                }

                let name = extract_cstring(&strtab_data, shdr.sh_name as usize);

                sections_info.push(SectionInfo {
                    name: name.clone(),
                    virtual_address: shdr.sh_addr,
                    virtual_size: shdr.sh_size, // ELF does not distinguish VSize/RawSize clearly in SH, mostly same
                    file_offset: shdr.sh_offset,
                    file_size: shdr.sh_size, // except NOBITS
                    is_executable: (shdr.sh_flags & 0x4) != 0,
                    is_readable: (shdr.sh_flags & 0x2) != 0,
                    is_writable: (shdr.sh_flags & 0x1) != 0,
                });

                // If this is a symbol table, read functions
                if shdr.sh_type == 2 || shdr.sh_type == 11 {
                    // SYMTAB or DYNSYM
                    Self::parse_symbols_64(
                        bytes,
                        shdr.sh_offset,
                        shdr.sh_size,
                        shdr.sh_entsize,
                        shdr.sh_link as usize, // Link to String Table
                        &shdrs,
                        &mut functions_info,
                        endian,
                    );
                }
            }
        }

        if image_base == u64::MAX {
            image_base = 0;
        }

        // Entry point fallback
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        LoadedBinaryBuilder::new(path, data)
            .format("ELF64 (binrw)")
            .arch_spec(arch_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .build()
    }

    fn parse_32(data: DataBuffer, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let mut reader = Cursor::new(bytes);
        // Read Header
        let header = Elf32Header::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "ELF32 Header: {}", e))?;

        let is_64bit = false;
        let entry_point = header.entry as u64;

        // Machine Arch
        let arch_spec = match header.machine {
            0x03 => "x86:LE:32:default", // 386
            0x28 => "ARM:LE:32:v7",      // ARM
            _ => "x86:LE:32:default",
        };

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;

        // Parse Sections
        if header.shoff != 0 && header.shnum > 0 {
            reader
                .seek(SeekFrom::Start(header.shoff as u64))
                .map_err(|_| err!(loader, "Seek error"))?;

            let mut shdrs = Vec::new();
            for _ in 0..header.shnum {
                shdrs.push(
                    Elf32Shdr::read_options(&mut reader, endian, ())
                        .map_err(|_| err!(loader, "Failed to read ELF32 section header"))?,
                );
            }

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

            for shdr in &shdrs {
                // Calculate simplified Image Base
                if (shdr.sh_flags & 0x2) != 0
                    && (shdr.sh_addr as u64) < image_base
                    && shdr.sh_addr != 0
                {
                    image_base = shdr.sh_addr as u64;
                }

                let name = extract_cstring(&strtab_data, shdr.sh_name as usize);

                sections_info.push(SectionInfo {
                    name,
                    virtual_address: shdr.sh_addr as u64,
                    virtual_size: shdr.sh_size as u64,
                    file_offset: shdr.sh_offset as u64,
                    file_size: shdr.sh_size as u64,
                    is_executable: (shdr.sh_flags & 0x4) != 0,
                    is_readable: (shdr.sh_flags & 0x2) != 0,
                    is_writable: (shdr.sh_flags & 0x1) != 0,
                });

                // If this is a symbol table, read functions
                if shdr.sh_type == 2 || shdr.sh_type == 11 {
                    Self::parse_symbols_32(
                        bytes,
                        shdr.sh_offset as u64,
                        shdr.sh_size as u64,
                        shdr.sh_entsize as u64,
                        shdr.sh_link as usize,
                        &shdrs,
                        &mut functions_info,
                        endian,
                    );
                }
            }
        }

        if image_base == u64::MAX {
            image_base = 0;
        }

        // Entry point fallback
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        LoadedBinaryBuilder::new(path, data)
            .format("ELF32 (binrw)")
            .arch_spec(arch_spec)
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
        out_funcs: &mut Vec<FunctionInfo>,
        endian: binrw::Endian,
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
        let count = if entry_size > 0 {
            size as usize / entry_size
        } else {
            0
        };

        let sym_start = offset as usize;
        let sym_end = sym_start + size as usize;
        if sym_end > full_data.len() {
            return;
        }

        let mut reader = Cursor::new(&full_data[sym_start..sym_end]);
        for _ in 0..count {
            let sym = match Elf64Sym::read_options(&mut reader, endian, ()) {
                Ok(s) => s,
                Err(_) => break,
            };

            // STT_FUNC = 2 (lower 4 bits of st_info)
            let sym_type = sym.st_info & 0xf;
            if sym_type != 2 {
                continue;
            }

            // Skip undefined symbols (SHN_UNDEF = 0)
            if sym.st_shndx == 0 {
                continue;
            }

            let name = extract_cstring(strtab, sym.st_name as usize);
            if name.is_empty() {
                continue;
            }

            // STB_GLOBAL = 1, STB_WEAK = 2 (upper 4 bits)
            let binding = sym.st_info >> 4;
            let is_export = binding == 1 || binding == 2;

            // Deduplicate by address
            if out_funcs.iter().any(|f| f.address == sym.st_value) {
                continue;
            }

            out_funcs.push(FunctionInfo {
                name,
                address: sym.st_value,
                size: sym.st_size,
                is_export,
                is_import: false,
            });
        }
    }

    fn parse_symbols_32(
        full_data: &[u8],
        offset: u64,
        size: u64,
        entsize: u64,
        strtab_shndx: usize,
        shdrs: &[Elf32Shdr],
        out_funcs: &mut Vec<FunctionInfo>,
        endian: binrw::Endian,
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
        let count = if entry_size > 0 {
            size as usize / entry_size
        } else {
            0
        };

        let sym_start = offset as usize;
        let sym_end = sym_start + size as usize;
        if sym_end > full_data.len() {
            return;
        }

        let mut reader = Cursor::new(&full_data[sym_start..sym_end]);
        for _ in 0..count {
            let sym = match Elf32Sym::read_options(&mut reader, endian, ()) {
                Ok(s) => s,
                Err(_) => break,
            };

            let sym_type = sym.st_info & 0xf;
            if sym_type != 2 {
                continue;
            }
            if sym.st_shndx == 0 {
                continue;
            }

            let name = extract_cstring(strtab, sym.st_name as usize);
            if name.is_empty() {
                continue;
            }

            let binding = sym.st_info >> 4;
            let is_export = binding == 1 || binding == 2;

            if out_funcs.iter().any(|f| f.address == sym.st_value as u64) {
                continue;
            }

            out_funcs.push(FunctionInfo {
                name,
                address: sym.st_value as u64,
                size: sym.st_size as u64,
                is_export,
                is_import: false,
            });
        }
    }
}
