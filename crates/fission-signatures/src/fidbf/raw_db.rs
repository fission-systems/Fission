use super::parser::FidbfParseError;
use super::raw_fields::{RawFieldType, RawValue, checked_slice, read_i32, read_i64, read_value};
use std::collections::{HashMap, HashSet};

const LOCAL_BUFFER_FILE_MAGIC: u64 = 0x2f30_312c_3429_2c2a;
const LOCAL_BUFFER_FILE_VERSION: i32 = 1;
const BLOCK_PREFIX_SIZE: usize = 5;

const NODE_LONGKEY_INTERIOR: u8 = 0;
const NODE_LONGKEY_VAR_REC: u8 = 1;
const NODE_LONGKEY_FIXED_REC: u8 = 2;
const NODE_CHAINED_BUFFER_INDEX: u8 = 8;
const NODE_CHAINED_BUFFER_DATA: u8 = 9;

#[derive(Debug, Clone)]
pub(crate) struct RawSchema {
    pub(crate) version: i32,
    pub(crate) key_type: RawFieldType,
    pub(crate) field_types: Vec<RawFieldType>,
    pub(crate) field_names: Vec<String>,
}

impl RawSchema {
    pub(crate) fn fixed_record_size(&self) -> Option<usize> {
        self.field_types
            .iter()
            .try_fold(0usize, |acc, ty| ty.fixed_size().map(|size| acc + size))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RawRecord {
    pub(crate) key: i64,
    pub(crate) values: Vec<RawValue>,
}

#[derive(Debug, Clone)]
pub(crate) struct RawTableMeta {
    pub(crate) name: String,
    pub(crate) schema: RawSchema,
    pub(crate) root_buffer_id: i32,
    pub(crate) index_column: i32,
    pub(crate) record_count: i32,
}

#[derive(Debug)]
struct RawBuffer<'a> {
    data: &'a [u8],
}

pub(crate) struct RawDbHandle<'a> {
    bytes: &'a [u8],
    block_size: usize,
    buffer_size: usize,
    buffer_count: i32,
}

