//! Fission Prelude
//!
//! Common imports for convenience. Use with:
//! ```
//! use crate::prelude::*;
//! ```

// Re-export config
pub use crate::config::CONFIG;

// Re-export error types
pub use crate::errors::{FissionError, Result};
pub use crate::err;

// Re-export common analysis types
pub use crate::analysis::loader::{LoadedBinary, FunctionInfo, SectionInfo};

// Re-export common debug types
pub use crate::debug::types::{RegisterState, DebugEvent, DebugStatus, Breakpoint};

// Re-export common std types
pub use std::collections::{HashMap, HashSet, BTreeMap};
pub use std::sync::{Arc, Mutex, RwLock};
pub use std::path::{Path, PathBuf};

// Re-export common third-party types
pub use anyhow;
