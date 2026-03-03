use crate::common::types::{BinaryInfo, FunctionInfo};
use crate::core::constants::FISSION_VERSION;
use anyhow::Result;
use std::any::Any;
use std::sync::Arc;

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

/// Context provided to plugins during callbacks
pub struct PluginContext {
    /// Access to Fission API
    pub api: Arc<dyn PluginAPI>,
    /// Optional extension (e.g. EventBus)
    pub extension: Option<Arc<dyn Any + Send + Sync>>,
}

impl PluginContext {
    pub fn new(api: Arc<dyn PluginAPI>, extension: Option<Arc<dyn Any + Send + Sync>>) -> Self {
        Self { api, extension }
    }

    /// Get typed extension (e.g. EventBus)
    pub fn get_extension<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.extension.as_ref()?.downcast_ref::<T>()
    }
}

/// The main trait for Fission plugins.
/// All plugins (native or script adapters) must implement this.
pub trait FissionPlugin: Send + Sync + Any {
    /// Get unique plugin ID
    fn id(&self) -> &str;

    /// Get human-readable name
    fn name(&self) -> &str;

    /// Get plugin version
    fn version(&self) -> &str {
        FISSION_VERSION
    }

    /// Get plugin description
    fn description(&self) -> &str {
        ""
    }

    /// Called when the plugin is loaded
    fn on_load(&mut self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// Called when the plugin is unloaded
    fn on_unload(&mut self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// Called when a binary is loaded
    fn on_binary_loaded(&self, _ctx: &PluginContext, _info: &BinaryInfo) {}

    /// Called when a function is decompiled
    fn on_function_decompiled(&self, _ctx: &PluginContext, _addr: u64, _code: &str) {}
}

// Allow downcasting for native plugins
impl dyn FissionPlugin {
    pub fn downcast_ref<T: FissionPlugin + 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }

    pub fn downcast_mut<T: FissionPlugin + 'static>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }
}