impl<'a> RawDbHandle<'a> {
    pub(crate) fn open(bytes: &'a [u8]) -> Result<Self, FidbfParseError> {
        let magic = u64::from_be_bytes(
            checked_slice(bytes, 0, 8)?
                .try_into()
                .expect("slice length"),
        );
        if magic != LOCAL_BUFFER_FILE_MAGIC {
            return Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
                "missing LocalBufferFile magic 0x{LOCAL_BUFFER_FILE_MAGIC:016x}"
            )));
        }

        let version = read_i32(bytes, 16)?;
        if version != LOCAL_BUFFER_FILE_VERSION {
            return Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
                "unsupported LocalBufferFile format version {version}"
            )));
        }

        let block_size = usize::try_from(read_i32(bytes, 20)?).map_err(|_| {
            FidbfParseError::MalformedRawFidDatabase("negative block size".to_string())
        })?;
        if block_size <= BLOCK_PREFIX_SIZE {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "invalid block size {block_size}"
            )));
        }
        if bytes.len() < block_size {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "file shorter than header block size {block_size}"
            )));
        }
        let buffer_count = i32::try_from(bytes.len() / block_size - 1).map_err(|_| {
            FidbfParseError::MalformedRawFidDatabase("buffer count overflow".to_string())
        })?;
        if buffer_count <= 0 {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "invalid buffer count {buffer_count}"
            )));
        }

        Ok(Self {
            bytes,
            block_size,
            buffer_size: block_size - BLOCK_PREFIX_SIZE,
            buffer_count,
        })
    }

    pub(crate) fn master_tables(&self) -> Result<Vec<RawTableMeta>, FidbfParseError> {
        let root_buffer_id = self.read_db_parm(0)?;
        let schema = master_table_schema()?;
        let records = self.read_table_records(root_buffer_id, &schema)?;
        records
            .into_iter()
            .map(|record| self.decode_table_meta(record))
            .collect()
    }

    pub(crate) fn read_table_records(
        &self,
        root_buffer_id: i32,
        schema: &RawSchema,
    ) -> Result<Vec<RawRecord>, FidbfParseError> {
        // A table Ghidra never wrote a record to has no root buffer. Its own
        // `TableRecord` sets the column to -1 with the comment "first buffer
        // not yet allocated", so this is an empty table and not damage.
        //
        // Reading it as a buffer id rejected two of the corpus's 57 databases
        // outright -- gcc-MIPS.BE.32.default and gcc-avr8.LE.16.extended, both
        // about 100KB where a populated one is megabytes -- and because
        // `discover_for_load_spec` collects parse failures into `errors` rather
        // than surfacing them, MIPS and AVR targets have simply had no FID
        // matching.
        if root_buffer_id < 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let mut visited = HashSet::new();
        self.collect_records(root_buffer_id, schema, &mut visited, &mut out)?;
        Ok(out)
    }

    fn read_db_parm(&self, index: usize) -> Result<i32, FidbfParseError> {
        let buffer = self.read_buffer(0)?;
        let node_type = node_type(buffer.data)?;
        if node_type != NODE_CHAINED_BUFFER_DATA {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "DBParms buffer has node type {node_type}, expected chained data"
            )));
        }
        let data_len = usize::try_from(read_i32(buffer.data, 1)? & i32::MAX).map_err(|_| {
            FidbfParseError::MalformedRawFidDatabase("negative DBParms length".to_string())
        })?;
        let parm_offset = 6usize
            .checked_add(index.checked_mul(4).ok_or_else(|| {
                FidbfParseError::MalformedRawFidDatabase("DBParms index overflow".to_string())
            })?)
            .ok_or_else(|| {
                FidbfParseError::MalformedRawFidDatabase("DBParms offset overflow".to_string())
            })?;
        if parm_offset + 4 > 5 + data_len {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "DBParms index {index} out of bounds"
            )));
        }
        read_i32(buffer.data, parm_offset)
    }

    fn decode_table_meta(&self, record: RawRecord) -> Result<RawTableMeta, FidbfParseError> {
        let mut values = record.values.into_iter();
        let name = expect_string(values.next(), "table name")?;
        let version = expect_int(values.next(), "schema version")?;
        let root_buffer_id = expect_int(values.next(), "root buffer id")?;
        let key_type_code = expect_byte(values.next(), "key type")? as u8;
        let field_type_bytes = expect_binary(values.next(), "field types")?;
        let field_names = expect_string(values.next(), "field names")?;
        let index_column = expect_int(values.next(), "index column")?;
        let _max_key = expect_long(values.next(), "max key")?;
        let record_count = expect_int(values.next(), "record count")?;

        let schema = schema_from_encoded(version, key_type_code, &field_type_bytes, &field_names)?;
        Ok(RawTableMeta {
            name,
            schema,
            root_buffer_id,
            index_column,
            record_count,
        })
    }

    fn collect_records(
        &self,
        buffer_id: i32,
        schema: &RawSchema,
        visited: &mut HashSet<i32>,
        out: &mut Vec<RawRecord>,
    ) -> Result<(), FidbfParseError> {
        if !visited.insert(buffer_id) {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "cycle in DB node tree at buffer {buffer_id}"
            )));
        }

        let buffer = self.read_buffer(buffer_id)?;
        let ty = node_type(buffer.data)?;
        match ty {
            NODE_LONGKEY_INTERIOR => {
                let key_count = usize::try_from(read_i32(buffer.data, 1)?).map_err(|_| {
                    FidbfParseError::MalformedRawFidDatabase(
                        "negative interior key count".to_string(),
                    )
                })?;
                let mut offset = 5usize;
                for _ in 0..key_count {
                    let _key = read_i64(buffer.data, offset)?;
                    let child = read_i32(buffer.data, offset + 8)?;
                    self.collect_records(child, schema, visited, out)?;
                    offset += 12;
                }
            }
            NODE_LONGKEY_VAR_REC => self.collect_var_leaf(buffer.data, schema, out)?,
            NODE_LONGKEY_FIXED_REC => self.collect_fixed_leaf(buffer.data, schema, out)?,
            NODE_CHAINED_BUFFER_DATA | NODE_CHAINED_BUFFER_INDEX => {
                return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                    "table root points at chained buffer node {ty}"
                )));
            }
            other => {
                return Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
                    "unsupported DB node type {other}"
                )));
            }
        }
        Ok(())
    }

    fn collect_var_leaf(
        &self,
        data: &[u8],
        schema: &RawSchema,
        out: &mut Vec<RawRecord>,
    ) -> Result<(), FidbfParseError> {
        let key_count = usize::try_from(read_i32(data, 1)?).map_err(|_| {
            FidbfParseError::MalformedRawFidDatabase("negative var leaf key count".to_string())
        })?;
        let mut entries = Vec::with_capacity(key_count);
        let mut offset = 13usize;
        for _ in 0..key_count {
            let key = read_i64(data, offset)?;
            let record_offset = usize::try_from(read_i32(data, offset + 8)?).map_err(|_| {
                FidbfParseError::MalformedRawFidDatabase("negative record offset".to_string())
            })?;
            let indirect = checked_slice(data, offset + 12, 1)?[0] != 0;
            entries.push((key, record_offset, indirect));
            offset += 13;
        }
        for (idx, &(key, record_offset, indirect)) in entries.iter().enumerate() {
            let record_end = if idx == 0 {
                data.len()
            } else {
                entries[idx - 1].1
            };
            if record_offset > record_end {
                return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                    "var leaf record offset {record_offset} exceeds end {record_end}"
                )));
            }
            let record_len = record_end - record_offset;
            let record_bytes = if indirect {
                let slice = checked_slice(data, record_offset, record_len)?;
                let chained_id = read_i32(slice, 0)?;
                self.read_chained_buffer(chained_id)?
            } else {
                checked_slice(data, record_offset, record_len)?.to_vec()
            };
            out.push(decode_record(key, schema, &record_bytes)?);
        }
        Ok(())
    }

    fn collect_fixed_leaf(
        &self,
        data: &[u8],
        schema: &RawSchema,
        out: &mut Vec<RawRecord>,
    ) -> Result<(), FidbfParseError> {
        let key_count = usize::try_from(read_i32(data, 1)?).map_err(|_| {
            FidbfParseError::MalformedRawFidDatabase("negative fixed leaf key count".to_string())
        })?;
        let record_size = schema.fixed_record_size().ok_or_else(|| {
            FidbfParseError::MalformedRawFidDatabase(
                "fixed record node used with variable-width schema".to_string(),
            )
        })?;
        let mut offset = 13usize;
        for _ in 0..key_count {
            let key = read_i64(data, offset)?;
            let record_offset = offset + 8;
            let record_bytes = checked_slice(data, record_offset, record_size)?;
            out.push(decode_record(key, schema, record_bytes)?);
            offset = record_offset + record_size;
        }
        Ok(())
    }

    fn read_chained_buffer(&self, buffer_id: i32) -> Result<Vec<u8>, FidbfParseError> {
        const DATA_BASE_OFFSET_NONINDEXED: usize = 5;
        const DATA_BASE_OFFSET_INDEXED: usize = 1;
        const INDEX_BASE_OFFSET: usize = 9;

        let first = self.read_buffer(buffer_id)?;
        match node_type(first.data)? {
            NODE_CHAINED_BUFFER_DATA => {
                let len_field = read_i32(first.data, 1)?;
                let use_xor = len_field < 0;
                let len = usize::try_from(len_field & i32::MAX).map_err(|_| {
                    FidbfParseError::MalformedRawFidDatabase(
                        "negative chained buffer length".to_string(),
                    )
                })?;
                let mut bytes =
                    checked_slice(first.data, DATA_BASE_OFFSET_NONINDEXED, len)?.to_vec();
                if use_xor {
                    xor_chained_data(&mut bytes, 0);
                }
                Ok(bytes)
            }
            NODE_CHAINED_BUFFER_INDEX => {
                let len_field = read_i32(first.data, 1)?;
                let use_xor = len_field < 0;
                let total_len = usize::try_from(len_field & i32::MAX).map_err(|_| {
                    FidbfParseError::MalformedRawFidDatabase(
                        "negative indexed chained buffer length".to_string(),
                    )
                })?;
                let max_data_per_buffer = self.buffer_size.saturating_sub(DATA_BASE_OFFSET_INDEXED);
                let indexes_per_buffer = self.buffer_size.saturating_sub(INDEX_BASE_OFFSET) / 4;
                if max_data_per_buffer == 0 || indexes_per_buffer == 0 {
                    return Err(FidbfParseError::MalformedRawFidDatabase(
                        "buffer size too small for indexed chained buffer".to_string(),
                    ));
                }

                let mut out = Vec::with_capacity(total_len);
                let mut current_index_id = buffer_id;
                let mut visited_index = HashSet::new();

                while out.len() < total_len {
                    if !visited_index.insert(current_index_id) {
                        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                            "cycle in chained buffer index chain at buffer {current_index_id}"
                        )));
                    }
                    let index_buf = self.read_buffer(current_index_id)?;
                    let idx_ty = node_type(index_buf.data)?;
                    if idx_ty != NODE_CHAINED_BUFFER_INDEX {
                        return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                            "expected chained buffer index node {NODE_CHAINED_BUFFER_INDEX}, got {idx_ty}"
                        )));
                    }
                    let next_index_id = read_i32(index_buf.data, 5)?;

                    for slot in 0..indexes_per_buffer {
                        if out.len() >= total_len {
                            break;
                        }
                        let entry_offset = INDEX_BASE_OFFSET + slot * 4;
                        let data_buffer_id = read_i32(index_buf.data, entry_offset)?;
                        let chunk_offset = out.len();
                        let take = (total_len - chunk_offset).min(max_data_per_buffer);
                        if data_buffer_id < 0 {
                            out.resize(chunk_offset + take, 0);
                        } else {
                            let data_buf = self.read_buffer(data_buffer_id)?;
                            let data_ty = node_type(data_buf.data)?;
                            if data_ty != NODE_CHAINED_BUFFER_DATA {
                                return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                                    "chained index referenced non-data buffer {data_buffer_id} of type {data_ty}"
                                )));
                            }
                            let mut chunk =
                                checked_slice(data_buf.data, DATA_BASE_OFFSET_INDEXED, take)?
                                    .to_vec();
                            if use_xor {
                                xor_chained_data(&mut chunk, chunk_offset);
                            }
                            out.extend_from_slice(&chunk);
                        }
                    }

                    if out.len() < total_len {
                        if next_index_id < 0 {
                            return Err(FidbfParseError::MalformedRawFidDatabase(
                                "indexed chained buffer ended before reading full payload"
                                    .to_string(),
                            ));
                        }
                        current_index_id = next_index_id;
                    }
                }

                Ok(out)
            }
            other => Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "invalid chained buffer node type {other}"
            ))),
        }
    }

    fn read_buffer(&self, buffer_id: i32) -> Result<RawBuffer<'a>, FidbfParseError> {
        if buffer_id < 0 || buffer_id >= self.buffer_count {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "buffer id {buffer_id} out of range"
            )));
        }
        let block_index = usize::try_from(buffer_id)
            .map_err(|_| FidbfParseError::MalformedRawFidDatabase("buffer id".to_string()))?
            + 1;
        let block_offset = block_index.checked_mul(self.block_size).ok_or_else(|| {
            FidbfParseError::MalformedRawFidDatabase("block offset overflow".to_string())
        })?;
        let block = checked_slice(self.bytes, block_offset, self.block_size)?;
        let stored_id = read_i32(block, 1)?;
        if stored_id != buffer_id {
            return Err(FidbfParseError::MalformedRawFidDatabase(format!(
                "buffer {buffer_id} stored id mismatch {stored_id}"
            )));
        }
        Ok(RawBuffer {
            data: checked_slice(block, BLOCK_PREFIX_SIZE, self.buffer_size)?,
        })
    }
}

