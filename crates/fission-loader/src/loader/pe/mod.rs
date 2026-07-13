use crate::loader::reader::ByteReader;
use crate::loader::types::{
    DataBuffer, LoadedBinary, LoadedBinaryBuilder, PdbDebugInfo, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use fission_core::architecture::select_pe_load_spec;

mod coff;
mod imports;
mod mingw_pseudo_reloc;
mod pdata;

pub struct PeLoader;

const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;
const PE_SIGNATURE_SIZE: usize = 4;
const PE_FILE_HEADER_SIZE: usize = 20;
const PE_SECTION_HEADER_SIZE: usize = 40;
const PE32_MAGIC: u16 = 0x10b;
const PE32_PLUS_MAGIC: u16 = 0x20b;
const IMAGE_DEBUG_DIRECTORY_SIZE: usize = 28;

#[derive(Clone, Debug)]
struct RawPeFile {
    e_lfanew: u32,
    section_table_offset: u32,
    file_header: PeFileHeader,
    optional_header: PeOptionalHeader,
    section_headers: Vec<PeSectionHeader>,
}

#[derive(Clone, Debug)]
struct PeFileHeader {
    machine: u16,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
}

#[derive(Clone, Debug)]
enum PeOptionalHeader {
    Pe32(PeOptionalHeaderData),
    Pe32Plus(PeOptionalHeaderData),
}

#[derive(Clone, Debug)]
struct PeOptionalHeaderData {
    image_base: u64,
    address_of_entry_point: u32,
    section_alignment: u32,
    data_directories: Vec<DataDirectory>,
}

#[derive(Clone, Debug)]
struct PeSectionHeader {
    name: String,
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    characteristics: u32,
}

#[derive(Clone, Debug)]
struct DataDirectory {
    virtual_address: u32,
    size: u32,
}

#[derive(Clone, Debug)]
struct ExportDirectory {
    number_of_functions: u32,
    number_of_names: u32,
    address_of_functions: u32,
    address_of_names: u32,
    address_of_name_ordinals: u32,
}

#[derive(Clone, Debug)]
struct ImportDescriptor {
    original_first_thunk: u32,
    name: u32,
    first_thunk: u32,
}

#[derive(Clone, Debug)]
struct ImageDebugDirectory {
    debug_type: u32,
    size_of_data: u32,
    address_of_raw_data: u32,
    pointer_to_raw_data: u32,
}

#[derive(Clone, Debug)]
struct CoffSymbol {
    name: SymbolName,
    value: u32,
    section_number: i16,
    symbol_type: u16,
    storage_class: u8,
    number_of_aux_symbols: u8,
}

#[derive(Clone, Debug)]
enum SymbolName {
    ShortName(String),
    LongName(u32),
}

mod storage_class {
    pub const C_EXT: u8 = 2;
    pub const C_STAT: u8 = 3;
    pub const C_LABEL: u8 = 6;
}

mod symbol_type {
    pub const DT_FCN: u16 = 2;
}

impl PeLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let main_started = std::time::Instant::now();
        let bytes = data.as_slice();

        let pe_started = std::time::Instant::now();
        let pe_file = parse_pe_file(bytes)?;
        tracing::debug!(
            "[PE Profiler] parse_pe_file took: {:?}",
            pe_started.elapsed()
        );

        // Extract basic info
        let is_64bit = match pe_file.optional_header {
            PeOptionalHeader::Pe32(_) => false,
            PeOptionalHeader::Pe32Plus(_) => true,
        };

        let (image_base, entry_point, _section_alignment) = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => (
                opt.image_base,
                opt.image_base + opt.address_of_entry_point as u64,
                opt.section_alignment,
            ),
        };

        let spec_started = std::time::Instant::now();
        let (architecture, load_spec) =
            select_pe_load_spec(pe_file.file_header.machine, is_64bit, image_base)
                .map_err(|e| err!(loader, "{}", e))?;
        tracing::debug!(
            "[PE Profiler] select_pe_load_spec took: {:?}",
            spec_started.elapsed()
        );

        // Sections
        let mut sections_info = Vec::new();
        for section in &pe_file.section_headers {
            let ch = section.characteristics;
            sections_info.push(SectionInfo {
                name: section.name.clone(),
                virtual_address: image_base + section.virtual_address as u64,
                virtual_size: section.virtual_size as u64,
                file_offset: section.pointer_to_raw_data as u64,
                file_size: section.size_of_raw_data as u64,
                is_executable: (ch & 0x20000000) != 0,
                is_readable: (ch & 0x40000000) != 0,
                is_writable: (ch & 0x80000000) != 0,
            });
        }

        // Build the binary
        let loader = PeLoaderImpl {
            data: bytes,
            sections: &sections_info,
            is_64bit,
            language_id: load_spec.pair.language_id.as_str().to_string(),
        };

        let mut functions_info = Vec::new();
        let mut function_candidates = Vec::new();
        let mut iat_symbols = std::collections::HashMap::new();
        let mut global_symbols = std::collections::HashMap::new();

        // Parse Exports
        // DataDirectory[0] is Export Table
        let export_started = std::time::Instant::now();
        let export_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(0)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
        };

        if export_dir_rva != 0 {
            if let Ok(mut exports) = loader.parse_exports(export_dir_rva, image_base) {
                functions_info.append(&mut exports);
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_exports took: {:?}",
            export_started.elapsed()
        );

        // Parse Imports
        // DataDirectory[1] is Import Table
        let import_started = std::time::Instant::now();
        let import_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(1)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
        };

        if import_dir_rva != 0 {
            if let Ok((mut imports, symbols)) = loader.parse_imports(import_dir_rva, image_base) {
                functions_info.append(&mut imports);
                iat_symbols = symbols;
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_imports took: {:?}",
            import_started.elapsed()
        );

        // Parse Delay Imports
        // DataDirectory[13] is Delay Import Table
        let delay_started = std::time::Instant::now();
        let delay_import_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(13)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
        };

        if delay_import_dir_rva != 0 {
            if let Ok((mut delay_imports, delay_symbols, mut delay_proxies)) =
                loader.parse_delay_imports(delay_import_dir_rva, image_base)
            {
                functions_info.append(&mut delay_imports);
                functions_info.append(&mut delay_proxies);
                iat_symbols.extend(delay_symbols);
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_delay_imports took: {:?}",
            delay_started.elapsed()
        );

        // Parse COFF Symbol Table (if present)
        let coff_started = std::time::Instant::now();
        let file_header = &pe_file.file_header;
        let mut cfg_label_leaders = Vec::new();
        if file_header.pointer_to_symbol_table != 0 && file_header.number_of_symbols > 0 {
            if let Ok(coff_functions) = loader.parse_coff_symbols(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                // Pre-populate address mapping to avoid O(N^2) linear scans
                let mut address_to_index = std::collections::HashMap::new();
                for (idx, f) in functions_info.iter().enumerate() {
                    address_to_index.insert(f.address, idx);
                }

                // Merge COFF symbols with existing functions, preferring COFF names over generated ones
                for coff_func in coff_functions {
                    if let Some(&index) = address_to_index.get(&coff_func.address) {
                        let existing = &mut functions_info[index];
                        // Replace generated name with real COFF symbol name
                        if existing.name.starts_with("FUN_0x") || existing.name.starts_with("sub_")
                        {
                            existing.name = coff_func.name;
                        }
                        if existing.size == 0 && coff_func.size > 0 {
                            existing.size = coff_func.size;
                        }
                    } else {
                        address_to_index.insert(coff_func.address, functions_info.len());
                        functions_info.push(coff_func);
                    }
                }
            }

            if let Ok(coff_data_symbols) = loader.parse_coff_data_symbols(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                global_symbols = coff_data_symbols;
            }

            if let Ok(leaders) = loader.parse_coff_cfg_label_leaders(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                cfg_label_leaders = leaders;
            }

            if let Ok(candidates) = coff::parse_coff_function_candidates(
                &loader,
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
            ) {
                function_candidates = candidates;
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_coff_symbols took: {:?}",
            coff_started.elapsed()
        );

        // Add entry point if not exists
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(crate::loader::types::FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
                origin: Some("pe-entry".to_string()),
                kind: Some("entry".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
                thunk_target: None,
            });
        }

        // Parse Exception Directory (PDATA) for x64 binaries - contains function metadata
        // DataDirectory[3] is Exception Table (.pdata section)
        let pdata_started = std::time::Instant::now();
        if is_64bit {
            let exception_dir_rva = match &pe_file.optional_header {
                PeOptionalHeader::Pe32Plus(opt) => opt
                    .data_directories
                    .get(3)
                    .map(|d| (d.virtual_address, d.size))
                    .unwrap_or((0, 0)),
                _ => (0, 0),
            };

            if exception_dir_rva.0 != 0 && exception_dir_rva.1 > 0 {
                if let Ok(pdata_functions) =
                    loader.parse_pdata(exception_dir_rva.0, exception_dir_rva.1, image_base)
                {
                    // Pre-populate address mapping to avoid O(N^2) linear scans
                    let mut address_to_index = std::collections::HashMap::new();
                    for (idx, f) in functions_info.iter().enumerate() {
                        address_to_index.insert(f.address, idx);
                    }

                    for pdata_func in pdata_functions {
                        if let Some(&index) = address_to_index.get(&pdata_func.address) {
                            let existing = &mut functions_info[index];
                            if existing.size == 0 {
                                existing.size = pdata_func.size;
                            }
                            if existing.kind.is_none() {
                                existing.kind = pdata_func.kind.clone();
                            }
                            if existing.source_section.is_none() {
                                existing.source_section = pdata_func.source_section.clone();
                            }
                        } else {
                            address_to_index.insert(pdata_func.address, functions_info.len());
                            functions_info.push(pdata_func);
                        }
                    }
                }
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_pdata took: {:?}",
            pdata_started.elapsed()
        );

        let pdb_started = std::time::Instant::now();
        let pdb_debug_info = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => {
                opt.data_directories.get(6).and_then(|dir| {
                    loader.parse_pdb_debug_info(dir.virtual_address, dir.size, image_base)
                })
            }
        };
        tracing::debug!(
            "[PE Profiler] parse_pdb_debug_info took: {:?}",
            pdb_started.elapsed()
        );

        let header_started = std::time::Instant::now();
        let (header_types, header_symbols) = generate_pe_header_types(
            is_64bit,
            image_base,
            pe_file.e_lfanew,
            pe_file.section_table_offset,
            pe_file.section_headers.len() as u16,
        );
        global_symbols.extend(header_symbols);
        tracing::debug!(
            "[PE Profiler] generate_pe_header_types took: {:?}",
            header_started.elapsed()
        );

        // Parse Base Relocations (Gap 3)
        let reloc_started = std::time::Instant::now();
        let reloc_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(5)
                .map(|d| (d.virtual_address, d.size))
                .unwrap_or((0, 0)),
        };
        let mut relocations = Vec::new();
        let mut relocation_symbols = std::collections::HashMap::new();
        if reloc_dir_rva.0 != 0 && reloc_dir_rva.1 > 0 {
            if let Ok(entries) =
                loader.parse_relocations(reloc_dir_rva.0, reloc_dir_rva.1, image_base)
            {
                for entry in &entries {
                    if entry.r_type != 0 && entry.size > 0 {
                        relocation_symbols
                            .entry(entry.address)
                            .or_insert_with(String::new);
                    }
                }
                relocations = entries;
            }
        }
        tracing::debug!(
            "[PE Profiler] parse_relocations took: {:?}",
            reloc_started.elapsed()
        );

        let rich_started = std::time::Instant::now();
        let rich_records = parse_rich_header(bytes, pe_file.e_lfanew);
        tracing::debug!(
            "[PE Profiler] parse_rich_header took: {:?}",
            rich_started.elapsed()
        );

        let likely_mingw =
            mingw_pseudo_reloc::is_likely_mingw_pe(rich_records.as_deref(), &global_symbols);
        let view_va = |va: u64, len: usize| -> Option<Vec<u8>> {
            if va < image_base {
                return None;
            }
            let rva = (va - image_base) as u32;
            let file_off = loader.rva_to_file_offset(rva, image_base)? as usize;
            loader
                .data
                .get(file_off..file_off.checked_add(len)?)
                .map(|slice| slice.to_vec())
        };
        let mingw_facts = mingw_pseudo_reloc::scan_mingw_pseudo_relocs(
            image_base,
            is_64bit,
            &global_symbols,
            &view_va,
            likely_mingw,
        );
        cfg_label_leaders.extend(mingw_facts.cfg_label_leaders);
        for site in &mingw_facts.relocation_use_sites {
            relocation_symbols.entry(*site).or_insert_with(String::new);
        }
        relocations.extend(mingw_pseudo_reloc::mingw_pseudo_reloc_entries(
            &mingw_facts.relocation_use_sites,
        ));

        let build_started = std::time::Instant::now();
        let mut builder = LoadedBinaryBuilder::new(path, data)
            .format("PE")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .pdb_debug_info(pdb_debug_info)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .add_global_symbols(global_symbols)
            .add_inferred_types(header_types)
            .add_relocations(relocations)
            .add_relocation_symbols(relocation_symbols)
            .cfg_label_leaders(cfg_label_leaders);

        if let Some(records) = rich_records {
            builder = builder.rich_header_records(records);
        }

        let mut res = builder.build()?;
        function_candidates
            .retain(|candidate| !res.function_addr_index.contains_key(&candidate.address));
        res.function_candidates = function_candidates;
        tracing::debug!(
            "[PE Profiler] LoadedBinaryBuilder::build took: {:?}",
            build_started.elapsed()
        );
        tracing::debug!(
            "[PE Profiler] TOTAL PeLoader::parse took: {:?}",
            main_started.elapsed()
        );
        Ok(res)
    }
}

