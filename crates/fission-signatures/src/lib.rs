//! Fission Signatures - Function signature database
//!
//! This crate provides a database of function signatures for common libraries
//! including Windows API, MSVC runtime, and other standard libraries.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod fidbf;
pub mod win_api;
pub mod win_constants;
pub mod win_types;

mod database;
mod msvc_sigs;
pub mod relation;
mod signature;

pub mod prelude;

// Re-export main types
pub use database::{IdentifyResult, SignatureDatabase};
pub use relation::{CallGraph, RelationValidation, validate_relation};
pub use signature::FunctionSignature;

// Re-export lazily-initialized global databases for efficient reuse
pub use fidbf::{
    FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfRelation, discover_fidbf_paths,
    parse_all_fidbf_for_arch, parse_fidbf,
};
pub use win_api::WIN_API_DB;
pub use win_constants::WIN_CONSTANTS_DB;
