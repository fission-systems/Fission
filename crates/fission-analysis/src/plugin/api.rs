//! Plugin API - Interface exposed to plugins for interacting with Fission.

// Re-export shared types from fission-core
pub use fission_core::common::types::BinaryInfo;
pub use fission_core::plugin::traits::PluginAPI;
pub use fission_core::plugin::types::{PluginInfo, PluginType};

use fission_loader::loader::LoadedBinary;

/// Helper to create BinaryInfo from LoadedBinary
pub fn create_binary_info(binary: &LoadedBinary) -> BinaryInfo {
    BinaryInfo {
        path: binary.path.clone(),
        format: binary.format.clone(),
        is_64bit: binary.is_64bit,
        entry_point: binary.entry_point,
        image_base: binary.image_base,
        function_count: binary.functions.len(),
        section_count: binary.sections.len(),
    }
}
