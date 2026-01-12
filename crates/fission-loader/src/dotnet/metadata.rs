use crate::dotnet::{DotNetError, DotNetResult};

/// Information about a .NET method discovered in metadata tables.
#[derive(Debug, Clone)]
pub struct DotNetMethod {
    /// Metadata token for the method (MethodDef)
    pub token: u32,
    /// RVA of the method body (0 for abstract/interface methods)
    pub rva: u32,
    /// Name pulled from the `#Strings` heap
    pub name: String,
    /// Parsed signature in a readable form
    pub signature: String,
    /// Raw method flags
    pub flags: u16,
    /// Raw implementation flags
    pub impl_flags: u16,
}

/// Information about a .NET field
#[derive(Debug, Clone)]
pub struct DotNetField {
    pub token: u32,
    pub name: String,
    pub signature: String,
    pub flags: u16,
}

/// Information about a .NET type (TypeDef)
#[derive(Debug, Clone)]
pub struct DotNetType {
    pub token: u32,
    pub name: String,
    pub namespace: String,
    pub flags: u32,
    pub methods: Vec<DotNetMethod>,
    pub fields: Vec<DotNetField>,
}

/// Top-level metadata summary extracted from the CLR metadata streams.
#[derive(Debug, Clone)]
pub struct DotNetMetadata {
    pub runtime_version: String,
    pub types: Vec<DotNetType>,
}

use fission_core::constants::DOTNET_TABLE_COUNT;

#[derive(Clone, Copy)]
struct StreamHeader<'a> {
    name: &'a str,
    offset: u32,
    size: u32,
}

#[derive(Default)]
struct Streams<'a> {
    tables: Option<StreamHeader<'a>>,
    strings: Option<StreamHeader<'a>>,
    user_strings: Option<StreamHeader<'a>>,
    guids: Option<StreamHeader<'a>>,
    blobs: Option<StreamHeader<'a>>,
}

#[derive(Debug, Default)]
struct HeapSizes {
    strings: usize,
    guids: usize,
    blobs: usize,
}

#[derive(Debug, Clone)]
struct TypeDefRow {
    token: u32,
    flags: u32,
    name: String,
    namespace: String,
    field_list: u32,
    method_list: u32,
}

#[derive(Debug, Clone)]
struct TypeRefRow {
    name: String,
    namespace: String,
}

#[derive(Debug, Clone)]
struct MethodRow {
    token: u32,
    rva: u32,
    impl_flags: u16,
    flags: u16,
    name: String,
    signature: Vec<u8>,
    #[allow(dead_code)]
    param_list: u32,
}

#[derive(Debug, Clone)]
struct FieldRow {
    token: u32,
    flags: u16,
    name: String,
    signature: Vec<u8>,
}

