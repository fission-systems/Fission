use crate::core::prelude::*;
use crate::analysis::loader::types::{LoadedBinary, LoadedBinaryBuilder, FunctionInfo, SectionInfo};
use super::BinaryParser;

pub struct StaticParser;

impl StaticParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse PE (Windows executable)
    fn parse_pe(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Try parsing with goblin first
        match goblin::pe::PE::parse(&data) {
            Ok(pe) => {
                // Extract all data we need first to avoid borrowing conflicts
                // We need to act on `pe` which borrows from `data`
                
                let is_64bit = pe.is_64;
                let image_base = pe.image_base as u64;
                let entry_point = image_base + pe.entry as u64;
                let mut is_dotnet = false;
                let mut dotnet_runtime_version = None;
                
                let mut sections_info = Vec::new();
                let mut functions_info = Vec::new();
                let mut iat_symbols_map = std::collections::HashMap::new();

                if let Some(optional_header) = pe.header.optional_header {
                    if let Some(clr_dir) = optional_header
                        .data_directories
                        .get_clr_runtime_header()
                    {
                        if clr_dir.virtual_address != 0 && clr_dir.size != 0 {
                            is_dotnet = true;
                            // Try to read runtime version from COR20 header
                            // Find section containing this RVA
                            for section in &pe.sections {
                                let start = section.virtual_address;
                                let size = if section.virtual_size == 0 {
                                    section.size_of_raw_data
                                } else {
                                    section.virtual_size
                                };
                                if clr_dir.virtual_address >= start && clr_dir.virtual_address < start + size {
                                    let delta = clr_dir.virtual_address - start;
                                    let cor20_offset = section.pointer_to_raw_data as usize + delta as usize;
                                    if cor20_offset + 8 < data.len() {
                                        let major = u16::from_le_bytes([data[cor20_offset + 4], data[cor20_offset + 5]]);
                                        let minor = u16::from_le_bytes([data[cor20_offset + 6], data[cor20_offset + 7]]);
                                        dotnet_runtime_version = Some(format!("v{}.{}", major, minor));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                // Determine architecture
                let arch_spec = if is_64bit {
                    "x86:LE:64:default"
                } else {
                    "x86:LE:32:default"
                };

                // Collect sections
                for section in &pe.sections {
                    let name = String::from_utf8_lossy(&section.name)
                        .trim_end_matches('\0')
                        .to_string();
                    
                    let characteristics = section.characteristics;
                    sections_info.push(SectionInfo {
                        name,
                        virtual_address: image_base + section.virtual_address as u64,
                        virtual_size: section.virtual_size as u64,
                        file_offset: section.pointer_to_raw_data as u64,
                        file_size: section.size_of_raw_data as u64,
                        is_executable: (characteristics & 0x20000000) != 0,
                        is_readable: (characteristics & 0x40000000) != 0,
                        is_writable: (characteristics & 0x80000000) != 0,
                    });
                }


                // Collect functions from exports
                for export in &pe.exports {
                    if let Some(name) = &export.name {
                        functions_info.push(FunctionInfo {
                            name: name.to_string(),
                            address: image_base + export.rva as u64,
                            size: 0,
                            is_export: true,
                            is_import: false,
                        });
                    }
                }

                // Add imports and build IAT symbol map
                for import in &pe.imports {
                    // IAT address = image_base + offset (this is what Ghidra uses)
                    let iat_va = image_base + import.offset as u64;
                    
                    iat_symbols_map.insert(iat_va, import.name.to_string());
                    
                    functions_info.push(FunctionInfo {
                        name: import.name.to_string(),
                        address: image_base + import.rva as u64,
                        size: 0,
                        is_export: false,
                        is_import: true,
                    });
                }

                // Add entry point
                let has_entry = functions_info.iter().any(|f| f.address == entry_point);
                if !has_entry {
                    functions_info.push(FunctionInfo {
                        name: "_start".to_string(),
                        address: entry_point,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }

                // Now construct builder after we are done reading from pe (data borrow)
                LoadedBinaryBuilder::new(path, data)
                    .format("PE")
                    .arch_spec(arch_spec)
                    .entry_point(entry_point)
                    .image_base(image_base)
                    .is_64bit(is_64bit)
                    .is_dotnet(is_dotnet)
                    .dotnet_runtime_version(dotnet_runtime_version)
                    .add_sections(sections_info)
                    .add_functions(functions_info)
                    .add_iat_symbols(iat_symbols_map)
                    .build()
            }
            Err(_e) => {
                // Fallback to 'object' crate if goblin fails
                use object::{Object, ObjectSection, ObjectSymbol};
                
                let file = object::File::parse(&*data).map_err(|e| err!(loader, "Failed fallback parsing: {}", e))?;
                
                let is_64bit = file.is_64();
                let entry_point = file.entry();
                let image_base = file.relative_address_base();
                let arch_spec = if is_64bit { "x86:LE:64:default" } else { "x86:LE:32:default" };
                
                // Extract all data first
                let mut sections_info = Vec::new();
                for section in file.sections() {
                    if let Ok(name) = section.name() {
                        sections_info.push(SectionInfo {
                            name: name.to_string(),
                            virtual_address: section.address(),
                            virtual_size: section.size(),
                            file_offset: section.file_range().map(|(start, _)| start).unwrap_or(0),
                            file_size: section.file_range().map(|(_, len)| len).unwrap_or(0),
                            is_executable: section.kind() == object::SectionKind::Text,
                            is_readable: true,
                            is_writable: section.kind() == object::SectionKind::Data,
                        });
                    }
                }
                
                // Extract functions from symbols
                let mut functions_info = Vec::new();
                for symbol in file.symbols() {
                    if symbol.is_definition() && symbol.kind() == object::SymbolKind::Text {
                        if let Ok(name) = symbol.name() {
                            functions_info.push(FunctionInfo {
                                name: name.to_string(),
                                address: symbol.address(),
                                size: symbol.size() as u64,
                                is_export: true,
                                is_import: false,
                            });
                        }
                    }
                }
                
                // Try exports
                if let Ok(exports) = file.exports() {
                    for export in exports {
                        let name = String::from_utf8_lossy(export.name()).to_string();
                        let addr = export.address();
                        if !functions_info.iter().any(|f| f.address == addr) {
                            functions_info.push(FunctionInfo {
                                name,
                                address: addr,
                                size: 0,
                                is_export: true,
                                is_import: false,
                            });
                        }
                    }
                }
                
                // Try imports
                if let Ok(imports) = file.imports() {
                    for import in imports {
                        let name = String::from_utf8_lossy(import.name()).to_string();
                        functions_info.push(FunctionInfo {
                            name,
                            address: 0, // Imports don't have fixed addresses in file
                            size: 0,
                            is_export: false,
                            is_import: true,
                        });
                    }
                }
                
                // Add entry point if not already present
                let has_entry = functions_info.iter().any(|f| f.address == entry_point);
                if !has_entry && entry_point != 0 {
                    functions_info.push(FunctionInfo {
                        name: "_start".to_string(),
                        address: entry_point,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }
                
                // Try to extract IAT symbols from raw PE headers
                // This is needed because object crate's imports() doesn't provide addresses
                let mut iat_symbols_map = std::collections::HashMap::new();
                
                // Parse PE manually for IAT with lenient options (skip certificate validation)
                let opts = goblin::pe::options::ParseOptions {
                    resolve_rva: true,
                    parse_attribute_certificates: false,  // Skip malformed certificates
                };
                
                match goblin::pe::PE::parse_with_opts(&data, &opts) {
                    Ok(pe) => {
                        eprintln!("[loader] goblin PE parsed (lenient): {} imports", pe.imports.len());
                        for import in &pe.imports {
                            let iat_va = image_base + import.offset as u64;
                            iat_symbols_map.insert(iat_va, import.name.to_string());
                        }
                    }
                    Err(e) => {
                        eprintln!("[loader] goblin PE parse failed in fallback (lenient): {}", e);
                    }
                }

                LoadedBinaryBuilder::new(path, data)
                    .format("PE (Fallback)")
                    .arch_spec(arch_spec)
                    .entry_point(entry_point)
                    .image_base(image_base)
                    .is_64bit(is_64bit)
                    .add_sections(sections_info)
                    .add_functions(functions_info)
                    .add_iat_symbols(iat_symbols_map)
                    .build()
            }
        }
    }

    /// Parse ELF (Linux executable)
    fn parse_elf(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        let elf = goblin::elf::Elf::parse(&data)?;
        
        let is_64bit = elf.is_64;
        let entry_point = elf.entry;

        // Determine architecture
        let arch_spec = match (elf.header.e_machine, is_64bit) {
            (goblin::elf::header::EM_X86_64, true) => "x86:LE:64:default",
            (goblin::elf::header::EM_386, false) => "x86:LE:32:default",
            (goblin::elf::header::EM_ARM, false) => "ARM:LE:32:v7",
            (goblin::elf::header::EM_AARCH64, true) => "AARCH64:LE:64:v8A",
            _ => "x86:LE:64:default",
        };

        // Get image base
        let image_base = elf.program_headers.iter()
            .filter(|ph| ph.p_type == goblin::elf::program_header::PT_LOAD)
            .map(|ph| ph.p_vaddr)
            .min()
            .unwrap_or(0);

        // Collect sections
        let mut sections_info = Vec::new();
        for section in &elf.section_headers {
            let name = elf.shdr_strtab.get_at(section.sh_name).unwrap_or("").to_string();
            let flags = section.sh_flags;
            sections_info.push(SectionInfo {
                name,
                virtual_address: section.sh_addr,
                virtual_size: section.sh_size,
                file_offset: section.sh_offset,
                file_size: section.sh_size,
                is_executable: (flags & goblin::elf::section_header::SHF_EXECINSTR as u64) != 0,
                is_readable: (flags & goblin::elf::section_header::SHF_ALLOC as u64) != 0,
                is_writable: (flags & goblin::elf::section_header::SHF_WRITE as u64) != 0,
            });
        }

        // Collect functions from symbols
        let mut functions_info = Vec::new();
        for sym in &elf.syms {
            if sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_value != 0 {
                let name = elf.strtab.get_at(sym.st_name).unwrap_or("").to_string();
                functions_info.push(FunctionInfo {
                    name,
                    address: sym.st_value,
                    size: sym.st_size,
                    is_export: sym.st_bind() == goblin::elf::sym::STB_GLOBAL,
                    is_import: sym.st_shndx == goblin::elf::section_header::SHN_UNDEF as usize,
                });
            }
        }

        // Dynamic symbols
        for sym in &elf.dynsyms {
            if sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_value != 0 {
                let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("").to_string();
                if !functions_info.iter().any(|f| f.address == sym.st_value) {
                    functions_info.push(FunctionInfo {
                        name,
                        address: sym.st_value,
                        size: sym.st_size,
                        is_export: sym.st_bind() == goblin::elf::sym::STB_GLOBAL,
                        is_import: sym.st_shndx == goblin::elf::section_header::SHN_UNDEF as usize,
                    });
                }
            }
        }

        // Add entry point
        let has_entry = functions_info.iter().any(|f| f.address == entry_point);
        if !has_entry && entry_point != 0 {
            functions_info.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }
        
        // Extract PLT/GOT symbols for imported function resolution
        let mut iat_symbols_map = std::collections::HashMap::new();
        
        // Get PLT relocations - these contain the mapping from PLT address to function name
        for reloc in &elf.pltrelocs {
            if let Some(sym) = elf.dynsyms.get(reloc.r_sym) {
                if sym.st_type() == goblin::elf::sym::STT_FUNC {
                    let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("").to_string();
                    if !name.is_empty() {
                        // The r_offset is the GOT entry address, but we want the PLT address
                        // For now, use the symbol value if non-zero, or the GOT offset
                        let addr = if sym.st_value != 0 { sym.st_value } else { reloc.r_offset };
                        iat_symbols_map.insert(addr, name);
                    }
                }
            }
        }
        
        // Also add dynamic symbols with undefined section (imports)
        for sym in &elf.dynsyms {
            if sym.st_type() == goblin::elf::sym::STT_FUNC 
                && sym.st_shndx == goblin::elf::section_header::SHN_UNDEF as usize
                && sym.st_value != 0 {
                let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("").to_string();
                if !name.is_empty() {
                    iat_symbols_map.insert(sym.st_value, name);
                }
            }
        }

        LoadedBinaryBuilder::new(path, data)
            .format("ELF")
            .arch_spec(arch_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols_map)
            .build()
    }

    /// Parse Mach-O (macOS executable)
    fn parse_macho(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        let mach = goblin::mach::Mach::parse(&data)?;
        
        match mach {
            goblin::mach::Mach::Binary(macho) => {
                let is_64bit = macho.is_64;
                let entry_point = macho.entry;

                let arch_spec = if is_64bit {
                    "x86:LE:64:default"
                } else {
                    "x86:LE:32:default"
                };

                let mut sections_info = Vec::new();
                for segment in &macho.segments {
                    let name = segment.name().unwrap_or("").to_string();
                    sections_info.push(SectionInfo {
                        name,
                        virtual_address: segment.vmaddr,
                        virtual_size: segment.vmsize,
                        file_offset: segment.fileoff,
                        file_size: segment.filesize,
                        is_executable: (segment.initprot & 0x4) != 0,
                        is_readable: (segment.initprot & 0x1) != 0,
                        is_writable: (segment.initprot & 0x2) != 0,
                    });
                }

                let mut functions_info = Vec::new();
                if let Ok(exports) = macho.exports() {
                    for export in exports {
                        functions_info.push(FunctionInfo {
                            name: export.name.to_string(),
                            address: export.offset,
                            size: 0,
                            is_export: true,
                            is_import: false,
                        });
                    }
                }

                if entry_point != 0 {
                    functions_info.push(FunctionInfo {
                        name: "_main".to_string(),
                        address: entry_point,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }
                
                // Extract import symbols from imports
                let mut iat_symbols_map = std::collections::HashMap::new();
                for import in macho.imports().unwrap_or_default() {
                    if import.address != 0 {
                        iat_symbols_map.insert(import.address, import.name.to_string());
                    }
                }

                LoadedBinaryBuilder::new(path, data)
                    .format("Mach-O")
                    .arch_spec(arch_spec)
                    .entry_point(entry_point)
                    .is_64bit(is_64bit)
                    .add_sections(sections_info)
                    .add_functions(functions_info)
                    .add_iat_symbols(iat_symbols_map)
                    .build()
            }
            goblin::mach::Mach::Fat(_) => Err(err!(loader, "Fat Mach-O binaries not yet supported")),
        }
    }
}

impl BinaryParser for StaticParser {
    fn parse(&self, data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Check magic bytes to determine format
        if data.len() < 4 {
            return Err(err!(loader, "File too small"));
        }
        
        // Check for PE (MZ header)
        if data.len() > 2 && data[0] == 0x4D && data[1] == 0x5A {
            let mut binary = Self::parse_pe(data, path)?;
            binary.sort_sections();
            return Ok(binary);
        }
        
        // Check for ELF
        if data.len() > 4 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
            let mut binary = Self::parse_elf(data, path)?;
            binary.sort_sections();
            return Ok(binary);
        }
        
        // Check for Mach-O
        if data.len() > 4 {
            let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if magic == 0xFEEDFACE || magic == 0xFEEDFACF || 
               magic == 0xCEFAEDFE || magic == 0xCFFAEDFE {
                let mut binary = Self::parse_macho(data, path)?;
                binary.sort_sections();
                return Ok(binary);
            }
        }
        
        Err(err!(loader, "Unknown binary format"))
    }
}
