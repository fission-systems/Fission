use super::types::{FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfRelation, FidbfRelationType};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FidbfParseError {
    #[error("failed to open .fidbf file: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("invalid .fidbf schema: {0}")]
    Schema(String),
}

pub fn parse_fidbf(path: &Path) -> Result<FidbfDatabase, FidbfParseError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    validate_schema(&connection)?;

    let libraries = parse_libraries(&connection)?;
    let functions = parse_functions(&connection)?;
    let relations = parse_relations(&connection)?;

    Ok(FidbfDatabase {
        source_path: path.to_string_lossy().into_owned(),
        libraries,
        functions,
        relations,
    })
}

fn validate_schema(connection: &Connection) -> Result<(), FidbfParseError> {
    for table in ["Libraries", "Functions", "Relations"] {
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )?;

        if count == 0 {
            return Err(FidbfParseError::Schema(format!(
                "required table '{table}' was not found"
            )));
        }
    }

    Ok(())
}

fn parse_libraries(connection: &Connection) -> Result<Vec<FidbfLibrary>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT key, libraryFamilyName, libraryVersion, libraryVariant, ghidraVersion, languageID
         FROM Libraries",
    )?;

    let rows = statement.query_map([], |row| {
        Ok(FidbfLibrary {
            key: row.get(0)?,
            family_name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            version: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            variant: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            ghidra_version: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            language_id: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        })
    })?;

    rows.collect()
}

fn parse_functions(connection: &Connection) -> Result<Vec<FidbfFunction>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT key, libraryID, name, fullHash, specificHash, codeUnitSize, entryPoint, hasTerminator
         FROM Functions",
    )?;

    let rows = statement.query_map([], |row| {
        let full_hash = row.get::<_, Option<i64>>(3)?.unwrap_or_default() as u64;
        let specific_hash = row.get::<_, Option<i64>>(4)?.unwrap_or_default() as u64;
        let code_unit_size = row.get::<_, Option<i64>>(5)?.unwrap_or_default() as u32;
        let entry_point = row.get::<_, Option<i64>>(6)?.unwrap_or_default() as u64;
        let has_terminator = row.get::<_, Option<i64>>(7)?.unwrap_or_default() != 0;

        Ok(FidbfFunction {
            key: row.get(0)?,
            library_id: row.get(1)?,
            name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            full_hash,
            specific_hash,
            code_unit_size,
            entry_point,
            has_terminator,
        })
    })?;

    rows.collect()
}

fn parse_relations(connection: &Connection) -> Result<Vec<FidbfRelation>, rusqlite::Error> {
    let mut statement =
        connection.prepare("SELECT functionID, relatedID, relationType FROM Relations")?;

    let rows = statement.query_map([], |row| {
        Ok(FidbfRelation {
            function_id: row.get(0)?,
            related_id: row.get(1)?,
            relation_type: FidbfRelationType::from(
                row.get::<_, Option<i32>>(2)?.unwrap_or_default(),
            ),
        })
    })?;

    rows.collect()
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
    fn parses_minimal_valid_fidbf() {
        let path = temp_db_path("minimal_valid");
        let connection = Connection::open(&path).expect("create sqlite file");

        connection
            .execute_batch(
                "
                CREATE TABLE Libraries (
                    key INTEGER PRIMARY KEY,
                    libraryFamilyName TEXT,
                    libraryVersion TEXT,
                    libraryVariant TEXT,
                    ghidraVersion TEXT,
                    languageID TEXT
                );
                CREATE TABLE Functions (
                    key INTEGER PRIMARY KEY,
                    libraryID INTEGER,
                    name TEXT,
                    fullHash INTEGER,
                    specificHash INTEGER,
                    codeUnitSize INTEGER,
                    entryPoint INTEGER,
                    hasTerminator INTEGER
                );
                CREATE TABLE Relations (
                    functionID INTEGER,
                    relatedID INTEGER,
                    relationType INTEGER
                );
                INSERT INTO Libraries VALUES (1, 'VS2019', '14.2', 'x64', '11.4.2', 'x86:LE:64:default');
                INSERT INTO Functions VALUES (10, 1, 'memcpy', 11, 22, 32, 4096, 1);
                INSERT INTO Relations VALUES (10, 10, 0);
                ",
            )
            .expect("seed schema");
        drop(connection);

        let parsed = parse_fidbf(&path).expect("parse valid fidbf");
        assert_eq!(parsed.libraries.len(), 1);
        assert_eq!(parsed.functions.len(), 1);
        assert_eq!(parsed.relations.len(), 1);
        assert_eq!(parsed.functions[0].name, "memcpy");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_missing_required_tables() {
        let path = temp_db_path("missing_tables");
        let connection = Connection::open(&path).expect("create sqlite file");
        connection
            .execute_batch("CREATE TABLE Dummy (key INTEGER PRIMARY KEY);")
            .expect("create dummy schema");
        drop(connection);

        let result = parse_fidbf(&path);
        assert!(matches!(result, Err(FidbfParseError::Schema(_))));

        let _ = std::fs::remove_file(path);
    }
}