/// Parse CLR metadata tables from a `#~` (or `#-`) stream alongside heap slices.
pub fn parse_metadata(
    metadata: &[u8],
    runtime_version: Option<String>,
) -> DotNetResult<DotNetMetadata> {
    let (header_version, streams, _offset) = parse_metadata_header(metadata)?;

    let mut stream_map = Streams::default();
    for stream in streams {
        match stream.name {
            "#~" | "#-" => stream_map.tables = Some(stream),
            "#Strings" => stream_map.strings = Some(stream),
            "#US" => stream_map.user_strings = Some(stream),
            "#GUID" => stream_map.guids = Some(stream),
            "#Blob" => stream_map.blobs = Some(stream),
            _ => {}
        }
    }

    let tables_stream = stream_slice(metadata, stream_map.tables.as_ref(), "tables")?;
    let strings_heap = stream_slice(metadata, stream_map.strings.as_ref(), "strings")?;
    let blobs_heap = stream_slice(metadata, stream_map.blobs.as_ref(), "blob")?;

    let mut reader = Cursor::new(tables_stream);
    let _reserved = reader.read_u32()?;
    let _major = reader.read_u8()?;
    let _minor = reader.read_u8()?;
    let heap_sizes = reader.read_u8()?;
    let _reserved2 = reader.read_u8()?;
    let valid = reader.read_u64()?;
    let _sorted = reader.read_u64()?;

    let mut row_counts = [0u32; DOTNET_TABLE_COUNT];
    for table in 0..DOTNET_TABLE_COUNT {
        if (valid >> table) & 1 == 1 {
            row_counts[table] = reader.read_u32()?;
        }
    }

    let heap_sizes = HeapSizes {
        strings: heap_size(heap_sizes, 0x1),
        guids: heap_size(heap_sizes, 0x2),
        blobs: heap_size(heap_sizes, 0x4),
    };

    // Prepare helpers for coded indexes
    let type_def_or_ref_size = coded_index_size(
        &[row_counts[2], row_counts[1], row_counts[27]],
        2, // TypeDefOrRef coded index
    );
    let resolution_scope_size = coded_index_size(
        &[row_counts[0], row_counts[1], row_counts[26], row_counts[35]],
        2,
    );

    let field_index_size = table_index_size(row_counts[4]);
    let method_index_size = table_index_size(row_counts[6]);
    let param_index_size = table_index_size(row_counts[8]);

    // Compute offsets through table stream until we reach the tables we care about
    let mut offset = reader.offset;
    let mut type_refs = Vec::new();
    let mut type_defs = Vec::new();
    let mut fields = Vec::new();
    let mut methods = Vec::new();

    for table in 0..DOTNET_TABLE_COUNT {
        let rows = row_counts[table] as usize;
        if rows == 0 {
            continue;
        }

        match table {
            0 => {
                // Module: Generation (u16), Name (string), Mvid (GUID), EncId (GUID), EncBaseId (GUID)
                let row_size = 2 + heap_sizes.strings + heap_sizes.guids * 3;
                offset += row_size * rows;
            }
            1 => {
                // TypeRef
                let row_size = resolution_scope_size + heap_sizes.strings * 2;
                for i in 0..rows {
                    let mut cursor = Cursor::with_offset(tables_stream, offset + row_size * i);
                    cursor.skip(resolution_scope_size)?;
                    let name_idx = cursor.read_index(heap_sizes.strings)?;
                    let ns_idx = cursor.read_index(heap_sizes.strings)?;
                    type_refs.push(TypeRefRow {
                        name: heap_string(strings_heap, name_idx)?,
                        namespace: heap_string(strings_heap, ns_idx)?,
                    });
                }
                offset += row_size * rows;
            }
            2 => {
                // TypeDef
                let row_size = 4
                    + heap_sizes.strings * 2
                    + type_def_or_ref_size
                    + field_index_size
                    + method_index_size;
                for i in 0..rows {
                    let row_offset = offset + row_size * i;
                    let mut cursor = Cursor::with_offset(tables_stream, row_offset);
                    let flags = cursor.read_u32()?;
                    let name_idx = cursor.read_index(heap_sizes.strings)?;
                    let ns_idx = cursor.read_index(heap_sizes.strings)?;
                    cursor.skip(type_def_or_ref_size)?; // extends
                    let field_list = cursor.read_index(field_index_size)?;
                    let method_list = cursor.read_index(method_index_size)?;

                    type_defs.push(TypeDefRow {
                        token: 0x02000000 | (i as u32 + 1),
                        flags,
                        name: heap_string(strings_heap, name_idx)?,
                        namespace: heap_string(strings_heap, ns_idx)?,
                        field_list,
                        method_list,
                    });
                }
                offset += row_size * rows;
            }
            3 => {
                // FieldPtr - skip over
                let row_size = field_index_size;
                offset += row_size * rows;
            }
            4 => {
                // Field
                let row_size = 2 + heap_sizes.strings + heap_sizes.blobs;
                for i in 0..rows {
                    let row_offset = offset + row_size * i;
                    let mut cursor = Cursor::with_offset(tables_stream, row_offset);
                    let flags = cursor.read_u16()?;
                    let name_idx = cursor.read_index(heap_sizes.strings)?;
                    let sig_idx = cursor.read_index(heap_sizes.blobs)?;
                    let sig = heap_blob(blobs_heap, sig_idx)?;

                    fields.push(FieldRow {
                        token: 0x04000000 | (i as u32 + 1),
                        flags,
                        name: heap_string(strings_heap, name_idx)?,
                        signature: sig.to_vec(),
                    });
                }
                offset += row_size * rows;
            }
            5 => {
                // MethodPtr
                let row_size = method_index_size;
                offset += row_size * rows;
            }
            6 => {
                // MethodDef
                let row_size = 4 + 2 + 2 + heap_sizes.strings + heap_sizes.blobs + param_index_size;
                for i in 0..rows {
                    let row_offset = offset + row_size * i;
                    let mut cursor = Cursor::with_offset(tables_stream, row_offset);
                    let rva = cursor.read_u32()?;
                    let impl_flags = cursor.read_u16()?;
                    let flags = cursor.read_u16()?;
                    let name_idx = cursor.read_index(heap_sizes.strings)?;
                    let sig_idx = cursor.read_index(heap_sizes.blobs)?;
                    let param_list = cursor.read_index(param_index_size)?;
                    let sig = heap_blob(blobs_heap, sig_idx)?;

                    methods.push(MethodRow {
                        token: 0x06000000 | (i as u32 + 1),
                        rva,
                        impl_flags,
                        flags,
                        name: heap_string(strings_heap, name_idx)?,
                        signature: sig.to_vec(),
                        param_list,
                    });
                }
                offset += row_size * rows;
            }
            _ => {
                // Skip other tables; we don't need them for the initial feature set.
                let row_size = guess_row_size(table, &heap_sizes);
                offset += row_size * rows;
            }
        }
    }

    // Map methods/fields to types using MethodList/FieldList ranges
    let mut types = Vec::new();
    for (i, td) in type_defs.iter().enumerate() {
        let method_start = td.method_list.max(1) as usize;
        let method_end = next_list_limit(i, &type_defs, methods.len(), |t| t.method_list);
        let field_start = td.field_list.max(1) as usize;
        let field_end = next_list_limit(i, &type_defs, fields.len(), |t| t.field_list);

        let mut type_methods = Vec::new();
        for m in methods
            .iter()
            .skip(method_start.saturating_sub(1))
            .take(method_end.saturating_sub(method_start))
        {
            let signature =
                decode_method_signature(&m.signature, &type_refs, &type_defs, strings_heap)
                    .unwrap_or_else(|_| format!("sig_{:08x}", m.token));

            type_methods.push(DotNetMethod {
                token: m.token,
                rva: m.rva,
                name: m.name.clone(),
                signature,
                flags: m.flags,
                impl_flags: m.impl_flags,
            });
        }

        let mut type_fields = Vec::new();
        for f in fields
            .iter()
            .skip(field_start.saturating_sub(1))
            .take(field_end.saturating_sub(field_start))
        {
            let signature =
                decode_field_signature(&f.signature, &type_refs, &type_defs, strings_heap)
                    .unwrap_or_else(|_| format!("sig_{:08x}", f.token));
            type_fields.push(DotNetField {
                token: f.token,
                name: f.name.clone(),
                signature,
                flags: f.flags,
            });
        }

        types.push(DotNetType {
            token: td.token,
            name: td.name.clone(),
            namespace: td.namespace.clone(),
            flags: td.flags,
            methods: type_methods,
            fields: type_fields,
        });
    }

    Ok(DotNetMetadata {
        runtime_version: runtime_version.unwrap_or_else(|| header_version.to_string()),
        types,
    })
}

