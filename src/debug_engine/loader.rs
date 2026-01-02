use super::types::*;
use crate::analysis::loader::types::{FunctionInfo, LoadedBinary, SectionInfo};
use crate::core::prelude::*;

/// Rust implementation of TitanEngine's file loading capabilities.
/// Focuses on mapping files as if they were loaded by the OS loader.
pub struct TitanLoader;

impl TitanLoader {
    pub fn new() -> Self {
        Self
    }

    /// Simulates the OS loader to map the binary into memory.
    /// This is crucial for "Dynamic Parsing" where we want to see the file
    /// exactly as it would appear in memory (sections aligned, imports resolved, etc.)
    pub fn load(&self, data: &[u8], path: &str) -> Result<LoadedBinary> {
        crate::core::logging::info(&format!(
            "[TitanLoader] Loading {} via TitanEngine-rs",
            path
        ));

        // Use our binrw PeLoader
        // Note: TitanLoader currently only supports PE.
        // We can check magic or just try PeLoader.
        if !data.starts_with(b"MZ") {
            return Err(FissionError::loader(
                "Only PE files are supported by TitanLoader currently",
            ));
        }

        let mut loaded_bin =
            crate::analysis::loader::pe::PeLoader::parse(data.to_vec(), path.to_string())?;

        // The PeLoader returns a LoadedBinary with `data` being the FILE content.
        // TitanLoader needs to convert this to MAPPED content (Virtual Memory layout).

        let image_base = loaded_bin.image_base;

        // Calculate SizeOfImage based on last section
        // (Rough calculation as binrw loader doesn't expose header's exact SizeOfImage yet,
        //  but we can iterate sections).
        // Actually, for proper emulation, we should respect the PE header's SizeOfImage.
        // But for now, let's span to the end of the last section.
        let mut max_va = 0;
        for section in &loaded_bin.sections {
            let end = section.virtual_address + section.virtual_size.max(section.file_size);
            if end > max_va {
                max_va = end;
            }
        }
        let size_of_image = (max_va - image_base) as usize;
        let size_of_image = (size_of_image + 0xFFF) & !0xFFF; // Align up to 4K

        // 1. Allocate memory
        let mut mapped_data = vec![0u8; size_of_image];

        // 2. Copy Headers (File Header is loaded at base)
        // We assume headers verify up to first section or 0x1000
        let size_of_headers = 0x400; // Minimal assumption or read from LoadedBinary if we stored it
        if size_of_headers <= data.len() {
            mapped_data[0..size_of_headers].copy_from_slice(&data[0..size_of_headers]);
        }

        // 3. Map Sections
        for section in &loaded_bin.sections {
            let va = (section.virtual_address - image_base) as usize;
            let size = section.file_size as usize;
            let offset = section.file_offset as usize;

            if offset + size <= data.len() && va + size <= mapped_data.len() {
                mapped_data[va..va + size].copy_from_slice(&data[offset..offset + size]);
            }
        }

        // Replace data with mapped data
        loaded_bin.data = mapped_data;

        // Rebuild indices
        loaded_bin.rebuild_function_indices();

        Ok(loaded_bin)
    }
}
