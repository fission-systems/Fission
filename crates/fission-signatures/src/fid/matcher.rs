use crate::fidbf::{FidbfDatabase, FidbfParseError, parse_fidbf};
use fission_core::resources::ResourceProvider;
use std::path::PathBuf;

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
        let paths = ResourceProvider::global().paths().get_preferred_fid_paths(
            is_64bit,
            format,
            compiler_id,
        );
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
