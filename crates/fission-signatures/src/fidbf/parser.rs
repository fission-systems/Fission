use super::tables::parse_raw_fidbf_database;
use super::types::FidbfDatabase;
use flate2::read::DeflateDecoder;
use std::fs;
use std::io::Read;
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
    #[error("failed to unpack Java-packed FID database {0}: {1}")]
    PackedFidDatabaseUnpackError(String, String),
    #[error("legacy SQLite .fidbf databases are not supported by the Ghidra DBHandle reader: {0}")]
    UnsupportedSqliteFidDatabase(String),
    #[error("unsupported raw Ghidra FID database: {0}")]
    UnsupportedRawFidDatabase(String),
    #[error("malformed raw Ghidra FID database: {0}")]
    MalformedRawFidDatabase(String),
}

/// Magic bytes for Java object serialization streams used by Ghidra packed FID databases.
const JAVA_SERIAL_MAGIC: &[u8] = &[0xAC, 0xED, 0x00, 0x05];
/// ZIP local file entry signature.
const ZIP_LOCAL_SIG: &[u8] = b"PK\x03\x04";
/// ZIP data descriptor signature written after compressed data in streaming mode.
const ZIP_DATA_DESC_SIG: &[u8] = b"PK\x07\x08";

pub fn parse_fidbf(path: &Path) -> Result<FidbfDatabase, FidbfParseError> {
    let data = fs::read(path).map_err(|source| FidbfParseError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    if data.starts_with(b"SQLite format 3") {
        return Err(FidbfParseError::UnsupportedSqliteFidDatabase(
            path.display().to_string(),
        ));
    }

    if data.starts_with(JAVA_SERIAL_MAGIC) {
        let unpacked = unpack_java_fid_database(path, &data)?;
        return parse_raw_fidbf_database(path, &unpacked);
    }

    parse_raw_fidbf_database(path, &data)
}

/// Unpack a Java-serialized Ghidra packed FID database.
///
/// Layout: `[Java serialization header][PK\x03\x04 local file header for FOLDER_ITEM]
///          [DEFLATE-compressed LocalBufferFile][PK\x07\x08 data descriptor]`
///
/// Compression method is always DEFLATE (8). The data descriptor at the end of
/// the file holds the actual compressed size because the local header uses
/// streaming mode (flag bit 3 set) with sizes zeroed in the header.
fn unpack_java_fid_database(path: &Path, data: &[u8]) -> Result<Vec<u8>, FidbfParseError> {
    let err = |msg: String| {
        FidbfParseError::PackedFidDatabaseUnpackError(path.display().to_string(), msg)
    };

    // Locate the ZIP local file header (`PK\x03\x04`) embedded in the stream.
    let zip_off = data
        .windows(ZIP_LOCAL_SIG.len())
        .position(|w| w == ZIP_LOCAL_SIG)
        .ok_or_else(|| err("ZIP local file header not found".into()))?;

    // Parse the 26-byte fixed part of the local file header.
    // Layout: PK\x03\x04 (4) | version(2) | flags(2) | comp(2) | mtime(2) | mdate(2)
    //         | crc(4) | csz(4) | usz(4) | namelen(2) | extralen(2)
    let lh_off = zip_off + 4;
    if data.len() < lh_off + 26 {
        return Err(err("truncated ZIP local file header".into()));
    }
    let comp = u16::from_le_bytes([data[lh_off + 4], data[lh_off + 5]]);
    if comp != 8 {
        return Err(err(format!(
            "unsupported ZIP compression method {comp} (expected DEFLATE=8)"
        )));
    }
    let namelen = u16::from_le_bytes([data[lh_off + 22], data[lh_off + 23]]) as usize;
    let extralen = u16::from_le_bytes([data[lh_off + 24], data[lh_off + 25]]) as usize;
    let data_start = lh_off + 26 + namelen + extralen;
    if data_start > data.len() {
        return Err(err("ZIP local file header extends past end of file".into()));
    }

    // Find the data descriptor `PK\x07\x08` that follows the compressed data.
    // It is at the end of the file: [PK\x07\x08(4)][crc(4)][csz(4)][usz(4)] = 16 bytes.
    let dd_off = data
        .windows(ZIP_DATA_DESC_SIG.len())
        .rposition(|w| w == ZIP_DATA_DESC_SIG)
        .ok_or_else(|| err("ZIP data descriptor (PK\\x07\\x08) not found".into()))?;
    if data.len() < dd_off + 16 {
        return Err(err("truncated ZIP data descriptor".into()));
    }
    let csz = u32::from_le_bytes(data[dd_off + 8..dd_off + 12].try_into().expect("slice")) as usize;

    if data_start + csz > dd_off {
        return Err(err(format!(
            "compressed size {csz} overruns data descriptor at offset {dd_off}"
        )));
    }

    // Inflate the raw DEFLATE stream.
    let compressed = &data[data_start..data_start + csz];
    let mut decoder = DeflateDecoder::new(compressed);
    let mut unpacked = Vec::new();
    decoder
        .read_to_end(&mut unpacked)
        .map_err(|e| err(format!("DEFLATE decompression failed: {e}")))?;

    Ok(unpacked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::resources::ResourceProvider;
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
        assert!(matches!(
            err,
            FidbfParseError::UnsupportedSqliteFidDatabase(_)
        ));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn detects_raw_ghidra_fidbf() {
        let Some(path) = ResourceProvider::global()
            .paths()
            .get_fid_path(true, Some("vs2019"))
        else {
            return;
        };
        if !path.exists() {
            return;
        }
        let parsed = parse_fidbf(&path).expect("parse raw fidbf records");
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
    fn packed_fidb_with_bad_content_is_unpack_error() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("packed_stub_{ts}.fidb"));
        // Write Java serialization magic followed by garbage — must fail gracefully.
        let mut stub = b"\xac\xed\x00\x05".to_vec();
        stub.extend_from_slice(b"not a real packed database");
        std::fs::write(&path, &stub).expect("write stub fidb");
        let err = parse_fidbf(&path).expect_err("invalid packed fidb must return error");
        assert!(matches!(
            err,
            FidbfParseError::PackedFidDatabaseUnpackError(..)
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_packed_gcc_fidbf() {
        let Some(fid_dir) = ResourceProvider::global().paths().fid_dir.clone() else {
            return;
        };
        let path = fid_dir.join("gcc-x86.LE.64.default.fidbf");
        if !path.exists() {
            return;
        }
        let parsed = parse_fidbf(&path).expect("parse Java-packed gcc fidbf");
        assert!(!parsed.libraries.is_empty(), "no libraries parsed");
        assert!(!parsed.functions.is_empty(), "no functions parsed");
        assert!(
            parsed
                .libraries
                .iter()
                .any(|l| l.language_id.contains("x86")),
            "expected x86 library entry"
        );
    }
}
