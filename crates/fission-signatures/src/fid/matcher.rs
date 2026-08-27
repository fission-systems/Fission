use crate::fidbf::{
    FidbfDatabase, FidbfMatch, FidbfParseError, fpk_store::LazyFidDatabase, parse_fidbf,
};
use fission_core::resources::ResourceProvider;
use std::path::PathBuf;

/// One database, however it was opened.
///
/// `Lazy` answers from the `.fpk` hash index without materialising anything;
/// `Eager` is the parsed `.fidbf`, still used where the packed tables are
/// absent or lack an index.
pub enum FidDatabase {
    Lazy(LazyFidDatabase),
    Eager(Box<FidbfDatabase>),
}

impl FidDatabase {
    pub fn identify_by_hashes(&self, full_hash: u64, specific_hash: u64) -> Vec<FidbfMatch> {
        match self {
            Self::Lazy(db) => db.identify_by_hashes(full_hash, specific_hash),
            Self::Eager(db) => db.identify_by_hashes(full_hash, specific_hash),
        }
    }

    pub fn source_path(&self) -> &str {
        match self {
            Self::Lazy(db) => db.source_path(),
            Self::Eager(db) => &db.source_path,
        }
    }
}

impl std::fmt::Debug for FidDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never materialises: formatting one for a log should not decode a table.
        let kind = match self {
            Self::Lazy(_) => "lazy",
            Self::Eager(_) => "eager",
        };
        write!(f, "FidDatabase({kind}, {})", self.source_path())
    }
}

#[derive(Debug, Default)]
pub struct FidDatabaseSet {
    pub databases: Vec<FidDatabase>,
    pub errors: Vec<(PathBuf, FidbfParseError)>,
}

impl FidDatabaseSet {
    pub fn discover_for_load_spec(
        language_id: Option<&str>,
        compiler_id: Option<&str>,
        format: Option<&str>,
        is_64bit: bool,
        processor: Option<&str>,
    ) -> Self {
        let paths = ResourceProvider::global().paths().get_preferred_fid_paths(
            is_64bit,
            format,
            compiler_id,
            processor,
        );
        let mut databases = Vec::new();
        let mut errors = Vec::new();
        for path in paths {
            // The packed tables answer `identify_by_hashes` from an index; the
            // `.fidbf` has to be decoded and indexed in full first, 65ms before
            // the first query. Language is checked the same way in both, from
            // library rows, which the lazy form reads once on open.
            if let Some(lazy) = LazyFidDatabase::open(&path) {
                if language_id.is_none_or(|id| lazy.has_language(id)) {
                    databases.push(FidDatabase::Lazy(lazy));
                }
                continue;
            }
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
                    databases.push(FidDatabase::Eager(Box::new(database)));
                }
                Err(error) => errors.push((path, error)),
            }
        }
        Self { databases, errors }
    }
}