fn node_type(data: &[u8]) -> Result<u8, FidbfParseError> {
    Ok(*checked_slice(data, 0, 1)?.first().expect("slice length"))
}

fn master_table_schema() -> Result<RawSchema, FidbfParseError> {
    schema_from_encoded(
        1,
        3,
        &[4, 2, 2, 0, 5, 4, 2, 3, 2],
        "Table Name;Schema Version;Root Buffer ID;Key Type;Field Types;Field Names;Index Column;Max Key;Record Count;",
    )
}

pub(crate) fn schema_from_encoded(
    version: i32,
    key_type_code: u8,
    field_type_bytes: &[u8],
    encoded_field_names: &str,
) -> Result<RawSchema, FidbfParseError> {
    let key_type = RawFieldType::from_code(key_type_code)?;
    let field_types = field_type_bytes
        .iter()
        .map(|code| RawFieldType::from_code(*code))
        .collect::<Result<Vec<_>, _>>()?;
    let field_names = encoded_field_names
        .split(';')
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Ok(RawSchema {
        version,
        key_type,
        field_types,
        field_names,
    })
}

fn decode_record(key: i64, schema: &RawSchema, bytes: &[u8]) -> Result<RawRecord, FidbfParseError> {
    if schema.key_type != RawFieldType::Long {
        return Err(FidbfParseError::UnsupportedRawFidDatabase(format!(
            "unsupported non-long table key type {:?}",
            schema.key_type
        )));
    }
    let mut values = Vec::with_capacity(schema.field_types.len());
    let mut offset = 0usize;
    for ty in &schema.field_types {
        let (value, next) = read_value(*ty, bytes, offset)?;
        values.push(value);
        offset = next;
    }
    Ok(RawRecord { key, values })
}

