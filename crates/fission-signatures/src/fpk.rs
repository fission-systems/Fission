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
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAGIC: &[u8; 4] = b"FPK1";
const HEADER_LEN: usize = 72;
const CODEC_ZLIB: u16 = 1;

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
        if u16_at(&map, 6) != CODEC_ZLIB {
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
            if offset.checked_add(comp_len).is_none_or(|e| e > index_offset) {
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
        flate2::read::ZlibDecoder::new(raw)
            .read_to_string(&mut out)
            .map_err(|e| FpkError::Corrupt(format!("block at {}: {e}", block.offset)))?;
        if out.len() != block.raw_len {
            return Err(FpkError::Corrupt(format!(
                "block at {} decompressed to {} bytes, index says {}",
                block.offset,
                out.len(),
                block.raw_len
            )));
        }
        Ok(out)
    }

    /// Lines of the one block that could hold `key`.
    ///
    /// Records are sorted, so the block whose first key is the last one at or
    /// before `key` is the only candidate; a key absent from that block is
    /// absent from the file.
    pub fn block_for(&self, key: &str) -> Result<Option<String>, FpkError> {
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
        self.decompress(&self.blocks[index]).map(Some)
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
    fn a_non_fpk_file_is_rejected() {
        let path = write_temp(b"not an fpk at all, just some bytes here to fill it out ok?...");
        assert!(matches!(
            FpkReader::open(&path),
            Err(FpkError::Malformed(_))
        ));
        std::fs::remove_file(&path).ok();
    }
}
