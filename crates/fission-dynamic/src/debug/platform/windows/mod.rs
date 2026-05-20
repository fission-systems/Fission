//! Windows platform integration: Win32 memory PAL + debugger API.

pub mod anti_debug;
pub mod debugger;
pub mod import_recon;
pub mod loader;
pub mod memory;
pub mod memory_map;
pub mod modules;
pub mod os_structs;
pub mod pe_raw;
pub mod process_dump;
pub mod seh;

pub use debugger::{WindowsDebugger, enumerate_processes, start_event_loop};
pub use loader::TitanLoader;
pub use memory::WindowsMemory;
