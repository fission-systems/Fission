//! Plugin API - Interface exposed to plugins for interacting with Fission.

use crate::analysis::loader::{FunctionInfo, LoadedBinary};
use std::sync::Arc;

/// Information about a loaded binary (safe to send to plugins)
#[derive(Debug, Clone)]
pub struct BinaryInfo {
    /// File path
    pub path: String,
    /// Binary format (PE, ELF, Mach-O)
    pub format: String,
    /// Is 64-bit
    pub is_64bit: bool,
    /// Entry point address
    pub entry_point: u64,
    /// Image base address
    pub image_base: u64,
    /// Number of functions
    pub function_count: usize,
    /// Number of sections
    pub section_count: usize,
}

impl From<&LoadedBinary> for BinaryInfo {
    fn from(binary: &LoadedBinary) -> Self {
        Self {
            path: binary.path.clone(),
            format: binary.format.clone(),
            is_64bit: binary.is_64bit,
            entry_point: binary.entry_point,
            image_base: binary.image_base,
            function_count: binary.functions.len(),
            section_count: binary.sections.len(),
        }
    }
}

/// Plugin API trait - methods available to plugins
pub trait PluginAPI: Send + Sync {
    /// Get information about the loaded binary
    fn get_binary(&self) -> Option<BinaryInfo>;

    /// Get list of all functions
    fn get_functions(&self) -> Vec<FunctionInfo>;

    /// Read memory from the loaded binary (static analysis)
    fn read_binary_bytes(&self, address: u64, size: usize) -> Option<Vec<u8>>;

    /// Log a message to the console
    fn log(&self, message: &str);

    /// Log an error message
    fn log_error(&self, message: &str);

    /// Decompile a function at the given address
    fn decompile(&self, address: u64) -> Option<String>;

    /// Get the current decompiled code (if any)
    fn get_current_decompiled_code(&self) -> Option<String>;

    /// Get disassembly for an address range
    fn disassemble(&self, address: u64, size: usize) -> Vec<String>;
}

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Unique plugin identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin author
    pub author: String,
    /// Plugin description
    pub description: String,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Is plugin currently enabled
    pub enabled: bool,
}

/// Types of plugins
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    /// Python script plugin
    Python,
    /// Lua script plugin
    Lua,
    /// Native Rust plugin (dynamic library)
    Native,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::from("Unknown Plugin"),
            version: String::from("0.1.0"),
            author: String::from("Unknown"),
            description: String::new(),
            plugin_type: PluginType::Python,
            enabled: true,
        }
    }
}
