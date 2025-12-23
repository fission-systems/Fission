//! Plugin Event Hooks - Define events that plugins can subscribe to.

use std::sync::Arc;
use crate::analysis::loader::LoadedBinary;
use crate::debug::types::RegisterState;

/// Events that plugins can hook into
#[derive(Debug, Clone)]
pub enum PluginEvent {
    /// A binary file was loaded
    BinaryLoaded {
        /// The loaded binary
        binary: Arc<LoadedBinary>,
    },
    
    /// A function was decompiled
    FunctionDecompiled {
        /// Function address
        address: u64,
        /// Function name
        name: String,
        /// Decompiled C code
        code: String,
    },
    
    /// A breakpoint was hit
    BreakpointHit {
        /// Breakpoint address
        address: u64,
        /// Thread ID
        thread_id: u32,
    },
    
    /// A debug step was executed
    DebugStep {
        /// Current register state
        registers: RegisterState,
        /// Thread ID
        thread_id: u32,
    },
    
    /// Application started
    AppStarted,
    
    /// Application is shutting down
    AppShutdown,
    
    /// User executed a command
    CommandExecuted {
        /// The command string
        command: String,
    },
}

/// Hook priority (lower = earlier execution)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HookPriority {
    /// Run first
    High = 0,
    /// Default priority
    Normal = 50,
    /// Run last
    Low = 100,
}

impl Default for HookPriority {
    fn default() -> Self {
        HookPriority::Normal
    }
}

/// A plugin hook registration
#[derive(Debug, Clone)]
pub struct PluginHook {
    /// Unique hook ID
    pub id: u64,
    /// Plugin ID that registered this hook
    pub plugin_id: String,
    /// Event type to hook
    pub event_type: PluginEventType,
    /// Execution priority
    pub priority: HookPriority,
}

/// Event types for filtering hooks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginEventType {
    BinaryLoaded,
    FunctionDecompiled,
    BreakpointHit,
    DebugStep,
    AppStarted,
    AppShutdown,
    CommandExecuted,
    /// Catch all events
    All,
}

impl PluginEvent {
    /// Get the event type
    pub fn event_type(&self) -> PluginEventType {
        match self {
            PluginEvent::BinaryLoaded { .. } => PluginEventType::BinaryLoaded,
            PluginEvent::FunctionDecompiled { .. } => PluginEventType::FunctionDecompiled,
            PluginEvent::BreakpointHit { .. } => PluginEventType::BreakpointHit,
            PluginEvent::DebugStep { .. } => PluginEventType::DebugStep,
            PluginEvent::AppStarted => PluginEventType::AppStarted,
            PluginEvent::AppShutdown => PluginEventType::AppShutdown,
            PluginEvent::CommandExecuted { .. } => PluginEventType::CommandExecuted,
        }
    }
}
