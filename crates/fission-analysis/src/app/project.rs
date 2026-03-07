//! Analysis Project Persistence - Saving and loading user analysis data
//!
//! This module provides functionality for persisting user-defined analysis data
//! (comments, renamed symbols, etc.) separately from the binary data.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents a persistent analysis session/project
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AnalysisProject {
    /// Hash of the binary this project belongs to (for verification)
    pub binary_hash: String,
    /// Original binary path
    pub binary_path: String,
    /// User-defined function names (address -> name)
    pub user_function_names: HashMap<u64, String>,
    /// User-defined comments (address -> comment)
    pub user_comments: HashMap<u64, String>,
    /// Bookmarked addresses: addr -> label
    #[serde(default)]
    pub bookmarks: HashMap<u64, String>,
    /// Future: xrefs filters, etc.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl AnalysisProject {
    /// Create a new project for a binary
    pub fn new(binary_hash: String, binary_path: String) -> Self {
        Self {
            binary_hash,
            binary_path,
            user_function_names: HashMap::new(),
            user_comments: HashMap::new(),
            bookmarks: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Save project to a JSON file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("Failed to serialize project")?;
        fs::write(path, json).context("Failed to write project file")?;
        Ok(())
    }

    /// Load project from a JSON file
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path).context("Failed to read project file")?;
        let project: AnalysisProject =
            serde_json::from_str(&json).context("Failed to parse project JSON")?;
        Ok(project)
    }
}
