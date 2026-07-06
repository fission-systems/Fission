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
pub use traits::{ExecutionBackend, TimeTravelDebugger};

// Re-export commonly used types
pub use types::{
    Breakpoint, DebugEvent, DebugState, DebugStatus, ExceptionPolicy, ModuleInfo, ProcessInfo,
    RegisterState, ThreadInfo,
};

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

// ============================================================================
// DebugSession — ergonomic session builder
// ============================================================================

use std::sync::{Arc, Mutex};
use timeline::Timeline;

/// High-level debug session wrapping a [`PlatformDebugger`] and an optional
/// [`Timeline`] for time-travel recording.
///
/// # Example
/// ```ignore
/// let mut session = DebugSession::new()
///     .with_timeline()
///     .build();
///
/// session.attach(pid)?;
/// session.debugger.poll_event(100)?;
/// session.detach()?;
/// ```
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub struct DebugSession {
    /// The platform-native or emulator debugger instance.
    pub debugger: Box<dyn ExecutionBackend>,
    /// Shared execution timeline (None if not requested at build time).
    pub timeline: Option<Arc<Mutex<Timeline>>>,
}

/// Builder for [`DebugSession`].
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub struct DebugSessionBuilder {
    with_timeline: bool,
    use_emulator: bool,
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
impl DebugSessionBuilder {
    /// Enable automatic TTD timeline recording.
    pub fn with_timeline(mut self) -> Self {
        self.with_timeline = true;
        self
    }

    /// Use the emulator backend instead of the native platform debugger.
    pub fn with_emulator(mut self) -> Self {
        self.use_emulator = true;
        self
    }

    /// Build the [`DebugSession`], wiring up the timeline if requested.
    pub fn build(self) -> DebugSession {
        if self.use_emulator {
            let debugger = crate::debug::EmulatorBackend::new();
            return DebugSession {
                debugger: Box::new(debugger),
                timeline: None, // Timeline recording with emulator not yet supported
            };
        }

        let mut debugger = PlatformDebugger::default();
        let timeline = if self.with_timeline {
            let arc = Arc::new(Mutex::new(Timeline::new()));
            #[cfg(target_os = "windows")]
            debugger.set_ttd_timeline(arc.clone());
            Some(arc)
        } else {
            None
        };
        DebugSession {
            debugger: Box::new(debugger),
            timeline,
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
impl DebugSession {
    /// Create a new session builder.
    pub fn new() -> DebugSessionBuilder {
        DebugSessionBuilder {
            with_timeline: false,
            use_emulator: false,
        }
    }

    /// Convenience: attach to a PID.
    pub fn attach(&mut self, pid: u32) -> fission_core::Result<()> {
        self.debugger.attach(pid)
    }

    /// Convenience: detach from the current process.
    pub fn detach(&mut self) -> fission_core::Result<()> {
        self.debugger.detach()
    }

    /// Convenience: write registers to a thread.
    pub fn set_registers(
        &mut self,
        thread_id: u32,
        regs: &crate::debug::types::RegisterState,
    ) -> fission_core::Result<()> {
        self.debugger.set_registers(thread_id, regs)
    }

    /// Convenience: launch a new process under the debugger.
    pub fn launch(&mut self, path: &str, args: &[String]) -> fission_core::Result<u32> {
        self.debugger.launch(path, args)
    }
}
pub mod emulator_backend;
pub use emulator_backend::EmulatorBackend;
