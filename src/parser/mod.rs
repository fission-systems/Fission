use crate::core::prelude::*;
use crate::analysis::loader::types::LoadedBinary;

pub mod static_parser;
pub mod dynamic_parser;

/// Trait for binary parsers
pub trait BinaryParser {
    /// Parse binary data into a LoadedBinary structure
    fn parse(&self, data: Vec<u8>, path: String) -> Result<LoadedBinary>;
}
