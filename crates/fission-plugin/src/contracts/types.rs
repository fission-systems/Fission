//! Plugin Types

use crate::security::PluginTrustTier;
use fission_core::FISSION_VERSION;

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
    /// Declared trust boundary of this plugin
    pub trust_tier: PluginTrustTier,
    /// Is plugin currently enabled
    pub enabled: bool,
}

/// Types of plugins
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    /// Lua script plugin
    Lua,
    /// Constrained Rhai script
    RhaiScript,
    /// Native Rust plugin (dynamic library)
    Native,
    /// Out-of-process external SDK worker
    ExternalWorker,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::from("Unknown Plugin"),
            version: FISSION_VERSION.to_string(),
            author: String::from("Unknown"),
            description: String::new(),
            plugin_type: PluginType::Native,
            trust_tier: PluginTrustTier::TrustedNative,
            enabled: true,
        }
    }
}
