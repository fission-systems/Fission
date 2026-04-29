use super::tables::parse_raw_fidbf_database;
use super::types::FidbfDatabase;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FidbfParseError {
    #[error("failed to read FID database file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid .fidbf schema: {0}")]
    Schema(String),
    #[error("packed .fidb databases are not supported by the raw FID reader: {0}")]
    UnsupportedPackedFidDatabase(String),
    #[error("legacy SQLite .fidbf databases are not supported by the Ghidra DBHandle reader: {0}")]
    UnsupportedSqliteFidDatabase(String),
    #[error("unsupported raw Ghidra FID database: {0}")]
    UnsupportedRawFidDatabase(String),
    #[error("malformed raw Ghidra FID database: {0}")]
    MalformedRawFidDatabase(String),
}

pub fn parse_fidbf(path: &Path) -> Result<FidbfDatabase, FidbfParseError> {
    if path.extension().is_some_and(|ext| ext == "fidb") {
        return Err(FidbfParseError::UnsupportedPackedFidDatabase(
            path.display().to_string(),
        ));
    }

    let header = fs::read(path).map_err(|source| FidbfParseError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    if header.starts_with(b"SQLite format 3") {
        return Err(FidbfParseError::UnsupportedSqliteFidDatabase(
            path.display().to_string(),
        ));
    }

    parse_raw_fidbf_database(path, &header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(file_name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{file_name}_{ts}.fidbf"))
    }

    #[test]
    fn sqlite_fidbf_is_typed_unsupported() {
        let path = temp_db_path("sqlite_legacy");
        std::fs::write(&path, b"SQLite format 3\0legacy").expect("write sqlite marker");

        let err = parse_fidbf(&path).expect_err("sqlite fidbf should be typed unsupported");
        assert!(matches!(err, FidbfParseError::UnsupportedSqliteFidDatabase(_)));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn detects_raw_ghidra_fidbf() {
        let path = std::path::Path::new("../../utils/signatures/fid/vs2019_x64.fidbf");
        if !path.exists() {
            return;
        }
        let parsed = parse_fidbf(path).expect("parse raw fidbf records");
        assert!(!parsed.libraries.is_empty());
        assert!(!parsed.functions.is_empty());
        assert_eq!(parsed.libraries[0].language_id, "x86:LE:64:default");
        assert!(
            parsed
                .libraries
                .iter()
                .any(|library| !library.compiler_spec_id.is_empty())
        );
        let function = parsed
            .functions
            .iter()
            .find(|function| !function.name.is_empty())
            .expect("named function record");
        assert!(function.code_unit_size > 0);
        assert!(!function.domain_path.is_empty());
        assert!(function.full_hash != 0 || function.specific_hash != 0);
    }

    #[test]
    fn marker_only_raw_fidbf_is_not_success() {
        let path = temp_db_path("marker_only_raw");
        std::fs::write(
            &path,
            b"Libraries Table\0Strings Table\0Functions Table\0Inferior Table\0Superior Table",
        )
        .expect("write marker-only fixture");

        let err = parse_fidbf(&path).expect_err("marker-only raw fidbf must fail closed");
        assert!(matches!(
            err,
            FidbfParseError::UnsupportedRawFidDatabase(_)
                | FidbfParseError::MalformedRawFidDatabase(_)
        ));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn packed_fidb_is_typed_unsupported() {
        let path = std::path::Path::new("../../utils/signatures/fidb_java/gcc-x86.LE.64.default.fidb");
        if !path.exists() {
            return;
        }
        let err = parse_fidbf(path).expect_err("packed fidb should be typed unsupported");
        assert!(matches!(err, FidbfParseError::UnsupportedPackedFidDatabase(_)));
    }
}
