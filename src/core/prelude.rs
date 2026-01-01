//! Fission Prelude
//!
//! Common imports for convenience. Use with:
//! ```
//! use fission::core::prelude::*;
//! // or
//! use fission::prelude::*;
//! ```

// Re-export config
pub use super::config::CONFIG;

// Re-export error types
pub use super::errors::{FissionError, Result};
pub use crate::err;

// Re-export common analysis types
pub use crate::analysis::loader::{FunctionInfo, LoadedBinary, SectionInfo};

// Re-export common debug types
pub use crate::debug::types::{Breakpoint, DebugEvent, DebugStatus, RegisterState};

// Re-export common std types
pub use std::collections::{BTreeMap, HashMap, HashSet};
pub use std::path::{Path, PathBuf};
pub use std::sync::{Arc, Mutex, RwLock};

// Re-export common third-party types
pub use anyhow;
