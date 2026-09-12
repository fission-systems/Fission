//! Windows platform integration: Win32 memory PAL + debugger API.
//!
//! The memory PAL, module walking and PE helpers build. The *debugger* --
//! attach, breakpoints, thread contexts, the Win32 debug-event loop -- does
//! not, and has not for a long time: see `windows_native_debugger` in this
//! crate's manifest for what is wrong with it. It is behind that feature so
//! that a build which does not turn it on is a build that says so, rather
//! than one that quietly contains no debugger at all.

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
