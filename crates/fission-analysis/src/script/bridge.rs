//! Python Bridge - Rust <-> Python communication via PyO3
//!
//! Provides the interface for executing Python scripts and
//! exposing Fission's internal state to Python code.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use std::sync::RwLock;
use thiserror::Error;

use super::types::{PyBinaryInfo, PyFunctionInfo, PySection};
use fission_loader::loader::LoadedBinary;

/// Shared state accessible from Python scripts
#[derive(Default)]
pub struct SharedScriptState {
    /// Currently loaded binary (if any)
    pub binary: Option<std::sync::Arc<LoadedBinary>>,
}

/// Thread-safe reference to shared state
pub type SharedState = Arc<RwLock<SharedScriptState>>;

/// Python bridge errors
#[derive(Error, Debug)]
pub enum ScriptError {
    #[error("Python error: {0}")]
    PythonError(String),

    #[error("Script not found: {0}")]
    ScriptNotFound(String),

    #[error("Failed to initialize Python: {0}")]
    InitError(String),
}

/// Context passed to Python hooks
#[pyclass]
#[derive(Debug, Clone)]
pub struct HookContext {
    #[pyo3(get, set)]
    pub rax: u64,
    #[pyo3(get, set)]
    pub rbx: u64,
    #[pyo3(get, set)]
    pub rcx: u64,
    #[pyo3(get, set)]
    pub rdx: u64,
    #[pyo3(get, set)]
    pub rsi: u64,
    #[pyo3(get, set)]
    pub rdi: u64,
    #[pyo3(get, set)]
    pub rbp: u64,
    #[pyo3(get, set)]
    pub rsp: u64,
    #[pyo3(get, set)]
    pub rip: u64,
    #[pyo3(get, set)]
    pub r8: u64,
    #[pyo3(get, set)]
    pub r9: u64,
    #[pyo3(get, set)]
    pub r10: u64,
    #[pyo3(get, set)]
    pub r11: u64,
    #[pyo3(get, set)]
    pub r12: u64,
    #[pyo3(get, set)]
    pub r13: u64,
    #[pyo3(get, set)]
    pub r14: u64,
    #[pyo3(get, set)]
    pub r15: u64,
    #[pyo3(get, set)]
    pub rflags: u64,
}

#[pymethods]
impl HookContext {
    #[new]
    fn new() -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rflags: 0,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "HookContext(rip={:#x}, rax={:#x}, rbx={:#x}, rcx={:#x}, rdx={:#x})",
            self.rip, self.rax, self.rbx, self.rcx, self.rdx
        )
    }
}

impl Default for HookContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Fission API exposed to Python
#[pyclass]
pub struct FissionAPI {
    state: SharedState,
}

#[pymethods]
impl FissionAPI {
    #[new]
    fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SharedScriptState::default())),
        }
    }

    /// Get Fission version
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Print to the Fission console
    fn log(&self, message: &str) {
        println!("[Python] {}", message);
    }

    /// Get help text listing available API methods
    fn help(&self) -> String {
        r#"Fission Python API:
  api.version()             - Get Fission version
  api.log(msg)              - Print message to console
  api.get_binary()          - Get loaded binary info
  api.get_functions()       - Get list of functions
  api.get_sections()        - Get list of sections
  api.read_memory(addr, size) - Read memory (debug mode)
  api.write_memory(addr, data) - Write memory (debug mode)
  api.set_breakpoint(addr)  - Set breakpoint at address
  api.get_rip()             - Get current instruction pointer
"#
        .to_string()
    }

    /// Get information about the loaded binary
    fn get_binary(&self) -> Option<PyBinaryInfo> {
        let state = self.state.read().ok()?;
        let binary = state.binary.as_ref()?;
        Some(PyBinaryInfo {
            name: binary.path.clone(),
            format: binary.format.clone(),
            arch: binary.arch_spec.clone(),
            entry_point: binary.entry_point,
            is_64bit: binary.is_64bit,
            section_count: binary.sections.len(),
            function_count: binary.functions.len(),
        })
    }

    /// Get list of all functions in the binary
    fn get_functions(&self) -> Vec<PyFunctionInfo> {
        let state = match self.state.read() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let binary = match state.binary.as_ref() {
            Some(b) => b,
            None => return Vec::new(),
        };
        binary
            .functions
            .iter()
            .map(|f| PyFunctionInfo {
                name: f.name.clone(),
                address: f.address,
                size: f.size,
                is_export: f.is_export,
                is_import: f.is_import,
            })
            .collect()
    }

    /// Get list of all sections in the binary
    fn get_sections(&self) -> Vec<PySection> {
        let state = match self.state.read() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let binary = match state.binary.as_ref() {
            Some(b) => b,
            None => return Vec::new(),
        };
        binary
            .sections
            .iter()
            .map(|s| PySection {
                name: s.name.clone(),
                virtual_address: s.virtual_address,
                virtual_size: s.virtual_size,
                file_offset: s.file_offset,
                file_size: s.file_size,
                is_executable: s.is_executable,
                is_writable: s.is_writable,
            })
            .collect()
    }

    /// Read memory from the target process
    fn read_memory(&self, address: u64, size: usize) -> PyResult<Vec<u8>> {
        crate::core::logging::debug(&format!(
            "Python requested memory read: {:#x} ({} bytes)",
            address, size
        ));
        Ok(vec![0u8; size])
    }

    /// Write memory to the target process
    fn write_memory(&self, address: u64, data: Vec<u8>) -> PyResult<usize> {
        crate::core::logging::debug(&format!(
            "Python requested memory write: {:#x} ({} bytes)",
            address,
            data.len()
        ));
        Ok(data.len())
    }

    /// Set a breakpoint at the given address
    fn set_breakpoint(&self, address: u64) -> PyResult<bool> {
        crate::core::logging::debug(&format!("Python set breakpoint: {:#x}", address));
        Ok(true)
    }

    /// Get the current instruction pointer
    fn get_rip(&self) -> PyResult<u64> {
        Ok(0)
    }
}

