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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointerSweepCoverage {
    pub candidate_sections: usize,
    pub completed_sections: usize,
    pub omitted_sections: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PointerSweepResult {
    pub xrefs: Vec<Xref>,
    pub coverage: PointerSweepCoverage,
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

        let is_little_endian = binary.is_little_endian();

        Self {
            valid_regions,
            pointer_size,
            is_little_endian,
        }
    }

    #[must_use]
    pub fn candidate_section_count(binary: &LoadedBinary) -> usize {
        binary
            .inner()
            .sections
            .iter()
            .filter(|section| is_pointer_sweep_candidate(section))
            .count()
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
        self.sweep_with_coverage(binary).xrefs
    }

    /// Sweeps the documented pointer-sized slots and reports file-backed coverage.
    pub fn sweep_with_coverage(&self, binary: &LoadedBinary) -> PointerSweepResult {
        let mut result = PointerSweepResult::default();

        for section in &binary.inner().sections {
            // Only sweep non-executable sections that are readable and have actual file data.
            if !is_pointer_sweep_candidate(section) {
                continue;
            }
            result.coverage.candidate_sections += 1;

            let start_offset = section.file_offset as usize;
            let end_offset = start_offset.saturating_add(section.file_size as usize);
            let Some(code) = binary.data.as_slice().get(start_offset..end_offset) else {
                result.coverage.omitted_sections += 1;
                continue;
            };
            result.coverage.completed_sections += 1;

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
                    result.xrefs.push(Xref {
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

        result
    }
}

fn is_pointer_sweep_candidate(section: &fission_loader::loader::SectionInfo) -> bool {
    !section.is_executable && section.is_readable && section.file_size > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, LoadedBinaryInner, SectionInfo};
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
            loader_symbols: Vec::new(),
            user_signatures: Default::default(),
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

    #[test]
    fn test_pointer_sweeper_respects_big_endian_target() {
        let sections = vec![
            SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x10,
                file_offset: 0,
                file_size: 0x10,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            },
            SectionInfo {
                name: ".data".to_string(),
                virtual_address: 0x2000,
                virtual_size: 0x1000,
                file_offset: 0x10,
                file_size: 0x18,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            },
        ];
        let mut file_data = vec![0u8; 0x28];
        file_data[0x10..0x18].copy_from_slice(&0x1008u64.to_be_bytes());
        file_data[0x18..0x20].copy_from_slice(&0x2050u64.to_be_bytes());
        file_data[0x20..0x28].copy_from_slice(&0x5000u64.to_be_bytes());

        let binary = LoadedBinaryBuilder::new(
            "big-endian-pointer-test".to_string(),
            DataBuffer::Heap(file_data),
        )
        .format("ELF")
        .arch_spec("MIPS:BE:64:default")
        .is_64bit(true)
        .add_sections(sections)
        .build()
        .expect("synthetic big-endian binary should build");

        let sweeper = PointerSweeper::new(&binary);
        assert!(!sweeper.is_little_endian);

        let xrefs = sweeper.sweep(&binary);
        assert_eq!(
            xrefs
                .iter()
                .map(|xref| (xref.from_addr, xref.to_addr))
                .collect::<Vec<_>>(),
            vec![(0x2000, 0x1008), (0x2008, 0x2050)]
        );
    }
}