fn parse_metadata_header<'a>(
    metadata: &'a [u8],
) -> DotNetResult<(String, Vec<StreamHeader<'a>>, usize)> {
    let mut cursor = Cursor::new(metadata);
    let signature = cursor.read_u32()?;
    if signature != 0x424A_5342 {
        return Err(DotNetError::Malformed(format!(
            "Invalid metadata signature: 0x{signature:08x}"
        )));
    }
    cursor.skip(2)?; // major
    cursor.skip(2)?; // minor
    cursor.skip(4)?; // reserved
    let version_len = cursor.read_u32()? as usize;
    let version_bytes = cursor.read_bytes(version_len)?;
    let version_str = String::from_utf8_lossy(version_bytes)
        .trim_matches('\0')
        .to_string();
    cursor.align(4);

    let _flags = cursor.read_u16()?;
    let streams = cursor.read_u16()? as usize;

    let mut headers = Vec::new();
    for _ in 0..streams {
        let offset = cursor.read_u32()?;
        let size = cursor.read_u32()?;
        let name = cursor.read_cstring()?;
        cursor.align(4);
        headers.push(StreamHeader { name, offset, size });
    }

    Ok((version_str, headers, cursor.offset))
}

fn stream_slice<'a>(
    metadata: &'a [u8],
    header: Option<&StreamHeader<'a>>,
    name: &str,
) -> DotNetResult<&'a [u8]> {
    let hdr = header.ok_or_else(|| DotNetError::Malformed(format!("Missing #{name} stream")))?;
    let start: usize = hdr
        .offset
        .try_into()
        .map_err(|_| DotNetError::Malformed(format!("Invalid offset for #{name}")))?;
    let end = start
        .checked_add(hdr.size as usize)
        .ok_or_else(|| DotNetError::Malformed(format!("Invalid size for #{name}")))?;
    metadata
        .get(start..end)
        .ok_or_else(|| DotNetError::Malformed("Stream range outside metadata".into()))
}

