//! Reader for `.fpk`: a sparse index over independently compressed blocks.
//!
//! Written by `scripts/fpk_pack.py`. The point is lazy reads. Parsing
//! `win_api_signatures.txt` costs ~120ms of the ~160ms every process spends
//! loading resource data, and compressing the file whole would only add a
//! decompress in front of that same parse. Sorting the records into ~64KB
//! blocks and compressing each one separately means a lookup touches one block.
//!
//! Records inside a block are the original text, byte for byte, so whatever
//! parsed the file before parses a block unchanged. Only opening it differs.
//!
//! Layout (little-endian):
//! ```text
//! [0..4)    magic "FPK1"
//! [4..6)    kind          u16
//! [6..8)    codec         u16   1 = zlib
//! [8..16)   record_count  u64
//! [16..24)  block_count   u64
//! [24..32)  index_offset  u64
//! [32..40)  index_len     u64
//! [40..72)  payload sha256
//! [72..)    blocks, then index
//!
//! index entry: u32 key_len | key | u64 offset | u32 comp_len | u32 raw_len
//! ```

use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

const MAGIC: &[u8; 4] = b"FPK1";
const HEADER_LEN: usize = 72;

/// The `codec` header field carries two things: compression in the low byte,
/// block layout in the high byte. The header has no spare space -- magic(4),
/// kind(2), codec(2), counts and offsets(32), sha256(32) fill all 72 bytes --
/// and a layout that the reader guessed wrong would decode records into the
/// wrong fields, so it has to be stated in the file.
const COMPRESS_ZLIB: u16 = 1;
const COMPRESS_ZSTD: u16 = 2;
const LAYOUT_ROW: u16 = 0 << 8;
const LAYOUT_COLUMNAR: u16 = 1 << 8;

pub const CODEC_ZLIB: u16 = COMPRESS_ZLIB | LAYOUT_ROW;
/// What the FID tables use: see [`pack_with`] for why.
pub const CODEC_ZSTD_COLUMNAR: u16 = COMPRESS_ZSTD | LAYOUT_COLUMNAR;

/// Separates one column from the next inside a columnar block.
const COLUMN_SEPARATOR: u8 = 0x1e;

/// Default block size. Bigger blocks compress better and cost more to decode
/// for a single lookup, so tables that are always loaded whole use more.
pub const BLOCK_TARGET_DEFAULT: usize = 64 * 1024;
pub const BLOCK_TARGET_BULK: usize = 1024 * 1024;

#[derive(Debug)]
pub enum FpkError {
    Io(std::io::Error),
    /// The file is not an `.fpk`, or its header does not describe its body.
    Malformed(&'static str),
    /// A block did not decompress, or came out the wrong length. Unlike a
    /// mis-parsed record this cannot pass silently.
    Corrupt(String),
}

impl std::fmt::Display for FpkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "fpk io: {e}"),
            Self::Malformed(m) => write!(f, "fpk malformed: {m}"),
            Self::Corrupt(m) => write!(f, "fpk corrupt: {m}"),
        }
    }
}

impl std::error::Error for FpkError {}

/// One block's location, read once from the index and kept in memory.
struct BlockRef {
    first_key: String,
    offset: usize,
    comp_len: usize,
    raw_len: usize,
}

pub struct FpkReader {
    map: Mmap,
    blocks: Vec<BlockRef>,
    record_count: u64,
    codec: u16,
    /// Decompressed blocks, kept because lookups cluster: a function's calls
    /// resolve names that sort near each other, so the same block answers many
    /// of them. Without this a lazy database pays a decompress per lookup
    /// instead of per block.
    cache: Mutex<HashMap<usize, Arc<String>>>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(buf)
}

