//! DWARF Section Data Management
//!
//! Provides a helper structure for loading DWARF debug sections from a
//! LoadedBinary and mapping them to canonical section names for gimli.

use crate::loader::LoadedBinary;
use std::collections::HashMap;

/// Wrapper that provides section data to gimli from LoadedBinary's sections.
pub(super) struct SectionData {
    sections: HashMap<&'static str, (usize, usize)>, // (file_offset, file_size)
}

impl SectionData {
    pub(super) fn new(binary: &LoadedBinary) -> Self {
        let mut sections = HashMap::new();
        let data_len = binary.data.as_slice().len();

        for section in &binary.sections {
            let name = section.name.as_str();
            // Map both ELF (.debug_*) and Mach-O (__debug_*) names to canonical keys
            let canonical = match name {
                ".debug_info" | "__debug_info" => Some(".debug_info"),
                ".debug_abbrev" | "__debug_abbrev" => Some(".debug_abbrev"),
                ".debug_str" | "__debug_str" => Some(".debug_str"),
                ".debug_line" | "__debug_line" => Some(".debug_line"),
                ".debug_ranges" | "__debug_ranges" => Some(".debug_ranges"),
                ".debug_rnglists" | "__debug_rnglists" => Some(".debug_rnglists"),
                ".debug_str_offsets" | "__debug_str_offs" => Some(".debug_str_offsets"),
                ".debug_addr" | "__debug_addr" => Some(".debug_addr"),
                ".debug_line_str" | "__debug_line_str" => Some(".debug_line_str"),
                ".debug_types" | "__debug_types" => Some(".debug_types"),
                ".debug_loc" | "__debug_loc" => Some(".debug_loc"),
                ".debug_loclists" | "__debug_loclists" => Some(".debug_loclists"),
                _ => None,
            };

            if let Some(key) = canonical {
                let file_offset = section.file_offset as usize;
                // PE rounds a section's on-disk size up to the file
                // alignment, so `.debug_info` carries a few hundred zero bytes
                // past its last unit. gimli's unit iterator walks into them,
                // reads a zero unit length, and reports `Hit the end of input
                // before it was expected` -- every PE with debug info failed
                // to parse this way. ELF sizes match, which is why only PE saw
                // it.
                let file_size = match section.virtual_size {
                    0 => section.file_size as usize,
                    virtual_size => (virtual_size.min(section.file_size)) as usize,
                };
                if file_offset + file_size <= data_len {
                    sections.insert(key, (file_offset, file_size));
                }
            }
        }

        Self { sections }
    }

    pub(super) fn get<'a>(&self, name: &str, data: &'a [u8]) -> &'a [u8] {
        if let Some(&(offset, size)) = self.sections.get(name) {
            &data[offset..offset + size]
        } else {
            &[]
        }
    }
}
