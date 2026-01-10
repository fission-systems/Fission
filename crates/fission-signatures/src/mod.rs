//! CRT Function Signature Database
//!
//! FLIRT-style pattern matching for recognizing CRT and standard library functions.
//! This helps the decompiler identify known functions without debug symbols.

pub mod win_api;
pub mod win_constants;
pub mod win_types;

mod signature;
mod database;
mod msvc_sigs;

// Re-export main types
pub use signature::FunctionSignature;
pub use database::SignatureDatabase;

// Re-export lazily-initialized global databases for efficient reuse
pub use win_api::WIN_API_DB;
pub use win_constants::WIN_CONSTANTS_DB;