fn heap_string(heap: &[u8], index: u32) -> DotNetResult<String> {
    if index == 0 {
        return Ok(String::new());
    }
    let start = index as usize;
    if start >= heap.len() {
        return Err(DotNetError::Malformed(format!(
            "String heap index {index} out of range"
        )));
    }
    let end = heap[start..]
        .iter()
        .position(|b| *b == 0)
        .map(|rel| start + rel)
        .unwrap_or(heap.len());
    let raw = &heap[start..end];
    Ok(String::from_utf8_lossy(raw).to_string())
}

fn heap_blob<'a>(heap: &'a [u8], index: u32) -> DotNetResult<&'a [u8]> {
    if index == 0 {
        return Ok(&[]);
    }
    let mut cursor = Cursor::with_offset(heap, index as usize);
    let len = read_compressed(&mut cursor)?;
    let data = cursor.read_bytes(len as usize)?;
    Ok(data)
}

fn heap_size(heap_sizes: u8, flag: u8) -> usize {
    if heap_sizes & flag != 0 { 4 } else { 2 }
}

fn coded_index_size(row_counts: &[u32], tag_bits: u8) -> usize {
    let max_rows = row_counts.iter().copied().max().unwrap_or(0) as u64;
    let bits = 16 - tag_bits as u64;
    if max_rows < (1u64 << bits) { 2 } else { 4 }
}

fn table_index_size(rows: u32) -> usize {
    if rows < 0x10000 { 2 } else { 4 }
}

fn next_list_limit<F>(
    current: usize,
    type_defs: &[TypeDefRow],
    table_len: usize,
    accessor: F,
) -> usize
where
    F: Fn(&TypeDefRow) -> u32,
{
    if current + 1 < type_defs.len() {
        accessor(&type_defs[current + 1]) as usize
    } else {
        table_len + 1
    }
}

fn guess_row_size(table: usize, heaps: &HeapSizes) -> usize {
    // A conservative estimate for tables we skip; keeps offsets moving even if slightly wrong.
    match table {
        8 => 4 + heaps.strings + heaps.blobs, // Param
        10 => heaps.strings + heaps.blobs + coded_index_size(&[0, 0, 0, 0, 0], 3), // MemberRef-ish
        11 => 2 + 1 + heaps.blobs,            // Constant
        _ => 4 + heaps.strings + heaps.blobs,
    }
}