#[cfg(test)]
fn merge_pdata_function(
    functions: &mut Vec<crate::loader::types::FunctionInfo>,
    pdata_func: crate::loader::types::FunctionInfo,
) {
    if let Some(existing) = functions
        .iter_mut()
        .find(|func| func.address == pdata_func.address)
    {
        if existing.size == 0 {
            existing.size = pdata_func.size;
        }
        if existing.kind.is_none() {
            existing.kind = pdata_func.kind;
        }
        if existing.source_section.is_none() {
            existing.source_section = pdata_func.source_section;
        }
        return;
    }

    functions.push(pdata_func);
}

pub fn detect_pe_is_64bit(bytes: &[u8]) -> bool {
    if bytes.len() < 0x40 {
        return true;
    }

    let pe_offset = if bytes.len() > 0x3F {
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize
    } else {
        return true;
    };

    if bytes.len() > pe_offset + 6 {
        let machine = u16::from_le_bytes([bytes[pe_offset + 4], bytes[pe_offset + 5]]);
        machine == 0x8664
    } else {
        true
    }
}

fn parse_pe_file(bytes: &[u8]) -> Result<RawPeFile> {
    let reader = ByteReader::little(bytes);
    if reader.slice(0, 2)? != b"MZ" {
        return Err(err!(loader, "MalformedHeader: missing DOS MZ signature"));
    }
    let pe_offset = reader.u32(0x3c)? as usize;
    if reader.slice(pe_offset, PE_SIGNATURE_SIZE)? != b"PE\0\0" {
        return Err(err!(loader, "MalformedHeader: missing PE signature"));
    }

    let file_header_offset = pe_offset + PE_SIGNATURE_SIZE;
    let machine = reader.u16(file_header_offset)?;
    let number_of_sections = reader.u16(file_header_offset + 2)?;
    let pointer_to_symbol_table = reader.u32(file_header_offset + 8)?;
    let number_of_symbols = reader.u32(file_header_offset + 12)?;
    let size_of_optional_header = reader.u16(file_header_offset + 16)? as usize;
    let optional_header_offset = file_header_offset + PE_FILE_HEADER_SIZE;
    let magic = reader.u16(optional_header_offset)?;

    let optional_header = match magic {
        PE32_MAGIC => {
            let data = parse_optional_header_data(
                &reader,
                optional_header_offset,
                false,
                size_of_optional_header,
            )?;
            PeOptionalHeader::Pe32(data)
        }
        PE32_PLUS_MAGIC => {
            let data = parse_optional_header_data(
                &reader,
                optional_header_offset,
                true,
                size_of_optional_header,
            )?;
            PeOptionalHeader::Pe32Plus(data)
        }
        _ => {
            return Err(err!(
                loader,
                "MalformedHeader: unsupported PE optional header magic 0x{magic:x}"
            ));
        }
    };

    let section_table_offset = optional_header_offset
        .checked_add(size_of_optional_header)
        .ok_or_else(|| err!(loader, "MalformedHeader: PE section table offset overflow"))?;
    let mut section_headers = Vec::with_capacity(number_of_sections as usize);
    for idx in 0..number_of_sections as usize {
        let offset = section_table_offset + idx * PE_SECTION_HEADER_SIZE;
        section_headers.push(PeSectionHeader {
            name: reader.fixed_string(offset, 8)?,
            virtual_size: reader.u32(offset + 8)?,
            virtual_address: reader.u32(offset + 12)?,
            size_of_raw_data: reader.u32(offset + 16)?,
            pointer_to_raw_data: reader.u32(offset + 20)?,
            characteristics: reader.u32(offset + 36)?,
        });
    }

    Ok(RawPeFile {
        e_lfanew: pe_offset as u32,
        section_table_offset: section_table_offset as u32,
        file_header: PeFileHeader {
            machine,
            pointer_to_symbol_table,
            number_of_symbols,
        },
        optional_header,
        section_headers,
    })
}

