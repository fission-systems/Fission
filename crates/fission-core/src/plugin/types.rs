//! Plugin Types

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
            plugin_type: PluginType::Native,
            enabled: true,
        }
    }
}
