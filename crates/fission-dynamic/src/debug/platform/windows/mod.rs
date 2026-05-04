//! Windows platform integration: Win32 memory PAL + debugger API.

pub mod debugger;
pub mod memory;

pub use debugger::{WindowsDebugger, enumerate_processes, start_event_loop};
pub use memory::WindowsMemory;
