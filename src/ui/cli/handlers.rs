//! CLI Command Handlers
//!
//! Command implementations are organized in the `commands/` subdirectory.

use std::sync::Arc;

use crate::analysis::disasm::DisasmEngine;
use crate::analysis::loader::LoadedBinary;

pub mod commands;

// Re-export commands for convenience
pub use commands::*;

/// CLI session state
pub struct CliState {
    /// Currently loaded binary
    pub binary: Option<Arc<LoadedBinary>>,
    /// Disassembler engine (lazy initialized)
    pub disasm: Option<DisasmEngine>,
}

impl Default for CliState {
    fn default() -> Self {
        Self {
            binary: None,
            disasm: None,
        }
    }
}

impl CliState {
    /// Get or create disassembler for the current binary
    pub fn get_disasm(&mut self) -> Option<&DisasmEngine> {
        if self.disasm.is_none() {
            if let Some(ref binary) = self.binary {
                self.disasm = DisasmEngine::new(binary.is_64bit).ok();
            }
        }
        self.disasm.as_ref()
    }
}
