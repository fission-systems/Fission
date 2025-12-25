//! Python Plugin Support - Load and execute Python scripts using PyO3.
//!
//! This module is only compiled when the `python` feature is enabled.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::path::Path;
use std::fs;
use std::ffi::CString;
use std::collections::HashMap;

use crate::core::events::{FissionEvent, FissionEventType};
use super::api::{PluginInfo, PluginType, BinaryInfo};

/// Python plugin runtime
pub struct PythonRuntime {
    /// Loaded Python modules by plugin ID
    modules: HashMap<String, Py<PyModule>>,
}

impl PythonRuntime {
    /// Create a new Python runtime
    pub fn new() -> PyResult<Self> {
        // Initialize Python interpreter
        pyo3::prepare_freethreaded_python();
        
        Ok(Self {
            modules: HashMap::new(),
        })
    }
    
    /// Load a Python plugin from file
    pub fn load_plugin(&mut self, path: &Path, plugin_id: &str) -> PyResult<PluginInfo> {
        let code = fs::read_to_string(path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))?;
        
        // Convert to CString for pyo3
        let code_cstr = CString::new(code.as_bytes())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let file_cstr = CString::new(path.to_str().unwrap_or("plugin.py"))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let module_cstr = CString::new(plugin_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        
        Python::with_gil(|py| {
            // Create a new module for this plugin using PyModule::from_code_bound
            let module = PyModule::from_code(
                py,
                code_cstr.as_c_str(),
                file_cstr.as_c_str(),
                module_cstr.as_c_str()
            )?;
            
            // Extract plugin metadata if available
            let name = module.getattr("PLUGIN_NAME")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_else(|_| plugin_id.to_string());
            
            let version = module.getattr("PLUGIN_VERSION")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_else(|_| "0.1.0".to_string());
            
            let author = module.getattr("PLUGIN_AUTHOR")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_else(|_| "Unknown".to_string());
            
            let description = module.getattr("PLUGIN_DESCRIPTION")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_else(|_| String::new());
            
            // Store the module
            self.modules.insert(plugin_id.to_string(), module.into());
            
            Ok(PluginInfo {
                id: plugin_id.to_string(),
                name,
                version,
                author,
                description,
                plugin_type: PluginType::Python,
                enabled: true,
            })
        })
    }
    
    /// Unload a Python plugin
    pub fn unload_plugin(&mut self, plugin_id: &str) -> bool {
        self.modules.remove(plugin_id).is_some()
    }
    
    /// Call a hook function in a plugin
    pub fn call_hook(&self, plugin_id: &str, hook_name: &str, event: &FissionEvent) -> PyResult<()> {
        let module = self.modules.get(plugin_id)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>(
                format!("Plugin '{}' not found", plugin_id)
            ))?;
        
        Python::with_gil(|py| {
            let module = module.bind(py);
            
            // Check if the hook function exists
            if let Ok(func) = module.getattr(hook_name) {
                if func.is_callable() {
                    // Convert event to Python dict
                    let event_dict = self.event_to_pydict(py, event)?;
                    func.call1((event_dict,))?;
                }
            }
            
            Ok(())
        })
    }
    
    /// Dispatch an event to all plugins
    pub fn dispatch_event(&self, event: &FissionEvent, event_bus: Option<&crate::core::events::EventBus>) {
        let hook_name = match event.event_type() {
            FissionEventType::BinaryLoaded => "on_binary_loaded",
            FissionEventType::DecompilationSuccess => "on_function_decompiled",
            FissionEventType::BreakpointHit => "on_breakpoint_hit",
            FissionEventType::DebugStep => "on_debug_step",
            FissionEventType::AppStarted => "on_app_started",
            FissionEventType::AppShutdown => "on_app_shutdown",
            FissionEventType::CommandExecuted => "on_command_executed",
            FissionEventType::All => return, // Can't dispatch to "All"
            // Other events don't have Python hooks yet
            _ => return,
        };
        
        for plugin_id in self.modules.keys() {
            if let Err(e) = self.call_hook(plugin_id, hook_name, event) {
                let error_msg = format!("Plugin '{}' hook '{}' error: {:?}", plugin_id, hook_name, e);
                crate::core::logging::error(&error_msg);
                
                if let Some(bus) = event_bus {
                    bus.publish(FissionEvent::LogMessage {
                        level: "error".into(),
                        message: error_msg,
                        target: "plugin".into(),
                    });
                }
            }
        }
    }
    
    /// Convert a FissionEvent to a Python dict
    fn event_to_pydict<'py>(&self, py: Python<'py>, event: &FissionEvent) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        
        match event {
            FissionEvent::BinaryLoaded(binary) => {
                let info = BinaryInfo::from(binary.as_ref());
                dict.set_item("type", "binary_loaded")?;
                dict.set_item("path", &info.path)?;
                dict.set_item("format", &info.format)?;
                dict.set_item("is_64bit", info.is_64bit)?;
                dict.set_item("entry_point", info.entry_point)?;
                dict.set_item("function_count", info.function_count)?;
            }
            FissionEvent::DecompilationSuccess { address, function_name, code } => {
                dict.set_item("type", "function_decompiled")?;
                dict.set_item("address", *address)?;
                dict.set_item("name", function_name.as_deref().unwrap_or(""))?;
                dict.set_item("code", code)?;
            }
            FissionEvent::BreakpointHit { address, thread_id } => {
                dict.set_item("type", "breakpoint_hit")?;
                dict.set_item("address", *address)?;
                dict.set_item("thread_id", *thread_id)?;
            }
            FissionEvent::DebugStep { registers, thread_id } => {
                dict.set_item("type", "debug_step")?;
                dict.set_item("thread_id", *thread_id)?;
                dict.set_item("rip", registers.rip)?;
                dict.set_item("rsp", registers.rsp)?;
                dict.set_item("rax", registers.rax)?;
            }
            FissionEvent::AppStarted => {
                dict.set_item("type", "app_started")?;
            }
            FissionEvent::AppShutdown => {
                dict.set_item("type", "app_shutdown")?;
            }
            FissionEvent::CommandExecuted { command } => {
                dict.set_item("type", "command_executed")?;
                dict.set_item("command", command)?;
            }
            // Other events not exposed to Python yet
            _ => {
                dict.set_item("type", "unknown")?;
            }
        }
        
        Ok(dict)
    }
    
    /// Get list of loaded plugin IDs
    pub fn loaded_plugins(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
}

impl Default for PythonRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to initialize Python runtime")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_python_runtime_creation() {
        let runtime = PythonRuntime::new();
        assert!(runtime.is_ok());
    }
}
