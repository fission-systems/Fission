//! Dump a running process to a reconstructed PE file on disk.
//!
//! Provides `dump_process` (memory dump) and `rebuild_imports`
//! (append a new import section to a dumped file).

use std::fs::File;
use std::io::Write;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::*;
use windows::Win32::System::SystemServices::*;

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

    let mut max_pointer = 0;
    for section in &sections {
        let end = section.PointerToRawData + section.SizeOfRawData;
        if end > max_pointer {
            max_pointer = end;
        }
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

        let read_size = if section.VirtualSize > 0 {
            section.VirtualSize as usize
        } else {
            raw_size
        };

        if let Ok(section_data) = pe_raw::read_memory(process_handle, virtual_addr, read_size) {
            if raw_ptr + section_data.len() <= file_buffer.len() {
                file_buffer[raw_ptr..raw_ptr + section_data.len()].copy_from_slice(&section_data);
            } else if raw_ptr < file_buffer.len() {
                let len = file_buffer.len() - raw_ptr;
                file_buffer[raw_ptr..].copy_from_slice(&section_data[0..len]);
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
pub fn rebuild_imports(
    file_path: &str,
    imports: &[ImportEntry],
    _original_base: u64,
) -> Result<(), String> {
    let mut file_data = std::fs::read(file_path).map_err(|e| e.to_string())?;

    let dos_header = unsafe { &*(file_data.as_ptr() as *const IMAGE_DOS_HEADER) };
    if dos_header.e_magic != IMAGE_DOS_SIGNATURE {
        return Err("Invalid DOS Signature".to_string());
    }

    let nt_offset = dos_header.e_lfanew as usize;
    let nt_headers =
        unsafe { &mut *(file_data.as_mut_ptr().add(nt_offset) as *mut IMAGE_NT_HEADERS64) };

    if nt_headers.Signature != IMAGE_NT_SIGNATURE {
        return Err("Invalid NT Signature".to_string());
    }

    let mut modules: std::collections::HashMap<String, Vec<&ImportEntry>> =
        std::collections::HashMap::new();
    for imp in imports {
        modules
            .entry(imp.module_name.clone())
            .or_default()
            .push(imp);
    }

    let mut sorted_modules: Vec<_> = modules.keys().cloned().collect();
    sorted_modules.sort();

    let descriptor_size = std::mem::size_of::<IMAGE_IMPORT_DESCRIPTOR>();
    let thunk_size = 8;

    let mut total_size = (sorted_modules.len() + 1) * descriptor_size;
    let mut current_offset = total_size;

    struct ModuleLayout {
        name_offset: usize,
        iat_offset: usize,
        ft_offset: usize,
    }

    let mut layout = std::collections::HashMap::new();

    for mod_name in &sorted_modules {
        let imps = match modules.get(mod_name) {
            Some(i) => i,
            None => continue,
        };

        let name_len = mod_name.len() + 1;
        let name_offset = current_offset;
        current_offset += name_len;

        if current_offset % 2 != 0 {
            current_offset += 1;
        }

        let ilt_offset = current_offset;
        let array_size = (imps.len() + 1) * thunk_size;
        current_offset += array_size;

        let ft_offset = current_offset;
        current_offset += array_size;

        layout.insert(
            mod_name.clone(),
            ModuleLayout {
                name_offset,
                iat_offset: ilt_offset,
                ft_offset,
            },
        );

        for imp in *imps {
            if let Some(func_name) = &imp.function_name {
                let entry_len = 2 + func_name.len() + 1;
                current_offset += entry_len;
                if current_offset % 2 != 0 {
                    current_offset += 1;
                }
            }
        }
    }

    let mut new_section_data = vec![0u8; current_offset];

    let mut descriptor_offset = 0;

    for mod_name in &sorted_modules {
        let imps = match modules.get(mod_name) {
            Some(i) => i,
            None => continue,
        };
        let mod_layout = match layout.get(mod_name) {
            Some(l) => l,
            None => continue,
        };

        let name_bytes = mod_name.as_bytes();
        new_section_data[mod_layout.name_offset..mod_layout.name_offset + name_bytes.len()]
            .copy_from_slice(name_bytes);

        let mut current_thunk_offset = 0;
        let mut hint_name_offset_counter = mod_layout.ft_offset + ((imps.len() + 1) * thunk_size);

        for imp in *imps {
            let thunk_value: u64;

            if let Some(func_name) = &imp.function_name {
                let entry_offset = hint_name_offset_counter;
                new_section_data[entry_offset] = 0;
                new_section_data[entry_offset + 1] = 0;
                let fname_bytes = func_name.as_bytes();
                new_section_data[entry_offset + 2..entry_offset + 2 + fname_bytes.len()]
                    .copy_from_slice(fname_bytes);

                hint_name_offset_counter += 2 + fname_bytes.len() + 1;
                if hint_name_offset_counter % 2 != 0 {
                    hint_name_offset_counter += 1;
                }

                thunk_value = entry_offset as u64;
            } else {
                thunk_value = (1u64 << 63) | (imp.ordinal as u64);
            }

            let ilt_pos = mod_layout.iat_offset + current_thunk_offset;
            new_section_data[ilt_pos..ilt_pos + 8].copy_from_slice(&thunk_value.to_le_bytes());

            let ft_pos = mod_layout.ft_offset + current_thunk_offset;
            new_section_data[ft_pos..ft_pos + 8].copy_from_slice(&thunk_value.to_le_bytes());

            current_thunk_offset += 8;
        }

        descriptor_offset += descriptor_size;
    }

    let file_align = nt_headers.OptionalHeader.FileAlignment;
    let sect_align = nt_headers.OptionalHeader.SectionAlignment;

    let raw_size = (new_section_data.len() as u32 + file_align - 1) & !(file_align - 1);
    new_section_data.resize(raw_size as usize, 0);

    let num_sections = nt_headers.FileHeader.NumberOfSections;
    let section_header_size = std::mem::size_of::<IMAGE_SECTION_HEADER>();
    let section_table_offset = nt_offset + std::mem::size_of::<IMAGE_NT_HEADERS64>();

    let last_section_offset =
        section_table_offset + (num_sections as usize - 1) * section_header_size;
    let last_section =
        unsafe { &*(file_data.as_ptr().add(last_section_offset) as *const IMAGE_SECTION_HEADER) };

    let last_section_end_rva = last_section.VirtualAddress + last_section.VirtualSize;
    let new_section_rva = (last_section_end_rva + sect_align - 1) & !(sect_align - 1);

    let new_section_raw_ptr = file_data.len() as u32;

    descriptor_offset = 0;
    for mod_name in &sorted_modules {
        let mod_layout = match layout.get(mod_name) {
            Some(l) => l,
            None => continue,
        };
        let imps = match modules.get(mod_name) {
            Some(i) => i,
            None => continue,
        };

        let mut current_thunk_offset = 0;
        for imp in *imps {
            if imp.function_name.is_some() {
                let ilt_pos = mod_layout.iat_offset + current_thunk_offset;
                let offset_bytes = &new_section_data[ilt_pos..ilt_pos + 8];
                let offset_val = u64::from_le_bytes(match offset_bytes.try_into() {
                    Ok(b) => b,
                    Err(_) => {
                        current_thunk_offset += thunk_size;
                        continue;
                    }
                });

                if (offset_val & (1u64 << 63)) == 0 {
                    let rva = new_section_rva as u64 + offset_val;
                    new_section_data[ilt_pos..ilt_pos + 8].copy_from_slice(&rva.to_le_bytes());

                    let ft_pos = mod_layout.ft_offset + current_thunk_offset;
                    new_section_data[ft_pos..ft_pos + 8].copy_from_slice(&rva.to_le_bytes());
                }
            }
            current_thunk_offset += 8;
        }

        let mut descriptor = IMAGE_IMPORT_DESCRIPTOR::default();
        descriptor.OriginalFirstThunk = (new_section_rva as usize + mod_layout.iat_offset) as u32;
        descriptor.FirstThunk = (new_section_rva as usize + mod_layout.ft_offset) as u32;
        descriptor.Name = (new_section_rva as usize + mod_layout.name_offset) as u32;

        let desc_ptr =
            new_section_data.as_mut_ptr().add(descriptor_offset) as *mut IMAGE_IMPORT_DESCRIPTOR;
        unsafe {
            *desc_ptr = descriptor;
        }

        descriptor_offset += descriptor_size;
    }

    let new_header_offset = section_table_offset + (num_sections as usize) * section_header_size;

    if new_header_offset + section_header_size > nt_headers.OptionalHeader.SizeOfHeaders as usize {
        return Err("Not enough space for new section header".to_string());
    }

    let mut new_section_header = IMAGE_SECTION_HEADER::default();
    new_section_header.Name = *b".fission";
    new_section_header.VirtualSize = current_offset as u32;
    new_section_header.VirtualAddress = new_section_rva;
    new_section_header.SizeOfRawData = raw_size;
    new_section_header.PointerToRawData = new_section_raw_ptr;
    new_section_header.Characteristics = 0xC0000040;

    unsafe {
        let header_ptr = file_data.as_mut_ptr().add(new_header_offset) as *mut IMAGE_SECTION_HEADER;
        *header_ptr = new_section_header;
    }

    nt_headers.FileHeader.NumberOfSections += 1;
    nt_headers.OptionalHeader.SizeOfImage =
        new_section_rva + ((current_offset as u32 + sect_align - 1) & !(sect_align - 1));

    nt_headers.OptionalHeader.DataDirectory[1].VirtualAddress = new_section_rva;
    nt_headers.OptionalHeader.DataDirectory[1].Size = total_size as u32;

    nt_headers.OptionalHeader.DataDirectory[12].VirtualAddress = 0;
    nt_headers.OptionalHeader.DataDirectory[12].Size = 0;

    file_data.extend_from_slice(&new_section_data);
    std::fs::write(file_path, file_data).map_err(|e| e.to_string())?;

    Ok(())
}