fn decode_method_signature(
    sig: &[u8],
    type_refs: &[TypeRefRow],
    type_defs: &[TypeDefRow],
    strings: &[u8],
) -> DotNetResult<String> {
    let mut cursor = Cursor::new(sig);
    if sig.is_empty() {
        return Err(DotNetError::Malformed("Empty method signature".into()));
    }
    let flags = cursor.read_u8()?;
    let has_this = flags & 0x20 != 0;
    let explicit_this = flags & 0x40 != 0;
    let has_generic = flags & 0x10 != 0;
    let call_conv = flags & 0x0F;
    if has_generic {
        let _ = read_compressed(&mut cursor)?; // generic param count
    }
    let param_count = read_compressed(&mut cursor)? as usize;
    let ret_type = decode_type(&mut cursor, type_refs, type_defs, strings)?;
    let mut params = Vec::new();
    for _ in 0..param_count {
        params.push(decode_type(&mut cursor, type_refs, type_defs, strings)?);
    }

    let mut prefix = Vec::new();
    if explicit_this {
        prefix.push("explicit");
    }
    if has_this {
        prefix.push("instance");
    }
    let call_conv_str = match call_conv {
        0x0 => "default",
        0x5 => "vararg",
        0x6 => "generic",
        _ => "unknown",
    };
    let mut head = prefix.join(" ");
    if !head.is_empty() {
        head.push(' ');
    }
    head.push_str(call_conv_str);
    if has_generic {
        head.push_str(" generic");
    }

    Ok(format!("{head} {} ({})", ret_type, params.join(", ")))
}

fn decode_field_signature(
    sig: &[u8],
    type_refs: &[TypeRefRow],
    type_defs: &[TypeDefRow],
    strings: &[u8],
) -> DotNetResult<String> {
    let mut cursor = Cursor::new(sig);
    if cursor.read_u8()? & 0x7 != 0x6 {
        return Err(DotNetError::Malformed("Invalid field signature".into()));
    }
    decode_type(&mut cursor, type_refs, type_defs, strings)
}

fn decode_type(
    cursor: &mut Cursor<'_>,
    type_refs: &[TypeRefRow],
    type_defs: &[TypeDefRow],
    strings: &[u8],
) -> DotNetResult<String> {
    let element = cursor.read_u8()?;
    let type_name = match element {
        0x01 => "void".to_string(),
        0x02 => "bool".to_string(),
        0x03 => "char".to_string(),
        0x04 => "int8".to_string(),
        0x05 => "uint8".to_string(),
        0x06 => "int16".to_string(),
        0x07 => "uint16".to_string(),
        0x08 => "int32".to_string(),
        0x09 => "uint32".to_string(),
        0x0A => "int64".to_string(),
        0x0B => "uint64".to_string(),
        0x0C => "float32".to_string(),
        0x0D => "float64".to_string(),
        0x0E => "string".to_string(),
        0x0F => {
            let inner = decode_type(cursor, type_refs, type_defs, strings)?;
            format!("*{inner}")
        }
        0x10 => {
            let inner = decode_type(cursor, type_refs, type_defs, strings)?;
            format!("&{inner}")
        }
        0x11 | 0x12 => {
            let coded = read_compressed(cursor)?;
            let resolved = resolve_type_ref(coded, type_refs, type_defs, strings);
            if element == 0x11 {
                format!("valuetype {resolved}")
            } else {
                format!("class {resolved}")
            }
        }
        0x14 => {
            let elem = decode_type(cursor, type_refs, type_defs, strings)?;
            let rank = read_compressed(cursor)? as usize;
            let num_sizes = read_compressed(cursor)? as usize;
            for _ in 0..num_sizes {
                let _ = read_compressed(cursor)?;
            }
            let num_bounds = read_compressed(cursor)? as usize;
            for _ in 0..num_bounds {
                let _ = read_compressed(cursor)?;
            }
            format!("{elem}[{rank}]")
        }
        0x15 => {
            let kind = cursor.read_u8()?;
            let coded = read_compressed(cursor)?;
            let ty = resolve_type_ref(coded, type_refs, type_defs, strings);
            let arg_count = read_compressed(cursor)? as usize;
            let mut args = Vec::new();
            for _ in 0..arg_count {
                args.push(decode_type(cursor, type_refs, type_defs, strings)?);
            }
            let ctor = if kind == 0x11 { "valuetype" } else { "class" };
            format!("{ctor} {ty}<{}>", args.join(", "))
        }
        0x1C => "object".to_string(),
        0x1D => {
            let inner = decode_type(cursor, type_refs, type_defs, strings)?;
            format!("{inner}[]")
        }
        other => format!("0x{other:02x}"),
    };
    Ok(type_name)
}