pub(crate) fn primary_tables_by_name(tables: Vec<RawTableMeta>) -> HashMap<String, RawTableMeta> {
    tables
        .into_iter()
        .filter(|table| table.index_column < 0)
        .map(|table| (table.name.clone(), table))
        .collect()
}

pub(crate) fn expect_byte(value: Option<RawValue>, name: &str) -> Result<i8, FidbfParseError> {
    match value {
        Some(RawValue::Byte(value)) => Ok(value),
        other => Err(type_error(name, "Byte", other)),
    }
}

pub(crate) fn expect_short(value: Option<RawValue>, name: &str) -> Result<i16, FidbfParseError> {
    match value {
        Some(RawValue::Short(value)) => Ok(value),
        other => Err(type_error(name, "Short", other)),
    }
}

pub(crate) fn expect_int(value: Option<RawValue>, name: &str) -> Result<i32, FidbfParseError> {
    match value {
        Some(RawValue::Int(value)) => Ok(value),
        other => Err(type_error(name, "Int", other)),
    }
}

pub(crate) fn expect_long(value: Option<RawValue>, name: &str) -> Result<i64, FidbfParseError> {
    match value {
        Some(RawValue::Long(value)) => Ok(value),
        other => Err(type_error(name, "Long", other)),
    }
}

