use super::fpk_store;
use super::parser::{FidbfParseError, parse_fidbf};
use super::types::FidbfDatabase;
use fission_core::resources::ResourceProvider;
use std::path::{Path, PathBuf};

/// Discover candidate `.fidbf` database paths for the target architecture.
pub fn discover_fidbf_paths(is_64bit: bool) -> Vec<PathBuf> {
    ResourceProvider::global()
        .paths()
        .get_all_fid_paths(is_64bit)
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
        // What ships is the packed pair, not the `.fidbf` the path names --
        // path resolution counts a database present when `<stem>.fn.fpk` is
        // there. Decoding it back yields the same `FidbfDatabase` the parser
        // would have produced, so callers holding an eager database (the
        // `identify` CLI path) keep working in a tree without `utils/source/`.
        if let Some(database) = decode_packed(&path) {
            databases.push(database);
            continue;
        }
        match parse_fidbf(&path) {
            Ok(database) => databases.push(database),
            Err(error) => errors.push((path, error)),
        }
    }

    (databases, errors)
}

/// Rebuild a database from the four packed tables beside `path`, if they exist.
fn decode_packed(path: &Path) -> Option<FidbfDatabase> {
    let stem = path.file_stem()?.to_str()?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let table = |suffix: &str| dir.join(format!("{stem}.{suffix}.fpk"));
    let (lib, func, rel, dom) = (
        table("lib"),
        table("fn"),
        table("rel"),
        table("dom"),
    );
    if !func.exists() {
        return None;
    }
    fpk_store::decode(path.display().to_string(), &lib, &func, &rel, &dom).ok()
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

    /// The `|| !errors.is_empty()` this used to allow made the test pass while
    /// every database failed to load, which is exactly what happened when the
    /// `.fidbf` moved to `utils/source/`: `identify` returned zero matches on
    /// every binary and no test noticed. Loading has to actually succeed.
    #[test]
    fn parses_all_fidbf_for_arch_without_hard_failure() {
        let (databases, errors) = parse_all_fidbf_for_arch(true);

        assert!(
            !databases.is_empty(),
            "no FID database loaded; errors: {errors:?}"
        );

        for database in &databases {
            assert!(!database.source_path.is_empty());
            assert!(!database.libraries.is_empty() || !database.functions.is_empty());
        }
    }
}
