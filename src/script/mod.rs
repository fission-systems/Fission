//! Script module - Python scripting integration
//!
//! Uses PyO3 to embed Python interpreter for scripting support.
//! This module is only available when the "python" feature is enabled.

#[cfg(feature = "python")]
pub mod bridge;
#[cfg(feature = "python")]
pub mod types;

#[cfg(feature = "python")]
pub use bridge::{PythonBridge, SharedScriptState, SharedState};

/// Stub PythonBridge for when Python feature is disabled
#[cfg(not(feature = "python"))]
pub struct PythonBridge;

#[cfg(not(feature = "python"))]
impl PythonBridge {
    pub fn new() -> Self {
        Self
    }
    pub fn initialize(&mut self) -> Result<(), String> {
        Err("Python support not compiled. Enable 'python' feature.".into())
    }
    pub fn execute(&self, _code: &str) -> Result<String, String> {
        Err("Python support not compiled".into())
    }
}

#[cfg(not(feature = "python"))]
impl Default for PythonBridge {
    fn default() -> Self {
        Self::new()
    }
}
