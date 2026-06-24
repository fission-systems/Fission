//! Read PE structures from a live process memory.
//!
//! Low-level helpers that use `ReadProcessMemory` to inspect PE headers
//! and export tables inside a debuggee or target process.

use windows::{
    Win32::Foundation::*, Win32::System::Diagnostics::Debug::*, Win32::System::SystemServices::*,
};

/// Raw memory read helper (avoids dependency on the unpacker memory module).
pub fn read_memory(process_handle: HANDLE, address: u64, size: usize) -> Result<Vec<u8>, String> {
    unsafe {
        let mut buffer = vec![0u8; size];
        let mut bytes_read = 0;
        if ReadProcessMemory(
            process_handle,
            address as *const _,
            buffer.as_mut_ptr() as *mut _,
            size,
            Some(&mut bytes_read),
        )
        .as_bool()
        {
            if bytes_read < size {
                buffer.truncate(bytes_read);
            }
            Ok(buffer)
        } else {
            Err(format!(
                "ReadProcessMemory failed: {:?}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

/// Reads a null-terminated ASCII string from the target process.
pub fn read_cstring(
    process_handle: HANDLE,
    address: u64,
    max_length: usize,
) -> Result<String, String> {
    let mut buffer = Vec::new();
    let chunk_size = 64;
    let mut current_addr = address;

    loop {
        if buffer.len() >= max_length {
            break;
        }
        let read_size = std::cmp::min(chunk_size, max_length - buffer.len());
        let chunk = match read_memory(process_handle, current_addr, read_size) {
            Ok(c) => c,
            Err(_) => break,
        };
        if chunk.is_empty() {
            break;
        }
        if let Some(pos) = chunk.iter().position(|&b| b == 0) {
            buffer.extend_from_slice(&chunk[..pos]);
            break;
        } else {
            buffer.extend_from_slice(&chunk);
            current_addr += chunk.len() as u64;
        }
    }

    String::from_utf8(buffer).map_err(|e| e.to_string())
}

/// Reads the DOS Header from the target process.
pub fn read_dos_header(
    process_handle: HANDLE,
    base_address: u64,
) -> Result<IMAGE_DOS_HEADER, String> {
    let size = std::mem::size_of::<IMAGE_DOS_HEADER>();
    let data = read_memory(process_handle, base_address, size)?;
    if data.len() != size {
        return Err("Failed to read DOS Header".to_string());
    }
    let header: IMAGE_DOS_HEADER = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    if header.e_magic != IMAGE_DOS_SIGNATURE {
        return Err("Invalid DOS Signature".to_string());
    }
    Ok(header)
}

/// Reads the NT Headers (64-bit) from the target process.
pub fn read_nt_headers64(
    process_handle: HANDLE,
    base_address: u64,
    e_lfanew: i32,
) -> Result<IMAGE_NT_HEADERS64, String> {
    let size = std::mem::size_of::<IMAGE_NT_HEADERS64>();
    let address = base_address + e_lfanew as u64;
    let data = read_memory(process_handle, address, size)?;
    if data.len() != size {
        return Err("Failed to read NT Headers".to_string());
    }
    let header: IMAGE_NT_HEADERS64 = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    if header.Signature != IMAGE_NT_SIGNATURE {
        return Err("Invalid NT Signature".to_string());
    }
    Ok(header)
}

/// Reads Section Headers from the target process.
pub fn read_section_headers(
    process_handle: HANDLE,
    base_address: u64,
    e_lfanew: i32,
    number_of_sections: u16,
) -> Result<Vec<IMAGE_SECTION_HEADER>, String> {
    let nt_header_size = std::mem::size_of::<IMAGE_NT_HEADERS64>();
    let section_header_size = std::mem::size_of::<IMAGE_SECTION_HEADER>();
    let start_address = base_address + e_lfanew as u64 + nt_header_size as u64;
    let total_size = section_header_size * number_of_sections as usize;

    let data = read_memory(process_handle, start_address, total_size)?;
    if data.len() != total_size {
        return Err("Failed to read Section Headers".to_string());
    }

    let mut sections = Vec::with_capacity(number_of_sections as usize);
    for i in 0..number_of_sections as usize {
        let offset = i * section_header_size;
        let section: IMAGE_SECTION_HEADER =
            unsafe { std::ptr::read(data[offset..].as_ptr() as *const _) };
        sections.push(section);
    }

    Ok(sections)
}

/// Reads the Export Directory from the target process.
pub fn read_export_directory(
    process_handle: HANDLE,
    base_address: u64,
    export_dir_rva: u32,
) -> Result<IMAGE_EXPORT_DIRECTORY, String> {
    let size = std::mem::size_of::<IMAGE_EXPORT_DIRECTORY>();
    let address = base_address + export_dir_rva as u64;
    let data = read_memory(process_handle, address, size)?;
    if data.len() != size {
        return Err("Failed to read Export Directory".to_string());
    }
    let dir: IMAGE_EXPORT_DIRECTORY = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    Ok(dir)
}

/// Represents an exported function.
#[derive(Debug, Clone)]
pub struct ExportedFunction {
    pub name: Option<String>,
    pub ordinal: u32,
    pub rva: u32,
}

/// Parses the Export Table to get a list of exported functions.
pub fn parse_exports(
    process_handle: HANDLE,
    base_address: u64,
    export_dir: &IMAGE_EXPORT_DIRECTORY,
) -> Result<Vec<ExportedFunction>, String> {
    let mut exports = Vec::new();

    let num_funcs = export_dir.NumberOfFunctions as usize;
    let num_names = export_dir.NumberOfNames as usize;

    let func_table_size = num_funcs * 4;
    let func_table_addr = base_address + export_dir.AddressOfFunctions as u64;
    let func_data = read_memory(process_handle, func_table_addr, func_table_size)?;
    let func_rvas: &[u32] =
        unsafe { std::slice::from_raw_parts(func_data.as_ptr() as *const u32, num_funcs) };

    let name_table_size = num_names * 4;
    let name_table_addr = base_address + export_dir.AddressOfNames as u64;
    let name_data = read_memory(process_handle, name_table_addr, name_table_size)?;
    let name_rvas: &[u32] =
        unsafe { std::slice::from_raw_parts(name_data.as_ptr() as *const u32, num_names) };

    let ordinal_table_size = num_names * 2;
    let ordinal_table_addr = base_address + export_dir.AddressOfNameOrdinals as u64;
    let ordinal_data = read_memory(process_handle, ordinal_table_addr, ordinal_table_size)?;
    let name_ordinals: &[u16] =
        unsafe { std::slice::from_raw_parts(ordinal_data.as_ptr() as *const u16, num_names) };

    let mut name_map: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

    for i in 0..num_names {
        let name_rva = name_rvas[i];
        let func_index = name_ordinals[i] as u32;
        if let Ok(name) = read_cstring(process_handle, base_address + name_rva as u64, 256) {
            name_map.insert(func_index, name);
        }
    }

    for i in 0..num_funcs {
        let rva = func_rvas[i];
        if rva == 0 {
            continue;
        }
        let ordinal = export_dir.Base + i as u32;
        let name = name_map.get(&(i as u32)).cloned();
        exports.push(ExportedFunction { name, ordinal, rva });
    }

    Ok(exports)
}
