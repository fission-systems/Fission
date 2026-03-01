use super::{FunctionInfo, LoadedBinary, SectionInfo};
use crate::prelude::*;

impl LoadedBinary {
    /// Sort sections by virtual address for binary search
    pub fn sort_sections(&mut self) {
        self.sections.sort_by_key(|s| s.virtual_address);
    }

    /// Get bytes at a given address using binary search for O(log N) lookup
    pub fn get_bytes(&self, address: u64, size: usize) -> Option<Vec<u8>> {
        self.view_bytes(address, size).map(|s| s.to_vec())
    }

    /// Get a slice of bytes at a given address (zero-copy)
    pub fn view_bytes(&self, address: u64, size: usize) -> Option<&[u8]> {
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
            let file_offset = section.file_offset as usize + offset_in_section as usize;

            if file_offset + size <= self.data.as_slice().len() {
                return Some(&self.data.as_slice()[file_offset..file_offset + size]);
            }
        }
        None
    }

    /// Read a pointer at the given address
    pub fn read_ptr(&self, address: u64) -> Result<u64> {
        let size = if self.is_64bit { 8 } else { 4 };
        let bytes = self.get_bytes(address, size).ok_or_else(|| {
            FissionError::loader(format!("Could not read pointer at 0x{:x}", address))
        })?;

        let ptr = if self.is_64bit {
            u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]))
        } else {
            u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4])) as u64
        };

        Ok(ptr)
    }

    /// Get executable sections only
    pub fn executable_sections(&self) -> Vec<&SectionInfo> {
        self.sections.iter().filter(|s| s.is_executable).collect()
    }

    /// Iterate over imported functions.
    pub fn imports(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter().filter(|f| f.is_import)
    }

    /// Iterate over exported functions.
    pub fn exports(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter().filter(|f| f.is_export)
    }

    /// Get functions sorted by address
    pub fn functions_sorted(&self) -> Vec<&FunctionInfo> {
        if self.functions_sorted {
            self.functions.iter().collect()
        } else {
            let mut funcs: Vec<_> = self.functions.iter().collect();
            funcs.sort_by_key(|f| f.address);
            funcs
        }
    }

    /// Get iterator over functions (already sorted by address)
    #[inline]
    pub fn functions_iter(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter()
    }

    /// Find a function by name using O(1) HashMap lookup
    pub fn find_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.function_name_index
            .get(name)
            .and_then(|&idx| self.functions.get(idx))
    }

    /// Find function at exact address using O(1) HashMap lookup
    pub fn function_at(&self, address: u64) -> Option<&FunctionInfo> {
        if let Some(&idx) = self.function_addr_index.get(&address) {
            return self.functions.get(idx);
        }

        self.functions
            .iter()
            .find(|f| f.size > 0 && address >= f.address && address < f.address + f.size)
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

    /// Convert a virtual address to file offset using binary search for O(log N) lookup
    pub fn va_to_file_offset(&self, va: u64) -> Option<usize> {
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
    pub fn get_memory_mapped_data(&self) -> Vec<u8> {
        let max_va_end = self
            .sections
            .iter()
            .map(|s| s.virtual_address + s.virtual_size)
            .max()
            .unwrap_or(0);

        let mut mapped = vec![0u8; (max_va_end - self.image_base) as usize];
        let binary_data = self.inner().data.as_slice();

        for section in &self.sections {
            if section.file_size == 0 || section.file_offset as usize >= binary_data.len() {
                continue;
            }

            let start = section.file_offset as usize;
            let end = std::cmp::min(start + section.file_size as usize, binary_data.len());
            let size = end - start;

            let dest_start = (section.virtual_address - self.image_base) as usize;
            if dest_start + size <= mapped.len() {
                mapped[dest_start..dest_start + size].copy_from_slice(&binary_data[start..end]);
            }
        }
        mapped
    }
}
