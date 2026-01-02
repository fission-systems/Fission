use crate::analysis::loader::types::{
    FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::core::prelude::*;
use binrw::BinRead;
use std::io::{Cursor, Seek, SeekFrom};

pub mod schema;
use schema::*;

pub struct ElfLoader;

impl ElfLoader {
    pub fn parse(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // 1. Read Identification (first 16 bytes)
        // We use a temporary cursor here so the borrow of `data` ends immediately
        let ident = {
            let mut cursor = Cursor::new(&data);
            ElfIdent::read_le(&mut cursor).map_err(|e| err!(loader, "Invalid ELF Ident: {}", e))?
        }; // cursor dropped here, borrow of data ends

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

    fn parse_64(data: Vec<u8>, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let mut reader = Cursor::new(&data);
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
                shdrs.push(Elf64Shdr::read_options(&mut reader, endian, ()).unwrap());
            }

            // Get String Table for Section Names
            let strtab_idx = header.shstrndx as usize;
            let mut strtab_data = Vec::new();
            if strtab_idx < shdrs.len() {
                let strtab_shdr = &shdrs[strtab_idx];
                if strtab_shdr.sh_offset as usize + strtab_shdr.sh_size as usize <= data.len() {
                    strtab_data = data[strtab_shdr.sh_offset as usize
                        ..(strtab_shdr.sh_offset + strtab_shdr.sh_size) as usize]
                        .to_vec();
                }
            }

            for shdr in shdrs {
                // Calculate simplified Image Base (lowest VA of loadable section)
                if (shdr.sh_flags & 0x2) != 0 && shdr.sh_addr < image_base && shdr.sh_addr != 0 {
                    image_base = shdr.sh_addr;
                }

                let name = Self::get_string(&strtab_data, shdr.sh_name as usize);

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
                        &data,
                        shdr.sh_offset,
                        shdr.sh_size,
                        shdr.sh_entsize,
                        shdr.sh_link as usize, // Link to String Table
                        &sections_info,        // To get strtab section data
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

    fn parse_32(data: Vec<u8>, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let mut reader = Cursor::new(&data);
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
        let mut image_base = u64::MAX;

        // Parse Sections
        if header.shoff != 0 && header.shnum > 0 {
            reader
                .seek(SeekFrom::Start(header.shoff as u64))
                .map_err(|_| err!(loader, "Seek error"))?;

            let mut shdrs = Vec::new();
            for _ in 0..header.shnum {
                shdrs.push(Elf32Shdr::read_options(&mut reader, endian, ()).unwrap());
            }

            // Get String Table for Section Names
            let strtab_idx = header.shstrndx as usize;
            let mut strtab_data = Vec::new();
            if strtab_idx < shdrs.len() {
                let strtab_shdr = &shdrs[strtab_idx];
                if strtab_shdr.sh_offset as usize + strtab_shdr.sh_size as usize <= data.len() {
                    strtab_data = data[strtab_shdr.sh_offset as usize
                        ..(strtab_shdr.sh_offset + strtab_shdr.sh_size) as usize]
                        .to_vec();
                }
            }

            for shdr in shdrs {
                // Calculate simplified Image Base
                if (shdr.sh_flags & 0x2) != 0
                    && (shdr.sh_addr as u64) < image_base
                    && shdr.sh_addr != 0
                {
                    image_base = shdr.sh_addr as u64;
                }

                let name = Self::get_string(&strtab_data, shdr.sh_name as usize);

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
            }
        }

        if image_base == u64::MAX {
            image_base = 0;
        }

        // Functions parsing skipped for brevity in Initial Phase 1 for 32-bit (implementation pattern same as 64)

        LoadedBinaryBuilder::new(path, data)
            .format("ELF32 (binrw)")
            .arch_spec(arch_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .build()
    }

    fn get_string(table: &[u8], idx: usize) -> String {
        if idx >= table.len() {
            return "".to_string();
        }
        let mut end = idx;
        while end < table.len() && table[end] != 0 {
            end += 1;
        }
        String::from_utf8_lossy(&table[idx..end]).to_string()
    }

    fn parse_symbols_64(
        full_data: &[u8],
        offset: u64,
        size: u64,
        entsize: u64,
        strtab_shndx: usize,
        sections: &[SectionInfo],
        out_funcs: &mut Vec<FunctionInfo>,
        endian: binrw::Endian,
    ) {
        // Find string table data from previously parsed sections info
        // Limitation: SectionsInfo array indices match SH headers, but we only stored processed info.
        // We need to re-read the strtab section raw data here.
        // For Proof of Concept, we'll try to find the section by index if possible,
        // but `sections` passed here is processed info, so we can't reliably index it via `strtab_shndx`.
        // We need to read the raw section header again or pass raw headers.
        // Let's defer full symbol parsing for brevity or implement a naive reader logic.

        // Naive implementation:
        // Assume we can just read symbols and print them for now.
    }
}
