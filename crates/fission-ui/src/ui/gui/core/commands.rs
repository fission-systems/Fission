use crate::app::events::FissionEvent;
use crate::ui::gui::core::state::AppState;
use fission_loader::loader::{LoadedBinary, SectionInfo};
use std::sync::Arc;

/// Trait for all undoable commands
pub trait Command: Send + Sync {
    /// Execute the command
    fn execute(&mut self, state: &mut AppState) -> Result<(), String>;

    /// Undo the command
    fn undo(&mut self, state: &mut AppState) -> Result<(), String>;

    /// Get description for UI (e.g. "Rename Function")
    fn description(&self) -> String;
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the loaded binary from state, returning an error if not loaded.
fn get_binary(state: &AppState) -> Result<&Arc<LoadedBinary>, String> {
    state
        .analysis
        .loaded_binary()
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())
}

/// Calculate file offset from a virtual address using section information.
fn va_to_file_offset(sections: &[SectionInfo], address: u64) -> Result<u64, String> {
    sections
        .iter()
        .find(|s| address >= s.virtual_address && address < s.virtual_address + s.virtual_size)
        .map(|s| s.file_offset + (address - s.virtual_address))
        .ok_or_else(|| format!("Address 0x{:x} not mapped to any section", address))
}

/// Update the loaded binary in state and publish the event.
fn update_binary(state: &mut AppState, binary: LoadedBinary) {
    let new_arc = Arc::new(binary);
    state.analysis.domain.loaded_binary = Some(new_arc.clone());
    state
        .event_bus()
        .publish(FissionEvent::BinaryLoaded(new_arc));
}

// ============================================================================
// Command Manager
// ============================================================================

/// Manages the undo/redo stacks
pub struct CommandManager {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_history: usize,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: 50,
        }
    }

    /// Execute a new command and push to undo stack
    pub fn execute(
        &mut self,
        mut command: Box<dyn Command>,
        state: &mut AppState,
    ) -> Result<(), String> {
        command.execute(state)?;
        self.undo_stack.push(command);
        self.redo_stack.clear();

        // Limit history
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }

        Ok(())
    }

    /// Undo the last command
    pub fn undo(&mut self, state: &mut AppState) -> Result<String, String> {
        if let Some(mut cmd) = self.undo_stack.pop() {
            if let Err(e) = cmd.undo(state) {
                // If undo fails, state might be inconsistent. push back?
                // For now, just error out.
                self.undo_stack.push(cmd);
                return Err(e);
            }
            let desc = cmd.description();
            self.redo_stack.push(cmd);
            Ok(format!("Undid: {}", desc))
        } else {
            Err("Nothing to undo".to_string())
        }
    }

    /// Redo the last undone command
    pub fn redo(&mut self, state: &mut AppState) -> Result<String, String> {
        if let Some(mut cmd) = self.redo_stack.pop() {
            if let Err(e) = cmd.execute(state) {
                self.redo_stack.push(cmd);
                return Err(e);
            }
            let desc = cmd.description();
            self.undo_stack.push(cmd);
            Ok(format!("Redid: {}", desc))
        } else {
            Err("Nothing to redo".to_string())
        }
    }
}

impl Default for CommandManager {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------------------
// Concrete Commands
// ----------------------------------------------------------------------------

/// Command to rename a function
pub struct RenameFunctionCommand {
    pub address: u64,
    pub old_name: String,
    pub new_name: String,
}

impl Command for RenameFunctionCommand {
    fn execute(&mut self, state: &mut AppState) -> Result<(), String> {
        let binary_arc = get_binary(state)?;

        // Copy-on-Write: Clone the inner binary
        let mut binary = (**binary_arc).clone();

        // Find and update function
        if let Some(func) = binary
            .functions
            .iter_mut()
            .find(|f| f.address == self.address)
        {
            // Save old name if not set (first run)
            if self.old_name.is_empty() {
                self.old_name = func.name.clone();
            }
            func.name = self.new_name.clone();

            update_binary(state, binary);
            state.viewmodels.xrefs.clear();
            state.log(format!(
                "Renamed function 0x{:x} to '{}'",
                self.address, self.new_name
            ));

            Ok(())
        } else {
            Err(format!("Function at 0x{:x} not found", self.address))
        }
    }

