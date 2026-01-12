//! Decompilation Worker Pool - Multi-threaded decompilation with FFI.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerNative: Direct FFI to libdecomp (10-100x faster than subprocess)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support

mod requests;

#[cfg(feature = "native_decomp")]
mod native;
#[cfg(feature = "native_decomp")]
mod pool;
#[cfg(feature = "native_decomp")]
mod worker;
#[cfg(all(feature = "native_decomp", feature = "legacy_single_worker"))]
mod legacy;
#[cfg(not(feature = "native_decomp"))]
mod stub;

#[allow(unused_imports)]
pub use requests::{
    CfgAnalysisRequest, ClearCacheRequest, DecompileTask, LoadBinaryRequest, WorkerRequest,
};

#[cfg(feature = "native_decomp")]
pub use pool::spawn_worker;

#[cfg(not(feature = "native_decomp"))]
pub use stub::spawn_worker;
