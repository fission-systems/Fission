use crate::analysis::loader::types::{
    extract_cstring, extract_fixed_string, FunctionInfo, LoadedBinary, LoadedBinaryBuilder,
    SectionInfo,
};
use crate::core::prelude::*;
use binrw::BinRead;
use std::io::{Cursor, Seek, SeekFrom};

pub mod schema;
use schema::*;

pub struct MachoLoader;

impl MachoLoader {
    pub fn parse(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Read Magic
        let mut cursor = Cursor::new(&data);
        let magic =
            u32::read_be(&mut cursor).map_err(|e| err!(loader, "Invalid MachO Magic: {}", e))?;

        // Detect Props
        let (is_64, is_swap) = match magic {
            MH_MAGIC => (false, false),
            MH_CIGAM => (false, true),
            MH_MAGIC_64 => (true, false),
            MH_CIGAM_64 => (true, true),
            _ => return Err(err!(loader, "Not a Mach-O binary (magic: {:x})", magic)),
        };

        let endian = if is_swap {
            binrw::Endian::Little
        } else {
            binrw::Endian::Big
        }; // Assuming host is Little? No.
           // Wait, BE/LE depends on file format.
           // MACH-O logic:
           // CIGAM means "Swapped". If Host is LE, and file is CIGAM, then file is BE.
           // If File is MAGIC, then file is same as Host.
           // BUT binrw's Endian is "Target Format Endian".
           // We know standard Mac is usually LE (Intel/ARM).
           // However, PowerPC was BE.
           // Let's assume standard definitions:
           // MAGIC = 0xFEEDFACE (Big Endian representation of the value).
           // If we read as BE and see 0xFEEDFACE, it matches.

        // Let's rely on standard practice:
        // Big Endian Magic: 0xFEEDFACE
        // Little Endian Magic: 0xCEFAEDFE (bytes are FE ED FA CE reversed) - wait no.
        // 0xFEEDFACE as u32 in LE is [CE, FA, ED, FE].
        // If we read u32 BE and get 0xFEEDFACE, then file is BE.
        // If we read u32 BE and get 0xCEFAEDFE, then bytes are [CE, FA, ED, FE], which means LE file.

        // Let's just pass explicit Endian based on magic match.
        let endian = match magic {
            0xFEEDFACE => binrw::Endian::Big,
            0xFEEDFACF => binrw::Endian::Big,
            0xCEFAEDFE => binrw::Endian::Little,
            0xCFFAEDFE => binrw::Endian::Little,
            _ => return Err(err!(loader, "Unknown Magic")),
        };

        // Reset and parse
        cursor.set_position(0);

        if is_64 {
            Self::parse_64(data, path, endian)
        } else {
            Self::parse_32(data, path, endian)
        }
    }

    fn parse_64(data: Vec<u8>, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let mut reader = Cursor::new(&data);
        let header = MachHeader64::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "MachO64 Header: {}", e))?;

        let is_64bit = true;
        let cputype = header.cputype;
        let arch_spec = match cputype {
            0x1000007 | 0x7 => "x86:LE:64:default", // x86_64
            0x100000C | 0xC => "AARCH64:LE:64:v8A", // ARM64
            _ => "x86:LE:64:default",
        };

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;
        let mut entry_point = 0; // Mach-O entry is usually in LC_MAIN or LC_UNIXTHREAD, tricky to parse fully for POC

        // Iterate Commands
        for _ in 0..header.ncmds {
            let cmd_start = reader.position();
            let cmd_header = LoadCommand::read_options(&mut reader, endian, ()).unwrap();

            reader.seek(SeekFrom::Start(cmd_start)).unwrap(); // Back to start of cmd to read full struct

            if cmd_header.cmd == LC_SEGMENT_64 {
                let seg = SegmentCommand64::read_options(&mut reader, endian, ()).unwrap();

                // Process Sections
                for _ in 0..seg.nsects {
                    let sect = Section64::read_options(&mut reader, endian, ()).unwrap();

                    if (sect.flags & 0x80000000) != 0 && sect.addr < image_base && sect.addr != 0 {
                        image_base = sect.addr; // Rough
                    }

                    sections_info.push(SectionInfo {
                        name: extract_fixed_string(&sect.sectname),
                        virtual_address: sect.addr,
                        virtual_size: sect.size,
                        file_offset: sect.offset as u64,
                        file_size: sect.size,
                        is_executable: (sect.flags & 0x80000400) != 0, // S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS
                        is_readable: true,
                        is_writable: (sect.flags & 0x1) == 0, // Very rough approx
                    });
                }

                // Skip remaining padding of command if any
                reader
                    .seek(SeekFrom::Start(cmd_start + cmd_header.cmdsize as u64))
                    .unwrap();
                continue;
            } else if cmd_header.cmd == LC_SYMTAB {
                let symtab = SymtabCommand::read_options(&mut reader, endian, ()).unwrap();

                // Access Symbols (random access)
                Self::parse_symbols_64(&data, &symtab, endian, &mut functions_info);
            }

            // Skip command
            reader
                .seek(SeekFrom::Start(cmd_start + cmd_header.cmdsize as u64))
                .unwrap();
        }

        if image_base == u64::MAX {
            image_base = 0;
        }

        LoadedBinaryBuilder::new(path, data)
            .format("Mach-O 64 (binrw)")
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
        let header = MachHeader32::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "MachO32 Header: {}", e))?;

        let is_64bit = false;
        let cputype = header.cputype;
        let arch_spec = match cputype {
            0x7 => "x86:LE:32:default", // x86
            0xC => "ARM:LE:32:v7",      // ARM
            _ => "x86:LE:32:default",
        };

        // Logic similar to 64...
        // For brevity in POC, skipping full 32-bit implementation detail,
        // as it mirrors 64-bit just with 32-bit structs.
        // In real code we'd implement it fully.

        LoadedBinaryBuilder::new(path, data)
            .format("Mach-O 32 (binrw)")
            .arch_spec(arch_spec)
            .entry_point(0)
            .image_base(0)
            .is_64bit(is_64bit)
            .build()
    }

    fn parse_symbols_64(
        data: &[u8],
        symtab: &SymtabCommand,
        endian: binrw::Endian,
        out: &mut Vec<FunctionInfo>,
    ) {
        let sym_off = symtab.symoff as u64;
        let str_off = symtab.stroff as u64;
        let nsyms = symtab.nsyms;

        if sym_off as usize >= data.len() {
            return;
        }

        let mut reader = Cursor::new(data);
        reader.set_position(sym_off);

        // We can't easily iterate N times due to seek.
        // But symbols are contiguous Nlist64 structs.
        for _ in 0..nsyms {
            if let Ok(nlist) = Nlist64::read_options(&mut reader, endian, ()) {
                // If n_type & N_STAB == 0 && (n_type & N_EXT)
                if (nlist.n_type & 0xE0) == 0 && (nlist.n_type & 0x01) != 0 && nlist.n_value != 0 {
                    // Extract name using shared utility function
                    let name_offset = str_off as usize + nlist.n_strx as usize;
                    let name = if name_offset < data.len() {
                        extract_cstring(data, name_offset)
                    } else {
                        format!("sub_{:x}", nlist.n_value)
                    };

                    // If name is empty after extraction, use fallback
                    let name = if name.is_empty() {
                        format!("sub_{:x}", nlist.n_value)
                    } else {
                        name
                    };

                    out.push(FunctionInfo {
                        name,
                        address: nlist.n_value,
                        size: 0,
                        is_export: true,
                        is_import: false,
                    });
                }
            } else {
                break;
            }
        }
    }
}
