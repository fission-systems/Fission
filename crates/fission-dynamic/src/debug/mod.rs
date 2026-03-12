//! Debug Module - Dynamic analysis and debugging functionality.
//!
//! Provides cross-platform debugging capabilities:
//! - Process attach/detach
//! - Breakpoint management
//! - Register/memory access
//! - Step execution
//! - Time Travel Debugging (TTD)
//! - RR (Record and Replay) integration (Linux)
//!
//! # Architecture
//!
//! ```text
//! debug/
//! ├── mod.rs       # This file - re-exports and platform selection
//! ├── traits.rs    # Platform-agnostic Debugger + TimeTravelDebugger traits
//! ├── types.rs     # Shared types (DebugEvent, RegisterState, etc.)
//! ├── memory.rs    # Cross-platform memory operations
//! ├── windows/     # Windows-specific implementation
//! ├── linux.rs     # Linux-specific implementation (ptrace)
//! ├── macos.rs     # macOS-specific implementation (Mach API stub)
//! ├── ttd/         # Time Travel Debugging (internal snapshots)
//! └── rr/          # RR debugger integration (Linux only)
//! ```

// Core modules
pub mod memory;
pub mod platform;
pub mod traits;
pub mod ttd;
pub mod types;

// RR (Record and Replay) module - Linux only but types available everywhere
pub mod rr;

// Platform-specific implementations
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

// Re-export the Debugger trait and TimeTravelDebugger trait
pub use traits::{Debugger, TimeTravelDebugger};

// Re-export commonly used types
pub use types::{Breakpoint, DebugEvent, DebugState, DebugStatus, ProcessInfo, RegisterState};

// ============================================================================
// Platform-specific exports
// ============================================================================

/// Windows: Use WindowsDebugger as the platform debugger
#[cfg(target_os = "windows")]
pub use windows::WindowsDebugger as PlatformDebugger;

#[cfg(target_os = "windows")]
pub use windows::enumerate_processes;

/// Linux: Use LinuxDebugger as the platform debugger
#[cfg(target_os = "linux")]
pub use linux::LinuxDebugger as PlatformDebugger;

#[cfg(target_os = "linux")]
pub use linux::enumerate_processes;

/// macOS: Use MacOSDebugger as the platform debugger (stub)
#[cfg(target_os = "macos")]
pub use macos::MacOSDebugger as PlatformDebugger;

#[cfg(target_os = "macos")]
pub use macos::enumerate_processes;

// ============================================================================
// Fallback for unsupported platforms
// ============================================================================

/// Fallback for unsupported platforms - returns empty process list
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    Vec::new()
}