pub(crate) fn expect_string(
    value: Option<RawValue>,
    name: &str,
) -> Result<String, FidbfParseError> {
    match value {
        Some(RawValue::String(value)) => value.ok_or_else(|| {
            FidbfParseError::MalformedRawFidDatabase(format!(
                "required string field {name} is null"
            ))
        }),
        other => Err(type_error(name, "String", other)),
    }
}

pub(crate) fn expect_binary(
    value: Option<RawValue>,
    name: &str,
) -> Result<Vec<u8>, FidbfParseError> {
    match value {
        Some(RawValue::Binary(value)) => value.ok_or_else(|| {
            FidbfParseError::MalformedRawFidDatabase(format!(
                "required binary field {name} is null"
            ))
        }),
        other => Err(type_error(name, "Binary", other)),
    }
}

fn type_error(name: &str, expected: &str, actual: Option<RawValue>) -> FidbfParseError {
    FidbfParseError::MalformedRawFidDatabase(format!(
        "field {name} expected {expected}, got {actual:?}"
    ))
}

fn xor_chained_data(bytes: &mut [u8], buffer_offset: usize) {
    const XOR_MASK_BYTES: [u8; 128] = [
        0x59, 0xea, 0x67, 0x23, 0xda, 0xb8, 0x00, 0xb8, 0xc3, 0x48, 0xdd, 0x8b, 0x21, 0xd6, 0x94,
        0x78, 0x35, 0xab, 0x2b, 0x7e, 0xb2, 0x4f, 0x82, 0x4e, 0x0e, 0x16, 0xc4, 0x57, 0x12, 0x8e,
        0x7e, 0xe6, 0xb6, 0xbd, 0x56, 0x91, 0x57, 0x72, 0xe6, 0x91, 0xdc, 0x52, 0x2e, 0xf2, 0x1a,
        0xb7, 0xd6, 0x6f, 0xda, 0xde, 0xe8, 0x48, 0xb1, 0xbb, 0x50, 0x6f, 0xf4, 0xdd, 0x11, 0xee,
        0xf2, 0x67, 0xfe, 0x48, 0x8d, 0xae, 0x69, 0x1a, 0xe0, 0x26, 0x8c, 0x24, 0x8e, 0x17, 0x76,
        0x51, 0xe2, 0x60, 0xd7, 0xe6, 0x83, 0x65, 0xd5, 0xf0, 0x7f, 0xf2, 0xa0, 0xd6, 0x4b, 0xbd,
        0x24, 0xd8, 0xab, 0xea, 0x9e, 0xa6, 0x48, 0x94, 0x3e, 0x7b, 0x2c, 0xf4, 0xce, 0xdc, 0x69,
        0x11, 0xf8, 0x3c, 0xa7, 0x3f, 0x5d, 0x77, 0x94, 0x3f, 0xe4, 0x8e, 0x48, 0x20, 0xdb, 0x56,
        0x32, 0xc1, 0x87, 0x01, 0x2e, 0xe3, 0x7f, 0x40,
    ];
    for (idx, byte) in bytes.iter_mut().enumerate() {
        *byte ^= XOR_MASK_BYTES[(buffer_offset + idx) % XOR_MASK_BYTES.len()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_block(bytes: &mut [u8], block_size: usize, buffer_id: i32, payload: &[u8]) {
        let block_index = (buffer_id as usize) + 1;
        let offset = block_index * block_size;
        bytes[offset] = 0;
        bytes[offset + 1..offset + 5].copy_from_slice(&buffer_id.to_be_bytes());
        bytes[offset + BLOCK_PREFIX_SIZE..offset + BLOCK_PREFIX_SIZE + payload.len()]
            .copy_from_slice(payload);
    }

    #[test]
    fn read_chained_buffer_decodes_indexed_multi_buffer_xor_payload() {
        // block_size = 32 -> buffer_size = 27
        // max_data_per_buffer = 26, indexes_per_buffer = (27 - 9) / 4 = 4
        let block_size = 32usize;
        let buffer_size = block_size - BLOCK_PREFIX_SIZE;
        let max_data = buffer_size - 1;
        let mut file_bytes = vec![0u8; block_size * 5];

        // Header in block 0
        file_bytes[0..8].copy_from_slice(&LOCAL_BUFFER_FILE_MAGIC.to_be_bytes());
        file_bytes[16..20].copy_from_slice(&LOCAL_BUFFER_FILE_VERSION.to_be_bytes());
        file_bytes[20..24].copy_from_slice(&(block_size as i32).to_be_bytes());

        let expected: Vec<u8> = (0u8..40)
            .map(|i| i.wrapping_mul(7).wrapping_add(3))
            .collect();
        let mut chunk0 = expected[..max_data].to_vec();
        xor_chained_data(&mut chunk0, 0);
        let mut chunk1 = expected[max_data..].to_vec();
        xor_chained_data(&mut chunk1, max_data);

        // Buffer 0: NODE_CHAINED_BUFFER_INDEX (8), length = 40 | 0x8000_0000 (XOR), next_id = -1, ids = [1, 2]
        let mut idx_payload = vec![0u8; buffer_size];
        idx_payload[0] = NODE_CHAINED_BUFFER_INDEX;
        idx_payload[1..5].copy_from_slice(&((expected.len() as i32) | i32::MIN).to_be_bytes());
        idx_payload[5..9].copy_from_slice(&(-1i32).to_be_bytes());
        idx_payload[9..13].copy_from_slice(&1i32.to_be_bytes());
        idx_payload[13..17].copy_from_slice(&2i32.to_be_bytes());
        write_block(&mut file_bytes, block_size, 0, &idx_payload);

        // Buffer 1: NODE_CHAINED_BUFFER_DATA (9)
        let mut data0 = vec![0u8; buffer_size];
        data0[0] = NODE_CHAINED_BUFFER_DATA;
        data0[1..1 + chunk0.len()].copy_from_slice(&chunk0);
        write_block(&mut file_bytes, block_size, 1, &data0);

        // Buffer 2: NODE_CHAINED_BUFFER_DATA (9)
        let mut data1 = vec![0u8; buffer_size];
        data1[0] = NODE_CHAINED_BUFFER_DATA;
        data1[1..1 + chunk1.len()].copy_from_slice(&chunk1);
        write_block(&mut file_bytes, block_size, 2, &data1);

        let handle = RawDbHandle::open(&file_bytes).expect("open synthetic raw db");
        let actual = handle
            .read_chained_buffer(0)
            .expect("read indexed chained buffer");
        assert_eq!(actual, expected);
    }

    #[test]
    fn collect_var_leaf_isolates_sibling_record_extents() {
        let block_size = 32usize;
        let mut file_bytes = vec![0u8; block_size * 2];
        file_bytes[0..8].copy_from_slice(&LOCAL_BUFFER_FILE_MAGIC.to_be_bytes());
        file_bytes[16..20].copy_from_slice(&LOCAL_BUFFER_FILE_VERSION.to_be_bytes());
        file_bytes[20..24].copy_from_slice(&(block_size as i32).to_be_bytes());
        write_block(&mut file_bytes, block_size, 0, &[0u8; 27]);
        let handle = RawDbHandle::open(&file_bytes).expect("open synthetic raw db");

        let schema = RawSchema {
            version: 1,
            key_type: RawFieldType::Long,
            field_types: vec![RawFieldType::Binary],
            field_names: vec!["Blob".to_string()],
        };

        // Leaf buffer with 2 entries:
        // Header (13 bytes): node_type = 1, key_count = 2 at offset 1..5
        // Entry 0 (offset 13..26): key = 10, record_offset = 47, indirect = 0 (6 bytes: len=2, [0xaa, 0xbb])
        // Entry 1 (offset 26..39): key = 20, record_offset = 39, indirect = 0 (8 bytes: corrupt len=10 that would bleed into Entry 0 if not bounded at 47)
        let mut leaf = vec![0u8; 53];
        leaf[0] = NODE_LONGKEY_VAR_REC;
        leaf[1..5].copy_from_slice(&2i32.to_be_bytes());
        // Entry 0
        leaf[13..21].copy_from_slice(&10i64.to_be_bytes());
        leaf[21..25].copy_from_slice(&47i32.to_be_bytes());
        leaf[25] = 0;
        leaf[47..51].copy_from_slice(&2i32.to_be_bytes());
        leaf[51..53].copy_from_slice(&[0xaa, 0xbb]);
        // Entry 1 (corrupt length 10 > 47 - 39 - 4 = 4)
        leaf[26..34].copy_from_slice(&20i64.to_be_bytes());
        leaf[34..38].copy_from_slice(&39i32.to_be_bytes());
        leaf[38] = 0;
        leaf[39..43].copy_from_slice(&10i32.to_be_bytes());

        let mut out = Vec::new();
        let err = handle
            .collect_var_leaf(&leaf, &schema, &mut out)
            .expect_err("entry 1 overflowing into entry 0 must be rejected");
        assert!(matches!(err, FidbfParseError::MalformedRawFidDatabase(_)));
    }
}
