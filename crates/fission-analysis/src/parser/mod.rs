use crate::analysis::loader::types::LoadedBinary;
use crate::prelude::*;

pub mod dynamic_parser;
pub mod static_parser;

/// Trait for binary parsers
pub trait BinaryParser {
    /// Parse binary data into a LoadedBinary structure
    fn parse(&self, data: Vec<u8>, path: String) -> Result<LoadedBinary>;
}
