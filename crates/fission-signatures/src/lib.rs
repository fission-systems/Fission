//! Fission Signatures — loaders, parsers, providers, and matchers for
//! Ghidra-style Function ID and type/signature facts.
//!
//! **Data ownership:** canonical imported/generated WinAPI/type/FID/signature
//! files live under the repository signatures directory (see [`utils/MANIFEST.md`](../../../utils/MANIFEST.md)). This crate reads them at
//! runtime via [`fission_core::resources::ResourceProvider`] / [`fission_core::PATHS`]; do not add duplicate corpora under
//! `crates/fission-signatures/data/` (CI-enforced).

// This crate carries imported/generated signature and FIDBF data where exact
// constants, casts, relation names, and table-shaped code are often intentional.
// Keep CI clippy useful without making data-resource style noise fail the build.
#![allow(clippy::pedantic)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_contains)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::unnecessary_sort_by)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod api_types;
pub mod fid;
pub mod fidbf;
pub mod import_flat;
pub mod provider;
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

pub static SIGNATURE_DB: std::sync::LazyLock<SignatureDatabase> =
    std::sync::LazyLock::new(SignatureDatabase::new);

pub use api_types::{ApiSignature, ApiTypeDatabase, ApiTypeError, ParamInfo};
pub use fid::{
    FidDatabaseSet, FidFunctionView, FidHashError, FidHashQuad, FidHashUnit, FidHasher,
    FidInstructionOperand, FidMatchError, FidMatcher, FidOperandValue, FidRelocationView,
    dissect_x86_function_to_fid_units,
};
pub use fidbf::{
    FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfMatch, FidbfRelation, discover_fidbf_paths,
    parse_all_fidbf_for_arch, parse_fidbf,
};
pub use import_flat::symbol_for_win_api_database_lookup;
pub use provider::{SIGNATURE_RESOURCES, SignatureResourceProvider};
pub use win_constants::WIN_CONSTANTS_DB;
