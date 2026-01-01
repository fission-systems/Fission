//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O executables using goblin and extracts:
//! - Entry point
//! - Exported/imported functions
//! - Sections with code
//! - Symbol information

use crate::core::prelude::*;
use std::fs;
use std::path::Path;

/// Information about a function found in the binary
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name (may be empty for unnamed functions)
    pub name: String,
    /// Virtual address of the function
    pub address: u64,
    /// Size in bytes (0 if unknown)
    pub size: u64,
    /// Whether this is an exported function
    pub is_export: bool,
    /// Whether this is an imported function (stub)
    pub is_import: bool,
}

/// Information about a section in the binary
#[derive(Debug, Clone)]
pub struct SectionInfo {
    /// Section name
    pub name: String,
    /// Virtual address
    pub virtual_address: u64,
    /// Size in memory
    pub virtual_size: u64,
    /// Offset in file
    pub file_offset: u64,
    /// Size in file
    pub file_size: u64,
    /// Is this section executable?
    pub is_executable: bool,
    /// Is this section readable?
    pub is_readable: bool,
    /// Is this section writable?
    pub is_writable: bool,
}

/// Parsed binary information
#[derive(Debug, Clone)]
pub struct LoadedBinary {
    /// Original file path
    pub path: String,
    /// Raw bytes of the file
    pub data: Vec<u8>,
    /// Detected architecture (e.g., "x86:LE:64:default")
    pub arch_spec: String,
    /// Entry point address
    pub entry_point: u64,
    /// Image base address
    pub image_base: u64,
    /// All discovered functions
    pub functions: Vec<FunctionInfo>,
    /// All sections
    pub sections: Vec<SectionInfo>,
    /// Is this a 64-bit binary?
    pub is_64bit: bool,
    /// Does the image contain a CLR (.NET) runtime header?
    pub is_dotnet: bool,
    /// Reported CLR metadata version string (e.g. "v4.0.30319")
    pub dotnet_runtime_version: Option<String>,
    /// Binary format (PE, ELF, Mach-O)
    pub format: String,
    /// IAT address to symbol name mapping for decompiler output
    pub iat_symbols: std::collections::HashMap<u64, String>,
    /// Index for O(1) function lookup by address (maps address to index in functions Vec)
    function_addr_index: std::collections::HashMap<u64, usize>,
    /// Index for O(1) function lookup by name (maps name to index in functions Vec)
    function_name_index: std::collections::HashMap<String, usize>,
}

/// Builder for LoadedBinary
pub struct LoadedBinaryBuilder {
    path: String,
    data: Vec<u8>,
    arch_spec: String,
    entry_point: u64,
    image_base: u64,
    functions: Vec<FunctionInfo>,
    sections: Vec<SectionInfo>,
    is_64bit: bool,
    is_dotnet: bool,
    dotnet_runtime_version: Option<String>,
    format: String,
    iat_symbols: std::collections::HashMap<u64, String>,
}

impl LoadedBinaryBuilder {
    pub fn new(path: String, data: Vec<u8>) -> Self {
        Self {
            path,
            data,
            arch_spec: "unknown".to_string(),
            entry_point: 0,
            image_base: 0,
            functions: Vec::new(),
            sections: Vec::new(),
            is_64bit: false,
            is_dotnet: false,
            dotnet_runtime_version: None,
            format: "unknown".to_string(),
            iat_symbols: std::collections::HashMap::new(),
        }
    }

    pub fn arch_spec(mut self, arch_spec: impl Into<String>) -> Self {
        self.arch_spec = arch_spec.into();
        self
    }

    pub fn entry_point(mut self, entry_point: u64) -> Self {
        self.entry_point = entry_point;
        self
    }

    pub fn image_base(mut self, image_base: u64) -> Self {
        self.image_base = image_base;
        self
    }

    pub fn is_64bit(mut self, is_64bit: bool) -> Self {
        self.is_64bit = is_64bit;
        self
    }
    
    pub fn is_dotnet(mut self, is_dotnet: bool) -> Self {
        self.is_dotnet = is_dotnet;
        self
    }
    
    pub fn dotnet_runtime_version(mut self, version: Option<String>) -> Self {
        self.dotnet_runtime_version = version;
        self
    }

    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    pub fn add_function(mut self, function: FunctionInfo) -> Self {
        self.functions.push(function);
        self
    }

    pub fn add_functions(mut self, functions: impl IntoIterator<Item = FunctionInfo>) -> Self {
        self.functions.extend(functions);
        self
    }

    pub fn add_section(mut self, section: SectionInfo) -> Self {
        self.sections.push(section);
        self
    }
    
    pub fn add_sections(mut self, sections: impl IntoIterator<Item = SectionInfo>) -> Self {
        self.sections.extend(sections);
        self
    }
    