fn parse_optional_header_data(
    reader: &ByteReader<'_>,
    optional_header_offset: usize,
    is_pe32_plus: bool,
    size_of_optional_header: usize,
) -> Result<PeOptionalHeaderData> {
    let address_of_entry_point = reader.u32(optional_header_offset + 16)?;
    let image_base = if is_pe32_plus {
        reader.u64(optional_header_offset + 24)?
    } else {
        u64::from(reader.u32(optional_header_offset + 28)?)
    };
    let section_alignment = reader.u32(optional_header_offset + 32)?;
    let (number_offset, directories_offset) = if is_pe32_plus {
        (optional_header_offset + 108, optional_header_offset + 112)
    } else {
        (optional_header_offset + 92, optional_header_offset + 96)
    };
    let number_of_rva_and_sizes = reader.u32(number_offset).unwrap_or(0) as usize;
    let max_dirs_by_size = size_of_optional_header
        .saturating_sub(directories_offset.saturating_sub(optional_header_offset))
        / 8;
    let directory_count = number_of_rva_and_sizes.min(max_dirs_by_size).min(32);
    let mut data_directories = Vec::with_capacity(directory_count);
    for idx in 0..directory_count {
        let offset = directories_offset + idx * 8;
        data_directories.push(DataDirectory {
            virtual_address: reader.u32(offset)?,
            size: reader.u32(offset + 4)?,
        });
    }

    Ok(PeOptionalHeaderData {
        image_base,
        address_of_entry_point,
        section_alignment,
        data_directories,
    })
}

struct PeLoaderImpl<'a> {
    data: &'a [u8],
    sections: &'a [SectionInfo],
    is_64bit: bool,
    language_id: String,
}

impl<'a> PeLoaderImpl<'a> {
    // Simplified version - main logic is in rva_to_file_offset
    #[allow(dead_code)]
    fn rva_to_offset(&self, _rva: u32) -> Option<u64> {
        None
    }

    fn read_string_at(&self, offset: u64) -> String {
        extract_cstring(self.data, offset as usize)
    }

