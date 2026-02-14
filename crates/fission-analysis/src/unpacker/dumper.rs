#[cfg(target_os = "windows")]
use std::fs::File;
#[cfg(target_os = "windows")]
use std::io::Write;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HANDLE;

/// Dumps the process memory to a file on disk.
/// This attempts to reconstruct a valid PE file from the memory image.
#[cfg(target_os = "windows")]
pub fn dump_process(
    process_handle: HANDLE,
    base_address: u64,
    output_path: &str,
) -> Result<(), String> {
    // 1. Read DOS Header
    let dos_header = super::pe::read_dos_header(process_handle, base_address)?;

    // 2. Read NT Headers
    let nt_headers =
        super::pe::read_nt_headers64(process_handle, base_address, dos_header.e_lfanew)?;

    // 3. Read Section Headers
    let sections = super::pe::read_section_headers(
        process_handle,
        base_address,
        dos_header.e_lfanew,
        nt_headers.FileHeader.NumberOfSections,
    )?;

    // 4. Calculate file size
    // A simple dump strategy: Dump headers + Dump each section at its RawAddress (PointerToRawData)
    // However, in memory, sections are aligned to SectionAlignment. On disk, FileAlignment.
    // TitanEngine's DumpProcess typically aligns sections to FileAlignment to make a valid executable.

    // For simplicity, we will create a file large enough to hold the last section's end.
    let mut max_pointer = 0;
    for section in &sections {
        let end = section.PointerToRawData + section.SizeOfRawData;
        if end > max_pointer {
            max_pointer = end;
        }
    }

    // If PointerToRawData is 0 (e.g. in memory), we might need to recalculate it based on VirtualAddress
    // But for a "Memory Dump", we often just dump the whole image as is (Virtual Layout)
    // OR we try to "Unmap" it (convert Virtual Layout back to File Layout).
    // Let's implement "Unmapping" (Virtual -> Raw) which is what DumpProcess usually does.

    let mut file_buffer = vec![0u8; max_pointer as usize];

    // 5. Copy Headers
    // Headers size is SizeOfHeaders.
    let headers_size = nt_headers.OptionalHeader.SizeOfHeaders as usize;
    let headers_data = super::memory::read_memory(process_handle, base_address, headers_size)?;

    if headers_data.len() > file_buffer.len() {
        // Resize if headers are larger than calculated sections (unlikely but possible)
        file_buffer.resize(headers_data.len(), 0);
    }
    file_buffer[0..headers_data.len()].copy_from_slice(&headers_data);

    // 6. Copy Sections
    for section in &sections {
        let virtual_addr = base_address + section.VirtualAddress as u64;
        let raw_ptr = section.PointerToRawData as usize;
        let raw_size = section.SizeOfRawData as usize;

        // Read section data from memory (Virtual Address)
        // Note: In memory, the size is VirtualSize. On disk, we want RawSize.
        // We read min(VirtualSize, RawSize) usually, or just VirtualSize and pad/truncate.
        let read_size = if section.VirtualSize > 0 {
            section.VirtualSize as usize
        } else {
            raw_size
        };

        if let Ok(section_data) =
            super::memory::read_memory(process_handle, virtual_addr, read_size)
        {
            // Write to file buffer at PointerToRawData
            if raw_ptr + section_data.len() <= file_buffer.len() {
                file_buffer[raw_ptr..raw_ptr + section_data.len()].copy_from_slice(&section_data);
            } else if raw_ptr < file_buffer.len() {
                // Partial write if buffer too small (shouldn't happen with correct max_pointer)
                let len = file_buffer.len() - raw_ptr;
                file_buffer[raw_ptr..].copy_from_slice(&section_data[0..len]);
            }
        } else {
            crate::core::logging::warn(&format!("Failed to read section at {:X}", virtual_addr));
        }
    }

    // 7. Write to disk
    let mut file = File::create(output_path).map_err(|e| e.to_string())?;
    file.write_all(&file_buffer).map_err(|e| e.to_string())?;

    Ok(())
}

