//! Debug Module - Dynamic analysis and debugging functionality.
//!
//! Provides cross-platform debugging capabilities:
//! - Process attach/detach
//! - Breakpoint management
//! - Register/memory access
//! - Step execution
//! - Execution timeline (`timeline` → `fission-ttd`, optional RR on Linux)
//! - RR (Record and Replay) integration (Linux)
//!
//! # Architecture
//!
//! ```text
//! debug/
//! ├── mod.rs       # Re-exports; thin `windows`/`linux`/`macos` shims → `platform::*`
//! ├── platform/    # Per-OS memory PAL + debugger (`memory`, `debugger`)
//! ├── traits.rs    # Debugger + TimeTravelDebugger traits
//! ├── types.rs     # Shared types (DebugEvent, RegisterState, etc.)
//! ├── memory.rs    # Cross-platform memory helpers over PlatformMemory
//! ├── timeline.rs  # Timeline façade over `fission-ttd` (+ RR on Linux)
//! └── rr/          # RR debugger integration (Linux only)
//! ```

// Core modules
pub mod memory;
pub mod platform;
pub mod timeline;
pub mod traits;
pub mod types;

// RR (Record and Replay) module - Linux only but types available everywhere
pub mod rr;

/// Compatibility shim — implementations live under [`platform::windows`](crate::debug::platform::windows).
#[cfg(target_os = "windows")]
pub mod windows {
    pub use crate::debug::platform::windows::{
        WindowsDebugger, enumerate_processes, start_event_loop,
    };
}

/// Compatibility shim — implementations live under [`platform::linux`](crate::debug::platform::linux).
#[cfg(target_os = "linux")]
pub mod linux {
    pub use crate::debug::platform::linux::{LinuxDebugger, enumerate_processes};
}

/// Compatibility shim — implementations live under [`platform::macos`](crate::debug::platform::macos).
#[cfg(target_os = "macos")]
pub mod macos {
    pub use crate::debug::platform::macos::{MacOSDebugger, enumerate_processes};
}

// Re-export the Debugger trait and TimeTravelDebugger trait
pub use traits::{Debugger, TimeTravelDebugger};

// Re-export commonly used types
pub use types::{Breakpoint, DebugEvent, DebugState, DebugStatus, ProcessInfo, RegisterState};

// ============================================================================
// Platform debugger / process list (canonical owner: `platform`)
// ============================================================================

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use platform::{PlatformDebugger, enumerate_processes};

/// Fallback for unsupported platforms - returns empty process list
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    Vec::new()
}
