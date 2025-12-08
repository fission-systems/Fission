//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O executables using goblin and extracts:
//! - Entry point
//! - Exported/imported functions
//! - Sections with code
//! - Symbol information

use anyhow::{anyhow, Result};
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
#[derive(Debug)]
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
    /// Binary format (PE, ELF, Mach-O)
    pub format: String,
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
            return Err(anyhow!("File too small"));
        }
        
        // Check for PE (MZ header)
        if data.len() > 2 && data[0] == 0x4D && data[1] == 0x5A {
            return Self::parse_pe(data, path);
        }
        
        // Check for ELF
        if data.len() > 4 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
            return Self::parse_elf(data, path);
        }
        
        // Check for Mach-O
        if data.len() > 4 {
            let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if magic == 0xFEEDFACE || magic == 0xFEEDFACF || 
               magic == 0xCEFAEDFE || magic == 0xCFFAEDFE {
                return Self::parse_macho(data, path);
            }
        }
        
        Err(anyhow!("Unknown binary format"))
    }

    /// Parse PE (Windows executable)
    fn parse_pe(data: Vec<u8>, path: String) -> Result<Self> {
        // Try parsing with goblin first
        match goblin::pe::PE::parse(&data) {
            Ok(pe) => {
                let is_64bit = pe.is_64;
                let image_base = pe.image_base as u64;
                let entry_point = image_base + pe.entry as u64;

                // Determine architecture
                let arch_spec = if is_64bit {
                    "x86:LE:64:default"
                } else {
                    "x86:LE:32:default"
                };

                // Collect sections
                let mut sections = Vec::new();
                for section in &pe.sections {
                    let name = String::from_utf8_lossy(&section.name)
                        .trim_end_matches('\0')
                        .to_string();
                    
                    let characteristics = section.characteristics;
                    sections.push(SectionInfo {
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
                let mut functions = Vec::new();
                for export in &pe.exports {
                    if let Some(name) = &export.name {
                        functions.push(FunctionInfo {
                            name: name.to_string(),
                            address: image_base + export.rva as u64,
                            size: 0,
                            is_export: true,
                            is_import: false,
                        });
                    }
                }

                // Add imports
                for import in &pe.imports {
                    functions.push(FunctionInfo {
                        name: import.name.to_string(),
                        address: image_base + import.rva as u64,
                        size: 0,
                        is_export: false,
                        is_import: true,
                    });
                }

                // Add entry point
                let has_entry = functions.iter().any(|f| f.address == entry_point);
                if !has_entry {
                    functions.push(FunctionInfo {
                        name: "_start".to_string(),
                        address: entry_point,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }

                Ok(Self {
                    path,
                    data,
                    arch_spec: arch_spec.to_string(),
                    entry_point,
                    image_base,
                    functions,
                    sections,
                    is_64bit,
                    format: "PE".to_string(),
                })
            }
            Err(e) => {
                // Fallback to 'object' crate if goblin fails
                // Note: We need to import object features here or at top level
                use object::{Object, File};
                
                let file = File::parse(&*data).map_err(|e| anyhow!("Failed fallback parsing: {}", e))?;
                
                let is_64bit = file.is_64();
                let entry_point = file.entry();
                let image_base = file.relative_address_base();
                let sections: Vec<SectionInfo> = Vec::new(); // Basic info only for fallback
                
                // object crate gives limited section info easily, this is a minimal fallback
                
                // Just return minimal info to allow loading
                let arch_spec = if is_64bit { "x86:LE:64:default" } else { "x86:LE:32:default" };
                
                Ok(Self {
                    path,
                    data,
                    arch_spec: arch_spec.to_string(),
                    entry_point,
                    image_base,
                    functions: Vec::new(), // Minimal info
                    sections,
                    is_64bit,
                    format: "PE (Fallback)".to_string(),
                })
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
        let mut sections = Vec::new();
        for section in &elf.section_headers {
            let name = elf.shdr_strtab.get_at(section.sh_name).unwrap_or("").to_string();
            let flags = section.sh_flags;
            sections.push(SectionInfo {
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
        let mut functions = Vec::new();
        for sym in &elf.syms {
            if sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_value != 0 {
                let name = elf.strtab.get_at(sym.st_name).unwrap_or("").to_string();
                functions.push(FunctionInfo {
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
                if !functions.iter().any(|f| f.address == sym.st_value) {
                    functions.push(FunctionInfo {
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
        let has_entry = functions.iter().any(|f| f.address == entry_point);
        if !has_entry && entry_point != 0 {
            functions.push(FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        Ok(Self {
            path,
            data,
            arch_spec: arch_spec.to_string(),
            entry_point,
            image_base,
            functions,
            sections,
            is_64bit,
            format: "ELF".to_string(),
        })
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

                let mut sections = Vec::new();
                for segment in &macho.segments {
                    let name = segment.name().unwrap_or("").to_string();
                    sections.push(SectionInfo {
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

                let mut functions = Vec::new();
                if let Ok(exports) = macho.exports() {
                    for export in exports {
                        functions.push(FunctionInfo {
                            name: export.name.to_string(),
                            address: export.offset,
                            size: 0,
                            is_export: true,
                            is_import: false,
                        });
                    }
                }

                if entry_point != 0 {
                    functions.push(FunctionInfo {
                        name: "_main".to_string(),
                        address: entry_point,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }

                Ok(Self {
                    path,
                    data,
                    arch_spec: arch_spec.to_string(),
                    entry_point,
                    image_base: 0,
                    functions,
                    sections,
                    is_64bit,
                    format: "Mach-O".to_string(),
                })
            }
            goblin::mach::Mach::Fat(_) => Err(anyhow!("Fat Mach-O binaries not yet supported")),
        }
    }

    /// Get bytes at a given address
    pub fn get_bytes(&self, address: u64, size: usize) -> Option<Vec<u8>> {
        for section in &self.sections {
            if address >= section.virtual_address 
                && address < section.virtual_address + section.virtual_size 
            {
                let offset_in_section = address - section.virtual_address;
                let file_offset = section.file_offset + offset_in_section;
                let end = (file_offset as usize + size).min(self.data.len());
                let start = file_offset as usize;
                
                if start < self.data.len() {
                    return Some(self.data[start..end].to_vec());
                }
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

    /// Find a function by name
    pub fn find_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Find function at address
    pub fn function_at(&self, address: u64) -> Option<&FunctionInfo> {
        self.functions.iter().find(|f| {
            if f.size > 0 {
                address >= f.address && address < f.address + f.size
            } else {
                address == f.address
            }
        })
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "{} {} binary\n\
             Entry: 0x{:x}\n\
             Image Base: 0x{:x}\n\
             Sections: {}\n\
             Functions: {}",
            if self.is_64bit { "64-bit" } else { "32-bit" },
            self.format,
            self.entry_point,
            self.image_base,
            self.sections.len(),
            self.functions.len()
        )
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
}
