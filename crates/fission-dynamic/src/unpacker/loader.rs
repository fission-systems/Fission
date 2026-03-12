use crate::prelude::*;
use fission_loader::loader::pe::PeLoader;
use fission_loader::loader::types::{DataBuffer, LoadedBinary};
use std::sync::Arc;

/// Fallback PE headers size (1 KB) — used when `SizeOfHeaders` cannot be read
const PE_MIN_HEADER_SIZE: usize = 0x400;

/// OS Loader Simulator - Maps binaries as they would appear in process memory.
///
/// TitanLoader wraps `PeLoader` and adds OS-level memory mapping simulation:
/// - Converts file-based layout to virtual memory layout
/// - Aligns sections according to section alignment
/// - Zero-fills uninitialized memory (BSS sections)
/// - Simulates how Windows LoadLibrary() would map a PE file
///
/// **Use Cases:**
/// - Dynamic analysis: See code as it executes in memory
/// - Unpacking: Analyze self-modifying code after memory mapping
/// - Debugging: Compare memory dumps with file on disk
///
/// **Differences from Static Loading (PeLoader):**
/// - PeLoader: Returns raw file bytes with RVA-based addresses
/// - TitanLoader: Returns memory-mapped layout with proper alignment
pub struct TitanLoader;

impl TitanLoader {
    pub fn new() -> Self {
        Self
    }

    /// Loads a PE file simulating OS loader behavior.
    ///
    /// Process:
    /// 1. Parse PE using PeLoader (static analysis)
    /// 2. Calculate SizeOfImage from section layout
    /// 3. Allocate virtual memory buffer (zero-filled)
    /// 4. Copy PE headers to base address
    /// 5. Map each section to its VirtualAddress
    /// 6. Return LoadedBinary with mapped data
    ///
    /// # Arguments
    /// - `data`: Raw PE file bytes (file on disk)
    /// - `path`: File path for logging/debugging
    ///
    /// # Returns
    /// `LoadedBinary` with `.data` containing memory-mapped layout
    pub fn load(&self, data: &[u8], path: &str) -> Result<LoadedBinary> {
        crate::core::logging::info(&format!("[TitanLoader] Simulating OS loader for {}", path));

        // Validate PE magic
        if !data.starts_with(b"MZ") {
            return Err(FissionError::loader("Not a PE file (missing MZ signature)"));
        }

        // Step 1: Parse PE file using PeLoader (static analysis)
        let mut loaded_bin = PeLoader::parse(DataBuffer::Heap(data.to_vec()), path.to_string())?;

        // Step 2: Calculate memory size needed (SizeOfImage)
        let image_base = loaded_bin.image_base;
        let size_of_image = Self::calculate_size_of_image(&loaded_bin, image_base);

        crate::core::logging::debug(&format!(
            "[TitanLoader] Mapping {} bytes (0x{:x} sections)",
            size_of_image,
            loaded_bin.sections.len()
        ));

        // Step 3: Allocate zero-filled memory (simulates VirtualAlloc)
        let mut mapped_data = vec![0u8; size_of_image];

        // Step 4: Copy PE headers (DOS header, NT headers, section table)
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

        // Step 5: Map sections from file to memory
        for section in &loaded_bin.sections {
            Self::map_section(section, data, &mut mapped_data, image_base)?;
        }

        // Step 6: Replace file data with mapped memory
        loaded_bin.inner_mut().data = Arc::new(DataBuffer::Heap(mapped_data));
        loaded_bin.rebuild_function_indices();

        crate::core::logging::info(&format!(
            "[TitanLoader] Successfully mapped {} at 0x{:x}",
            path, image_base
        ));

        Ok(loaded_bin)
    }

    /// Calculates SizeOfImage (total virtual memory needed)
    fn calculate_size_of_image(binary: &LoadedBinary, image_base: u64) -> usize {
        let mut max_va = image_base;

        for section in &binary.sections {
            // Section end = VA + max(VirtualSize, RawSize)
            let section_end = section.virtual_address + section.virtual_size.max(section.file_size);

            if section_end > max_va {
                max_va = section_end;
            }
        }

        // Align to page boundary (4KB)
        let size = (max_va - image_base) as usize;
        (size + 0xFFF) & !0xFFF
    }

    /// Gets size of PE headers (DOS + NT + Section Table)
    fn get_size_of_headers(data: &[u8], binary: &LoadedBinary) -> usize {
        // Try to get actual SizeOfHeaders from PE
        // If that fails, use first section's file offset as approximation

        if data.len() < 0x80 {
            return 0; // Too small for valid PE
        }

        // Read e_lfanew (offset to NT headers) at 0x3C
        let e_lfanew =
            u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;

        // Validate NT header offset
        if e_lfanew < 0x40 || e_lfanew + 0x100 > data.len() {
            // Fall back to first section offset
            return binary
                .sections
                .iter()
                .map(|s| s.file_offset as usize)
                .min()
                .unwrap_or(PE_MIN_HEADER_SIZE)
                .min(data.len());
        }

        // Read SizeOfHeaders from Optional Header
        // NT headers: Signature(4) + FileHeader(20) + OptionalHeader
        // SizeOfHeaders is at offset 60 in OptionalHeader (both PE32 and PE32+)
        let optional_header_offset = e_lfanew + 4 + 20;

        if optional_header_offset + 64 > data.len() {
            // Fall back
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

        // Validate
        if size_of_headers > 0 && size_of_headers <= data.len() {
            size_of_headers
        } else {
            // Fall back to first section offset
            binary
                .sections
                .iter()
                .map(|s| s.file_offset as usize)
                .min()
                .unwrap_or(PE_MIN_HEADER_SIZE)
                .min(data.len())
        }
    }

    /// Maps a single section from file to virtual memory
    fn map_section(
        section: &fission_loader::loader::types::SectionInfo,
        file_data: &[u8],
        mapped_data: &mut [u8],
        image_base: u64,
    ) -> Result<()> {
        let va_offset = (section.virtual_address - image_base) as usize;
        let file_offset = section.file_offset as usize;
        let raw_size = section.file_size as usize;

        // Validate bounds
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

        // Copy section data
        if raw_size > 0 {
            mapped_data[va_offset..va_offset + raw_size]
                .copy_from_slice(&file_data[file_offset..file_offset + raw_size]);
        }

        // Note: VirtualSize > RawSize means the rest is zero-filled (BSS)
        // Already handled by vec![0u8; size] initialization

        Ok(())
    }
}

impl Default for TitanLoader {
    fn default() -> Self {
        Self::new()
    }
}