impl FpkReader {
    pub fn open(path: &Path) -> Result<Self, FpkError> {
        let file = File::open(path).map_err(FpkError::Io)?;
        // SAFETY: the bundle is read-only data shipped alongside the binary.
        // A concurrent writer would be a packaging bug, not a runtime state.
        let map = unsafe { Mmap::map(&file) }.map_err(FpkError::Io)?;
        if map.len() < HEADER_LEN || &map[0..4] != MAGIC {
            return Err(FpkError::Malformed("bad magic"));
        }
        let codec = u16_at(&map, 6);
        if !matches!(codec & 0xff, COMPRESS_ZLIB | COMPRESS_ZSTD)
            || !matches!(codec & 0xff00, LAYOUT_ROW | LAYOUT_COLUMNAR)
        {
            return Err(FpkError::Malformed("unknown codec"));
        }
        let record_count = u64_at(&map, 8);
        let index_offset = u64_at(&map, 24) as usize;
        let index_len = u64_at(&map, 32) as usize;
        if index_offset
            .checked_add(index_len)
            .is_none_or(|end| end > map.len())
        {
            return Err(FpkError::Malformed("index out of range"));
        }

        let mut blocks = Vec::new();
        let mut pos = index_offset;
        let end = index_offset + index_len;
        while pos < end {
            if pos + 4 > end {
                return Err(FpkError::Malformed("truncated index entry"));
            }
            let key_len = u32_at(&map, pos) as usize;
            pos += 4;
            if pos + key_len + 16 > end {
                return Err(FpkError::Malformed("truncated index entry"));
            }
            let first_key = String::from_utf8_lossy(&map[pos..pos + key_len]).into_owned();
            pos += key_len;
            let offset = u64_at(&map, pos) as usize;
            let comp_len = u32_at(&map, pos + 8) as usize;
            let raw_len = u32_at(&map, pos + 12) as usize;
            pos += 16;
            if offset
                .checked_add(comp_len)
                .is_none_or(|e| e > index_offset)
            {
                return Err(FpkError::Malformed("block outside payload"));
            }
            blocks.push(BlockRef {
                first_key,
                offset,
                comp_len,
                raw_len,
            });
        }
        Ok(Self {
            map,
            blocks,
            record_count,
            codec,
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    fn decompress(&self, block: &BlockRef) -> Result<String, FpkError> {
        let raw = &self.map[block.offset..block.offset + block.comp_len];
        let mut out = String::with_capacity(block.raw_len);
        match self.codec & 0xff {
            COMPRESS_ZSTD => {
                let bytes = zstd::decode_all(raw)
                    .map_err(|e| FpkError::Corrupt(format!("block at {}: {e}", block.offset)))?;
                out = String::from_utf8(bytes)
                    .map_err(|e| FpkError::Corrupt(format!("block at {}: {e}", block.offset)))?;
            }
            _ => {
                flate2::read::ZlibDecoder::new(raw)
                    .read_to_string(&mut out)
                    .map_err(|e| FpkError::Corrupt(format!("block at {}: {e}", block.offset)))?;
            }
        }
        if out.len() != block.raw_len {
            return Err(FpkError::Corrupt(format!(
                "block at {} decompressed to {} bytes, index says {}",
                block.offset,
                out.len(),
                block.raw_len
            )));
        }
        if self.codec & 0xff00 == LAYOUT_COLUMNAR {
            return rows_from_columns(&out, block.offset);
        }
        Ok(out)
    }

    /// Lines of the one block that could hold `key`.
    ///
    /// Records are sorted, so the block whose first key is the last one at or
    /// before `key` is the only candidate; a key absent from that block is
    /// absent from the file.
    pub fn block_for(&self, key: &str) -> Result<Option<Arc<String>>, FpkError> {
        if self.blocks.is_empty() || key < self.blocks[0].first_key.as_str() {
            return Ok(None);
        }
        let index = match self
            .blocks
            .binary_search_by(|block| block.first_key.as_str().cmp(key))
        {
            Ok(hit) => hit,
            Err(0) => return Ok(None),
            Err(after) => after - 1,
        };
        self.block_at(index).map(Some)
    }

    /// Decompressed block `index`, from the cache when it is already there.
    pub fn block_at(&self, index: usize) -> Result<Arc<String>, FpkError> {
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.get(&index)
        {
            return Ok(Arc::clone(hit));
        }
        let text = Arc::new(self.decompress(&self.blocks[index])?);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(index, Arc::clone(&text));
        }
        Ok(text)
    }

    /// The index directory, or `None` when the file has none.
    fn hash_directory(&self) -> Option<&[u8]> {
        let len = self.map.len();
        if len < HASH_TRAILER_LEN {
            return None;
        }
        let trailer = len - HASH_TRAILER_LEN;
        if &self.map[trailer..trailer + 4] != HASH_INDEX_MAGIC {
            return None;
        }
        if u16_at(&self.map, trailer + 4) != 2 {
            return None; // an index this build does not know how to read
        }
        let directory_offset = u64_at(&self.map, trailer + 16) as usize;
        if directory_offset > trailer {
            return None;
        }
        Some(&self.map[directory_offset..trailer])
    }

    pub fn has_hash_index(&self) -> bool {
        self.hash_directory().is_some()
    }

    /// Records whose index key is `key`, decoding only what it takes to find
    /// them.
    ///
    /// The directory is searched in place -- it is one entry per 512 keys, a
    /// few KB for the largest table -- and then a single index chunk and a
    /// single payload block are decoded. A key that is absent decodes one 6KB
    /// chunk and stops.
    pub fn records_by_key(&self, key: u64) -> Result<Vec<String>, FpkError> {
        let Some(directory) = self.hash_directory() else {
            return Err(FpkError::Malformed("no hash index"));
        };
        let chunks = directory.len() / HASH_DIR_LEN;
        if chunks == 0 {
            return Ok(Vec::new());
        }
        let first_key_at = |i: usize| u64_at(directory, i * HASH_DIR_LEN);
        if key < first_key_at(0) {
            return Ok(Vec::new());
        }
        // A run of equal keys can span chunks in BOTH directions: the key may
        // be a chunk's first key while earlier copies sit at the end of the one
        // before. Find the leftmost chunk that could hold it and scan forward
        // from there.
        let mut lo = 0usize;
        let mut hi = chunks;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if first_key_at(mid) < key {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        // `lo` is the first chunk whose first key is >= `key`. Anything earlier
        // in the run is in the preceding chunk, and only there, because a chunk
        // that started below `key` and does not end on it cannot contain it.
        let mut chunk = lo.saturating_sub(1);

        let mut out = Vec::new();
        while chunk < chunks {
            let base = chunk * HASH_DIR_LEN;
            let offset = u64_at(directory, base + 8) as usize;
            let comp_len = u32_at(directory, base + 16) as usize;
            let count = u32_at(directory, base + 20) as usize;
            if offset
                .checked_add(comp_len)
                .is_none_or(|e| e > self.map.len())
            {
                return Err(FpkError::Malformed("index chunk outside file"));
            }
            let raw = zstd::decode_all(&self.map[offset..offset + comp_len])
                .map_err(|e| FpkError::Corrupt(format!("index chunk at {offset}: {e}")))?;
            if raw.len() != count * HASH_ENTRY_LEN {
                return Err(FpkError::Corrupt(format!(
                    "index chunk at {offset} decoded to {} bytes, directory says {}",
                    raw.len(),
                    count * HASH_ENTRY_LEN
                )));
            }
            let mut found_here = false;
            for i in 0..count {
                if u64_at(&raw, i * HASH_ENTRY_LEN) != key {
                    continue;
                }
                found_here = true;
                let locator = u32_at(&raw, i * HASH_ENTRY_LEN + 8);
                let block = (locator >> LOCATOR_ROW_BITS) as usize;
                let row = (locator & LOCATOR_ROW_MASK) as usize;
                if block >= self.blocks.len() {
                    return Err(FpkError::Corrupt(format!(
                        "hash index points at block {block}, file has {}",
                        self.blocks.len()
                    )));
                }
                let text = self.block_at(block)?;
                match text.lines().nth(row) {
                    Some(line) => out.push(line.to_owned()),
                    None => {
                        return Err(FpkError::Corrupt(format!(
                            "hash index points at row {row} of block {block}"
                        )));
                    }
                }
            }
            // Stop once a chunk has moved past the key. A chunk with no match
            // is only worth passing through when it ends below the key, which
            // is the "search landed one chunk early" case.
            let last_key = u64_at(&raw, (count - 1) * HASH_ENTRY_LEN);
            if last_key > key {
                break;
            }
            let _ = found_here;
            chunk += 1;
        }
        Ok(out)
    }

    /// Every record, decompressing each block once. For callers that genuinely
    /// need the whole table; a lookup should use [`FpkReader::block_for`].
    pub fn read_all(&self) -> Result<Vec<String>, FpkError> {
        let mut out = Vec::with_capacity(self.record_count as usize);
        for block in &self.blocks {
            out.extend(self.decompress(block)?.lines().map(str::to_owned));
        }
        if out.len() as u64 != self.record_count {
            return Err(FpkError::Corrupt(format!(
                "read {} records, header says {}",
                out.len(),
                self.record_count
            )));
        }
        Ok(out)
    }
}

/// Rebuild rows from a columnar block.
///
/// A columnar block stores every record's first field, then every record's
/// second, and so on, each column terminated by [`COLUMN_SEPARATOR`]. Records
/// sorted so that neighbours are alike make each column nearly homogeneous,
/// which is what the compressor exploits -- on the FID function table this is
/// worth 25.47M against 32.96M for the same records stored row by row.
///
/// Every column must hold the same number of values. A block that decodes to
/// ragged columns is corrupt, and saying so here is what keeps a mis-decoded
/// block from silently producing records with fields shifted between them.
fn rows_from_columns(block: &str, offset: usize) -> Result<String, FpkError> {
    let mut columns: Vec<Vec<&str>> = Vec::new();
    for column in block.split(COLUMN_SEPARATOR as char) {
        if column.is_empty() {
            continue;
        }
        let values: Vec<&str> = column
            .strip_suffix('\n')
            .unwrap_or(column)
            .split('\n')
            .collect();
        columns.push(values);
    }
    let Some(rows) = columns.first().map(Vec::len) else {
        return Ok(String::new());
    };
    if let Some(bad) = columns.iter().position(|c| c.len() != rows) {
        return Err(FpkError::Corrupt(format!(
            "columnar block at {offset}: column {bad} has {} values, column 0 has {rows}",
            columns[bad].len()
        )));
    }
    let mut out = String::with_capacity(block.len());
    for row in 0..rows {
        for (index, column) in columns.iter().enumerate() {
            if index > 0 {
                out.push('|');
            }
            out.push_str(column[row]);
        }
        out.push('\n');
    }
    // `read_all`/`block_for` split on newlines, so drop the trailing one to
    // match how a row block is stored.
    out.pop();
    Ok(out)
}

/// Lay a group of records out column by column.
fn columns_from_rows(group: &[&String]) -> Vec<u8> {
    let columns = group.first().map(|r| r.split('|').count()).unwrap_or(0);
    let mut out = Vec::new();
    for column in 0..columns {
        for record in group {
            out.extend_from_slice(record.split('|').nth(column).unwrap_or("").as_bytes());
            out.push(b'\n');
        }
        out.push(COLUMN_SEPARATOR);
    }
    out
}

// ── Auxiliary hash index ────────────────────────────────────────────────────
//
// Appended after the block index, described by a trailer at EOF. The payload's
// bytes are untouched: the canonical records stay exactly what `pack_with`
// wrote, and this is derived data that lives outside them. `.fpk` already
// carried one index beside the payload -- the sparse per-block one -- so this
// is that idea extended to the key a caller actually queries by.
//
// It exists because payload order and lookup order are different problems. FID
// records are stored in symbol-name order, which is what compresses; matching
// asks for a `full_hash`. Without an index the only way to answer is to decode
// and parse everything, which measured 65ms per process.

const HASH_INDEX_MAGIC: &[u8; 4] = b"FIDX";
const HASH_TRAILER_LEN: usize = 56;
/// `u64` key plus a packed `u32` locator.
///
/// Block and row share one word because neither is large: across the FID
/// function tables the widest is 29 blocks and 9,399 rows in a block, 5 and 14
/// bits. Storing them as two `u32`s cost 28.0M over 1,834,901 entries where 21.0M
/// does.
const HASH_ENTRY_LEN: usize = 12;

/// Entries per compressed index chunk.
///
/// The index was stored flat and uncompressed so it could be binary-searched in
/// place, and at 21.0M it was the largest thing in the packed FID corpus after
/// the payload. Sorted 64-bit hashes compress well despite being random -- the
/// high bits advance slowly -- so the flat array of the largest table goes 2.48M
/// to 0.80M under zstd.
///
/// Chunking keeps the lookup: a directory of first keys is searched in place,
/// then one chunk is decoded. 512 entries costs 0.82M against 0.80M for the
/// whole array in one piece, and decodes 6KB instead of 2.5M to answer a query.
const HASH_CHUNK_ENTRIES: usize = 512;
const LOCATOR_ROW_BITS: u32 = 20;
const LOCATOR_ROW_MASK: u32 = (1 << LOCATOR_ROW_BITS) - 1;

/// One `key -> record` mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashEntry {
    pub key: u64,
    pub block: u32,
    pub row: u32,
}

/// Append a sorted `key -> locator` index to a packed image.
///
/// Duplicate keys are allowed and kept adjacent, because a full hash can name
/// several functions and dropping the extras would silently narrow matching.
pub fn append_hash_index(image: &mut Vec<u8>, mut entries: Vec<HashEntry>) {
    entries.sort_by_key(|e| (e.key, e.block, e.row));
    let chunks_offset = image.len() as u64;

    let mut payload: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for chunk in entries.chunks(HASH_CHUNK_ENTRIES) {
        let mut raw = Vec::with_capacity(chunk.len() * HASH_ENTRY_LEN);
        for entry in chunk {
            assert!(
                entry.row <= LOCATOR_ROW_MASK,
                "row {} does not fit the locator; lower the block target",
                entry.row
            );
            raw.extend_from_slice(&entry.key.to_le_bytes());
            raw.extend_from_slice(&((entry.block << LOCATOR_ROW_BITS) | entry.row).to_le_bytes());
        }
        let compressed = zstd::encode_all(&raw[..], 19).expect("zstd encode to Vec cannot fail");
        // first key | offset | compressed len | entry count
        directory.extend_from_slice(&chunk[0].key.to_le_bytes());
        directory
            .extend_from_slice(&((chunks_offset as usize + payload.len()) as u64).to_le_bytes());
        directory.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        payload.extend_from_slice(&compressed);
    }

    let directory_offset = chunks_offset as usize + payload.len();
    let digest = sha256(&payload);
    image.extend_from_slice(&payload);
    image.extend_from_slice(&directory);
    image.extend_from_slice(HASH_INDEX_MAGIC);
    image.extend_from_slice(&2u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    image.extend_from_slice(&(directory_offset as u64).to_le_bytes());
    image.extend_from_slice(&digest);
}

/// Directory entry length: first key, offset, compressed length, entry count.
const HASH_DIR_LEN: usize = 24;

/// Pack sorted records into an `.fpk` image.
///
/// Byte-for-byte the same container `scripts/fpk_pack.py` writes, so a table
/// can be produced by whichever side already holds the data -- Python for the
/// text tables it extracts, Rust for anything that must go through a parser
/// first, like the FID databases.
///
/// Records are sorted by their key here rather than by the caller: block
/// boundaries depend on the order, and a caller that sorted differently would
/// produce a file whose index does not describe it.
pub fn pack(records: &[String], kind: u16) -> Vec<u8> {
    pack_with(records, kind, CODEC_ZLIB, 0, BLOCK_TARGET_DEFAULT)
}

/// Pack with an explicit codec, sort column and block size.
///
/// `sort_field` is which `|`-separated column orders the records, and it is the
/// single biggest lever on size. The FID function table sorted by its database
/// key packs to 45.44M; the same records sorted by symbol name pack to 32.96M,
/// because neighbouring names share prefixes and drag their library and build
/// path along with them. Add columnar layout and it is 25.47M.
///
/// Sorting by a column other than the first means the block index no longer
/// answers lookups by the leading field. That is the right trade for a table
/// that is always read whole -- FID builds its own hash index after loading --
/// and the wrong one for a table queried by key.
pub fn pack_with(
    records: &[String],
    kind: u16,
    codec: u16,
    sort_field: usize,
    block_target: usize,
) -> Vec<u8> {
    pack_with_locators(records, kind, codec, sort_field, block_target).0
}

/// Where a record ended up, so a caller can build an index into the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locator {
    pub block: u32,
    pub row: u32,
}

/// [`pack_with`], also reporting each input record's physical position.
///
/// `locators[i]` describes `records[i]`, in the caller's order rather than the
/// sorted one, so a caller that knows something else about record `i` -- FID
/// knows its `full_hash` -- can pair the two without redoing the sort.
///
/// This is what lets payload order and lookup order come apart. Sorting by name
/// is worth 25.47M against 32.96M on the FID function table, and hash lookup
/// wants a different order entirely; with locators the payload can keep the
/// order that compresses and an index can carry the order that answers queries.
pub fn pack_with_locators(
    records: &[String],
    kind: u16,
    codec: u16,
    sort_field: usize,
    block_target: usize,
) -> (Vec<u8>, Vec<Locator>) {
    use std::io::Write;

    let key_of = |line: &str| line.split('|').nth(sort_field).unwrap_or("").to_owned();
    // Indices into `records`, so the caller's ordering can be recovered.
    let mut order: Vec<usize> = (0..records.len()).collect();
    order.sort_by_key(|i| key_of(&records[*i]));
    let sorted: Vec<&String> = order.iter().map(|i| &records[*i]).collect();

    let mut locators = vec![Locator { block: 0, row: 0 }; records.len()];
    let mut groups: Vec<Vec<&String>> = Vec::new();
    let mut current: Vec<&String> = Vec::new();
    let mut size = 0usize;
    for (position, record) in sorted.iter().enumerate() {
        if size + record.len() + 1 > block_target && !current.is_empty() {
            groups.push(std::mem::take(&mut current));
            size = 0;
        }
        locators[order[position]] = Locator {
            block: groups.len() as u32,
            row: current.len() as u32,
        };
        current.push(record);
        size += record.len() + 1;
    }
    if !current.is_empty() {
        groups.push(current);
    }

    let mut payload: Vec<u8> = Vec::new();
    let mut index: Vec<u8> = Vec::new();
    for group in &groups {
        let raw: Vec<u8> = if codec & 0xff00 == LAYOUT_COLUMNAR {
            columns_from_rows(group)
        } else {
            let mut buf = Vec::new();
            for record in group {
                buf.extend_from_slice(record.as_bytes());
                buf.push(b'\n');
            }
            buf
        };
        let compressed = if codec & 0xff == COMPRESS_ZSTD {
            zstd::encode_all(&raw[..], 19).expect("zstd encode to Vec cannot fail")
        } else {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            encoder
                .write_all(&raw)
                .expect("zlib write to Vec cannot fail");
            encoder.finish().expect("zlib finish on Vec cannot fail")
        };
        let first_key = key_of(group[0]);
        index.extend_from_slice(&(first_key.len() as u32).to_le_bytes());
        index.extend_from_slice(first_key.as_bytes());
        index.extend_from_slice(&((HEADER_LEN + payload.len()) as u64).to_le_bytes());
        index.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        index.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        payload.extend_from_slice(&compressed);
    }

    let mut out = Vec::with_capacity(HEADER_LEN + payload.len() + index.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&codec.to_le_bytes());
    out.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    out.extend_from_slice(&(groups.len() as u64).to_le_bytes());
    out.extend_from_slice(&((HEADER_LEN + payload.len()) as u64).to_le_bytes());
    out.extend_from_slice(&(index.len() as u64).to_le_bytes());
    out.extend_from_slice(&sha256(&payload));
    debug_assert_eq!(out.len(), HEADER_LEN);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&index);
    (out, locators)
}

/// SHA-256 of the payload, stored so a truncated or swapped body is caught.
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a tiny archive by hand so the reader is tested against the format,
    /// not against whatever the packer happens to emit.
    fn pack(records: &[&str]) -> Vec<u8> {
        let mut sorted: Vec<&str> = records.to_vec();
        sorted.sort_by_key(|r| r.split('|').next().unwrap_or(""));
        // One record per block, so block boundaries are exercised.
        let mut payload = Vec::new();
        let mut index = Vec::new();
        for record in &sorted {
            let raw = format!("{record}\n").into_bytes();
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(&raw).unwrap();
            let compressed = encoder.finish().unwrap();
            let key = record.split('|').next().unwrap_or("").as_bytes();
            index.extend_from_slice(&(key.len() as u32).to_le_bytes());
            index.extend_from_slice(key);
            index.extend_from_slice(&((HEADER_LEN + payload.len()) as u64).to_le_bytes());
            index.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            index.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            payload.extend_from_slice(&compressed);
        }
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&CODEC_ZLIB.to_le_bytes());
        out.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
        out.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
        out.extend_from_slice(&((HEADER_LEN + payload.len()) as u64).to_le_bytes());
        out.extend_from_slice(&(index.len() as u64).to_le_bytes());
        out.extend_from_slice(&[0u8; 32]);
        assert_eq!(out.len(), HEADER_LEN);
        out.extend_from_slice(&payload);
        out.extend_from_slice(&index);
        out
    }

    fn write_temp(bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("fpk_test_{}.fpk", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn lookup_reads_only_the_block_that_could_hold_the_key() {
        let path = write_temp(&pack(&["alpha|int|void", "mid|int|void", "zulu|int|void"]));
        let reader = FpkReader::open(&path).unwrap();
        assert_eq!(reader.block_count(), 3);
        assert!(reader.block_for("mid").unwrap().unwrap().contains("mid|"));
        // A key before every block cannot be in the file.
        assert!(reader.block_for("aaa").unwrap().is_none());
        // A key past the last block still lands in it, and is simply absent.
        let last = reader.block_for("zzz").unwrap().unwrap();
        assert!(last.contains("zulu|") && !last.contains("mid|"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn read_all_returns_every_record() {
        let records = ["a|int|void", "b|int|void", "c|int|void"];
        let path = write_temp(&pack(&records));
        let reader = FpkReader::open(&path).unwrap();
        let mut all = reader.read_all().unwrap();
        all.sort();
        assert_eq!(all, records);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_wrong_block_offset_fails_instead_of_returning_garbage() {
        // The failure mode this format exists to avoid: Ghidra's tables were
        // read at the wrong stride and returned plausible wrong values.
        let mut bytes = pack(&["a|int|void", "b|int|void"]);
        let index_offset = u64_at(&bytes, 24) as usize;
        let index_len = u64_at(&bytes, 32) as usize;
        // Shift the first block's offset by one byte.
        let key_len = u32_at(&bytes, index_offset) as usize;
        let at = index_offset + 4 + key_len;
        let shifted = (u64_at(&bytes, at) + 1).to_le_bytes();
        bytes[at..at + 8].copy_from_slice(&shifted);
        assert!(index_len > 0);
        let path = write_temp(&bytes);
        let reader = FpkReader::open(&path).unwrap();
        assert!(matches!(reader.read_all(), Err(FpkError::Corrupt(_))));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rust_and_python_packers_produce_equivalent_files() {
        // Two packers exist because each side already holds some of the data:
        // Python for the text tables it extracts, Rust for anything that has to
        // go through a parser first. They cannot agree byte for byte -- Python
        // uses zlib and flate2 uses miniz_oxide, which emit different streams at
        // the same level -- so what is pinned is what matters: both files carry
        // the same records, in the same order, with the same block boundaries.
        let src = std::path::Path::new(
            "/Users/sjkim1127/Fission/utils/signatures/typeinfo/win32/wdk_signatures.txt",
        );
        let Ok(text) = std::fs::read_to_string(src) else {
            return; // bundle not present in this checkout
        };
        let records: Vec<String> = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();

        let ours_path = std::env::temp_dir().join("fpk_ours.fpk");
        std::fs::write(&ours_path, super::pack(&records, 1)).unwrap();

        let theirs_path = std::env::temp_dir().join("fpk_theirs.fpk");
        let Ok(run) = std::process::Command::new("python3")
            .args([
                "/Users/sjkim1127/Fission/scripts/fpk_pack.py",
                src.to_str().unwrap(),
                "--kind",
                "pipe-text",
                "--output",
                theirs_path.to_str().unwrap(),
            ])
            .output()
        else {
            std::fs::remove_file(&ours_path).ok();
            return;
        };
        if !run.status.success() {
            std::fs::remove_file(&ours_path).ok();
            return;
        }

        let ours = FpkReader::open(&ours_path).unwrap();
        let theirs = FpkReader::open(&theirs_path).unwrap();
        assert_eq!(ours.record_count(), theirs.record_count());
        assert_eq!(
            ours.block_count(),
            theirs.block_count(),
            "block boundaries differ"
        );
        assert_eq!(ours.read_all().unwrap(), theirs.read_all().unwrap());
        // And both agree with the source they were built from.
        let mut expected = records;
        expected.sort_by(|a, b| {
            a.split('|')
                .next()
                .unwrap_or("")
                .cmp(b.split('|').next().unwrap_or(""))
        });
        assert_eq!(ours.read_all().unwrap(), expected);

        std::fs::remove_file(&ours_path).ok();
        std::fs::remove_file(&theirs_path).ok();
    }

    #[test]
    fn a_columnar_block_round_trips_through_zstd() {
        let records: Vec<String> = (0..500)
            .map(|i| format!("name{i:04}|{:016x}|lib{}|flag{}", i * 7919, i % 3, i % 2))
            .collect();
        let path = write_temp(&super::pack_with(&records, 1, CODEC_ZSTD_COLUMNAR, 0, 4096));
        let reader = FpkReader::open(&path).unwrap();
        assert!(reader.block_count() > 1, "test needs several blocks");
        let mut all = reader.read_all().unwrap();
        all.sort();
        let mut expected = records;
        expected.sort();
        assert_eq!(all, expected);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_ragged_columnar_block_is_rejected() {
        // Fields shifting between records is the failure this layout could
        // produce silently, so a column of the wrong length has to be an error.
        let err = rows_from_columns("a\nb\n\x1ec\n\x1e", 0);
        assert!(matches!(err, Err(FpkError::Corrupt(_))), "got {err:?}");
    }

    #[test]
    fn sorting_on_a_later_column_orders_by_that_column() {
        let records: Vec<String> = vec![
            "3|charlie".to_string(),
            "1|alpha".to_string(),
            "2|bravo".to_string(),
        ];
        let path = write_temp(&super::pack_with(&records, 1, CODEC_ZLIB, 1, 64 * 1024));
        let reader = FpkReader::open(&path).unwrap();
        assert_eq!(
            reader.read_all().unwrap(),
            vec!["1|alpha", "2|bravo", "3|charlie"]
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_hash_index_finds_records_without_reading_the_rest() {
        // Sorted by column 1 so payload order and lookup order differ, which is
        // the whole point of carrying an index.
        let records: Vec<String> = (0..2000u64)
            .map(|i| format!("{:016x}|name{:04}|payload", i * 2654435761, i))
            .collect();
        let (mut image, locators) = super::pack_with_locators(&records, 1, CODEC_ZLIB, 1, 4096);
        let entries: Vec<HashEntry> = records
            .iter()
            .zip(&locators)
            .map(|(r, l)| HashEntry {
                key: u64::from_str_radix(r.split('|').next().unwrap(), 16).unwrap(),
                block: l.block,
                row: l.row,
            })
            .collect();
        super::append_hash_index(&mut image, entries);

        let path = write_temp(&image);
        let reader = FpkReader::open(&path).unwrap();
        assert!(reader.has_hash_index());
        assert!(reader.block_count() > 1, "test needs several blocks");

        for i in [0u64, 7, 1999] {
            let key = i * 2654435761;
            let found = reader.records_by_key(key).unwrap();
            assert_eq!(found.len(), 1, "key {key:x}");
            assert!(found[0].contains(&format!("name{i:04}")));
        }
        // A key no record holds returns nothing rather than a near miss.
        assert!(reader.records_by_key(0xdead_beef).unwrap().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn duplicate_keys_all_come_back() {
        // A full hash can name several functions; keeping only one would
        // silently narrow matching.
        let records: Vec<String> = (0..5).map(|i| format!("dup|name{i}|payload")).collect();
        let (mut image, locators) = super::pack_with_locators(&records, 1, CODEC_ZLIB, 1, 64);
        let entries: Vec<HashEntry> = locators
            .iter()
            .map(|l| HashEntry {
                key: 42,
                block: l.block,
                row: l.row,
            })
            .collect();
        super::append_hash_index(&mut image, entries);
        let path = write_temp(&image);
        let reader = FpkReader::open(&path).unwrap();
        assert_eq!(reader.records_by_key(42).unwrap().len(), 5);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_run_of_equal_keys_spanning_chunks_comes_back_whole() {
        // The index is chunked, and a repeated key can straddle a boundary in
        // both directions -- the key can be a chunk's first while earlier
        // copies sit at the end of the one before. Searching forward from the
        // chunk that starts with the key loses those. Enough entries here to
        // cross several 512-entry chunks with one key repeated across a
        // boundary.
        let records: Vec<String> = (0..1500u64)
            .map(|i| format!("{i:016x}|record{i:04}"))
            .collect();
        let (mut image, locators) = super::pack_with_locators(&records, 1, CODEC_ZLIB, 0, 4096);
        // Key 7 is carried by 40 records straddling the first chunk boundary.
        let entries: Vec<HashEntry> = locators
            .iter()
            .enumerate()
            .map(|(i, l)| HashEntry {
                key: if (492..532).contains(&i) {
                    7
                } else {
                    1000 + i as u64
                },
                block: l.block,
                row: l.row,
            })
            .collect();
        super::append_hash_index(&mut image, entries);

        let path = write_temp(&image);
        let reader = FpkReader::open(&path).unwrap();
        assert_eq!(
            reader.records_by_key(7).unwrap().len(),
            40,
            "run split across chunks"
        );
        assert_eq!(reader.records_by_key(1000).unwrap().len(), 1);
        assert!(reader.records_by_key(9_999_999).unwrap().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_without_a_hash_index_says_so() {
        let path = write_temp(&pack(&["a|1", "b|2"]));
        let reader = FpkReader::open(&path).unwrap();
        assert!(!reader.has_hash_index());
        assert!(matches!(
            reader.records_by_key(1),
            Err(FpkError::Malformed(_))
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_non_fpk_file_is_rejected() {
        let path = write_temp(b"not an fpk at all, just some bytes here to fill it out ok?...");
        assert!(matches!(
            FpkReader::open(&path),
            Err(FpkError::Malformed(_))
        ));
        std::fs::remove_file(&path).ok();
    }
}