    fn undo(&mut self, state: &mut AppState) -> Result<(), String> {
        let binary_arc = get_binary(state)?;

        let mut binary = (**binary_arc).clone();

        if let Some(func) = binary
            .functions
            .iter_mut()
            .find(|f| f.address == self.address)
        {
            func.name = self.old_name.clone();

            update_binary(state, binary);
            state.viewmodels.xrefs.clear();
            state.log(format!(
                "Reverted rename of function 0x{:x} to '{}'",
                self.address, self.old_name
            ));
            Ok(())
        } else {
            Err(format!("Function at 0x{:x} not found", self.address))
        }
    }

    fn description(&self) -> String {
        format!(
            "Rename function at 0x{:x} to '{}'",
            self.address, self.new_name
        )
    }
}

/// Command to patch bytes at an address
pub struct PatchBytesCommand {
    pub address: u64,
    pub old_bytes: Vec<u8>,
    pub new_bytes: Vec<u8>,
}

impl Command for PatchBytesCommand {
    fn execute(&mut self, state: &mut AppState) -> Result<(), String> {
        let binary_arc = get_binary(state)?;

        let mut binary = (**binary_arc).clone();

        // Calculate file offset from virtual address
        let offset = va_to_file_offset(&binary.sections, self.address)?;

        if offset as usize + self.new_bytes.len() > binary.data.len() {
            return Err("Patch out of bounds".to_string());
        }

        // Save old bytes if not set (first run)
        if self.old_bytes.is_empty() {
            self.old_bytes =
                binary.data[offset as usize..offset as usize + self.new_bytes.len()].to_vec();
        }

        // Apply patch using COW-enabled method
        binary
            .patch_bytes(offset, &self.new_bytes)
            .ok_or_else(|| "Patch failed".to_string())?;

        update_binary(state, binary);
        state.log(format!(
            "Patched {} bytes at 0x{:x}",
            self.new_bytes.len(),
            self.address
        ));
        Ok(())
    }

    fn undo(&mut self, state: &mut AppState) -> Result<(), String> {
        let binary_arc = get_binary(state)?;

        let mut binary = (**binary_arc).clone();

        let offset = va_to_file_offset(&binary.sections, self.address)?;

        // Revert patch using COW-enabled method
        binary
            .patch_bytes(offset, &self.old_bytes)
            .ok_or_else(|| "Revert patch failed".to_string())?;

        update_binary(state, binary);
        state.log(format!("Reverted patch at 0x{:x}", self.address));
        Ok(())
    }

    fn description(&self) -> String {
        format!(
            "Patch {} bytes at 0x{:x}",
            self.new_bytes.len(),
            self.address
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{FunctionInfo, LoadedBinaryBuilder};

    #[test]
    fn test_command_undo_redo() {
        // Setup AppState with a dummy binary
        let mut state = AppState::default();
        let builder =
            LoadedBinaryBuilder::new("test.bin".to_string(), vec![]).add_function(FunctionInfo {
                name: "func1".to_string(),
                address: 0x1000,
                size: 10,
                is_export: false,
                is_import: false,
            });
        let binary = builder.build().unwrap();
        state.analysis.domain.loaded_binary = Some(Arc::new(binary));

        // Create CommandManager
        let mut mgr = CommandManager::new();

        // 1. Execute Rename Command
        let cmd = Box::new(RenameFunctionCommand {
            address: 0x1000,
            old_name: String::new(),
            new_name: "renamed_func".to_string(),
        });

        mgr.execute(cmd, &mut state).unwrap();

        // Verify rename
        {
            let binary = state.analysis.domain.loaded_binary.as_ref().unwrap();
            let func = binary.function_at_exact(0x1000).unwrap();
            assert_eq!(func.name, "renamed_func");
        }

        // 2. Undo
        mgr.undo(&mut state).unwrap();

        // Verify revert
        {
            let binary = state.analysis.domain.loaded_binary.as_ref().unwrap();
            let func = binary.function_at_exact(0x1000).unwrap();
            assert_eq!(func.name, "func1");
        }

        // 3. Redo
        mgr.redo(&mut state).unwrap();

        // Verify re-rename
        {
            let binary = state.analysis.domain.loaded_binary.as_ref().unwrap();
            let func = binary.function_at_exact(0x1000).unwrap();
            assert_eq!(func.name, "renamed_func");
        }
    }
}