/// Rebuilds the Import Table in the dumped file.
/// This appends a new section containing the IAT and updates the PE headers.
#[cfg(target_os = "windows")]
pub fn rebuild_imports(
    file_path: &str,
    imports: &[super::importer::ImportEntry],
    original_base: u64,
) -> Result<(), String> {
    // 1. Read the dumped file
    let mut file_data = std::fs::read(file_path).map_err(|e| e.to_string())?;

    // 2. Parse PE Headers from file data
    // We need to modify headers, so we need mutable access or offsets.
    // Using unsafe pointer casting for simplicity in this "Clean Room" implementation.

    let dos_header = unsafe {
        &*(file_data.as_ptr() as *const windows::Win32::System::SystemServices::IMAGE_DOS_HEADER)
    };
    if dos_header.e_magic != windows::Win32::System::SystemServices::IMAGE_DOS_SIGNATURE {
        return Err("Invalid DOS Signature".to_string());
    }

    let nt_offset = dos_header.e_lfanew as usize;
    let nt_headers = unsafe {
        &mut *(file_data.as_mut_ptr().add(nt_offset)
            as *mut windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64)
    };

    if nt_headers.Signature != windows::Win32::System::SystemServices::IMAGE_NT_SIGNATURE {
        return Err("Invalid NT Signature".to_string());
    }

    // 3. Calculate size needed for new Import Table
    // Structure:
    // [Import Directory (Descriptors)] - (NumModules + 1) * 20 bytes
    // [IATs (Thunks)] - (NumImports + NumModules) * 8 bytes
    // [Names / Hints] - Variable length
    // [Module Names] - Variable length

    // Group imports by module
    let mut modules: std::collections::HashMap<String, Vec<&super::importer::ImportEntry>> =
        std::collections::HashMap::new();
    for imp in imports {
        modules
            .entry(imp.module_name.clone())
            .or_default()
            .push(imp);
    }

    // Sort modules for deterministic output
    let mut sorted_modules: Vec<_> = modules.keys().cloned().collect();
    sorted_modules.sort();

    // Calculate sizes
    let descriptor_size =
        std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_IMPORT_DESCRIPTOR>();
    let thunk_size = 8; // x64

    let mut total_size = (sorted_modules.len() + 1) * descriptor_size; // Descriptors + Null terminator

    // Calculate offsets relative to the start of our new section
    let mut current_offset = total_size;

    // We need to store where things will be placed
    struct ModuleLayout {
        name_offset: usize,
        iat_offset: usize, // OriginalFirstThunk (ILT)
        ft_offset: usize, // FirstThunk (IAT) - Optional, usually same as ILT in file, but loader overwrites FT.
                          // In a rebuilt import table, we usually point FT to the same array as OFT,
                          // or have separate arrays. Let's use separate arrays for correctness.
    }

    let mut layout = std::collections::HashMap::new();

    for mod_name in &sorted_modules {
        let imps = match modules.get(mod_name) {
            Some(i) => i,
            None => continue,
        };

        // Module Name string
        let name_len = mod_name.len() + 1;
        let name_offset = current_offset;
        current_offset += name_len;

        // Align to 2 bytes
        if current_offset % 2 != 0 {
            current_offset += 1;
        }

        // ILT (OriginalFirstThunk) array
        let ilt_offset = current_offset;
        let array_size = (imps.len() + 1) * thunk_size; // +1 for null terminator
        current_offset += array_size;

        // IAT (FirstThunk) array
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

        // Hint/Name entries
        for imp in *imps {
            if let Some(func_name) = &imp.function_name {
                // Hint (2 bytes) + Name + Null
                let entry_len = 2 + func_name.len() + 1;
                current_offset += entry_len;
                if current_offset % 2 != 0 {
                    current_offset += 1;
                }
            }
        }
    }

    // 4. Create new section data
    let mut new_section_data = vec![0u8; current_offset];

    // Fill data
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

        // Write Module Name
        let name_bytes = mod_name.as_bytes();
        new_section_data[mod_layout.name_offset..mod_layout.name_offset + name_bytes.len()]
            .copy_from_slice(name_bytes);

        // Write Hint/Name entries and fill ILT/IAT
        let mut current_thunk_offset = 0;
        let mut hint_name_offset_counter = mod_layout.ft_offset + ((imps.len() + 1) * thunk_size); // Start after IAT

        for imp in *imps {
            let thunk_value: u64;

            if let Some(func_name) = &imp.function_name {
                // Import by Name
                // Write Hint/Name entry
                let entry_offset = hint_name_offset_counter;
                // Hint (0)
                new_section_data[entry_offset] = 0;
                new_section_data[entry_offset + 1] = 0;
                // Name
                let fname_bytes = func_name.as_bytes();
                new_section_data[entry_offset + 2..entry_offset + 2 + fname_bytes.len()]
                    .copy_from_slice(fname_bytes);

                hint_name_offset_counter += 2 + fname_bytes.len() + 1;
                if hint_name_offset_counter % 2 != 0 {
                    hint_name_offset_counter += 1;
                }

                // Thunk points to RVA of Hint/Name entry
                // We don't know the RVA yet (need section RVA). Store offset for now.
                // We will fix up RVAs later.
                thunk_value = entry_offset as u64;
            } else {
                // Import by Ordinal
                thunk_value = (1u64 << 63) | (imp.ordinal as u64);
            }

            // Write to ILT
            let ilt_pos = mod_layout.iat_offset + current_thunk_offset;
            new_section_data[ilt_pos..ilt_pos + 8].copy_from_slice(&thunk_value.to_le_bytes());

            // Write to IAT
            let ft_pos = mod_layout.ft_offset + current_thunk_offset;
            new_section_data[ft_pos..ft_pos + 8].copy_from_slice(&thunk_value.to_le_bytes());

            current_thunk_offset += 8;
        }

        // Descriptor will be written later once we have RVAs
        descriptor_offset += descriptor_size;
    }

    // 5. Append section to file
    // Find space for new section header
    let file_align = nt_headers.OptionalHeader.FileAlignment;
    let sect_align = nt_headers.OptionalHeader.SectionAlignment;

    // Align new section size to FileAlignment
    let raw_size = (new_section_data.len() as u32 + file_align - 1) & !(file_align - 1);
    new_section_data.resize(raw_size as usize, 0);

    // Calculate new section RVA
    // It should be after the last section's VirtualAddress + VirtualSize (aligned)
    let num_sections = nt_headers.FileHeader.NumberOfSections;
    let section_header_size =
        std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER>();
    let section_table_offset = nt_offset
        + std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64>();

    let last_section_offset =
        section_table_offset + (num_sections as usize - 1) * section_header_size;
    let last_section = unsafe {
        &*(file_data.as_ptr().add(last_section_offset)
            as *const windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER)
    };

    let last_section_end_rva = last_section.VirtualAddress + last_section.VirtualSize;
    let new_section_rva = (last_section_end_rva + sect_align - 1) & !(sect_align - 1);

    // Calculate new section Raw Pointer (end of file)
    let new_section_raw_ptr = file_data.len() as u32;

    // 6. Fix up RVAs in new_section_data
    // We stored offsets in ILT/IAT/Descriptors. Now convert to RVA = new_section_rva + offset.

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

        // Fix ILT/IAT values (if they are names)
        let mut current_thunk_offset = 0;
        for imp in *imps {
            if imp.function_name.is_some() {
                let ilt_pos = mod_layout.iat_offset + current_thunk_offset;
                let offset_bytes = if thunk_size == 8 {
                    &new_section_data[ilt_pos..ilt_pos + 8]
                } else {
                    &new_section_data[ilt_pos..ilt_pos + 4]
                };

                let offset_val = if thunk_size == 8 {
                    let bytes: [u8; 8] = match offset_bytes.try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            current_thunk_offset += thunk_size;
                            continue;
                        }
                    };
                    u64::from_le_bytes(bytes)
                } else {
                    let bytes: [u8; 4] = match offset_bytes.try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            current_thunk_offset += thunk_size;
                            continue;
                        }
                    };
                    u32::from_le_bytes(bytes) as u64
                };

                // If it's not ordinal (high bit not set), it's an offset to Hint/Name
                if (offset_val & (1u64 << 63)) == 0 {
                    let rva = new_section_rva as u64 + offset_val;
                    new_section_data[ilt_pos..ilt_pos + 8].copy_from_slice(&rva.to_le_bytes());

                    let ft_pos = mod_layout.ft_offset + current_thunk_offset;
                    new_section_data[ft_pos..ft_pos + 8].copy_from_slice(&rva.to_le_bytes());
                }
            }
            current_thunk_offset += 8;
        }

        // Create Descriptor
        let mut descriptor =
            windows::Win32::System::SystemServices::IMAGE_IMPORT_DESCRIPTOR::default();
        descriptor.OriginalFirstThunk = (new_section_rva as usize + mod_layout.iat_offset) as u32;
        descriptor.FirstThunk = (new_section_rva as usize + mod_layout.ft_offset) as u32;
        descriptor.Name = (new_section_rva as usize + mod_layout.name_offset) as u32;

        // Write Descriptor
        let desc_ptr = new_section_data.as_mut_ptr().add(descriptor_offset)
            as *mut windows::Win32::System::SystemServices::IMAGE_IMPORT_DESCRIPTOR;
        unsafe {
            *desc_ptr = descriptor;
        }

        descriptor_offset += descriptor_size;
    }

    // 7. Add New Section Header
    // Check if there is space for a new header
    // Usually there is padding after section headers.
    let new_header_offset = section_table_offset + (num_sections as usize) * section_header_size;
    // Check if we overlap with first section's data?
    // Usually SizeOfHeaders is aligned, so we might have space.
    // For safety in this simple implementation, we assume there is space (often true).
    // A robust dumper would shift data if needed.

    if new_header_offset + section_header_size > nt_headers.OptionalHeader.SizeOfHeaders as usize {
        return Err("Not enough space for new section header".to_string());
    }

    let mut new_section_header =
        windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER::default();
    new_section_header.Name = *b".fission"; // 8 bytes
    new_section_header.VirtualSize = current_offset as u32; // Actual used size
    new_section_header.VirtualAddress = new_section_rva;
    new_section_header.SizeOfRawData = raw_size;
    new_section_header.PointerToRawData = new_section_raw_ptr;
    new_section_header.Characteristics = 0xC0000040; // RW | INITIALIZED_DATA

    unsafe {
        let header_ptr = file_data.as_mut_ptr().add(new_header_offset)
            as *mut windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER;
        *header_ptr = new_section_header;
    }

    // 8. Update NT Headers
    nt_headers.FileHeader.NumberOfSections += 1;
    nt_headers.OptionalHeader.SizeOfImage =
        new_section_rva + ((current_offset as u32 + sect_align - 1) & !(sect_align - 1));

    // Update DataDirectory[1] (Import Table)
    nt_headers.OptionalHeader.DataDirectory[1].VirtualAddress = new_section_rva;
    nt_headers.OptionalHeader.DataDirectory[1].Size = total_size as u32; // Size of descriptors

    // Clear IAT Directory (12) to avoid conflict
    nt_headers.OptionalHeader.DataDirectory[12].VirtualAddress = 0;
    nt_headers.OptionalHeader.DataDirectory[12].Size = 0;

    // 9. Write updated file
    file_data.extend_from_slice(&new_section_data);
    std::fs::write(file_path, file_data).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn rebuild_imports(
    _file_path: &str,
    _imports: &[super::importer::ImportEntry],
    _original_base: u64,
) -> Result<(), String> {
    Err("Not supported on this OS".to_string())
}
