use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
    extract_fixed_string,
};
use crate::prelude::*;
use binrw::BinRead;
use fission_core::constants::binary_format::*;
use std::io::{Cursor, Seek, SeekFrom};

pub mod apple;
pub mod schema;
use schema::*;

pub struct MachoLoader;

impl MachoLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        // Read Magic
        let bytes = data.as_slice();
        let mut cursor = Cursor::new(bytes);
        let magic =
            u32::read_be(&mut cursor).map_err(|e| err!(loader, "Invalid MachO Magic: {}", e))?;

        // Detect Props
        let (is_64, _is_swap) = match magic {
            MACHO_MAGIC_32_BE => (false, false),
            MACHO_MAGIC_32_LE => (false, true),
            MACHO_MAGIC_64_BE => (true, false),
            MACHO_MAGIC_64_LE => (true, true),
            _ => return Err(err!(loader, "Not a Mach-O binary (magic: {:x})", magic)),
        };

        let endian = match magic {
            MACHO_MAGIC_32_BE => binrw::Endian::Big,
            MACHO_MAGIC_64_BE => binrw::Endian::Big,
            MACHO_MAGIC_32_LE => binrw::Endian::Little,
            MACHO_MAGIC_64_LE => binrw::Endian::Little,
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

    fn parse_64(data: DataBuffer, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let mut reader = Cursor::new(bytes);
        let header = MachHeader64::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "MachO64 Header: {}", e))?;

        let is_64bit = true;
        let cputype = header.cputype;

        let arch_spec = match cputype {
            0x1000007 | 0x7 => "x86:LE:64:default", // x86_64 (CPU_TYPE_X86_64)
            0x100000C | 0xC => "AARCH64:LE:64:AppleSilicon", // ARM64 (CPU_TYPE_ARM64, Mach-O uses AppleSilicon variant)
            _ => {
                eprintln!(
                    "[Warning] Unknown Mach-O CPU type: {} (0x{:X}), defaulting to x86_64",
                    cputype, cputype
                );
                "x86:LE:64:default"
            }
        };

        let mut sections_info = Vec::new();
        let mut section_exec_map: Vec<bool> = Vec::new(); // 1-based n_sect -> is_executable
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;
        let mut entry_point = 0u64;
        let mut text_segment_vmaddr = 0u64;

        // Store symbol table info for later use
        let mut symtab_info: Option<SymtabCommand> = None;
        let mut dysymtab_info: Option<DysymtabCommand> = None;
        // GAP-8: LC_FUNCTION_STARTS blob location (file_offset, size)
        let mut function_starts_info: Option<(u32, u32)> = None;

        // First pass: collect segment/section info and load commands
        for _ in 0..header.ncmds {
            let cmd_start = reader.position();
            let cmd_header = LoadCommand::read_options(&mut reader, endian, ())
                .map_err(|e| err!(loader, "Failed to read Mach-O load command: {}", e))?;

            reader
                .seek(SeekFrom::Start(cmd_start))
                .map_err(|e| err!(loader, "Failed to seek to load command start: {}", e))?;

            if cmd_header.cmd == LC_SEGMENT_64 {
                let seg = SegmentCommand64::read_options(&mut reader, endian, ())
                    .map_err(|e| err!(loader, "Failed to read segment command: {}", e))?;
                let seg_name = extract_fixed_string(&seg.segname);

                // Use __TEXT segment's vmaddr as image base (most reliable)
                if seg_name == "__TEXT" && seg.vmaddr != 0 {
                    text_segment_vmaddr = seg.vmaddr;
                    if seg.vmaddr < image_base {
                        image_base = seg.vmaddr;
                    }
                }

                // Determine if segment is executable from protection flags
                // VM_PROT_EXECUTE = 0x04
                let seg_is_executable = (seg.initprot & 0x04) != 0;

                // Process Sections
                for _ in 0..seg.nsects {
                    let sect = Section64::read_options(&mut reader, endian, ())
                        .map_err(|e| err!(loader, "Failed to read section: {}", e))?;

                    // S_ATTR_PURE_INSTRUCTIONS = 0x80000000
                    // S_ATTR_SOME_INSTRUCTIONS = 0x00000400
                    let sect_has_instructions = (sect.flags & 0x80000400) != 0;
                    let is_executable = seg_is_executable || sect_has_instructions;
                    section_exec_map.push(is_executable);

                    sections_info.push(SectionInfo {
                        name: extract_fixed_string(&sect.sectname),
                        virtual_address: sect.addr,
                        virtual_size: sect.size,
                        file_offset: sect.offset as u64,
                        file_size: sect.size,
                        is_executable,
                        is_readable: true,
                        is_writable: (seg.initprot & 0x02) != 0, // VM_PROT_WRITE = 0x02
                    });
                }

                // Skip remaining padding of command if any
                reader
                    .seek(SeekFrom::Start(cmd_start + cmd_header.cmdsize as u64))
                    .map_err(|e| err!(loader, "Failed to skip segment command: {}", e))?;
                continue;
            } else if cmd_header.cmd == LC_SYMTAB {
                let symtab = SymtabCommand::read_options(&mut reader, endian, ())
                    .map_err(|e| err!(loader, "Failed to read symtab command: {}", e))?;
                symtab_info = Some(symtab.clone());
            } else if cmd_header.cmd == LC_DYSYMTAB {
                let dysymtab = DysymtabCommand::read_options(&mut reader, endian, ())
                    .map_err(|e| err!(loader, "Failed to read dysymtab command: {}", e))?;
                dysymtab_info = Some(dysymtab);
            } else if cmd_header.cmd == LC_MAIN {
                // Parse LC_MAIN for entry point
                let entry_cmd = EntryPointCommand::read_options(&mut reader, endian, ())
                    .map_err(|e| err!(loader, "Failed to read entry point command: {}", e))?;
                // entryoff is offset from __TEXT segment start
                entry_point = text_segment_vmaddr + entry_cmd.entryoff;
            } else if cmd_header.cmd == LC_FUNCTION_STARTS {
                // GAP-8: Parse LC_FUNCTION_STARTS — ULEB128-encoded function addresses.
                // Equivalent to Ghidra's MachoFunctionStartsAnalyzer which uses this
                // table to discover all functions including unsymbolicated ones.
                let lc = LinkeditDataCommand::read_options(&mut reader, endian, ())
                    .map_err(|e| err!(loader, "Failed to read LC_FUNCTION_STARTS: {}", e))?;
                if lc.datasize > 0 {
                    function_starts_info = Some((lc.dataoff, lc.datasize));
                }
            }

            // Skip command
            reader
                .seek(SeekFrom::Start(cmd_start + cmd_header.cmdsize as u64))
                .map_err(|e| err!(loader, "Failed to skip load command: {}", e))?;
        }

        if image_base == u64::MAX {
            image_base = 0;
        }

        // Parse symbols after all sections are collected so n_sect can be filtered
        // against executable sections. This avoids treating data symbols as functions.
        if let Some(symtab) = symtab_info.as_ref() {
            Self::parse_symbols_64(
                bytes,
                symtab,
                endian,
                &section_exec_map,
                &mut functions_info,
            );
        }

        // Parse dynamic symbols to get external function imports
        let mut iat_symbols = std::collections::HashMap::new();
        if let (Some(symtab), Some(dysymtab)) = (symtab_info, dysymtab_info) {
            // __stubs entry size differs by architecture:
            // - x86_64: 6 bytes
            // - arm64: 12 bytes
            let stub_size = match cputype {
                0x100000C | 0xC => 12u64, // ARM64
                _ => 6u64,                // x86_64 and default fallback
            };
            Self::parse_dynamic_symbols_64(
                bytes,
                &symtab,
                &dysymtab,
                &sections_info,
                endian,
                stub_size,
                &mut iat_symbols,
            );
        }

        // GAP-8: Decode LC_FUNCTION_STARTS ULEB128 address table.
        // This mirrors Ghidra's MachoFunctionStartsAnalyzer which recovers
        // function boundaries for unsymbolicated / stripped binaries.
        if let Some((fs_offset, fs_size)) = function_starts_info {
            let fs_end = (fs_offset as usize).saturating_add(fs_size as usize);
            if fs_end <= bytes.len() {
                let fs_data = &bytes[fs_offset as usize..fs_end];
                let mut current_addr = image_base; // first entry is absolute VA
                let mut i = 0usize;
                let mut new_count = 0usize;
                while i < fs_data.len() {
                    // Decode one ULEB128 value
                    let mut delta: u64 = 0;
                    let mut shift = 0u64;
                    let mut consumed = 0usize;
                    loop {
                        if i + consumed >= fs_data.len() {
                            break;
                        }
                        let b = fs_data[i + consumed];
                        consumed += 1;
                        delta |= ((b & 0x7f) as u64) << shift;
                        shift += 7;
                        if b & 0x80 == 0 {
                            break;
                        }
                    }
                    i += consumed;
                    if delta == 0 {
                        break; // terminator
                    }
                    current_addr = current_addr.wrapping_add(delta);
                    // Only add if not already known
                    let already_known = functions_info.iter().any(|f| f.address == current_addr);
                    if !already_known && current_addr > image_base {
                        functions_info.push(FunctionInfo {
                            name: String::new(),
                            address: current_addr,
                            size: 0,
                            is_export: false,
                            is_import: false,
                        });
                        new_count += 1;
                    }
                }
                if new_count > 0 {
                    eprintln!(
                        "[MachoLoader] LC_FUNCTION_STARTS: added {} function entry points",
                        new_count
                    );
                }
            }
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

    fn parse_32(data: DataBuffer, path: String, endian: binrw::Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let mut reader = Cursor::new(bytes);
        let header = MachHeader32::read_options(&mut reader, endian, ())
            .map_err(|e| err!(loader, "MachO32 Header: {}", e))?;

        let is_64bit = false;
        let cputype = header.cputype;
        let arch_spec = match cputype {
            0x7 => "x86:LE:32:default", // x86 (CPU_TYPE_X86)
            0xC => "ARM:LE:32:v7",      // ARM (CPU_TYPE_ARM)
            _ => {
                eprintln!(
                    "[Warning] Unknown Mach-O CPU type: {} (0x{:X}), defaulting to x86",
                    cputype, cputype
                );
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
        section_exec_map: &[bool],
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
                // (n_type & N_TYPE) == N_SECT (0x0e)
                let n_type = nlist.n_type & 0x0e;
                if n_type == 0x0e && nlist.n_value != 0 {
                    // Only keep symbols that belong to executable sections.
                    // n_sect is 1-based across all sections in Mach-O.
                    let sect_index = nlist.n_sect as usize;
                    if sect_index == 0 || sect_index > section_exec_map.len() {
                        continue;
                    }
                    if !section_exec_map[sect_index - 1] {
                        continue;
                    }

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
        stub_size: u64,
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
                (stubs.virtual_size / stub_size) as u32
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
