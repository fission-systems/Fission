//! OS Loader Simulator — Maps PE binaries as they would appear in process memory.
//!
//! Wraps `PeLoader` and adds memory-mapping simulation:
//! - Converts file-based layout to virtual memory layout
//! - Aligns sections according to section alignment
//! - Zero-fills uninitialized memory (BSS sections)
//! - Simulates how Windows `LoadLibrary()` would map a PE file

use crate::prelude::*;
use fission_loader::loader::pe::PeLoader;
use fission_loader::loader::types::{DataBuffer, LoadedBinary};
use std::sync::Arc;

/// Fallback PE headers size (1 KB) — used when `SizeOfHeaders` cannot be read
const PE_MIN_HEADER_SIZE: usize = 0x400;

pub struct TitanLoader;

impl TitanLoader {
    pub fn new() -> Self {
        Self
    }

    /// Loads a PE file simulating OS loader behavior.
    pub fn load(&self, data: &[u8], path: &str) -> Result<LoadedBinary> {
        crate::core::logging::info(&format!("[TitanLoader] Simulating OS loader for {}", path));

        if !data.starts_with(b"MZ") {
            return Err(FissionError::loader("Not a PE file (missing MZ signature)"));
        }

        let mut loaded_bin = PeLoader::parse(DataBuffer::Heap(data.to_vec()), path.to_string())?;
        let image_base = loaded_bin.image_base;
        let size_of_image = Self::calculate_size_of_image(&loaded_bin, image_base);

        crate::core::logging::debug(&format!(
            "[TitanLoader] Mapping {} bytes (0x{:x} sections)",
            size_of_image,
            loaded_bin.sections.len()
        ));

        let mut mapped_data = vec![0u8; size_of_image];

        let size_of_headers = Self::get_size_of_headers(data, &loaded_bin);
        if size_of_headers > 0
            && size_of_headers <= data.len()
            && size_of_headers <= mapped_data.len()
        {
            mapped_data[0..size_of_headers].copy_from_slice(&data[0..size_of_headers]);
            crate::core::logging::debug(&format!(
                "[TitanLoader] Copied {} bytes of PE headers",
                size_of_headers
            ));
        } else {
            return Err(FissionError::loader(format!(
                "Invalid header size: {} (file: {}, mapped: {})",
                size_of_headers,
                data.len(),
                mapped_data.len()
            )));
        }

        for section in &loaded_bin.sections {
            Self::map_section(section, data, &mut mapped_data, image_base)?;
        }

        loaded_bin.inner_mut().data = Arc::new(DataBuffer::Heap(mapped_data));
        loaded_bin.rebuild_function_indices();

        crate::core::logging::info(&format!(
            "[TitanLoader] Successfully mapped {} at 0x{:x}",
            path, image_base
        ));

        Ok(loaded_bin)
    }

    fn calculate_size_of_image(binary: &LoadedBinary, image_base: u64) -> usize {
        let mut max_va = image_base;
        for section in &binary.sections {
            let section_end = section.virtual_address + section.virtual_size.max(section.file_size);
            if section_end > max_va {
                max_va = section_end;
            }
        }
        let size = (max_va - image_base) as usize;
        (size + 0xFFF) & !0xFFF
    }

    fn get_size_of_headers(data: &[u8], binary: &LoadedBinary) -> usize {
        if data.len() < 0x80 {
            return 0;
        }
        let e_lfanew = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
        if e_lfanew < 0x40 || e_lfanew + 0x100 > data.len() {
            return binary
                .sections
                .iter()
                .map(|s| s.file_offset as usize)
                .min()
                .unwrap_or(PE_MIN_HEADER_SIZE)
                .min(data.len());
        }

        let optional_header_offset = e_lfanew + 4 + 20;
        if optional_header_offset + 64 > data.len() {
            return binary
                .sections
                .iter()
                .map(|s| s.file_offset as usize)
                .min()
                .unwrap_or(PE_MIN_HEADER_SIZE)
                .min(data.len());
        }

        let size_of_headers = u32::from_le_bytes([
            data[optional_header_offset + 60],
            data[optional_header_offset + 61],
            data[optional_header_offset + 62],
            data[optional_header_offset + 63],
        ]) as usize;

        if size_of_headers > 0 && size_of_headers <= data.len() {
            size_of_headers
        } else {
            binary
                .sections
                .iter()
                .map(|s| s.file_offset as usize)
                .min()
                .unwrap_or(PE_MIN_HEADER_SIZE)
                .min(data.len())
        }
    }

    fn map_section(
        section: &fission_loader::loader::types::SectionInfo,
        file_data: &[u8],
        mapped_data: &mut [u8],
        image_base: u64,
    ) -> Result<()> {
        let va_offset = (section.virtual_address - image_base) as usize;
        let file_offset = section.file_offset as usize;
        let raw_size = section.file_size as usize;

        if file_offset + raw_size > file_data.len() {
            return Err(FissionError::loader(format!(
                "Section {} extends beyond file (offset: 0x{:x}, size: 0x{:x})",
                section.name, file_offset, raw_size
            )));
        }

        if va_offset + raw_size > mapped_data.len() {
            return Err(FissionError::loader(format!(
                "Section {} exceeds allocated memory (VA: 0x{:x}, size: 0x{:x})",
                section.name, section.virtual_address, raw_size
            )));
        }

        if raw_size > 0 {
            mapped_data[va_offset..va_offset + raw_size]
                .copy_from_slice(&file_data[file_offset..file_offset + raw_size]);
        }

        Ok(())
    }
}

impl Default for TitanLoader {
    fn default() -> Self {
        Self::new()
    }
}
