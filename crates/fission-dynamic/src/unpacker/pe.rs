#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::*,
    Win32::System::Diagnostics::Debug::*,
    Win32::System::SystemServices::*, // For IMAGE_DOS_HEADER etc.
    core::*,
};

/// Reads the DOS Header from the target process.
#[cfg(target_os = "windows")]
pub fn read_dos_header(
    process_handle: HANDLE,
    base_address: u64,
) -> Result<windows::Win32::System::SystemServices::IMAGE_DOS_HEADER, String> {
    let size = std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_DOS_HEADER>();
    let data = super::memory::read_memory(process_handle, base_address, size)?;

    if data.len() != size {
        return Err("Failed to read DOS Header".to_string());
    }

    let header: windows::Win32::System::SystemServices::IMAGE_DOS_HEADER =
        unsafe { std::ptr::read(data.as_ptr() as *const _) };

    if header.e_magic != windows::Win32::System::SystemServices::IMAGE_DOS_SIGNATURE {
        return Err("Invalid DOS Signature".to_string());
    }

    Ok(header)
}

/// Reads the NT Headers (64-bit) from the target process.
#[cfg(target_os = "windows")]
pub fn read_nt_headers64(
    process_handle: HANDLE,
    base_address: u64,
    e_lfanew: i32,
) -> Result<windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64, String> {
    let size = std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64>();
    let address = base_address + e_lfanew as u64;
    let data = super::memory::read_memory(process_handle, address, size)?;

    if data.len() != size {
        return Err("Failed to read NT Headers".to_string());
    }

    let header: windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64 =
        unsafe { std::ptr::read(data.as_ptr() as *const _) };

    if header.Signature != windows::Win32::System::SystemServices::IMAGE_NT_SIGNATURE {
        return Err("Invalid NT Signature".to_string());
    }

    Ok(header)
}

/// Reads Section Headers from the target process.
#[cfg(target_os = "windows")]
pub fn read_section_headers(
    process_handle: HANDLE,
    base_address: u64,
    e_lfanew: i32,
    number_of_sections: u16,
) -> Result<Vec<windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER>, String> {
    let nt_header_size =
        std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_NT_HEADERS64>();
    let section_header_size =
        std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER>();

    // Section headers start immediately after NT Headers
    let start_address = base_address + e_lfanew as u64 + nt_header_size as u64;
    let total_size = section_header_size * number_of_sections as usize;

    let data = super::memory::read_memory(process_handle, start_address, total_size)?;

    if data.len() != total_size {
        return Err("Failed to read Section Headers".to_string());
    }

    let mut sections = Vec::with_capacity(number_of_sections as usize);
    for i in 0..number_of_sections as usize {
        let offset = i * section_header_size;
        let section: windows::Win32::System::SystemServices::IMAGE_SECTION_HEADER =
            unsafe { std::ptr::read(data[offset..].as_ptr() as *const _) };
        sections.push(section);
    }

    Ok(sections)
}

#[cfg(not(target_os = "windows"))]
pub fn read_dos_header(_process_handle: usize, _base_address: u64) -> Result<(), String> {
    Err("Not supported on this OS".to_string())
}

/// Reads the Export Directory from the target process.
#[cfg(target_os = "windows")]
pub fn read_export_directory(
    process_handle: HANDLE,
    base_address: u64,
    export_dir_rva: u32,
) -> Result<windows::Win32::System::SystemServices::IMAGE_EXPORT_DIRECTORY, String> {
    let size =
        std::mem::size_of::<windows::Win32::System::SystemServices::IMAGE_EXPORT_DIRECTORY>();
    let address = base_address + export_dir_rva as u64;
    let data = super::memory::read_memory(process_handle, address, size)?;

    if data.len() != size {
        return Err("Failed to read Export Directory".to_string());
    }

    let dir: windows::Win32::System::SystemServices::IMAGE_EXPORT_DIRECTORY =
        unsafe { std::ptr::read(data.as_ptr() as *const _) };
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
#[cfg(target_os = "windows")]
pub fn parse_exports(
    process_handle: HANDLE,
    base_address: u64,
    export_dir: &windows::Win32::System::SystemServices::IMAGE_EXPORT_DIRECTORY,
) -> Result<Vec<ExportedFunction>, String> {
    let mut exports = Vec::new();

    let num_funcs = export_dir.NumberOfFunctions as usize;
    let num_names = export_dir.NumberOfNames as usize;

    // Read AddressOfFunctions (Array of u32 RVAs)
    let func_table_size = num_funcs * 4;
    let func_table_addr = base_address + export_dir.AddressOfFunctions as u64;
    let func_data = super::memory::read_memory(process_handle, func_table_addr, func_table_size)?;
    let func_rvas: &[u32] =
        unsafe { std::slice::from_raw_parts(func_data.as_ptr() as *const u32, num_funcs) };

    // Read AddressOfNames (Array of u32 RVAs pointing to strings)
    let name_table_size = num_names * 4;
    let name_table_addr = base_address + export_dir.AddressOfNames as u64;
    let name_data = super::memory::read_memory(process_handle, name_table_addr, name_table_size)?;
    let name_rvas: &[u32] =
        unsafe { std::slice::from_raw_parts(name_data.as_ptr() as *const u32, num_names) };

    // Read AddressOfNameOrdinals (Array of u16 indices into AddressOfFunctions)
    let ordinal_table_size = num_names * 2;
    let ordinal_table_addr = base_address + export_dir.AddressOfNameOrdinals as u64;
    let ordinal_data =
        super::memory::read_memory(process_handle, ordinal_table_addr, ordinal_table_size)?;
    let name_ordinals: &[u16] =
        unsafe { std::slice::from_raw_parts(ordinal_data.as_ptr() as *const u16, num_names) };

    // Map Ordinal -> Name
    // Note: Ordinals in AddressOfNameOrdinals are indices into AddressOfFunctions.
    // The actual Ordinal value is index + Base.

    // Create a map of Index -> Name
    let mut name_map: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

    for i in 0..num_names {
        let name_rva = name_rvas[i];
        let func_index = name_ordinals[i] as u32;

        // Read name string
        if let Ok(name) =
            super::memory::read_cstring(process_handle, base_address + name_rva as u64, 256)
        {
            name_map.insert(func_index, name);
        }
    }

    for i in 0..num_funcs {
        let rva = func_rvas[i];
        if rva == 0 {
            continue;
        } // Skip empty entries

        let ordinal = export_dir.Base + i as u32;
        let name = name_map.get(&(i as u32)).cloned();

        exports.push(ExportedFunction { name, ordinal, rva });
    }

    Ok(exports)
}
