//! Decompiler Module - Ghidra Integration
//!
//! Provides high-performance decompilation via Ghidra engine.
//!
//! ## Architecture
//!
//! The decompiler uses FFI bindings provided by `fission-ffi` crate
//! to communicate with the native Ghidra decompiler library.
//!
//! ```text
//! fission-analysis/decomp (safe wrapper)
//!         ↓
//! fission-ffi (unsafe FFI boundary)
//!         ↓
//! libdecomp.so (native Ghidra)
//! ```
//!
//! ## Usage
//!
//! Use the `fission-ffi` crate directly for decompilation:
//!
//! ```rust,ignore
//! use fission_ffi::DecompilerNative;
//!
//! let decomp = DecompilerNative::new(binary)?;
//! let result = decomp.decompile_function(addr)?;
//! ```

// NOTE: FFI bindings have been moved to fission-ffi crate
// This module now provides high-level safe wrappers only

pub mod cache;
#[cfg(feature = "native_decomp")]
mod caching_decompiler;
pub mod facts;
#[path = "nir/context.rs"]
pub(crate) mod nir_context;
#[path = "nir/engine.rs"]
pub mod nir_engine;
#[path = "nir/recovery.rs"]
pub mod nir_recovery;
#[path = "nir/render.rs"]
pub(crate) mod nir_render;
#[path = "nir/routing.rs"]
pub(crate) mod nir_routing;
#[path = "nir/taxonomy.rs"]
pub mod nir_taxonomy;
#[path = "nir/types.rs"]
pub(crate) mod nir_types;
#[path = "nir/worker.rs"]
pub mod nir_worker;
pub mod postprocess;
#[cfg(feature = "native_decomp")]
pub mod prepare;

#[cfg(feature = "native_decomp")]
pub use caching_decompiler::{CachingDecompiler, DecompilerNative, RecommendedDecompiler};
pub use facts::{FactProvenance, FactStore, FunctionFacts, NameFact, TypeFact, log_type_diag};
pub use nir_engine::{
    NirEngineMode, NirRoutingDecision, NirRoutingResolver, NirSelection, NirSource, NirSurfaceKind,
    auto_nir_eligible, classify_native_failure_kind, native_failure_routing_decision,
    nir_fallback_reason_with_kind, rescue_nir_output, rescue_nir_output_with_facts,
    select_nir_output, select_nir_output_from_pcode, select_nir_output_from_pcode_with_facts,
    select_nir_output_with_facts,
};
pub use nir_taxonomy::{classified_nir_error, classify_nir_failure, classify_nir_failure_refined};
pub use nir_types::{NirWorkerRequest, NirWorkerResponse};
pub use nir_worker::execute_nir_worker;
pub use postprocess::RustPostProcessOptions;

pub fn fallback_reason_with_kind(kind: &str, detail: impl AsRef<str>) -> String {
    format!("{kind}: {}", detail.as_ref())
}

#[cfg(feature = "native_decomp")]
pub use prepare::{
    PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_win_api_signatures_json,
};
