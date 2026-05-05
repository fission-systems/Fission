//! Plugin API - Interface exposed to plugins for interacting with Fission.

pub use crate::contracts::traits::PluginAPI;
pub use crate::contracts::types::{PluginInfo, PluginType};
pub use fission_core::common::types::BinaryInfo;

use fission_loader::loader::LoadedBinary;

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
