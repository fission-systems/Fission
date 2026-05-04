//! Linux platform integration: procfs memory PAL + ptrace debugger.

pub mod debugger;
pub mod memory;

pub use debugger::{LinuxDebugger, enumerate_processes};
pub use memory::LinuxMemory;