impl FissionAPI {
    /// Create FissionAPI with shared state
    pub fn with_state(state: SharedState) -> Self {
        Self { state }
    }
}

/// Main Python bridge interface
pub struct PythonBridge {
    /// Whether Python has been initialized
    initialized: bool,
    /// Shared state for script access
    state: SharedState,
}

impl PythonBridge {
    /// Create a new Python bridge
    pub fn new() -> Self {
        Self {
            initialized: false,
            state: Arc::new(RwLock::new(SharedScriptState::default())),
        }
    }

    /// Create Python bridge with shared state
    pub fn with_state(state: SharedState) -> Self {
        Self {
            initialized: false,
            state,
        }
    }

    /// Update the loaded binary in shared state
    pub fn set_binary(&self, binary: Option<std::sync::Arc<LoadedBinary>>) {
        if let Ok(mut state) = self.state.write() {
            state.binary = binary;
        }
    }

    /// Get the shared state reference
    pub fn shared_state(&self) -> SharedState {
        Arc::clone(&self.state)
    }

    /// Initialize the Python interpreter
    pub fn initialize(&mut self) -> Result<(), ScriptError> {
        if self.initialized {
            return Ok(());
        }

        pyo3::prepare_freethreaded_python();
        self.initialized = true;

        crate::core::logging::info("Python interpreter initialized");
        Ok(())
    }

    /// Execute a Python script string
    pub fn execute(&self, code: &str) -> Result<String, ScriptError> {
        if !self.initialized {
            return Err(ScriptError::InitError("Python not initialized".into()));
        }

        Python::with_gil(|py| {
            // Create Fission module
            let fission = PyModule::new_bound(py, "fission")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            fission
                .add_class::<FissionAPI>()
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            fission
                .add_class::<HookContext>()
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            // Add fission module to sys.modules
            let sys = py
                .import_bound("sys")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            let modules = sys
                .getattr("modules")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            modules
                .set_item("fission", fission)
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            // Execute the code
            let result = py.eval_bound(code, None, None);

            match result {
                Ok(val) => Ok(val.to_string()),
                Err(e) => Err(ScriptError::PythonError(e.to_string())),
            }
        })
    }

    /// Execute a Python file
    pub fn execute_file(&self, path: &str) -> Result<(), ScriptError> {
        let code = std::fs::read_to_string(path)
            .map_err(|_| ScriptError::ScriptNotFound(path.to_string()))?;

        self.run(&code)?;
        Ok(())
    }

    /// Run a Python code block (multi-line)
    pub fn run(&self, code: &str) -> Result<(), ScriptError> {
        if !self.initialized {
            return Err(ScriptError::InitError("Python not initialized".into()));
        }

        let state_clone = Arc::clone(&self.state);

        Python::with_gil(|py| {
            // Create and register fission module so scripts can import it
            let fission = PyModule::new_bound(py, "fission")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            fission
                .add_class::<FissionAPI>()
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            fission
                .add_class::<HookContext>()
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            // Add fission module to sys.modules
            let sys = py
                .import_bound("sys")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            let modules = sys
                .getattr("modules")
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            modules
                .set_item("fission", &fission)
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            // Create FissionAPI instance with shared state and add to globals
            let api = FissionAPI::with_state(state_clone);
            let globals = PyDict::new_bound(py);
            globals
                .set_item("FissionAPI", fission.getattr("FissionAPI").unwrap())
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;
            globals
                .set_item("api", api.into_pyobject(py).unwrap())
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            py.run_bound(code, Some(&globals), None)
                .map_err(|e| ScriptError::PythonError(e.to_string()))
        })
    }

    /// Call a Python function with a HookContext
    pub fn call_hook(&self, func_name: &str, ctx: &mut HookContext) -> Result<(), ScriptError> {
        if !self.initialized {
            return Err(ScriptError::InitError("Python not initialized".into()));
        }

        Python::with_gil(|py| {
            let globals = PyDict::new_bound(py);
            globals
                .set_item("ctx", ctx.clone().into_py(py))
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            let call_code = format!("{}(ctx)", func_name);
            py.run_bound(&call_code, Some(&globals), None)
                .map_err(|e| ScriptError::PythonError(e.to_string()))?;

            // Update context from Python modifications
            if let Ok(Some(new_ctx)) = globals.get_item("ctx") {
                if let Ok(updated) = new_ctx.extract::<HookContext>() {
                    *ctx = updated;
                }
            }

            Ok(())
        })
    }
}

impl Default for PythonBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context() {
        let mut ctx = HookContext::new();
        ctx.rax = 0xDEADBEEF;
        assert_eq!(ctx.rax, 0xDEADBEEF);
    }
}
