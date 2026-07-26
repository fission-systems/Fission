mod convert;
mod function;
mod prehir;

pub use convert::*;
pub use function::*;
pub use prehir::*;

/// Convenience re-exports of genuinely shared substrate types (no embedded
/// AST) from `fission-midend-core`, so callers within this crate can write
/// `crate::ir::NirType` etc. without needing to know which crate a given
/// type actually lives in.
pub use fission_midend_core::ir::{DecompFacts, NirBinding, NirBuildStats, NirType};
