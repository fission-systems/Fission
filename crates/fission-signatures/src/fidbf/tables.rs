use super::parser::FidbfParseError;
use super::raw_db::{
    RawDbHandle, RawRecord, RawTableMeta, expect_byte, expect_int, expect_long, expect_short,
    expect_string, primary_tables_by_name,
};
use super::types::{FidbfDatabase, FidbfFunction, FidbfLibrary, FidbfRelation, FidbfRelationType};
use std::collections::HashMap;
use std::path::Path;

const LIBRARIES_TABLE: &str = "Libraries Table";
const STRINGS_TABLE: &str = "Strings Table";
const FUNCTIONS_TABLE: &str = "Functions Table";
const INFERIOR_TABLE: &str = "Inferior Table";
const SUPERIOR_TABLE: &str = "Superior Table";
const FID_SCHEMA_VERSION: i32 = 6;

const FLAG_HAS_TERMINATOR: u8 = 1;
const FLAG_AUTO_PASS: u8 = 2;
const FLAG_AUTO_FAIL: u8 = 4;
const FLAG_FORCE_SPECIFIC: u8 = 8;
const FLAG_FORCE_RELATION: u8 = 16;

pub(crate) fn parse_raw_fidbf_database(
    path: &Path,
    data: &[u8],
) -> Result<FidbfDatabase, FidbfParseError> {
    let handle = RawDbHandle::open(data)?;
    let tables = primary_tables_by_name(handle.master_tables()?);

    let strings = parse_strings(&handle, required_table(&tables, STRINGS_TABLE)?)?;
    let libraries = parse_libraries(&handle, required_table(&tables, LIBRARIES_TABLE)?)?;
    let functions = parse_functions(
        &handle,
        required_table(&tables, FUNCTIONS_TABLE)?,
        &strings,
    )?;
    let mut relations = parse_relation_table(
        &handle,
        required_table(&tables, INFERIOR_TABLE)?,
        FidbfRelationType::Inferior,
    )?;
    relations.extend(parse_relation_table(
        &handle,
        required_table(&tables, SUPERIOR_TABLE)?,
        FidbfRelationType::Superior,
    )?);

    if libraries.is_empty() {
        return Err(FidbfParseError::UnsupportedRawFidDatabase(
            "Libraries Table contains no records".to_string(),
        ));
    }
    if functions.is_empty() {
        return Err(FidbfParseError::UnsupportedRawFidDatabase(
            "Functions Table contains no records".to_string(),
        ));
    }

    Ok(FidbfDatabase::new(
        path.to_string_lossy().into_owned(),
        libraries,
        functions,
        relations,
    ))
}

fn required_table<'a>(
    tables: &'a HashMap<String, RawTableMeta>,
    name: &str,
) -> Result<&'a RawTableMeta, FidbfParseError> {
    tables.get(name).ok_or_else(|| {
        FidbfParseError::UnsupportedRawFidDatabase(format!("raw FID database missing {name}"))
    })
}

fn validate_version(table: &RawTableMeta, name: &str) -> Result<(), FidbfParseError> {
    if table.schema.version != FID_SCHEMA_VERSION {
        return Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
            "{name} schema version {} != expected {FID_SCHEMA_VERSION}",
            table.schema.version
        )));
    }
    Ok(())
}

fn validate_record_count(table: &RawTableMeta, actual: usize) -> Result<(), FidbfParseError> {
    if table.record_count >= 0 && table.record_count as usize != actual {
        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
            "{} record count metadata {} != decoded {}",
            table.name, table.record_count, actual
        )));
    }
    Ok(())
}

fn validate_field_names(table: &RawTableMeta, expected: &[&str]) -> Result<(), FidbfParseError> {
    if table.schema.field_names != expected {
        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
            "{} schema fields {:?} != expected {:?}",
            table.name, table.schema.field_names, expected
        )));
    }
    Ok(())
}

fn parse_strings(
    handle: &RawDbHandle<'_>,
    table: &RawTableMeta,
) -> Result<HashMap<i64, String>, FidbfParseError> {
    validate_version(table, STRINGS_TABLE)?;
    validate_field_names(table, &["String ID", "String Value"])?;
    if table.schema.field_types.len() != 1 {
        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
            "{STRINGS_TABLE} expected 1 field, found {}",
            table.schema.field_types.len()
        )));
    }

    let records = handle.read_table_records(table.root_buffer_id, &table.schema)?;
    validate_record_count(table, records.len())?;
    records
        .into_iter()
        .map(|record| {
            let value = expect_string(record.values.into_iter().next(), "string value")?;
            Ok((record.key, value))
        })
        .collect()
}

