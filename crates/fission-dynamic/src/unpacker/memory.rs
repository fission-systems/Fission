#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::*, Win32::System::Diagnostics::Debug::*, Win32::System::Memory::*, core::*,
};

/// Reads memory from a target process.
///
/// # Arguments
/// * `process_handle` - Handle to the target process.
/// * `address` - Virtual address to read from.
/// * `size` - Number of bytes to read.
#[cfg(target_os = "windows")]
pub fn read_memory(process_handle: HANDLE, address: u64, size: usize) -> Result<Vec<u8>, String> {
    unsafe {
        let mut buffer = vec![0u8; size];
        let mut bytes_read = 0;

        // Clean Room: Using ReadProcessMemory directly via windows-rs
        if ReadProcessMemory(
            process_handle,
            address as *const _,
            buffer.as_mut_ptr() as *mut _,
            size,
            Some(&mut bytes_read),
        )
        .as_bool()
        {
            // If we read less than requested, truncate the buffer?
            // Or just return what we got. TitanEngine logic checks if bytes_read == 0.
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

/// Reads a null-terminated string from the target process.
#[cfg(target_os = "windows")]
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

#[cfg(not(target_os = "windows"))]
pub fn read_memory(_process_handle: usize, _address: u64, _size: usize) -> Result<Vec<u8>, String> {
    Err("Not supported on this OS".to_string())
}

/// Writes memory to a target process.
///
/// # Arguments
/// * `process_handle` - Handle to the target process.
/// * `address` - Virtual address to write to.
/// * `data` - Data to write.
#[cfg(target_os = "windows")]
pub fn write_memory(process_handle: HANDLE, address: u64, data: &[u8]) -> Result<usize, String> {
    unsafe {
        let mut bytes_written = 0;

        // Clean Room: Using WriteProcessMemory directly via windows-rs
        if WriteProcessMemory(
            process_handle,
            address as *const _,
            data.as_ptr() as *const _,
            data.len(),
            Some(&mut bytes_written),
        )
        .as_bool()
        {
            Ok(bytes_written)
        } else {
            Err(format!(
                "WriteProcessMemory failed: {:?}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn write_memory(_process_handle: usize, _address: u64, _data: &[u8]) -> Result<usize, String> {
    Err("Not supported on this OS".to_string())
}

/// Queries memory information (permissions, state, etc.)
#[cfg(target_os = "windows")]
pub fn query_memory(
    process_handle: HANDLE,
    address: u64,
) -> Result<MEMORY_BASIC_INFORMATION, String> {
    unsafe {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        if VirtualQueryEx(
            process_handle,
            Some(address as *const _),
            &mut mbi,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        ) != 0
        {
            Ok(mbi)
        } else {
            Err(format!(
                "VirtualQueryEx failed: {:?}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn query_memory(_process_handle: usize, _address: u64) -> Result<(), String> {
    Err("Not supported on this OS".to_string())
}
