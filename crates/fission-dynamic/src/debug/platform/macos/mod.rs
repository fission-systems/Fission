//! macOS platform integration: Mach-memory PAL stub + native debugger.

pub mod debugger;
mod mach;
pub mod memory;
mod regs;

pub use debugger::{MacOSDebugger, enumerate_processes};
pub use memory::MacOSMemory;
