use crate::analysis::loader::types::{
    extract_cstring, extract_fixed_string, FunctionInfo, LoadedBinary, LoadedBinaryBuilder,
    SectionInfo,
};
use crate::prelude::*;
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
            0x1000007 | 0x7 => "x86:LE:64:default", // x86_64 (CPU_TYPE_X86_64)
            0x100000C | 0xC => "AARCH64:LE:64:v8A", // ARM64 (CPU_TYPE_ARM64)
            _ => {
                eprintln!("[Warning] Unknown Mach-O CPU type: {} (0x{:X}), defaulting to x86_64", cputype, cputype);
                "x86:LE:64:default"
            }
        };

        let mut sections_info = Vec::new();
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;
        let entry_point = 0; // Mach-O entry is usually in LC_MAIN or LC_UNIXTHREAD, tricky to parse fully for POC
        
        // Store symbol table info for later use
        let mut symtab_info: Option<SymtabCommand> = None;
        let mut dysymtab_info: Option<DysymtabCommand> = None;

        // First pass: collect segment/section info and load commands
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
                symtab_info = Some(symtab.clone());

                // Access Symbols (random access)
                Self::parse_symbols_64(&data, &symtab, endian, &mut functions_info);
            } else if cmd_header.cmd == LC_DYSYMTAB {
                let dysymtab = DysymtabCommand::read_options(&mut reader, endian, ()).unwrap();
                dysymtab_info = Some(dysymtab);
            }

            // Skip command
            reader
                .seek(SeekFrom::Start(cmd_start + cmd_header.cmdsize as u64))
                .unwrap();
        }

        if image_base == u64::MAX {
            image_base = 0;
        }
        
        // Parse dynamic symbols to get external function imports
        let mut iat_symbols = std::collections::HashMap::new();
        if let (Some(symtab), Some(dysymtab)) = (symtab_info, dysymtab_info) {
            Self::parse_dynamic_symbols_64(
                &data,
                &symtab,
                &dysymtab,
                &sections_info,
                endian,
                &mut iat_symbols,
            );
        }

        LoadedBinaryBuilder::new(path, data)
            .format("Mach-O 64 (binrw)")
            .arch_spec(arch_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .build()
    }

    fn parse_32(data: Vec<u8>, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let mut reader = Cursor::new(&data);
        let header = MachHeader32::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "MachO32 Header: {}", e))?;

        let is_64bit = false;
        let cputype = header.cputype;
        let arch_spec = match cputype {
            0x7 => "x86:LE:32:default",      // x86 (CPU_TYPE_X86)
            0xC => "ARM:LE:32:v7",           // ARM (CPU_TYPE_ARM)
            _ => {
                eprintln!("[Warning] Unknown Mach-O CPU type: {} (0x{:X}), defaulting to x86", cputype, cputype);
                "x86:LE:32:default"
            }
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
                    // Use checked_add to prevent potential overflow
                    let name_offset = (str_off as usize).checked_add(nlist.n_strx as usize);
                    let extracted_name = match name_offset {
                        Some(offset) if offset < data.len() => extract_cstring(data, offset),
                        _ => String::new(),
                    };

                    // Use fallback name if extracted name is empty or extraction failed
                    let final_name = if extracted_name.is_empty() {
                        format!("sub_{:x}", nlist.n_value)
                    } else {
                        extracted_name
                    };

                    out.push(FunctionInfo {
                        name: final_name,
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
    
    fn parse_dynamic_symbols_64(
        data: &[u8],
        symtab: &SymtabCommand,
        dysymtab: &DysymtabCommand,
        sections: &[SectionInfo],
        endian: binrw::Endian,
        iat_symbols: &mut std::collections::HashMap<u64, String>,
    ) {
        // Find __stubs and __got sections
        let stubs_section = sections.iter().find(|s| s.name == "__stubs");
        let got_section = sections.iter().find(|s| s.name == "__got");
        
        if dysymtab.nindirectsyms == 0 {
            return;
        }
        
        let mut reader = Cursor::new(data);
        let indirect_off = dysymtab.indirectsymoff as u64;
        
        if indirect_off as usize + (dysymtab.nindirectsyms as usize * 4) > data.len() {
            return;
        }
        
        // Parse __stubs section
        if let Some(stubs) = stubs_section {
            let stub_size = 6; // Standard stub size on x86_64: jmp *offset(%rip) = 6 bytes
            let num_stubs = (stubs.virtual_size / stub_size).min(dysymtab.nindirectsyms as u64);
            
            for i in 0..num_stubs {
                let stub_addr = stubs.virtual_address + (i * stub_size);
                
                // Read indirect symbol table entry
                reader.set_position(indirect_off + (i * 4));
                if let Ok(sym_idx) = u32::read_options(&mut reader, endian, ()) {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(stub_addr, name);
                        }
                    }
                }
            }
        }
        
        // Parse __got section
        if let Some(got) = got_section {
            let entry_size = 8; // GOT entry is 8 bytes on 64-bit
            let num_entries = (got.virtual_size / entry_size).min(dysymtab.nindirectsyms as u64);
            
            // GOT entries come after stubs in indirect symbol table
            let stubs_count = if let Some(stubs) = stubs_section {
                (stubs.virtual_size / 6) as u32
            } else {
                0
            };
            
            for i in 0..num_entries {
                let got_addr = got.virtual_address + (i * entry_size);
                
                // Read indirect symbol table entry (offset by stubs count)
                reader.set_position(indirect_off + ((stubs_count as u64 + i) * 4));
                if let Ok(sym_idx) = u32::read_options(&mut reader, endian, ()) {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(got_addr, name);
                        }
                    }
                }
            }
        }
    }
    
    fn get_symbol_name(
        data: &[u8],
        symtab: &SymtabCommand,
        sym_idx: u32,
        endian: binrw::Endian,
    ) -> String {
        let sym_off = symtab.symoff as u64 + (sym_idx as u64 * 16); // Nlist64 is 16 bytes
        let mut reader = Cursor::new(data);
        reader.set_position(sym_off);
        
        if let Ok(nlist) = Nlist64::read_options(&mut reader, endian, ()) {
            let str_off = symtab.stroff as usize + nlist.n_strx as usize;
            if str_off < data.len() {
                return extract_cstring(data, str_off);
            }
        }
        String::new()
    }
}
