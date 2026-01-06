use super::BinaryParser;
use crate::analysis::loader::types::LoadedBinary;
use crate::prelude::*;

/// Dynamic Parser intended for handling memory dumps, running processes,
/// and obfuscated binaries where standard static parsers (like goblin) might fail.
///
/// This parser should be more lenient and capable of reconstructing headers
/// from memory structures.
pub struct DynamicParser;

impl DynamicParser {
    pub fn new() -> Self {
        Self
    }
}

impl BinaryParser for DynamicParser {
    fn parse(&self, data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Use the new DebugEngine loader for dynamic parsing
        // This simulates the OS loader to provide an accurate memory view
        crate::core::logging::info("Using DebugEngine for dynamic parsing...");

        let loader = crate::unpacker::TitanLoader::new();
        loader.load(&data, &path)
    }
}
