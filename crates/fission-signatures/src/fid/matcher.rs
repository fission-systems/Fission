use super::hash::{FidHashError, FidHashQuad, FidHashUnit, FidHasher};
use crate::fidbf::{FidbfDatabase, FidbfMatch, FidbfParseError, parse_fidbf};
use fission_core::PATHS;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FidMatchError {
    #[error(transparent)]
    Hash(#[from] FidHashError),
    #[error(transparent)]
    Database(#[from] FidbfParseError),
}

#[derive(Debug, Clone, Default)]
pub struct FidFunctionView {
    pub units: Vec<FidHashUnit>,
}

#[derive(Debug, Clone, Default)]
pub struct FidRelocationView;

#[derive(Debug, Default)]
pub struct FidDatabaseSet {
    pub databases: Vec<FidbfDatabase>,
    pub errors: Vec<(PathBuf, FidbfParseError)>,
}

impl FidDatabaseSet {
    pub fn discover_for_load_spec(
        language_id: Option<&str>,
        compiler_id: Option<&str>,
        format: Option<&str>,
        is_64bit: bool,
    ) -> Self {
        let paths = PATHS.get_preferred_fid_paths(is_64bit, format, compiler_id);
        let mut databases = Vec::new();
        let mut errors = Vec::new();
        for path in paths {
            match parse_fidbf(&path) {
                Ok(database) => {
                    if let Some(language_id) = language_id {
                        let has_matching_language = database.libraries.iter().any(|library| {
                            library.language_id.is_empty() || library.language_id == language_id
                        });
                        if !has_matching_language {
                            continue;
                        }
                    }
                    databases.push(database);
                }
                Err(error) => errors.push((path, error)),
            }
        }
        Self { databases, errors }
    }
}

#[derive(Debug)]
pub struct FidMatcher {
    hasher: FidHasher,
    databases: FidDatabaseSet,
}

impl FidMatcher {
    pub fn new(databases: FidDatabaseSet) -> Self {
        Self {
            hasher: FidHasher::default(),
            databases,
        }
    }

    pub fn identify_function(
        &self,
        function: &FidFunctionView,
        _relocations: &FidRelocationView,
    ) -> Result<Vec<FidbfMatch>, FidMatchError> {
        let FidHashQuad {
            full_hash,
            specific_hash,
            ..
        } = self.hasher.hash(&function.units)?;
        let mut matches = Vec::new();
        for database in &self.databases.databases {
            matches.extend(database.identify_by_hashes(full_hash, specific_hash));
        }
        Ok(matches)
    }
}