    fn reader(&self) -> ByteReader<'a> {
        ByteReader::little(self.data)
    }

    fn read_u16(&self, offset: u64) -> Result<u16> {
        self.reader().u16(offset as usize)
    }

    fn read_i16(&self, offset: u64) -> Result<i16> {
        self.reader().i16(offset as usize)
    }

    fn read_u32(&self, offset: u64) -> Result<u32> {
        self.reader().u32(offset as usize)
    }

    fn read_u64(&self, offset: u64) -> Result<u64> {
        self.reader().u64(offset as usize)
    }

    fn is_x86_language(&self) -> bool {
        self.language_id.starts_with("x86:")
    }

    fn executable_section_contains(&self, va: u64) -> bool {
        self.sections.iter().any(|section| {
            section.is_executable
                && va >= section.virtual_address
                && va < section.virtual_address.saturating_add(section.virtual_size)
        })
    }

    fn exact_relative_jump_export_target(&self, func_rva: u32, image_base: u64) -> Option<u64> {
        if !self.is_x86_language() {
            return None;
        }
        let file_offset = self.rva_to_file_offset(func_rva, image_base)? as usize;
        let bytes = self.data.get(file_offset..)?;
        let source_va = image_base.checked_add(u64::from(func_rva))?;
        let target = match bytes.first().copied()? {
            0xe9 => {
                let disp_bytes: [u8; 4] = bytes.get(1..5)?.try_into().ok()?;
                source_va
                    .checked_add(5)?
                    .checked_add_signed(i64::from(i32::from_le_bytes(disp_bytes)))?
            }
            0xeb => {
                let disp = i8::from_ne_bytes([*bytes.get(1)?]);
                source_va
                    .checked_add(2)?
                    .checked_add_signed(i64::from(disp))?
            }
            _ => return None,
        };
        self.executable_section_contains(target).then_some(target)
    }

    fn read_import_descriptor(&self, offset: u64) -> Result<ImportDescriptor> {
        Ok(ImportDescriptor {
            original_first_thunk: self.read_u32(offset)?,
            name: self.read_u32(offset + 12)?,
            first_thunk: self.read_u32(offset + 16)?,
        })
    }

    fn read_export_directory(&self, offset: u64) -> Result<ExportDirectory> {
        Ok(ExportDirectory {
            number_of_functions: self.read_u32(offset + 20)?,
            number_of_names: self.read_u32(offset + 24)?,
            address_of_functions: self.read_u32(offset + 28)?,
            address_of_names: self.read_u32(offset + 32)?,
            address_of_name_ordinals: self.read_u32(offset + 36)?,
        })
    }

    fn read_debug_directory(&self, offset: u64) -> Result<ImageDebugDirectory> {
        Ok(ImageDebugDirectory {
            debug_type: self.read_u32(offset + 12)?,
            size_of_data: self.read_u32(offset + 16)?,
            address_of_raw_data: self.read_u32(offset + 20)?,
            pointer_to_raw_data: self.read_u32(offset + 24)?,
        })
    }

    fn read_coff_symbol(&self, offset: u64) -> Result<CoffSymbol> {
        let name_bytes = self.reader().slice(offset as usize, 8)?;
        let name = if name_bytes[0..4] == [0, 0, 0, 0] {
            SymbolName::LongName(u32::from_le_bytes(name_bytes[4..8].try_into().unwrap()))
        } else {
            let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            SymbolName::ShortName(String::from_utf8_lossy(&name_bytes[..len]).to_string())
        };
        Ok(CoffSymbol {
            name,
            value: self.read_u32(offset + 8)?,
            section_number: self.read_i16(offset + 12)?,
            symbol_type: self.read_u16(offset + 14)?,
            storage_class: self.reader().u8(offset as usize + 16)?,
            number_of_aux_symbols: self.reader().u8(offset as usize + 17)?,
        })
    }

    fn rva_to_file_offset(&self, rva: u32, image_base: u64) -> Option<u64> {
        // rva is relative to image_base.
        // section.virtual_address is image_base + section_rva

        for section in self.sections {
            let section_va = section.virtual_address;
            let section_rva = (section_va - image_base) as u32;
            let section_size = section.virtual_size as u32;

            // Check if RVA is within this section
            if rva >= section_rva && rva < section_rva + section_size {
                let delta = rva - section_rva;
                return Some(section.file_offset + delta as u64);
            }
        }

        // Header fallback: if RVA is small (in headers), direct map
        if rva < 0x1000 {
            return Some(rva as u64);
        }

        None
    }

    fn parse_exports(
        &self,
        dir_rva: u32,
        image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        let offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Export Dir RVA"))?;
        let export_dir = self.read_export_directory(offset)?;

        let mut functions = Vec::new();

        // Parse Names
        if export_dir.number_of_names > 0 && export_dir.address_of_names != 0 {
            let names_offset = self
                .rva_to_file_offset(export_dir.address_of_names, image_base)
                .unwrap_or(0);
            let ordinals_offset = self
                .rva_to_file_offset(export_dir.address_of_name_ordinals, image_base)
                .unwrap_or(0);
            let funcs_offset = self
                .rva_to_file_offset(export_dir.address_of_functions, image_base)
                .unwrap_or(0);

            if names_offset != 0 && ordinals_offset != 0 && funcs_offset != 0 {
                for idx in 0..export_dir.number_of_names.min(10000) {
                    // Safety limit
                    let name_rva = self
                        .read_u32(names_offset + u64::from(idx) * 4)
                        .unwrap_or(0);
                    let ordinal = self
                        .read_u16(ordinals_offset + u64::from(idx) * 2)
                        .unwrap_or(0);

                    if name_rva != 0 {
                        let name_offset =
                            self.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                        let name = self.read_string_at(name_offset);

                        // Lookup function RVA using ordinal
                        // AddressOfFunctions is indexed by Ordinal (Base subtracted)
                        let func_idx = ordinal as u64; // Indices are 0-based from table start
                        if func_idx < export_dir.number_of_functions as u64 {
                            let entry_offset = funcs_offset + func_idx * 4;
                            let func_rva = self.read_u32(entry_offset).unwrap_or(0);

                            if func_rva != 0 {
                                let thunk_target =
                                    self.exact_relative_jump_export_target(func_rva, image_base);
                                let is_thunk_like = thunk_target.is_some();
                                functions.push(crate::loader::types::FunctionInfo {
                                    name,
                                    address: image_base + func_rva as u64,
                                    size: 0,
                                    is_export: true,
                                    is_import: false,
                                    origin: Some("pe-export-table".to_string()),
                                    kind: Some(
                                        if is_thunk_like {
                                            "export_thunk"
                                        } else {
                                            "export"
                                        }
                                        .to_string(),
                                    ),
                                    source_section: None,
                                    external_library: None,
                                    is_thunk_like,
                                    thunk_target,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(functions)
    }

    fn parse_imports(
        &self,
        dir_rva: u32,
        image_base: u64,
    ) -> Result<(
        Vec<crate::loader::types::FunctionInfo>,
        std::collections::HashMap<u64, String>,
    )> {
        imports::parse_imports(self, dir_rva, image_base)
    }

    fn parse_delay_imports(
        &self,
        dir_rva: u32,
        image_base: u64,
    ) -> Result<(
        Vec<crate::loader::types::FunctionInfo>,
        std::collections::HashMap<u64, String>,
        Vec<crate::loader::types::FunctionInfo>,
    )> {
        imports::parse_delay_imports(self, dir_rva, image_base)
    }

    fn parse_pdata(
        &self,
        pdata_rva: u32,
        pdata_size: u32,
        image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        pdata::parse_pdata(self, pdata_rva, pdata_size, image_base)
    }

    fn parse_relocations(
        &self,
        dir_rva: u32,
        dir_size: u32,
        image_base: u64,
    ) -> Result<Vec<crate::loader::types::RelocationEntry>> {
        let mut offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Reloc Dir RVA"))?;
        let end_offset = offset.saturating_add(dir_size as u64);
        let mut relocs = Vec::new();

        while offset < end_offset {
            let page_rva = self.read_u32(offset).unwrap_or(0);
            let block_size = self.read_u32(offset + 4).unwrap_or(0);
            if block_size < 8 || block_size > 4096 {
                break;
            }

            let num_entries = (block_size - 8) / 2;
            let mut entry_offset = offset + 8;
            for _ in 0..num_entries {
                let entry = self.read_u16(entry_offset).unwrap_or(0);
                entry_offset += 2;

                let r_type = (entry >> 12) as u32;
                let r_offset = (entry & 0x0FFF) as u32;

                if r_type == 0 {
                    continue;
                }

                let address = image_base + page_rva as u64 + r_offset as u64;
                let size = match r_type {
                    10 => 8,    // DIR64
                    3 => 4,     // HIGHLOW
                    1 | 2 => 2, // HIGH / LOW
                    _ => 0,
                };

                relocs.push(crate::loader::types::RelocationEntry {
                    address,
                    r_type,
                    size,
                    addend: 0,
                    symbol_name: None,
                });
            }

            offset = offset.saturating_add(block_size as u64);
        }

        Ok(relocs)
    }

    fn parse_coff_symbols(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        _image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        coff::parse_coff_symbols(self, symbol_table_offset, symbol_count, _image_base)
    }

    fn parse_coff_data_symbols(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        _image_base: u64,
    ) -> Result<std::collections::HashMap<u64, String>> {
        coff::parse_coff_data_symbols(self, symbol_table_offset, symbol_count, _image_base)
    }

    fn parse_coff_cfg_label_leaders(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        _image_base: u64,
    ) -> Result<Vec<u64>> {
        coff::parse_coff_cfg_label_leaders(self, symbol_table_offset, symbol_count, _image_base)
    }

    fn parse_pdb_debug_info(
        &self,
        dir_rva: u32,
        dir_size: u32,
        image_base: u64,
    ) -> Option<PdbDebugInfo> {
        if dir_rva == 0 || dir_size < 28 {
            return None;
        }
        let dir_offset = self.rva_to_file_offset(dir_rva, image_base)?;
        let entry_count = (dir_size as usize) / IMAGE_DEBUG_DIRECTORY_SIZE;
        for idx in 0..entry_count {
            let entry = self
                .read_debug_directory(dir_offset + (idx * IMAGE_DEBUG_DIRECTORY_SIZE) as u64)
                .ok()?;
            if entry.debug_type != IMAGE_DEBUG_TYPE_CODEVIEW || entry.size_of_data < 4 {
                continue;
            }

            let data_offset = if entry.pointer_to_raw_data != 0 {
                u64::from(entry.pointer_to_raw_data)
            } else {
                self.rva_to_file_offset(entry.address_of_raw_data, image_base)?
            };
            let data_end = data_offset.checked_add(u64::from(entry.size_of_data))? as usize;
            let data_offset = data_offset as usize;
            if data_end > self.data.len() || data_offset >= data_end {
                continue;
            }
            let data = &self.data[data_offset..data_end];
            let signature = data.get(0..4)?;
            match signature {
                b"RSDS" => {
                    if data.len() < 24 {
                        continue;
                    }
                    let guid_hex = data[4..20]
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    let age = u32::from_le_bytes(data[20..24].try_into().ok()?);
                    let path_hint = Some(extract_cstring(data, 24)).filter(|s| !s.is_empty());
                    return Some(PdbDebugInfo {
                        path_hint,
                        guid_hex: Some(guid_hex),
                        age: Some(age),
                        has_codeview: true,
                    });
                }
                b"NB10" => {
                    if data.len() < 16 {
                        continue;
                    }
                    let age = u32::from_le_bytes(data[12..16].try_into().ok()?);
                    let path_hint = Some(extract_cstring(data, 16)).filter(|s| !s.is_empty());
                    return Some(PdbDebugInfo {
                        path_hint,
                        guid_hex: None,
                        age: Some(age),
                        has_codeview: true,
                    });
                }
                _ => {}
            }
        }
        None
    }
}

/// TLS / debug-directory facts for [`crate::loader::identity`] (never affects loader parse outcome).
#[derive(Debug, Clone)]
pub(crate) struct IdentityPeFacts {
    pub tls_directory_present: bool,
    pub tls_callback_count: usize,
    pub debug_directory_kinds: Vec<String>,
}

pub(crate) fn identity_pe_facts(binary: &LoadedBinary) -> Option<IdentityPeFacts> {
    let fmt = binary.format.to_ascii_uppercase();
    if !fmt.starts_with("PE") {
        return None;
    }
    let bytes = binary.data.as_slice();
    let raw = parse_pe_file(bytes).ok()?;
    let (image_base, data_directories, is_pe32_plus) = match &raw.optional_header {
        PeOptionalHeader::Pe32(d) => (d.image_base, &d.data_directories, false),
        PeOptionalHeader::Pe32Plus(d) => (d.image_base, &d.data_directories, true),
    };

    let tls_directory_present = data_directories
        .get(9)
        .is_some_and(|d| d.virtual_address != 0 && d.size != 0);
    let mut tls_callback_count = 0usize;

    if let Some(tls_dd) = data_directories.get(9)
        && tls_dd.virtual_address != 0
        && tls_dd.size != 0
    {
        let tls_rva = tls_dd.virtual_address;
        let tls_va = image_base.checked_add(u64::from(tls_rva))?;
        let tls_fo = identity_va_to_file_offset(binary, tls_va)?;
        let cb_va = if is_pe32_plus {
            let end = tls_fo.checked_add(32)?;
            let slice = bytes.get(tls_fo..end)?;
            u64::from_le_bytes(slice[24..32].try_into().ok()?)
        } else {
            let end = tls_fo.checked_add(16)?;
            let slice = bytes.get(tls_fo..end)?;
            u64::from(u32::from_le_bytes(slice[12..16].try_into().ok()?))
        };

        if cb_va != 0 {
            if let Some(cb_fo) = identity_va_to_file_offset(binary, cb_va) {
                let ptr_sz = if is_pe32_plus { 8usize } else { 4usize };
                let mut off = cb_fo;
                for _ in 0..128usize {
                    let end = off.checked_add(ptr_sz)?;
                    if end > bytes.len() {
                        break;
                    }
                    let ptr = if is_pe32_plus {
                        u64::from_le_bytes(bytes[off..end].try_into().unwrap())
                    } else {
                        u64::from(u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()))
                    };
                    if ptr == 0 {
                        break;
                    }
                    tls_callback_count += 1;
                    off = end;
                }
            }
        }
    }

    let mut debug_directory_kinds = Vec::new();
    if let Some(dd) = data_directories.get(6)
        && dd.virtual_address != 0
        && dd.size >= IMAGE_DEBUG_DIRECTORY_SIZE as u32
    {
        let dbg_va = image_base.checked_add(u64::from(dd.virtual_address))?;
        let dbg_fo = identity_va_to_file_offset(binary, dbg_va)?;
        let entry_count = (dd.size as usize) / IMAGE_DEBUG_DIRECTORY_SIZE;
        for idx in 0..entry_count {
            let ent_fo = dbg_fo.saturating_add(idx * IMAGE_DEBUG_DIRECTORY_SIZE);
            let end = ent_fo.checked_add(IMAGE_DEBUG_DIRECTORY_SIZE)?;
            let slice = bytes.get(ent_fo..end)?;
            let ty = u32::from_le_bytes(slice[12..16].try_into().ok()?);
            debug_directory_kinds.push(debug_directory_kind_name(ty));
        }
    }

    Some(IdentityPeFacts {
        tls_directory_present,
        tls_callback_count,
        debug_directory_kinds,
    })
}

fn identity_va_to_file_offset(binary: &LoadedBinary, va: u64) -> Option<usize> {
    let data_len = binary.data.as_slice().len();
    for sec in &binary.sections {
        let end = sec.virtual_address.saturating_add(sec.virtual_size);
        if va >= sec.virtual_address && va < end {
            let delta = va.checked_sub(sec.virtual_address)?;
            let fo = sec.file_offset.checked_add(delta)?;
            let fo_usize = usize::try_from(fo).ok()?;
            return (fo_usize < data_len).then_some(fo_usize);
        }
    }
    None
}

fn parse_rich_header(
    bytes: &[u8],
    e_lfanew: u32,
) -> Option<Vec<crate::loader::types::RichHeaderRecord>> {
    use crate::loader::types::RichHeaderRecord;

    if e_lfanew < 0x80 || e_lfanew as usize > bytes.len() {
        return None;
    }

    let limit = (e_lfanew as usize).min(bytes.len()) - 8;
    let mut rich_offset = None;
    for offset in (0x40..limit).rev() {
        if &bytes[offset..offset + 4] == b"Rich" {
            rich_offset = Some(offset);
            break;
        }
    }

    let rich_offset = rich_offset?;
    let xor_key = u32::from_le_bytes(bytes[rich_offset + 4..rich_offset + 8].try_into().ok()?);

    let mut records = Vec::new();
    let mut offset = rich_offset;
    loop {
        if offset < 0x40 + 8 {
            break;
        }
        offset -= 8;
        let val1 = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) ^ xor_key;
        let val2 = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) ^ xor_key;

        if val1 == 0x536e6144 {
            // "DanS" found!
            records.reverse();
            return Some(records);
        }

        let build_number = (val1 & 0xFFFF) as u16;
        let product_id = (val1 >> 16) as u16;

        records.push(RichHeaderRecord {
            comp_id: val1,
            build_number,
            product_id,
            count: val2,
        });

        if records.len() > 100 {
            break;
        }
    }

    None
}

fn generate_pe_header_types(
    is_64bit: bool,
    image_base: u64,
    e_lfanew: u32,
    section_table_offset: u32,
    section_count: u16,
) -> (
    Vec<crate::loader::types::InferredTypeInfo>,
    std::collections::HashMap<u64, String>,
) {
    use crate::loader::types::{InferredFieldInfo, InferredTypeInfo};
    let mut types = Vec::new();
    let mut symbols = std::collections::HashMap::new();

    // 1. IMAGE_DOS_HEADER
    let dos_fields = vec![
        InferredFieldInfo {
            name: "e_magic".to_string(),
            type_name: "WORD".to_string(),
            offset: 0,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_cblp".to_string(),
            type_name: "WORD".to_string(),
            offset: 2,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_cp".to_string(),
            type_name: "WORD".to_string(),
            offset: 4,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_crlc".to_string(),
            type_name: "WORD".to_string(),
            offset: 6,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_cparhdr".to_string(),
            type_name: "WORD".to_string(),
            offset: 8,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_minalloc".to_string(),
            type_name: "WORD".to_string(),
            offset: 10,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_maxalloc".to_string(),
            type_name: "WORD".to_string(),
            offset: 12,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_ss".to_string(),
            type_name: "WORD".to_string(),
            offset: 14,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_sp".to_string(),
            type_name: "WORD".to_string(),
            offset: 16,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_csum".to_string(),
            type_name: "WORD".to_string(),
            offset: 18,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_ip".to_string(),
            type_name: "WORD".to_string(),
            offset: 20,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_cs".to_string(),
            type_name: "WORD".to_string(),
            offset: 22,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_lfarlc".to_string(),
            type_name: "WORD".to_string(),
            offset: 24,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_ovno".to_string(),
            type_name: "WORD".to_string(),
            offset: 26,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_res".to_string(),
            type_name: "WORD[4]".to_string(),
            offset: 28,
            size: 8,
        },
        InferredFieldInfo {
            name: "e_oemid".to_string(),
            type_name: "WORD".to_string(),
            offset: 36,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_oeminfo".to_string(),
            type_name: "WORD".to_string(),
            offset: 38,
            size: 2,
        },
        InferredFieldInfo {
            name: "e_res2".to_string(),
            type_name: "WORD[10]".to_string(),
            offset: 40,
            size: 20,
        },
        InferredFieldInfo {
            name: "e_lfanew".to_string(),
            type_name: "LONG".to_string(),
            offset: 60,
            size: 4,
        },
    ];
    types.push(InferredTypeInfo {
        name: "IMAGE_DOS_HEADER".to_string(),
        mangled_name: "IMAGE_DOS_HEADER".to_string(),
        kind: "struct".to_string(),
        fields: dos_fields,
        size: 64,
        metadata_address: image_base,
    });
    symbols.insert(image_base, "DOS_HEADER".to_string());

    // 2. IMAGE_NT_HEADERS
    let nt_name = if is_64bit {
        "IMAGE_NT_HEADERS64"
    } else {
        "IMAGE_NT_HEADERS32"
    };
    let opt_name = if is_64bit {
        "IMAGE_OPTIONAL_HEADER64"
    } else {
        "IMAGE_OPTIONAL_HEADER32"
    };
    let opt_size = if is_64bit { 240 } else { 224 };
    let nt_size = 24 + opt_size;

    let nt_fields = vec![
        InferredFieldInfo {
            name: "Signature".to_string(),
            type_name: "DWORD".to_string(),
            offset: 0,
            size: 4,
        },
        InferredFieldInfo {
            name: "FileHeader".to_string(),
            type_name: "IMAGE_FILE_HEADER".to_string(),
            offset: 4,
            size: 20,
        },
        InferredFieldInfo {
            name: "OptionalHeader".to_string(),
            type_name: opt_name.to_string(),
            offset: 24,
            size: opt_size,
        },
    ];
    let nt_va = image_base + e_lfanew as u64;
    types.push(InferredTypeInfo {
        name: nt_name.to_string(),
        mangled_name: nt_name.to_string(),
        kind: "struct".to_string(),
        fields: nt_fields,
        size: nt_size,
        metadata_address: nt_va,
    });
    symbols.insert(nt_va, "NT_HEADERS".to_string());

    // 3. IMAGE_SECTION_HEADER
    let sect_fields = vec![
        InferredFieldInfo {
            name: "Name".to_string(),
            type_name: "BYTE[8]".to_string(),
            offset: 0,
            size: 8,
        },
        InferredFieldInfo {
            name: "VirtualSize".to_string(),
            type_name: "DWORD".to_string(),
            offset: 8,
            size: 4,
        },
        InferredFieldInfo {
            name: "VirtualAddress".to_string(),
            type_name: "DWORD".to_string(),
            offset: 12,
            size: 4,
        },
        InferredFieldInfo {
            name: "SizeOfRawData".to_string(),
            type_name: "DWORD".to_string(),
            offset: 16,
            size: 4,
        },
        InferredFieldInfo {
            name: "PointerToRawData".to_string(),
            type_name: "DWORD".to_string(),
            offset: 20,
            size: 4,
        },
        InferredFieldInfo {
            name: "PointerToRelocations".to_string(),
            type_name: "DWORD".to_string(),
            offset: 24,
            size: 4,
        },
        InferredFieldInfo {
            name: "PointerToLinenumbers".to_string(),
            type_name: "DWORD".to_string(),
            offset: 28,
            size: 4,
        },
        InferredFieldInfo {
            name: "NumberOfRelocations".to_string(),
            type_name: "WORD".to_string(),
            offset: 32,
            size: 2,
        },
        InferredFieldInfo {
            name: "NumberOfLinenumbers".to_string(),
            type_name: "WORD".to_string(),
            offset: 34,
            size: 2,
        },
        InferredFieldInfo {
            name: "Characteristics".to_string(),
            type_name: "DWORD".to_string(),
            offset: 36,
            size: 4,
        },
    ];

    let section_table_va = image_base + section_table_offset as u64;
    types.push(InferredTypeInfo {
        name: "IMAGE_SECTION_HEADER".to_string(),
        mangled_name: "IMAGE_SECTION_HEADER".to_string(),
        kind: "struct".to_string(),
        fields: sect_fields,
        size: 40 * section_count as u32,
        metadata_address: section_table_va,
    });
    symbols.insert(section_table_va, "SECTION_HEADERS".to_string());

    (types, symbols)
}

fn debug_directory_kind_name(ty: u32) -> String {
    match ty {
        0 => "UNKNOWN".into(),
        1 => "COFF".into(),
        2 => "CODEVIEW".into(),
        3 => "FPO".into(),
        4 => "MISC".into(),
        5 => "EXCEPTION".into(),
        6 => "FIXUP".into(),
        7 => "OMAP_TO_SRC".into(),
        8 => "OMAP_FROM_SRC".into(),
        9 => "BORLAND".into(),
        10 => "RESERVED10".into(),
        11 => "CLSID".into(),
        12 => "VC_FEATURE".into(),
        13 => "POGO".into(),
        14 => "ILTCG".into(),
        15 => "MPX".into(),
        16 => "REPRO".into(),
        17 => "EX_DLLCHARACTERISTICS".into(),
        other => format!("TYPE_{other:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_synthetic_pe() {
        let mut data = vec![0u8; 1024];

        // DOS Header
        data[0] = 0x4D;
        data[1] = 0x5A; // MZ
        data[0x3C] = 0x40; // e_lfanew = 0x40

        // PE Header (at 0x40)
        data[0x40] = 0x50;
        data[0x41] = 0x45; // PE\0\0

        // File Header (at 0x44)
        data[0x44] = 0x4C;
        data[0x45] = 0x01; // Machine = 0x14C (x86)
        data[0x46] = 0x01; // NumberOfSections = 1
        data[0x54] = 0xE0; // SizeOfOptionalHeader = 224 (0xE0)
        data[0x55] = 0x00;

        // Optional Header (at 0x58)
        data[0x58] = 0x0B;
        data[0x59] = 0x01; // Magic = 0x10B (PE32)
        // ImageBase (at 0x58 + 28 = 0x74)
        data[0x74] = 0x00;
        data[0x75] = 0x00;
        data[0x76] = 0x40; // 0x400000
        // Data Directories (16 entries)
        data[0x58 + 92] = 16; // NumberOfRvaAndSizes

        // Section Headers (at 0x40 + 4 + 20 + 0xE0 = 0x138)
        let section_offset = 0x138;
        // Name: .text
        data[section_offset] = b'.';
        data[section_offset + 1] = b't';
        data[section_offset + 2] = b'e';
        data[section_offset + 3] = b'x';
        data[section_offset + 4] = b't';

        // Characteristics (at +36)
        data[section_offset + 36] = 0x20;
        data[section_offset + 37] = 0x00;
        data[section_offset + 38] = 0x00;
        data[section_offset + 39] = 0x60; // Executable | Readable (0x60000020)

        let path = "test.exe".to_string();
        let result = PeLoader::parse(DataBuffer::Heap(data), path);

        if let Err(e) = &result {
            println!("Parse error: {}", e);
        }

        assert!(result.is_ok());
        let Ok(bin) = result else {
            panic!("PE parsing should succeed")
        };
        assert_eq!(bin.format, "PE");
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].name, ".text");
        assert_eq!(bin.sections[0].is_executable, true);
        assert_eq!(bin.arch_spec, "x86:LE:32:default");
        assert_eq!(bin.sleigh_language_id(), Some("x86:LE:32:default"));
    }

    #[test]
    fn test_parse_synthetic_pe_arm64_sets_aarch64_arch_spec() {
        let mut data = vec![0u8; 1024];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0x64;
        data[0x45] = 0xAA; // Machine = 0xAA64 (ARM64)
        data[0x46] = 0x01;
        data[0x54] = 0xF0; // SizeOfOptionalHeader = 240 (PE32+)
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x02; // Magic = 0x20B (PE32+)
        data[0x58 + 108] = 16; // NumberOfRvaAndSizes

        let section_offset = 0x148;
        data[section_offset] = b'.';
        data[section_offset + 1] = b't';
        data[section_offset + 2] = b'e';
        data[section_offset + 3] = b'x';
        data[section_offset + 4] = b't';
        data[section_offset + 36] = 0x20;
        data[section_offset + 39] = 0x60;

        let result = PeLoader::parse(DataBuffer::Heap(data), "arm64.exe".to_string());
        assert!(result.is_ok());
        let bin = result.expect("arm64 pe should parse");
        assert_eq!(bin.arch_spec, "AARCH64:LE:64:v8A");
        assert_eq!(bin.sleigh_language_id(), Some("AARCH64:LE:64:v8A"));
        assert!(bin.is_64bit);
    }

    #[test]
    fn test_parse_synthetic_pe_unknown_machine_fails_closed() {
        let mut data = vec![0u8; 1024];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0xff;
        data[0x45] = 0xff; // unknown Machine = 0xffff
        data[0x46] = 0x01;
        data[0x54] = 0xF0;
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x02; // PE32+
        data[0x58 + 108] = 16;

        let result = PeLoader::parse(DataBuffer::Heap(data), "unknown.exe".to_string());
        assert!(result.is_err());
        let err = format!("{}", result.expect_err("unknown machine must fail"));
        assert!(err.contains("unsupported machine"));
        assert!(!err.contains("defaulting to x86"));
    }

    #[test]
    fn pe_export_relative_jump_thunk_target_is_exact() {
        let image_base = 0x180000000;
        let mut data = vec![0u8; 0x800];
        let thunk_rva = 0x100u64;
        let target_rva = 0x180u64;
        let thunk_file = 0x200usize;
        let disp = ((image_base + target_rva) as i64 - (image_base + thunk_rva + 5) as i64) as i32;
        data[thunk_file] = 0xe9;
        data[thunk_file + 1..thunk_file + 5].copy_from_slice(&disp.to_le_bytes());
        let sections = vec![SectionInfo {
            name: ".text".to_string(),
            virtual_address: image_base + 0x100,
            virtual_size: 0x200,
            file_offset: 0x200,
            file_size: 0x200,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        }];
        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };

        assert_eq!(
            loader.exact_relative_jump_export_target(thunk_rva as u32, image_base),
            Some(image_base + target_rva)
        );
    }

    #[test]
    fn pe_export_relative_jump_thunk_requires_x86_language() {
        let image_base = 0x140000000;
        let mut data = vec![0u8; 0x400];
        data[0x200] = 0xe9;
        data[0x201..0x205].copy_from_slice(&0x10i32.to_le_bytes());
        let sections = vec![SectionInfo {
            name: ".text".to_string(),
            virtual_address: image_base + 0x100,
            virtual_size: 0x100,
            file_offset: 0x200,
            file_size: 0x100,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        }];
        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "AARCH64:LE:64:v8A".to_string(),
        };

        assert_eq!(
            loader.exact_relative_jump_export_target(0x100, image_base),
            None
        );
    }

    #[test]
    fn pe_pdata_merge_preserves_coff_name_and_adds_extent() {
        let mut functions = vec![crate::loader::types::FunctionInfo {
            name: "fibonacci".to_string(),
            address: 0x140001470,
            size: 0,
            is_export: false,
            is_import: false,
            origin: Some("pe-coff-symbol-table".to_string()),
            kind: Some("code".to_string()),
            source_section: Some(".text".to_string()),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        }];
        let pdata_func = crate::loader::types::FunctionInfo {
            name: "FUN_0x140001470".to_string(),
            address: 0x140001470,
            size: 0x3b0,
            is_export: false,
            is_import: false,
            origin: Some("pe-pdata".to_string()),
            kind: Some("code".to_string()),
            source_section: Some(".pdata".to_string()),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        };

        merge_pdata_function(&mut functions, pdata_func);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "fibonacci");
        assert_eq!(functions[0].size, 0x3b0);
        assert_eq!(functions[0].origin.as_deref(), Some("pe-coff-symbol-table"));
        assert_eq!(functions[0].source_section.as_deref(), Some(".text"));
    }

    #[test]
    fn test_parse_synthetic_pe_rsds_sets_pdb_debug_info() {
        let mut data = vec![0u8; 2048];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0x4C;
        data[0x45] = 0x01; // x86
        data[0x46] = 0x01; // NumberOfSections = 1
        data[0x54] = 0xE0;
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x01; // PE32
        data[0x74] = 0x00;
        data[0x75] = 0x00;
        data[0x76] = 0x40; // ImageBase = 0x400000
        data[0x58 + 92] = 16; // NumberOfRvaAndSizes

        let debug_dir_offset = 0x58 + 96 + (6 * 8);
        data[debug_dir_offset..debug_dir_offset + 4].copy_from_slice(&0x200u32.to_le_bytes());
        data[debug_dir_offset + 4..debug_dir_offset + 8].copy_from_slice(&28u32.to_le_bytes());

        let section_offset = 0x138;
        data[section_offset] = b'.';
        data[section_offset + 1] = b'r';
        data[section_offset + 2] = b'd';
        data[section_offset + 3] = b'a';
        data[section_offset + 4] = b't';
        data[section_offset + 5] = b'a';
        data[section_offset + 8..section_offset + 12].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 12..section_offset + 16].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 16..section_offset + 20].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 20..section_offset + 24].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 36] = 0x40;
        data[section_offset + 39] = 0x40; // readable data

        let debug_dir_file = 0x200usize;
        data[debug_dir_file + 12..debug_dir_file + 16]
            .copy_from_slice(&IMAGE_DEBUG_TYPE_CODEVIEW.to_le_bytes());
        let rsds_path = b"C:\\symbols\\has_pdb.pdb\0";
        let rsds_size = 24u32 + rsds_path.len() as u32;
        data[debug_dir_file + 16..debug_dir_file + 20].copy_from_slice(&rsds_size.to_le_bytes());
        data[debug_dir_file + 20..debug_dir_file + 24].copy_from_slice(&0x220u32.to_le_bytes());
        data[debug_dir_file + 24..debug_dir_file + 28].copy_from_slice(&0x220u32.to_le_bytes());

        let rsds_offset = 0x220usize;
        data[rsds_offset..rsds_offset + 4].copy_from_slice(b"RSDS");
        for (idx, byte) in data[rsds_offset + 4..rsds_offset + 20]
            .iter_mut()
            .enumerate()
        {
            *byte = (idx as u8) + 1;
        }
        data[rsds_offset + 20..rsds_offset + 24].copy_from_slice(&1u32.to_le_bytes());
        data[rsds_offset + 24..rsds_offset + 24 + rsds_path.len()].copy_from_slice(rsds_path);

        let result = PeLoader::parse(DataBuffer::Heap(data), "has_pdb.exe".to_string());
        assert!(result.is_ok());
        let bin = result.expect("rsds pe should parse");
        let pdb = bin.inner().pdb_debug_info.as_ref().expect("pdb debug info");
        assert!(pdb.has_codeview);
        assert_eq!(pdb.age, Some(1));
        assert!(
            pdb.path_hint
                .as_deref()
                .is_some_and(|path| path.ends_with("has_pdb.pdb"))
        );
    }

    #[test]
    fn test_parse_rich_header_and_delay_imports() {
        let mut data = vec![0u8; 2048];

        // DOS Header
        data[0] = 0x4D;
        data[1] = 0x5A; // MZ
        data[0x3C] = 0x80; // e_lfanew = 0x80

        // PE Header (at 0x80)
        data[0x80] = 0x50;
        data[0x81] = 0x45; // PE\0\0

        // File Header (at 0x84)
        data[0x84] = 0x4C;
        data[0x85] = 0x01; // Machine = 0x14C (x86)
        data[0x86] = 0x01; // NumberOfSections = 1
        data[0x94] = 0xE0; // SizeOfOptionalHeader = 224 (0xE0)

        // Optional Header (at 0x98)
        data[0x98] = 0x0B;
        data[0x99] = 0x01; // Magic = 0x10B (PE32)
        // ImageBase (at 0x98 + 28 = 0xB4)
        data[0xB4] = 0x00;
        data[0xB5] = 0x00;
        data[0xB6] = 0x40; // ImageBase = 0x400000
        // Data Directories (16 entries)
        data[0xF4] = 16; // NumberOfRvaAndSizes

        // Delay Load Import Table (Directory Index 13) at 0x98 + 96 + 13 * 8 = 0x160
        // virtual_address = 0x1000, size = 64
        data[0x160..0x164].copy_from_slice(&0x1000u32.to_le_bytes());
        data[0x164..0x168].copy_from_slice(&64u32.to_le_bytes());

        // Section Headers (at 0x80 + 4 + 20 + 224 = 0x178)
        let section_offset = 0x178;
        // Name: .text
        data[section_offset..section_offset + 5].copy_from_slice(b".text");
        // virtual_size = 0x1000
        data[section_offset + 8..section_offset + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        // virtual_address = 0x1000
        data[section_offset + 12..section_offset + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        // size_of_raw_data = 0x1000
        data[section_offset + 16..section_offset + 20].copy_from_slice(&0x1000u32.to_le_bytes());
        // pointer_to_raw_data = 0x200
        data[section_offset + 20..section_offset + 24].copy_from_slice(&0x200u32.to_le_bytes());
        // characteristics = Executable | Readable (0x60000020)
        data[section_offset + 36..section_offset + 40]
            .copy_from_slice(&0x60000020u32.to_le_bytes());

        // Rich Header (at 0x60..0x78)
        // xor_key = 0x11223344
        data[0x70..0x74].copy_from_slice(b"Rich");
        data[0x74..0x78].copy_from_slice(&0x11223344u32.to_le_bytes());
        // Record 1: comp_id = 0x00100020 (build = 0x0020, product = 0x0010), count = 5
        let val1 = 0x00100020u32 ^ 0x11223344;
        let val2 = 5u32 ^ 0x11223344;
        data[0x68..0x6C].copy_from_slice(&val1.to_le_bytes());
        data[0x6c..0x70].copy_from_slice(&val2.to_le_bytes());
        // DanS Marker (Record 0)
        let dans1 = 0x536e6144u32 ^ 0x11223344;
        let dans2 = 0u32 ^ 0x11223344;
        data[0x60..0x64].copy_from_slice(&dans1.to_le_bytes());
        data[0x64..0x68].copy_from_slice(&dans2.to_le_bytes());

        // --- Delay Import Data in Section 1 (file offset 0x200) ---
        // Delay import descriptor (32 bytes) at RVA 0x1000 (file offset 0x200)
        // gr_attrs = 1 (RVA based)
        data[0x200..0x204].copy_from_slice(&1u32.to_le_bytes());
        // rva_dll_name = 0x1040 (file offset 0x240)
        data[0x204..0x208].copy_from_slice(&0x1040u32.to_le_bytes());
        // rva_iat = 0x1060 (file offset 0x260)
        data[0x20C..0x210].copy_from_slice(&0x1060u32.to_le_bytes());
        // rva_int = 0x1080 (file offset 0x280)
        data[0x210..0x214].copy_from_slice(&0x1080u32.to_le_bytes());

        // DLL name at RVA 0x1040 (file offset 0x240)
        data[0x240..0x24d].copy_from_slice(b"my_delay.dll\0");

        // IAT at RVA 0x1060 (file offset 0x260)
        // Entry 0 points to proxy thunk code at RVA 0x10C0
        data[0x260..0x264].copy_from_slice(&0x10C0u32.to_le_bytes());

        // INT at RVA 0x1080 (file offset 0x280)
        // Entry 0 points to Name record at RVA 0x10A0
        data[0x280..0x284].copy_from_slice(&0x10A0u32.to_le_bytes());

        // Name Record at RVA 0x10A0 (file offset 0x2A0)
        // Hint = 0x0000, Name = "DelayFunc"
        data[0x2A0..0x2A2].copy_from_slice(&0u16.to_le_bytes());
        data[0x2A2..0x2Ac].copy_from_slice(b"DelayFunc\0");

        // Run PeLoader
        let result = PeLoader::parse(DataBuffer::Heap(data), "synthetic_delay.exe".to_string());
        if let Err(e) = &result {
            println!("PE parsing failed: {:?}", e);
        }
        assert!(result.is_ok());
        let bin = result.expect("synthetic pe should parse");

        println!("Parsed sections: {:?}", bin.sections);
        println!("Parsed functions: {:?}", bin.functions);
        println!("Parsed rich header: {:?}", bin.rich_header_records);

        // Verify Rich Header
        let rich = bin
            .rich_header_records
            .as_ref()
            .expect("Rich Header records should be parsed");
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].build_number, 0x0020);
        assert_eq!(rich[0].product_id, 0x0010);
        assert_eq!(rich[0].count, 5);
        assert_eq!(bin.get_ghidra_compiler_id(), Some("windows".to_string()));

        // Verify Delay-Load Import Function
        let delay_func = bin
            .functions
            .iter()
            .find(|f| f.name == "my_delay.dll!DelayFunc");
        assert!(delay_func.is_some(), "DelayFunc import should be recovered");
        let df = delay_func.unwrap();
        assert_eq!(df.address, 0x401060);
        assert_eq!(df.is_import, true);
        assert_eq!(df.external_library, Some("my_delay.dll".to_string()));

        // Verify Delay-Load Proxy Function
        let delay_proxy = bin
            .functions
            .iter()
            .find(|f| f.name == "DelayLoad_DelayFunc");
        assert!(
            delay_proxy.is_some(),
            "DelayLoad_DelayFunc proxy should be recovered"
        );
        let dp = delay_proxy.unwrap();
        assert_eq!(dp.address, 0x4010C0);
        assert_eq!(dp.is_import, false);
        assert_eq!(dp.kind, Some("delay_proxy".to_string()));
    }
}
