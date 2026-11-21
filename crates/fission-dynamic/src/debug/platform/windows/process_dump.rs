//! Dump a running process to a reconstructed PE file on disk.
//!
//! Provides `dump_process` (memory dump) and `rebuild_imports`
//! (append a new import section to a dumped file).

use std::fs::File;
use std::io::Write;

use windows::Win32::Foundation::HANDLE;

use super::import_recon::ImportEntry;
use super::pe_raw;

/// Dumps the process memory to a file on disk.
/// This attempts to reconstruct a valid PE file from the memory image.
pub fn dump_process(
    process_handle: HANDLE,
    base_address: u64,
    output_path: &str,
) -> Result<(), String> {
    let dos_header = pe_raw::read_dos_header(process_handle, base_address)?;
    let nt_headers = pe_raw::read_nt_headers64(process_handle, base_address, dos_header.e_lfanew)?;
    let sections = pe_raw::read_section_headers(
        process_handle,
        base_address,
        dos_header.e_lfanew,
        nt_headers.FileHeader.NumberOfSections,
    )?;

    let mut max_pointer = 0u64;
    for section in &sections {
        // u32 + u32 overflowed for a header claiming a section near 4 GiB.
        let end = u64::from(section.PointerToRawData) + u64::from(section.SizeOfRawData);
        max_pointer = max_pointer.max(end);
    }
    if max_pointer > 1 << 32 {
        return Err(format!("the sections claim a {max_pointer}-byte file"));
    }

    let mut file_buffer = vec![0u8; max_pointer as usize];

    let headers_size = nt_headers.OptionalHeader.SizeOfHeaders as usize;
    let headers_data = pe_raw::read_memory(process_handle, base_address, headers_size)?;

    if headers_data.len() > file_buffer.len() {
        file_buffer.resize(headers_data.len(), 0);
    }
    file_buffer[0..headers_data.len()].copy_from_slice(&headers_data);

    for section in &sections {
        let virtual_addr = base_address + section.VirtualAddress as u64;
        let raw_ptr = section.PointerToRawData as usize;
        let raw_size = section.SizeOfRawData as usize;

        let virtual_size = unsafe { section.Misc.VirtualSize };
        let read_size = if virtual_size > 0 {
            virtual_size as usize
        } else {
            raw_size
        };

        if let Ok(section_data) = pe_raw::read_memory(process_handle, virtual_addr, read_size) {
            if raw_ptr + section_data.len() <= file_buffer.len() {
                file_buffer[raw_ptr..raw_ptr + section_data.len()].copy_from_slice(&section_data);
            } else if raw_ptr < file_buffer.len() {
                // What fits; a short read can be shorter than the room left.
                let len = (file_buffer.len() - raw_ptr).min(section_data.len());
                file_buffer[raw_ptr..raw_ptr + len].copy_from_slice(&section_data[..len]);
            }
        } else {
            crate::core::logging::warn(&format!("Failed to read section at {:X}", virtual_addr));
        }
    }

    let mut file = File::create(output_path).map_err(|e| e.to_string())?;
    file.write_all(&file_buffer).map_err(|e| e.to_string())?;

    Ok(())
}

/// Rebuilds the Import Table in the dumped file.
/// This appends a new section containing the IAT and updates the PE headers.
///
/// The header work is `windows_layout::append_import_section`: byte offsets
/// with every bound checked. This used to cast the file buffer to header
/// structs at offsets nobody validated, computed `NumberOfSections - 1` with no
/// sections, and assumed a PE32+ layout for every image.
pub fn rebuild_imports(
    file_path: &str,
    imports: &[ImportEntry],
    _original_base: u64,
) -> Result<(), String> {
    let mut file_data = std::fs::read(file_path).map_err(|e| e.to_string())?;
    let specs = imports
        .iter()
        .map(|import| {
            Ok(crate::debug::platform::windows_layout::ImportSpec {
                module: import.module_name.clone(),
                function: import.function_name.clone(),
                ordinal: u16::try_from(import.ordinal).map_err(|_| {
                    format!(
                        "{}: ordinal {} does not fit an import thunk",
                        import.module_name, import.ordinal
                    )
                })?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    crate::debug::platform::windows_layout::append_import_section(&mut file_data, &specs)?;
    std::fs::write(file_path, file_data).map_err(|e| e.to_string())
}