fn parse_libraries(
    handle: &RawDbHandle<'_>,
    table: &RawTableMeta,
) -> Result<Vec<FidbfLibrary>, FidbfParseError> {
    validate_version(table, LIBRARIES_TABLE)?;
    validate_field_names(
        table,
        &[
            "Library ID",
            "Library Family Name",
            "Library Version",
            "Library Variant",
            "Ghidra Version",
            "Ghidra Language ID",
            "Ghidra Language Version",
            "Ghidra Language Minor Version",
            "Ghidra Compiler Spec ID",
        ],
    )?;
    let records = handle.read_table_records(table.root_buffer_id, &table.schema)?;
    validate_record_count(table, records.len())?;
    records
        .into_iter()
        .map(decode_library)
        .collect()
}

fn decode_library(record: RawRecord) -> Result<FidbfLibrary, FidbfParseError> {
    let mut values = record.values.into_iter();
    Ok(FidbfLibrary {
        key: record.key,
        family_name: expect_string(values.next(), "library family name")?,
        version: expect_string(values.next(), "library version")?,
        variant: expect_string(values.next(), "library variant")?,
        ghidra_version: expect_string(values.next(), "ghidra version")?,
        language_id: expect_string(values.next(), "language id")?,
        language_version: expect_int(values.next(), "language version")?,
        language_minor_version: expect_int(values.next(), "language minor version")?,
        compiler_spec_id: expect_string(values.next(), "compiler spec id")?,
    })
}

fn parse_functions(
    handle: &RawDbHandle<'_>,
    table: &RawTableMeta,
    strings: &HashMap<i64, String>,
) -> Result<Vec<FidbfFunction>, FidbfParseError> {
    validate_version(table, FUNCTIONS_TABLE)?;
    validate_field_names(
        table,
        &[
            "Function ID",
            "Code Unit Size",
            "Full Hash",
            "Specific Hash Additional Size",
            "Specific Hash",
            "Library ID",
            "Name ID",
            "Entry Point",
            "Domain Path ID",
            "Flags",
        ],
    )?;
    let records = handle.read_table_records(table.root_buffer_id, &table.schema)?;
    validate_record_count(table, records.len())?;
    records
        .into_iter()
        .map(|record| decode_function(record, strings))
        .collect()
}

fn decode_function(
    record: RawRecord,
    strings: &HashMap<i64, String>,
) -> Result<FidbfFunction, FidbfParseError> {
    let mut values = record.values.into_iter();
    let code_unit_size = u32::try_from(expect_short(values.next(), "code unit size")?).map_err(
        |_| FidbfParseError::MalformedRawFidDatabase("negative code unit size".to_string()),
    )?;
    let full_hash = expect_long(values.next(), "full hash")? as u64;
    let specific_hash_additional_size =
        u8::try_from(expect_byte(values.next(), "specific hash additional size")?).map_err(
            |_| {
                FidbfParseError::MalformedRawFidDatabase(
                    "negative specific hash additional size".to_string(),
                )
            },
        )?;
    let specific_hash = expect_long(values.next(), "specific hash")? as u64;
    let library_id = expect_long(values.next(), "library id")?;
    let name_id = expect_long(values.next(), "name id")?;
    let entry_point = expect_long(values.next(), "entry point")? as u64;
    let domain_path_id = expect_long(values.next(), "domain path id")?;
    let flags = u8::try_from(expect_byte(values.next(), "flags")?).map_err(|_| {
        FidbfParseError::MalformedRawFidDatabase("negative function flags".to_string())
    })?;

    let name = strings.get(&name_id).cloned().ok_or_else(|| {
        FidbfParseError::MalformedRawFidDatabase(format!(
            "function {} references missing name string id {name_id}",
            record.key
        ))
    })?;
    let domain_path = strings.get(&domain_path_id).cloned().ok_or_else(|| {
        FidbfParseError::MalformedRawFidDatabase(format!(
            "function {} references missing domain path string id {domain_path_id}",
            record.key
        ))
    })?;

    Ok(FidbfFunction {
        key: record.key,
        library_id,
        name,
        full_hash,
        specific_hash,
        code_unit_size,
        entry_point,
        has_terminator: flags & FLAG_HAS_TERMINATOR != 0,
        specific_hash_additional_size,
        domain_path,
        flags,
        auto_pass: flags & FLAG_AUTO_PASS != 0,
        auto_fail: flags & FLAG_AUTO_FAIL != 0,
        force_specific: flags & FLAG_FORCE_SPECIFIC != 0,
        force_relation: flags & FLAG_FORCE_RELATION != 0,
    })
}

fn parse_relation_table(
    handle: &RawDbHandle<'_>,
    table: &RawTableMeta,
    relation_type: FidbfRelationType,
) -> Result<Vec<FidbfRelation>, FidbfParseError> {
    validate_version(table, &table.name)?;
    if !table.schema.field_types.is_empty() {
        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
            "{} expected key-only schema, found {} fields",
            table.name,
            table.schema.field_types.len()
        )));
    }

    let records = handle.read_table_records(table.root_buffer_id, &table.schema)?;
    validate_record_count(table, records.len())?;
    Ok(records
        .into_iter()
        .map(|record| FidbfRelation {
            function_id: record.key,
            related_id: 0,
            relation_type,
        })
        .collect())
}
