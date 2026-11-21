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
        .is_ok()
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
    // A `Vec<u8>` is only byte-aligned: the read must not assume more.
    let header: IMAGE_DOS_HEADER = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const _) };
    if header.e_magic != IMAGE_DOS_SIGNATURE {
        return Err("Invalid DOS Signature".to_string());
    }
    Ok(header)
}

/// Where the NT headers are: `base + e_lfanew`, refusing a negative
/// `e_lfanew` (it sign-extended to an address near the top of memory) and one
/// that wraps.
fn nt_address(base_address: u64, e_lfanew: i32) -> Result<u64, String> {
    let offset = u64::try_from(e_lfanew).map_err(|_| format!("e_lfanew {e_lfanew} is negative"))?;
    base_address
        .checked_add(offset)
        .ok_or_else(|| "the NT headers lie past the address space".to_string())
}

/// Reads the NT Headers (64-bit) from the target process.
///
/// A `PE32` image's headers are shorter and laid out differently; reading
/// them as `IMAGE_NT_HEADERS64` gave wrong values for every field after the
/// file header, so such an image is refused by its optional-header magic.
pub fn read_nt_headers64(
    process_handle: HANDLE,
    base_address: u64,
    e_lfanew: i32,
) -> Result<IMAGE_NT_HEADERS64, String> {
    let size = std::mem::size_of::<IMAGE_NT_HEADERS64>();
    let address = nt_address(base_address, e_lfanew)?;
    let data = read_memory(process_handle, address, size)?;
    if data.len() != size {
        return Err("Failed to read NT Headers".to_string());
    }
    let header: IMAGE_NT_HEADERS64 = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const _) };
    if header.Signature != IMAGE_NT_SIGNATURE {
        return Err("Invalid NT Signature".to_string());
    }
    if header.OptionalHeader.Magic.0 != 0x20b {
        return Err(format!(
            "not a PE32+ image (optional header magic 0x{:x})",
            header.OptionalHeader.Magic.0
        ));
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
    // The section table follows the optional header, whose size the file
    // header gives; it is not `size_of::<IMAGE_NT_HEADERS64>()` for a PE32
    // image or one with a non-default optional header.
    let nt = nt_address(base_address, e_lfanew)?;
    let field = read_memory(process_handle, nt + 4 + 16, 2)?;
    let size_of_optional_header = crate::debug::platform::windows_layout::read_u16(&field, 0)
        .ok_or("Failed to read the file header")?;
    let section_header_size = std::mem::size_of::<IMAGE_SECTION_HEADER>();
    let start_address = nt + 4 + 20 + u64::from(size_of_optional_header);
    let total_size = section_header_size * number_of_sections as usize;

    let data = read_memory(process_handle, start_address, total_size)?;
    if data.len() != total_size {
        return Err("Failed to read Section Headers".to_string());
    }

    let mut sections = Vec::with_capacity(number_of_sections as usize);
    for i in 0..number_of_sections as usize {
        let offset = i * section_header_size;
        let section: IMAGE_SECTION_HEADER =
            unsafe { std::ptr::read_unaligned(data[offset..].as_ptr() as *const _) };
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
    let dir: IMAGE_EXPORT_DIRECTORY =
        unsafe { std::ptr::read_unaligned(data.as_ptr() as *const _) };
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
    use crate::debug::platform::windows_layout::{
        MAX_EXPORT_ENTRIES, export_slots, u16_table, u32_table,
    };

    // The counts come from the target's memory. A count beyond any real DLL
    // is a corrupt or hostile header, and allocating for it is the attack.
    if export_dir.NumberOfFunctions > MAX_EXPORT_ENTRIES
        || export_dir.NumberOfNames > MAX_EXPORT_ENTRIES
    {
        return Err(format!(
            "export directory claims {} functions and {} names",
            export_dir.NumberOfFunctions, export_dir.NumberOfNames
        ));
    }
    let num_funcs = export_dir.NumberOfFunctions as usize;
    let num_names = export_dir.NumberOfNames as usize;

    // Each table is read in full or not at all, and decoded byte by byte: the
    // old code cast a byte buffer to `&[u32]` (unaligned) of `count` words
    // whatever had actually been read.
    let func_rvas = u32_table(
        &read_memory(
            process_handle,
            base_address + export_dir.AddressOfFunctions as u64,
            num_funcs * 4,
        )?,
        num_funcs,
    )?;
    let name_rvas = u32_table(
        &read_memory(
            process_handle,
            base_address + export_dir.AddressOfNames as u64,
            num_names * 4,
        )?,
        num_names,
    )?;
    let name_ordinals = u16_table(
        &read_memory(
            process_handle,
            base_address + export_dir.AddressOfNameOrdinals as u64,
            num_names * 2,
        )?,
        num_names,
    )?;

    Ok(export_slots(export_dir.Base, &func_rvas, &name_ordinals)
        .into_iter()
        .map(|slot| ExportedFunction {
            name: slot.name_index.and_then(|index| {
                read_cstring(process_handle, base_address + name_rvas[index] as u64, 256).ok()
            }),
            ordinal: slot.ordinal,
            rva: slot.rva,
        })
        .collect())
}
