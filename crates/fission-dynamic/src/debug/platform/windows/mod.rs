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
