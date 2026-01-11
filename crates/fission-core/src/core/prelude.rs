//! Fission Prelude
//!
//! Common imports for convenience. Use with:
//! ```
//! use fission_core::core::prelude::*;
//! // or
//! use fission_core::prelude::*;
//! ```

// Re-export config
pub use super::config::CONFIG;

// Re-export error types
pub use super::errors::{FissionError, Result};
pub use super::models::QuickPatch;
pub use crate::err;

// Re-export settings types
pub use crate::core::settings::{SettingsState, ThemeMode};

// Re-export common std types
pub use std::collections::{BTreeMap, HashMap, HashSet};
pub use std::path::{Path, PathBuf};
pub use std::sync::{Arc, Mutex, RwLock};

// Re-export common third-party types
pub use anyhow;
