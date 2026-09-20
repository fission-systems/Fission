use super::{FunctionInfo, LoadedBinary, SectionInfo};
use crate::loader::reader::Endian;
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
            return self.view_section_bytes(section, address, size);
        }
        None
    }

    /// Get bytes for instruction decoding, preferring executable sections when
    /// relocatable objects map multiple sections to the same virtual address.
    pub fn view_executable_bytes(&self, address: u64, size: usize) -> Option<&[u8]> {
        let section = self.executable_section_containing(address)?;
        self.view_section_bytes(section, address, size)
    }

    /// Return the executable section containing `address`.
    pub fn executable_section_containing(&self, address: u64) -> Option<&SectionInfo> {
        self.sections
            .iter()
            .filter(|section| section.is_executable && section_contains(section, address))
            .min_by_key(|section| section.file_offset)
    }

    /// Return any section containing `address`, preferring executable sections
    /// for overlapping relocatable-section address ranges.
    pub fn section_containing_for_execution(&self, address: u64) -> Option<&SectionInfo> {
        self.executable_section_containing(address).or_else(|| {
            self.sections
                .iter()
                .find(|section| section_contains(section, address))
        })
    }

    /// Get a slice from a known section without re-running address lookup.
    pub fn view_section_bytes(
        &self,
        section: &SectionInfo,
        address: u64,
        size: usize,
    ) -> Option<&[u8]> {
        if !section_contains(section, address) {
            return None;
        }
        let offset_in_section = address - section.virtual_address;
        // A section can be larger in memory than in the file -- `.bss` is the
        // extreme case, `virtual_size` bytes against `file_size` of zero -- and
        // the bytes past `file_size` are not in the file at all. They must not
        // be read from wherever `file_offset + offset` happens to land: for
        // `.bss`, `file_offset` is 0, so every read returned the start of the
        // image. A PE's `.bss` came back holding "This program cannot be run in
        // DOS mode", which is how mingw's CRT ended up spinning for ever on a
        // once-lock whose zero it never saw.
        if offset_in_section >= section.file_size {
            return None;
        }
        // Short reads are normal -- a decoder asks for a window and takes what
        // it gets -- so hand back what the section actually holds rather than
        // refusing. What must never happen is reading *past* `file_size` into
        // whatever follows in the file.
        let available = (section.file_size - offset_in_section) as usize;
        let take = size.min(available);
        if take == 0 {
            return None;
        }
        let file_offset = section.file_offset as usize + offset_in_section as usize;
        if file_offset + take <= self.data.as_slice().len() {
            Some(&self.data.as_slice()[file_offset..file_offset + take])
        } else {
            None
        }
    }

    /// Return how many bytes can be read from the execution section at `address`.
    pub fn available_execution_bytes(&self, address: u64) -> Option<usize> {
        let section = self.section_containing_for_execution(address)?;
        if !section_contains(section, address) {
            return None;
        }
        let offset_in_section = address.checked_sub(section.virtual_address)?;
        let virtual_size = if section.virtual_size > 0 {
            section.virtual_size
        } else {
            section.file_size
        };
        let virtual_remaining = virtual_size.checked_sub(offset_in_section)?;
        let file_remaining = section.file_size.checked_sub(offset_in_section)?;
        let file_offset = section.file_offset.checked_add(offset_in_section)?;
        let data_remaining = u64::try_from(self.data.as_slice().len())
            .ok()?
            .checked_sub(file_offset)?;
        let available = virtual_remaining.min(file_remaining).min(data_remaining);
        usize::try_from(available)
            .ok()
            .filter(|available| *available > 0)
    }

    /// Return the target byte order recorded by the loader.
    ///
    /// Fully parsed binaries carry this fact in `ArchitectureDescriptor`. The
    /// load-spec and legacy language-id fallbacks keep synthetic binaries and
    /// older snapshots compatible while preserving the historical little-endian
    /// default for inputs with no architecture metadata.
    pub fn endian(&self) -> Endian {
        self.architecture
            .as_ref()
            .and_then(|architecture| parse_endian_name(&architecture.endian))
            .or_else(|| {
                self.load_spec
                    .as_ref()
                    .and_then(|spec| parse_language_endian(spec.pair.language_id.as_str()))
            })
            .or_else(|| parse_language_endian(&self.arch_spec))
            .unwrap_or(Endian::Little)
    }

    /// Whether target integer fields are encoded most-significant byte first.
    #[inline]
    pub fn is_little_endian(&self) -> bool {
        self.endian() == Endian::Little
    }

    fn read_exact<const N: usize>(&self, address: u64, kind: &str) -> Result<[u8; N]> {
        let bytes = self.get_bytes(address, N).ok_or_else(|| {
            FissionError::loader(format!("Could not read {kind} at 0x{address:x}"))
        })?;
        bytes
            .try_into()
            .map_err(|_| FissionError::loader(format!("Could not read {kind} at 0x{address:x}")))
    }

    /// Read a target-endian 16-bit integer at the given address.
    pub fn read_u16(&self, address: u64) -> Result<u16> {
        let raw = self.read_exact::<2>(address, "u16")?;
        Ok(match self.endian() {
            Endian::Little => u16::from_le_bytes(raw),
            Endian::Big => u16::from_be_bytes(raw),
        })
    }

    /// Read a target-endian 32-bit integer at the given address.
    pub fn read_u32(&self, address: u64) -> Result<u32> {
        let raw = self.read_exact::<4>(address, "u32")?;
        Ok(match self.endian() {
            Endian::Little => u32::from_le_bytes(raw),
            Endian::Big => u32::from_be_bytes(raw),
        })
    }

    /// Read a target-endian 64-bit integer at the given address.
    pub fn read_u64(&self, address: u64) -> Result<u64> {
        let raw = self.read_exact::<8>(address, "u64")?;
        Ok(match self.endian() {
            Endian::Little => u64::from_le_bytes(raw),
            Endian::Big => u64::from_be_bytes(raw),
        })
    }

    /// Read a target-endian pointer at the given address.
    pub fn read_ptr(&self, address: u64) -> Result<u64> {
        if self.is_64bit {
            self.read_u64(address)
        } else {
            self.read_u32(address).map(u64::from)
        }
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

    /// Return the function with the lowest start address strictly greater than `address`.
    /// Useful for estimating the byte range of a function whose size is not recorded.
    pub fn function_after(&self, address: u64) -> Option<&FunctionInfo> {
        self.functions
            .iter()
            .filter(|f| f.address > address)
            .min_by_key(|f| f.address)
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
            if offset_in_section >= section.file_size {
                return None;
            }
            let file_offset = section.file_offset.checked_add(offset_in_section)?;
            return usize::try_from(file_offset).ok();
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

fn parse_endian_name(value: &str) -> Option<Endian> {
    match value.trim().to_ascii_lowercase().as_str() {
        "le" | "little" => Some(Endian::Little),
        "be" | "big" => Some(Endian::Big),
        _ => None,
    }
}

fn parse_language_endian(language_id: &str) -> Option<Endian> {
    language_id.split(':').nth(1).and_then(parse_endian_name)
}

fn section_contains(section: &SectionInfo, address: u64) -> bool {
    let section_size = if section.virtual_size > 0 {
        section.virtual_size
    } else {
        section.file_size
    };
    section_size > 0
        && address >= section.virtual_address
        && address < section.virtual_address + section_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::types::{DataBuffer, LoadedBinaryBuilder};

    fn binary_with_sections(sections: Vec<SectionInfo>) -> LoadedBinary {
        LoadedBinaryBuilder::new("query-test".to_string(), DataBuffer::Heap(vec![0; 0x100]))
            .add_sections(sections)
            .build()
            .expect("synthetic binary should build")
    }

    #[test]
    fn va_to_file_offset_rejects_virtual_only_section_bytes() {
        let binary = binary_with_sections(vec![SectionInfo {
            name: ".bss".to_string(),
            virtual_address: 0x2000,
            virtual_size: 0x100,
            file_offset: 0,
            file_size: 0,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        }]);

        assert_eq!(binary.va_to_file_offset(0x2050), None);
    }

    #[test]
    fn va_to_file_offset_stops_at_file_backed_section_tail() {
        let binary = binary_with_sections(vec![SectionInfo {
            name: ".data".to_string(),
            virtual_address: 0x1000,
            virtual_size: 0x100,
            file_offset: 0x40,
            file_size: 0x20,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        }]);

        assert_eq!(binary.va_to_file_offset(0x101f), Some(0x5f));
        assert_eq!(binary.va_to_file_offset(0x1020), None);
    }

    fn binary_with_integer_bytes(arch_spec: &str, is_64bit: bool, bytes: Vec<u8>) -> LoadedBinary {
        LoadedBinaryBuilder::new("integer-test".to_string(), DataBuffer::Heap(bytes.clone()))
            .format("ELF")
            .arch_spec(arch_spec)
            .is_64bit(is_64bit)
            .add_section(SectionInfo {
                name: ".data".to_string(),
                virtual_address: 0x1000,
                virtual_size: bytes.len() as u64,
                file_offset: 0,
                file_size: bytes.len() as u64,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            })
            .build()
            .expect("synthetic integer binary should build")
    }

    #[test]
    fn read_integer_helpers_follow_target_endianness() {
        let little =
            binary_with_integer_bytes("MIPS:LE:32:default", false, vec![0x78, 0x56, 0x34, 0x12]);
        assert_eq!(little.endian(), Endian::Little);
        assert!(little.is_little_endian());
        assert_eq!(little.read_u16(0x1000).unwrap(), 0x5678);
        assert_eq!(little.read_u32(0x1000).unwrap(), 0x1234_5678);
        assert_eq!(little.read_ptr(0x1000).unwrap(), 0x1234_5678);

        let big =
            binary_with_integer_bytes("MIPS:BE:32:default", false, vec![0x12, 0x34, 0x56, 0x78]);
        assert_eq!(big.endian(), Endian::Big);
        assert!(!big.is_little_endian());
        assert_eq!(big.read_u16(0x1000).unwrap(), 0x1234);
        assert_eq!(big.read_u32(0x1000).unwrap(), 0x1234_5678);
        assert_eq!(big.read_ptr(0x1000).unwrap(), 0x1234_5678);
    }
}