    pub fn add_iat_symbol(mut self, va: u64, name: String) -> Self {
        self.iat_symbols.insert(va, name);
        self
    }
    
    pub fn add_iat_symbols(mut self, symbols: std::collections::HashMap<u64, String>) -> Self {
        self.iat_symbols.extend(symbols);
        self
    }

    pub fn build(self) -> Result<LoadedBinary> {
        // Build function indices for O(1) lookups
        let mut function_addr_index = std::collections::HashMap::with_capacity(self.functions.len());
        let mut function_name_index = std::collections::HashMap::with_capacity(self.functions.len());
        
        for (idx, func) in self.functions.iter().enumerate() {
            function_addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                function_name_index.insert(func.name.clone(), idx);
            }
        }
        
        Ok(LoadedBinary {
            path: self.path,
            data: self.data,
            arch_spec: self.arch_spec,
            entry_point: self.entry_point,
            image_base: self.image_base,
            functions: self.functions,
            sections: self.sections,
            is_64bit: self.is_64bit,
            is_dotnet: self.is_dotnet,
            dotnet_runtime_version: self.dotnet_runtime_version,
            format: self.format,
            iat_symbols: self.iat_symbols,
            function_addr_index,
            function_name_index,
        })
    }
}

impl LoadedBinary {
    /// Load and parse a binary file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let data = fs::read(&path)?;
        Self::from_bytes(data, path_str)
    }

    /// Parse binary from bytes
    pub fn from_bytes(data: Vec<u8>, path: String) -> Result<Self> {
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
    
    /// Sort sections by virtual address for binary search
    fn sort_sections(&mut self) {
        self.sections.sort_by_key(|s| s.virtual_address);
    }

    /// Parse PE (Windows executable)
    fn parse_pe(data: Vec<u8>, path: String) -> Result<Self> {
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
    fn parse_elf(data: Vec<u8>, path: String) -> Result<Self> {
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
    fn parse_macho(data: Vec<u8>, path: String) -> Result<Self> {
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

    /// Get bytes at a given address using binary search for O(log N) lookup
    pub fn get_bytes(&self, address: u64, size: usize) -> Option<Vec<u8>> {
        // Binary search to find the section containing this address
        // Sections must be sorted by virtual_address (done during parsing)
        let idx = self.sections.binary_search_by(|section| {
            if address < section.virtual_address {
                std::cmp::Ordering::Greater
            } else if address >= section.virtual_address + section.virtual_size {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });
        
        if let Ok(idx) = idx {
            let section = &self.sections[idx];
            let offset_in_section = address - section.virtual_address;
            let file_offset = section.file_offset + offset_in_section;
            let end = (file_offset as usize + size).min(self.data.len());
            let start = file_offset as usize;
            
            if start < self.data.len() {
                return Some(self.data[start..end].to_vec());
            }
        }
        None
    }

    /// Get executable sections only
    pub fn executable_sections(&self) -> Vec<&SectionInfo> {
        self.sections.iter().filter(|s| s.is_executable).collect()
    }

    /// Get functions sorted by address
    pub fn functions_sorted(&self) -> Vec<&FunctionInfo> {
        let mut funcs: Vec<_> = self.functions.iter().collect();
        funcs.sort_by_key(|f| f.address);
        funcs
    }

    /// Find a function by name using O(1) HashMap lookup
    pub fn find_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.function_name_index
            .get(name)
            .and_then(|&idx| self.functions.get(idx))
    }

    /// Find function at exact address using O(1) HashMap lookup
    pub fn function_at(&self, address: u64) -> Option<&FunctionInfo> {
        // First try exact address match using the index (O(1))
        if let Some(&idx) = self.function_addr_index.get(&address) {
            return self.functions.get(idx);
        }
        
        // Fall back to range check for addresses within function bounds (O(N))
        // This is needed when the address is inside a function but not at its start
        self.functions.iter().find(|f| {
            f.size > 0 && address > f.address && address < f.address + f.size
        })
    }

    /// Find function at exact address only (no range check) - O(1) lookup
    #[inline]
    pub fn function_at_exact(&self, address: u64) -> Option<&FunctionInfo> {
        self.function_addr_index
            .get(&address)
            .and_then(|&idx| self.functions.get(idx))
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "{} {} binary\n\
             Entry: 0x{:x}\n\
             Image Base: 0x{:x}\n\
             .NET: {}{}\n\
             Sections: {}\n\
             Functions: {}",
            if self.is_64bit { "64-bit" } else { "32-bit" },
            self.format,
            self.entry_point,
            self.image_base,
            if self.is_dotnet { "yes" } else { "no" },
            self.dotnet_runtime_version
                .as_ref()
                .map(|v| format!(" (runtime {v})"))
                .unwrap_or_default(),
            self.sections.len(),
            self.functions.len()
        )
    }
    
    /// Convert a virtual address to file offset using binary search for O(log N) lookup
    pub fn va_to_file_offset(&self, va: u64) -> Option<usize> {
        // Binary search to find the section containing this VA
        let idx = self.sections.binary_search_by(|section| {
            let section_size = if section.virtual_size > 0 {
                section.virtual_size
            } else {
                section.file_size
            };
            
            if va < section.virtual_address {
                std::cmp::Ordering::Greater
            } else if va >= section.virtual_address + section_size {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });
        
        if let Ok(idx) = idx {
            let section = &self.sections[idx];
            let offset_in_section = va - section.virtual_address;
            return Some(section.file_offset as usize + offset_in_section as usize);
        }
        None
    }
    
    /// Create a memory-mapped representation of the binary for the decompiler.
    /// This places each section at its virtual address offset (relative to image_base).
    /// The returned Vec starts at image_base, so loadFill(VA) can use offset = VA - image_base.
    pub fn get_memory_mapped_data(&self) -> Vec<u8> {
        // Find the maximum virtual address extent to determine buffer size
        let max_va_end = self.sections.iter()
            .map(|s| {
                let size = if s.virtual_size > 0 { s.virtual_size } else { s.file_size };
                s.virtual_address + size
            })
            .max()
            .unwrap_or(self.image_base);
        
        // Calculate required buffer size (max_va relative to image_base)
        let buffer_size = if max_va_end > self.image_base {
            (max_va_end - self.image_base) as usize
        } else {
            0
        };
        
        // Create zeroed buffer
        let mut mapped = vec![0u8; buffer_size];
        
        // Map each section into the buffer at its RVA offset
        for section in &self.sections {
            let rva = section.virtual_address.saturating_sub(self.image_base);
            let file_start = section.file_offset as usize;
            let file_end = (section.file_offset + section.file_size) as usize;
            
            if file_end <= self.data.len() {
                let section_data = &self.data[file_start..file_end];
                let dest_start = rva as usize;
                let dest_end = dest_start + section_data.len();
                
                if dest_end <= mapped.len() {
                    mapped[dest_start..dest_end].copy_from_slice(section_data);
                }
            }
        }
        
        mapped
    }
    
    /// Discover internal functions by scanning executable code for CALL instructions
    /// This finds functions that are called but not exported/imported
    pub fn discover_internal_functions(&mut self) {
        use crate::analysis::disasm::DisasmEngine;
        use std::collections::HashSet;
        
        // Collect existing function addresses to avoid duplicates
        let existing_addrs: HashSet<u64> = self.functions.iter().map(|f| f.address).collect();
        
        // Create disassembler for this binary's architecture
        let engine = match DisasmEngine::new(self.is_64bit) {
            Ok(e) => e,
            Err(_) => return,
        };
        
        let mut discovered: HashSet<u64> = HashSet::new();
        
        // Scan all executable sections
        for section in &self.sections {
            if !section.is_executable {
                continue;
            }
            
            // Get section bytes
            let start = section.file_offset as usize;
            let size = section.file_size as usize;
            if start + size > self.data.len() {
                continue;
            }
            let bytes = &self.data[start..start + size];
            
            // Discover call targets in this section
            let targets = engine.discover_call_targets(bytes, section.virtual_address);
            
            for target in targets {
                // Only add if not already known and within our address space
                if !existing_addrs.contains(&target) && !discovered.contains(&target) {
                    // Verify target is within an executable section
                    let in_code = self.sections.iter().any(|s| {
                        s.is_executable 
                            && target >= s.virtual_address 
                            && target < s.virtual_address + s.virtual_size as u64
                    });
                    
                    if in_code {
                        discovered.insert(target);
                    }
                }
            }
        }
        
        // Add discovered functions
        for addr in discovered {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }
        
        // Sort functions by address
        self.functions.sort_by_key(|f| f.address);
        
        // Rebuild function indices after adding new functions
        self.rebuild_function_indices();
    }
    
    /// Rebuild function lookup indices after modifying the functions vector
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();
        
        for (idx, func) in self.functions.iter().enumerate() {
            self.function_addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                self.function_name_index.insert(func.name.clone(), idx);
            }
        }
    }
    
    // ========================================================================
    // Binary Patching
    // ========================================================================
    
    /// Patch bytes at a file offset
    /// Returns the original bytes that were replaced
    pub fn patch_bytes(&mut self, offset: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + new_bytes.len();
        
        if end > self.data.len() {
            return None;
        }
        
        // Save original bytes
        let original = self.data[offset..end].to_vec();
        
        // Apply patch
        self.data[offset..end].copy_from_slice(new_bytes);
        
        Some(original)
    }
    
    /// Patch bytes at a virtual address
    /// Converts VA to file offset and applies the patch
    pub fn patch_bytes_va(&mut self, va: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = self.va_to_file_offset(va)?;
        self.patch_bytes(offset as u64, new_bytes)
    }
    
    /// Get bytes at a file offset (for displaying original)
    pub fn get_bytes_at_offset(&self, offset: u64, size: usize) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + size;
        
        if end > self.data.len() {
            return None;
        }
        
        Some(self.data[offset..end].to_vec())
    }
    
    /// Save the (potentially patched) binary to a file
    pub fn save_as<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        std::fs::write(path, &self.data)?;
        Ok(())
    }
    
    /// Apply a quick patch at a file offset
    pub fn apply_quick_patch(&mut self, offset: u64, patch_type: super::patch::QuickPatch) -> Option<Vec<u8>> {
        self.patch_bytes(offset, &patch_type.bytes())
    }
    
    /// Apply a quick patch at a virtual address
    pub fn apply_quick_patch_va(&mut self, va: u64, patch_type: super::patch::QuickPatch) -> Option<Vec<u8>> {
        let offset = self.va_to_file_offset(va)?;
        self.patch_bytes(offset as u64, &patch_type.bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_self() {
        // Parse the test executable itself
        let exe_path = std::env::current_exe().unwrap();
        let result = LoadedBinary::from_file(&exe_path);
        
        if let Ok(binary) = result {
            println!("{}", binary.summary());
            println!("\nFirst 10 functions:");
            for func in binary.functions_sorted().iter().take(10) {
                println!("  0x{:08x}: {} (size: {})", func.address, func.name, func.size);
            }
            assert!(binary.entry_point != 0);
            assert!(!binary.sections.is_empty());
        } else {
            println!("Could not parse self: {:?}", result);
        }
    }

    #[test]
    fn test_loaded_binary_builder() {
        let builder = LoadedBinaryBuilder::new("test.bin".to_string(), vec![0x90; 100])
            .format("RAW")
            .entry_point(0x1000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "main".to_string(),
                address: 0x1000,
                size: 20,
                is_export: true,
                is_import: false,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 100,
                file_offset: 0,
                file_size: 100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            });
            
        let binary = builder.build().expect("Failed to build LoadedBinary");
        
        assert_eq!(binary.path, "test.bin");
        assert_eq!(binary.data.len(), 100);
        assert_eq!(binary.entry_point, 0x1000);
        assert_eq!(binary.format, "RAW");
        assert!(binary.is_64bit);
        assert_eq!(binary.functions.len(), 1);
        assert_eq!(binary.sections.len(), 1);
        
        let func = binary.find_function("main").unwrap();
        assert_eq!(func.address, 0x1000);
    }
    
    #[test]
    fn test_function_lookup_o1() {
        // Test that O(1) function lookups work correctly
        let builder = LoadedBinaryBuilder::new("test.bin".to_string(), vec![0x90; 1000])
            .format("RAW")
            .entry_point(0x1000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "func_a".to_string(),
                address: 0x1000,
                size: 50,
                is_export: true,
                is_import: false,
            })
            .add_function(FunctionInfo {
                name: "func_b".to_string(),
                address: 0x1100,
                size: 100,
                is_export: false,
                is_import: false,
            })
            .add_function(FunctionInfo {
                name: "func_c".to_string(),
                address: 0x1200,
                size: 0,  // Unknown size
                is_export: false,
                is_import: true,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 1000,
                file_offset: 0,
                file_size: 1000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            });
            
        let binary = builder.build().expect("Failed to build LoadedBinary");
        
        // Test find_function by name (O(1) lookup)
        assert!(binary.find_function("func_a").is_some());
        assert!(binary.find_function("func_b").is_some());
        assert!(binary.find_function("func_c").is_some());
        assert!(binary.find_function("nonexistent").is_none());
        
        // Test function_at_exact (O(1) lookup)
        assert!(binary.function_at_exact(0x1000).is_some());
        assert_eq!(binary.function_at_exact(0x1000).unwrap().name, "func_a");
        assert!(binary.function_at_exact(0x1100).is_some());
        assert_eq!(binary.function_at_exact(0x1100).unwrap().name, "func_b");
        assert!(binary.function_at_exact(0x1050).is_none()); // Not at start of function
        
        // Test function_at with range check (exact match is O(1), range check is O(N))
        assert!(binary.function_at(0x1000).is_some());
        assert_eq!(binary.function_at(0x1000).unwrap().name, "func_a");
        assert!(binary.function_at(0x1020).is_some()); // Inside func_a (size=50)
        assert_eq!(binary.function_at(0x1020).unwrap().name, "func_a");
        assert!(binary.function_at(0x1150).is_some()); // Inside func_b (size=100)
        assert_eq!(binary.function_at(0x1150).unwrap().name, "func_b");
    }
}
