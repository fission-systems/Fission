//! Windows platform integration: Win32 memory PAL + debugger API.
//!
//! The live-process debugger and its auxiliary Windows analysis APIs are
//! gated by `windows_native_debugger`; the feature stays opt-in so the normal
//! cross-platform emulator/debugger build does not activate Windows runtime
//! dependencies.

// Everything that operates on a *live* Windows process. All of it is the
// same unported code and nothing outside this directory uses any of it.
#[cfg(feature = "windows_native_debugger")]
pub mod anti_debug;
#[cfg(feature = "windows_native_debugger")]
pub mod debugger;
#[cfg(feature = "windows_native_debugger")]
pub mod import_recon;
#[cfg(feature = "windows_native_debugger")]
pub mod os_structs;
#[cfg(feature = "windows_native_debugger")]
pub mod process_dump;
#[cfg(feature = "windows_native_debugger")]
pub mod seh;

pub mod loader;
pub mod memory;

/// A kernel handle closed when it goes out of scope, so an early return
/// cannot leak it.
#[cfg(feature = "windows_native_debugger")]
pub(crate) struct OwnedHandle(pub windows::Win32::Foundation::HANDLE);

#[cfg(feature = "windows_native_debugger")]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by the owner of this guard and is
        // closed exactly once, here.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// `size_of::<T>()` bytes read from the target into a `T`.
///
/// `ReadProcessMemory` can succeed with fewer bytes than were asked for; the
/// fields after them were reported as whatever the variable held before.
#[cfg(feature = "windows_native_debugger")]
pub(crate) fn read_value<T: Copy + Default>(
    process: windows::Win32::Foundation::HANDLE,
    address: u64,
) -> Result<T, String> {
    let mut value = T::default();
    let size = std::mem::size_of::<T>();
    let mut read = 0usize;
    // SAFETY: `value` is `size` writable bytes of a plain-data type.
    unsafe {
        windows::Win32::System::Diagnostics::Debug::ReadProcessMemory(
            process,
            address as *const std::ffi::c_void,
            &mut value as *mut T as *mut std::ffi::c_void,
            size,
            Some(&mut read),
        )
    }
    .map_err(|e| format!("ReadProcessMemory at 0x{address:x} failed: {e:?}"))?;
    if read != size {
        return Err(format!(
            "ReadProcessMemory at 0x{address:x} read {read} of {size} bytes"
        ));
    }
    Ok(value)
}
pub mod memory_map;
pub mod modules;
pub mod pe_raw;

#[cfg(feature = "windows_native_debugger")]
pub use debugger::{WindowsDebugger, enumerate_processes, start_event_loop};

#[cfg(not(feature = "windows_native_debugger"))]
mod unported;
#[cfg(not(feature = "windows_native_debugger"))]
pub use unported::{WindowsDebugger, enumerate_processes, start_event_loop};

pub use loader::TitanLoader;
pub use memory::WindowsMemory;
