use super::parser::{FidbfParseError, parse_fidbf};
use super::types::FidbfDatabase;
use fission_core::PATHS;
use std::path::PathBuf;

/// Discover candidate `.fidbf` database paths for the target architecture.
pub fn discover_fidbf_paths(is_64bit: bool) -> Vec<PathBuf> {
    PATHS.get_all_fid_paths(is_64bit)
}

/// Parse all discovered `.fidbf` databases for the target architecture.
///
/// Returns `(parsed_databases, parse_errors)` so callers can continue
/// even if some files are invalid.
pub fn parse_all_fidbf_for_arch(
    is_64bit: bool,
) -> (Vec<FidbfDatabase>, Vec<(PathBuf, FidbfParseError)>) {
    let mut databases = Vec::new();
    let mut errors = Vec::new();

    for path in discover_fidbf_paths(is_64bit) {
        match parse_fidbf(&path) {
            Ok(database) => databases.push(database),
            Err(error) => errors.push((path, error)),
        }
    }

    (databases, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_fidbf_paths_for_arch() {
        let x64 = discover_fidbf_paths(true);
        let x86 = discover_fidbf_paths(false);

        assert!(
            !x64.is_empty() || !x86.is_empty(),
            "expected at least one FID database path to be discovered"
        );
    }

    #[test]
    fn parses_all_fidbf_for_arch_without_hard_failure() {
        let (databases, errors) = parse_all_fidbf_for_arch(true);

        assert!(
            !databases.is_empty() || !errors.is_empty(),
            "expected at least one parse attempt"
        );

        for database in &databases {
            assert!(!database.source_path.is_empty());
            assert!(!database.libraries.is_empty() || !database.functions.is_empty());
        }
    }
}
