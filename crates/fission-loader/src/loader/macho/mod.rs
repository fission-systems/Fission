use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
    extract_fixed_string,
};
use crate::prelude::*;
use fission_core::architecture::select_macho_load_spec;
use fission_core::constants::binary_format::*;

pub mod apple;
pub mod schema;
use schema::*;

pub struct MachoLoader;

impl MachoLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        // Read Magic
        let bytes = data.as_slice();
        if let Some(slice) = select_fat_slice(bytes) {
            return Self::parse(DataBuffer::Heap(bytes[slice].to_vec()), path);
        }
        let magic = ByteReader::big(bytes).u32(0)?;

        // Detect Props
        let (is_64, _is_swap) = match magic {
            MACHO_MAGIC_32_BE => (false, false),
            MACHO_MAGIC_32_LE => (false, true),
            MACHO_MAGIC_64_BE => (true, false),
            MACHO_MAGIC_64_LE => (true, true),
            _ => return Err(err!(loader, "Not a Mach-O binary (magic: {:x})", magic)),
        };

        let endian = match magic {
            MACHO_MAGIC_32_BE => Endian::Big,
            MACHO_MAGIC_64_BE => Endian::Big,
            MACHO_MAGIC_32_LE => Endian::Little,
            MACHO_MAGIC_64_LE => Endian::Little,
            _ => return Err(err!(loader, "Unknown Magic")),
        };

        if is_64 {
            Self::parse_64(data, path, endian)
        } else {
            Self::parse_32(data, path, endian)
        }
    }

    fn parse_64(data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        let header = MachHeader64::parse(&reader)?;

        let is_64bit = true;
        let cputype = header.cputype;

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
        let mut cursor = 32usize;
        for _ in 0..header.ncmds {
            let cmd_start = cursor;
            let cmd_header = LoadCommand::parse(&reader, cmd_start)?;

            if cmd_header.cmd == LC_SEGMENT_64 {
                let seg = SegmentCommand64::parse(&reader, cmd_start)?;
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
                let mut section_offset = cmd_start + SegmentCommand64::SIZE;
                for _ in 0..seg.nsects {
                    let sect = Section64::parse(&reader, section_offset)?;
                    section_offset += Section64::SIZE;

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

                cursor = cmd_start + cmd_header.cmdsize as usize;
                continue;
            } else if cmd_header.cmd == LC_SYMTAB {
                let symtab = SymtabCommand::parse(&reader, cmd_start)?;
                symtab_info = Some(symtab.clone());
            } else if cmd_header.cmd == LC_DYSYMTAB {
                let dysymtab = DysymtabCommand::parse(&reader, cmd_start)?;
                dysymtab_info = Some(dysymtab);
            } else if cmd_header.cmd == LC_MAIN {
                // Parse LC_MAIN for entry point
                let entry_cmd = EntryPointCommand::parse(&reader, cmd_start)?;
                // entryoff is offset from __TEXT segment start
                entry_point = text_segment_vmaddr + entry_cmd.entryoff;
            } else if cmd_header.cmd == LC_FUNCTION_STARTS {
                // GAP-8: Parse LC_FUNCTION_STARTS — ULEB128-encoded function addresses.
                // Equivalent to Ghidra's MachoFunctionStartsAnalyzer which uses this
                // table to discover all functions including unsymbolicated ones.
                let lc = LinkeditDataCommand::parse(&reader, cmd_start)?;
                if lc.datasize > 0 {
                    function_starts_info = Some((lc.dataoff, lc.datasize));
                }
            }

            // Skip command
            cursor = cmd_start + cmd_header.cmdsize as usize;
        }

        if image_base == u64::MAX {
            image_base = 0;
        }
        let (architecture, load_spec) =
            select_macho_load_spec(cputype, header.cpusubtype, is_64bit, image_base)
                .map_err(|e| err!(loader, "{}", e))?;

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
            let stub_size = match architecture.processor.as_str() {
                "AARCH64" => 12u64,
                _ => 6u64,
            };
            Self::parse_dynamic_symbols_64(
                bytes,
                &symtab,
                &dysymtab,
                &sections_info,
                endian,
                stub_size,
                &mut iat_symbols,
                &mut functions_info,
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
                            origin: Some("macho-function-starts".to_string()),
                            kind: Some("code".to_string()),
                            source_section: None,
                            external_library: None,
                            is_thunk_like: false,
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
            .format("Mach-O 64")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .build()
    }

    fn parse_32(data: DataBuffer, path: String, endian: Endian) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let reader = ByteReader::new(bytes, endian);
        let header = MachHeader32::parse(&reader)?;

        let is_64bit = false;
        let cputype = header.cputype;
        let mut sections_info = Vec::new();
        let mut section_exec_map = Vec::new();
        let mut functions_info = Vec::new();
        let mut image_base = u64::MAX;
        let mut entry_point = 0u64;
        let mut text_segment_vmaddr = 0u64;
        let mut symtab_info: Option<SymtabCommand> = None;
        let mut dysymtab_info: Option<DysymtabCommand> = None;

        let mut cursor = 28usize;
        for _ in 0..header.ncmds {
            let cmd_start = cursor;
            let cmd_header = LoadCommand::parse(&reader, cmd_start)?;

            if cmd_header.cmd == LC_SEGMENT {
                let seg = SegmentCommand32::parse(&reader, cmd_start)?;
                let seg_name = extract_fixed_string(&seg.segname);
                if seg_name == "__TEXT" && seg.vmaddr != 0 {
                    text_segment_vmaddr = seg.vmaddr as u64;
                    image_base = image_base.min(seg.vmaddr as u64);
                }
                let seg_is_executable = (seg.initprot & 0x04) != 0;
                let mut section_offset = cmd_start + SegmentCommand32::SIZE;
                for _ in 0..seg.nsects {
                    let sect = Section32::parse(&reader, section_offset)?;
                    section_offset += Section32::SIZE;
                    let sect_has_instructions = (sect.flags & 0x80000400) != 0;
                    let is_executable = seg_is_executable || sect_has_instructions;
                    section_exec_map.push(is_executable);
                    sections_info.push(SectionInfo {
                        name: extract_fixed_string(&sect.sectname),
                        virtual_address: sect.addr as u64,
                        virtual_size: sect.size as u64,
                        file_offset: sect.offset as u64,
                        file_size: sect.size as u64,
                        is_executable,
                        is_readable: true,
                        is_writable: (seg.initprot & 0x02) != 0,
                    });
                }
            } else if cmd_header.cmd == LC_SYMTAB {
                symtab_info = Some(SymtabCommand::parse(&reader, cmd_start)?);
            } else if cmd_header.cmd == LC_DYSYMTAB {
                dysymtab_info = Some(DysymtabCommand::parse(&reader, cmd_start)?);
            } else if cmd_header.cmd == LC_MAIN {
                let entry_cmd = EntryPointCommand::parse(&reader, cmd_start)?;
                entry_point = text_segment_vmaddr + entry_cmd.entryoff;
            }

            cursor = cmd_start + cmd_header.cmdsize as usize;
        }

        if image_base == u64::MAX {
            image_base = 0;
        }
        let (architecture, load_spec) =
            select_macho_load_spec(cputype, header.cpusubtype, is_64bit, image_base)
                .map_err(|e| err!(loader, "{}", e))?;

        if let Some(symtab) = symtab_info.as_ref() {
            Self::parse_symbols_32(
                bytes,
                symtab,
                endian,
                &section_exec_map,
                &mut functions_info,
            );
        }
        let mut iat_symbols = std::collections::HashMap::new();
        if let (Some(symtab), Some(dysymtab)) = (symtab_info, dysymtab_info) {
            Self::parse_dynamic_symbols_32(
                bytes,
                &symtab,
                &dysymtab,
                &sections_info,
                endian,
                &mut iat_symbols,
                &mut functions_info,
            );
        }

        LoadedBinaryBuilder::new(path, data)
            .format("Mach-O 32")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .build()
    }

    fn parse_symbols_64(
        data: &[u8],
        symtab: &SymtabCommand,
        endian: Endian,
        section_exec_map: &[bool],
        out: &mut Vec<FunctionInfo>,
    ) {
        let sym_off = symtab.symoff as u64;
        let str_off = symtab.stroff as u64;
        let nsyms = symtab.nsyms;

        if sym_off as usize >= data.len() {
            return;
        }

        let reader = ByteReader::new(data, endian);

        // We can't easily iterate N times due to seek.
        // But symbols are contiguous Nlist64 structs.
        for index in 0..nsyms {
            let offset = sym_off as usize + index as usize * Nlist64::SIZE;
            if let Ok(nlist) = Nlist64::parse(&reader, offset) {
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
                        origin: Some("macho-symtab".to_string()),
                        kind: Some("code".to_string()),
                        source_section: Some(format!("section_{}", sect_index)),
                        external_library: None,
                        is_thunk_like: false,
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
        endian: Endian,
        stub_size: u64,
        iat_symbols: &mut std::collections::HashMap<u64, String>,
        functions: &mut Vec<FunctionInfo>,
    ) {
        // Find __stubs and __got sections
        let stubs_section = sections.iter().find(|s| s.name == "__stubs");
        let got_section = sections.iter().find(|s| s.name == "__got");

        if dysymtab.nindirectsyms == 0 {
            return;
        }

        let reader = ByteReader::new(data, endian);
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
                if let Ok(sym_idx) = reader.u32((indirect_off + i * 4) as usize) {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(stub_addr, name);
                            if let Some(name) = iat_symbols.get(&stub_addr) {
                                push_macho_import(functions, stub_addr, name.clone(), "__stubs");
                            }
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
                if let Ok(sym_idx) =
                    reader.u32((indirect_off + (stubs_count as u64 + i) * 4) as usize)
                {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(got_addr, name);
                            if let Some(name) = iat_symbols.get(&got_addr) {
                                push_macho_import(functions, got_addr, name.clone(), "__got");
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_symbols_32(
        data: &[u8],
        symtab: &SymtabCommand,
        endian: Endian,
        section_exec_map: &[bool],
        out: &mut Vec<FunctionInfo>,
    ) {
        let reader = ByteReader::new(data, endian);
        for index in 0..symtab.nsyms {
            let offset = symtab.symoff as usize + index as usize * Nlist32::SIZE;
            let Ok(nlist) = Nlist32::parse(&reader, offset) else {
                break;
            };
            let n_type = nlist.n_type & 0x0e;
            if n_type != 0x0e || nlist.n_value == 0 {
                continue;
            }
            let sect_index = nlist.n_sect as usize;
            if sect_index == 0 || sect_index > section_exec_map.len() {
                continue;
            }
            if !section_exec_map[sect_index - 1] {
                continue;
            }
            let name_offset = (symtab.stroff as usize).checked_add(nlist.n_strx as usize);
            let extracted_name = match name_offset {
                Some(offset) if offset < data.len() => extract_cstring(data, offset),
                _ => String::new(),
            };
            let final_name = if extracted_name.is_empty() {
                format!("sub_{:x}", nlist.n_value)
            } else {
                extracted_name
            };
            out.push(FunctionInfo {
                name: final_name,
                address: nlist.n_value as u64,
                size: 0,
                is_export: true,
                is_import: false,
                origin: Some("macho-symtab".to_string()),
                kind: Some("code".to_string()),
                source_section: Some(format!("section_{}", sect_index)),
                external_library: None,
                is_thunk_like: false,
            });
        }
    }

    fn parse_dynamic_symbols_32(
        data: &[u8],
        symtab: &SymtabCommand,
        dysymtab: &DysymtabCommand,
        sections: &[SectionInfo],
        endian: Endian,
        iat_symbols: &mut std::collections::HashMap<u64, String>,
        functions: &mut Vec<FunctionInfo>,
    ) {
        if dysymtab.nindirectsyms == 0 {
            return;
        }
        let stubs_section = sections.iter().find(|s| s.name == "__stubs");
        let got_section = sections.iter().find(|s| {
            matches!(
                s.name.as_str(),
                "__got" | "__la_symbol_ptr" | "__nl_symbol_ptr"
            )
        });
        let indirect_off = dysymtab.indirectsymoff as u64;
        if indirect_off as usize + (dysymtab.nindirectsyms as usize * 4) > data.len() {
            return;
        }
        let reader = ByteReader::new(data, endian);
        if let Some(stubs) = stubs_section {
            let stub_size = 6u64;
            let num_stubs = (stubs.virtual_size / stub_size).min(dysymtab.nindirectsyms as u64);
            for i in 0..num_stubs {
                let stub_addr = stubs.virtual_address + i * stub_size;
                if let Ok(sym_idx) = reader.u32((indirect_off + i * 4) as usize) {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name_32(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(stub_addr, name.clone());
                            push_macho_import(functions, stub_addr, name, "__stubs");
                        }
                    }
                }
            }
        }
        if let Some(got) = got_section {
            let entry_size = 4u64;
            let num_entries = (got.virtual_size / entry_size).min(dysymtab.nindirectsyms as u64);
            let stubs_count = stubs_section
                .map(|stubs| (stubs.virtual_size / 6) as u32)
                .unwrap_or(0);
            for i in 0..num_entries {
                let got_addr = got.virtual_address + i * entry_size;
                if let Ok(sym_idx) =
                    reader.u32((indirect_off + (stubs_count as u64 + i) * 4) as usize)
                {
                    if sym_idx < symtab.nsyms {
                        let name = Self::get_symbol_name_32(data, symtab, sym_idx, endian);
                        if !name.is_empty() {
                            iat_symbols.insert(got_addr, name.clone());
                            push_macho_import(functions, got_addr, name, &got.name);
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
        endian: Endian,
    ) -> String {
        let sym_off = symtab.symoff as u64 + (sym_idx as u64 * 16); // Nlist64 is 16 bytes
        let reader = ByteReader::new(data, endian);

        if let Ok(nlist) = Nlist64::parse(&reader, sym_off as usize) {
            let str_off = symtab.stroff as usize + nlist.n_strx as usize;
            if str_off < data.len() {
                return extract_cstring(data, str_off);
            }
        }
        String::new()
    }

    fn get_symbol_name_32(
        data: &[u8],
        symtab: &SymtabCommand,
        sym_idx: u32,
        endian: Endian,
    ) -> String {
        let sym_off = symtab.symoff as u64 + (sym_idx as u64 * 12);
        let reader = ByteReader::new(data, endian);

        if let Ok(nlist) = Nlist32::parse(&reader, sym_off as usize) {
            let str_off = symtab.stroff as usize + nlist.n_strx as usize;
            if str_off < data.len() {
                return extract_cstring(data, str_off);
            }
        }
        String::new()
    }
}

fn push_macho_import(functions: &mut Vec<FunctionInfo>, address: u64, name: String, section: &str) {
    if functions
        .iter()
        .any(|function| function.address == address && function.is_import)
    {
        return;
    }
    functions.push(FunctionInfo {
        name,
        address,
        size: 0,
        is_export: false,
        is_import: true,
        origin: Some("macho-indirect-symbols".to_string()),
        kind: Some("import_thunk".to_string()),
        source_section: Some(section.to_string()),
        external_library: None,
        is_thunk_like: true,
    });
}

fn select_fat_slice(bytes: &[u8]) -> Option<std::ops::Range<usize>> {
    if bytes.len() < 8 {
        return None;
    }
    let magic = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
    if !matches!(magic, MACHO_FAT_MAGIC | MACHO_FAT_CIGAM) {
        return None;
    }
    let nfat_arch = u32::from_be_bytes(bytes[4..8].try_into().ok()?) as usize;
    let mut best = None;
    let mut offset = 8usize;
    for _ in 0..nfat_arch {
        if offset + 20 > bytes.len() {
            return best;
        }
        let cputype = i32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?);
        let slice_offset =
            u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().ok()?) as usize;
        let slice_size =
            u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().ok()?) as usize;
        if slice_offset.checked_add(slice_size)? <= bytes.len() {
            let candidate = slice_offset..slice_offset + slice_size;
            if best.is_none()
                || matches!(
                    cputype,
                    MACHO_CPU_TYPE_X86_64
                        | MACHO_CPU_TYPE_ARM64
                        | MACHO_CPU_TYPE_X86
                        | MACHO_CPU_TYPE_ARM
                )
            {
                best = Some(candidate);
                if matches!(cputype, MACHO_CPU_TYPE_X86_64 | MACHO_CPU_TYPE_ARM64) {
                    break;
                }
            }
        }
        offset += 20;
    }
    best
}