fn resolve_type_ref(
    coded: u32,
    type_refs: &[TypeRefRow],
    type_defs: &[TypeDefRow],
    _strings: &[u8],
) -> String {
    let tag = coded & 0x3;
    let idx = (coded >> 2) as usize;
    match tag {
        0 if idx > 0 && idx <= type_defs.len() => {
            let td = &type_defs[idx - 1];
            if td.namespace.is_empty() {
                td.name.clone()
            } else {
                format!("{}.{}", td.namespace, td.name)
            }
        }
        1 if idx > 0 && idx <= type_refs.len() => {
            let tr = &type_refs[idx - 1];
            if tr.namespace.is_empty() {
                tr.name.clone()
            } else {
                format!("{}.{}", tr.namespace, tr.name)
            }
        }
        _ => format!("type_{coded:08x}"),
    }
}

fn read_compressed(cursor: &mut Cursor<'_>) -> DotNetResult<u32> {
    let first = cursor.read_u8()?;
    if first & 0x80 == 0 {
        Ok(first as u32)
    } else if first & 0xC0 == 0x80 {
        let second = cursor.read_u8()?;
        Ok((((first & 0x3F) as u32) << 8) | second as u32)
    } else {
        let b2 = cursor.read_u8()?;
        let b3 = cursor.read_u8()?;
        let b4 = cursor.read_u8()?;
        Ok((((first & 0x1F) as u32) << 24) | (b2 as u32) << 16 | (b3 as u32) << 8 | b4 as u32)
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pub offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn with_offset(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }

    fn read_u8(&mut self) -> DotNetResult<u8> {
        self.data
            .get(self.offset)
            .copied()
            .ok_or_else(|| DotNetError::Malformed("Unexpected EOF".into()))
            .map(|v| {
                self.offset += 1;
                v
            })
    }

    fn read_u16(&mut self) -> DotNetResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> DotNetResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> DotNetResult<u64> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_bytes(&mut self, len: usize) -> DotNetResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| DotNetError::Malformed("Overflow in cursor".into()))?;
        let slice = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| DotNetError::Malformed("Unexpected EOF".into()))?;
        self.offset = end;
        Ok(slice)
    }

    fn read_index(&mut self, size: usize) -> DotNetResult<u32> {
        match size {
            2 => Ok(self.read_u16()? as u32),
            4 => self.read_u32(),
            _ => Err(DotNetError::Malformed(format!(
                "Unsupported index size {size}"
            ))),
        }
    }

    fn read_cstring(&mut self) -> DotNetResult<&'a str> {
        let start = self.offset;
        let end = self
            .data
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, b)| **b == 0)
            .map(|(idx, _)| idx)
            .ok_or_else(|| DotNetError::Malformed("Missing null terminator".into()))?;
        let slice = self
            .data
            .get(start..end)
            .ok_or_else(|| DotNetError::Malformed("CString OOB".into()))?;
        self.offset = end + 1;
        std::str::from_utf8(slice)
            .map_err(|_| DotNetError::Malformed("Invalid UTF-8 in stream name".into()))
    }

    fn skip(&mut self, len: usize) -> DotNetResult<()> {
        self.offset = self
            .offset
            .checked_add(len)
            .ok_or_else(|| DotNetError::Malformed("Cursor overflow".into()))?;
        if self.offset > self.data.len() {
            return Err(DotNetError::Malformed("Unexpected EOF".into()));
        }
        Ok(())
    }

    fn align(&mut self, alignment: usize) {
        let mask = alignment - 1;
        if alignment > 0 {
            self.offset = (self.offset + mask) & !mask;
        }
    }
}
