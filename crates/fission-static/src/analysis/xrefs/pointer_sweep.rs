use crate::analysis::xrefs::{OPERAND_INDEX_MNEMONIC, Xref, XrefType};
use fission_loader::loader::LoadedBinary;
use std::collections::BTreeMap;

/// Sweeps non-executable data sections to find hardcoded pointers (e.g. vtables, callback arrays).
pub struct PointerSweeper {
    /// Ordered map from start address to end address for fast lookup.
    valid_regions: BTreeMap<u64, u64>,
    pointer_size: usize,
    is_little_endian: bool,
}

impl PointerSweeper {
    pub fn new(binary: &LoadedBinary) -> Self {
        let mut valid_regions = BTreeMap::new();
        for section in &binary.inner().sections {
            let start = section.virtual_address;
            let end = start.saturating_add(section.virtual_size);
            if start < end {
                valid_regions.insert(start, end);
            }
        }

        // Detect pointer size based on architecture.
        // Default to 8 bytes for 64-bit, 4 bytes for 32-bit.
        let pointer_size = if binary.is_64bit { 8 } else { 4 };

        // We assume little endian by default. If we need big endian support, we can check the format/arch.
        // Fission load_spec usually handles it, but for raw pointer sweeping we'll assume little-endian
        // unless it's explicitly a big-endian architecture. Fission doesn't explicitly expose endianness
        // cleanly on LoadedBinary yet, so we assume LE which covers x86/x64 and most ARM.
        let is_little_endian = true;

        Self {
            valid_regions,
            pointer_size,
            is_little_endian,
        }
    }

    /// Checks if a given value is a valid virtual address within the binary's mapped sections.
    pub fn is_valid_pointer(&self, val: u64) -> bool {
        if val == 0 {
            return false;
        }
        // Find the section that might contain this address.
        // range(..=val).next_back() gives the section with the largest start address <= val.
        if let Some((&start, &end)) = self.valid_regions.range(..=val).next_back() {
            if val >= start && val < end {
                return true;
            }
        }
        false
    }

    /// Sweeps all non-executable data sections and returns newly discovered Xrefs.
    pub fn sweep(&self, binary: &LoadedBinary) -> Vec<Xref> {
        let mut new_xrefs = Vec::new();

        for section in &binary.inner().sections {
            // Only sweep non-executable sections that are readable and have actual file data.
            if section.is_executable || !section.is_readable || section.file_size == 0 {
                continue;
            }

            let start_offset = section.file_offset as usize;
            let end_offset = start_offset.saturating_add(section.file_size as usize);
            let Some(code) = binary.data.as_slice().get(start_offset..end_offset) else {
                continue;
            };

            let base_addr = section.virtual_address;

            // We iterate with stride = pointer_size for aligned pointers,
            // but to be safe against unaligned packed structs we could stride by 4.
            // Ghidra usually aligns. Let's do stride = pointer_size to reduce false positives.
            // If we want to be exhaustive, stride = 1 or 4. Let's use pointer_size alignment.
            // The section.virtual_address might not be properly aligned, but usually it is.
            let alignment = self.pointer_size;
            let mut i = 0;

            while i + self.pointer_size <= code.len() {
                let chunk = &code[i..i + self.pointer_size];
                let val = if self.pointer_size == 8 {
                    if self.is_little_endian {
                        u64::from_le_bytes(chunk.try_into().unwrap())
                    } else {
                        u64::from_be_bytes(chunk.try_into().unwrap())
                    }
                } else {
                    if self.is_little_endian {
                        u32::from_le_bytes(chunk.try_into().unwrap()) as u64
                    } else {
                        u32::from_be_bytes(chunk.try_into().unwrap()) as u64
                    }
                };

                if self.is_valid_pointer(val) {
                    new_xrefs.push(Xref {
                        from_addr: base_addr + i as u64,
                        to_addr: val,
                        xref_type: XrefType::Data,
                        operand_index: OPERAND_INDEX_MNEMONIC,
                        sleigh_kind: None,
                        flow_kind: None,
                    });
                }

                i += alignment;
            }
        }

        new_xrefs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryInner, SectionInfo};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_pointer_sweeper() {
        let mut sections = Vec::new();

        // Dummy text section (0x1000..0x2000)
        sections.push(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x1000,
            virtual_size: 0x1000,
            file_offset: 0x1000,
            file_size: 0x1000,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        });

        // Dummy data section (0x2000..0x3000)
        sections.push(SectionInfo {
            name: ".data".to_string(),
            virtual_address: 0x2000,
            virtual_size: 0x1000,
            file_offset: 0x2000,
            file_size: 0x1000,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        });

        // Construct 4096 bytes of dummy memory for the file.
        // We only care about file offset 0x2000 (which is the .data section).
        // Let's create a file of size 0x3000
        let mut file_data = vec![0u8; 0x3000];

        // Plant some valid 64-bit pointers in .data at offset 0x2000
        // Pointer 1: 0x1008 (points to .text) - Little Endian 64-bit
        file_data[0x2000..0x2008].copy_from_slice(&0x1008u64.to_le_bytes());

        // Pointer 2: 0x2050 (points to .data)
        file_data[0x2008..0x2010].copy_from_slice(&0x2050u64.to_le_bytes());

        // Pointer 3: 0x5000 (invalid pointer, outside sections)
        file_data[0x2010..0x2018].copy_from_slice(&0x5000u64.to_le_bytes());

        let inner = LoadedBinaryInner {
            path: "".to_string(),
            hash: "".to_string(),
            data: Arc::new(DataBuffer::Heap(file_data)),
            arch_spec: "x86:LE:64:default".to_string(),
            load_spec: None,
            architecture: None,
            entry_point: 0x1000,
            image_base: 0x0,
            functions: Vec::new(),
            sections,
            is_64bit: true,
            format: "ELF".to_string(),
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            global_symbol_sizes: HashMap::new(),
            relocation_symbols: HashMap::new(),
            function_addr_index: HashMap::new(),
            function_name_index: HashMap::new(),
            functions_sorted: true,
            inferred_types: Vec::new(),
            string_map: HashMap::new(),
            pdb_debug_info: None,
            relocations: Vec::new(),
            rich_header_records: None,
            symbol_versions: HashMap::new(),
            cfg_label_leaders: Vec::new(),
        };

        let binary = LoadedBinary::from_inner(inner);

        let sweeper = PointerSweeper::new(&binary);

        let xrefs = sweeper.sweep(&binary);

        // We expect exactly 2 xrefs (0x1008 and 0x2050)
        assert_eq!(xrefs.len(), 2);

        // Check first xref
        assert_eq!(xrefs[0].from_addr, 0x2000);
        assert_eq!(xrefs[0].to_addr, 0x1008);
        assert_eq!(xrefs[0].xref_type, XrefType::Data);

        // Check second xref
        assert_eq!(xrefs[1].from_addr, 0x2008);
        assert_eq!(xrefs[1].to_addr, 0x2050);
        assert_eq!(xrefs[1].xref_type, XrefType::Data);
    }
}
