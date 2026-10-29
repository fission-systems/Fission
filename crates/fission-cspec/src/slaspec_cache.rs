//! On-disk cache for the register list extracted from a `.slaspec`.
//!
//! Expanding the preprocessor over the whole include tree and scanning it for
//! `define register` blocks reads and rewrites every line of a spec that is
//! hundreds of kilobytes for x86 -- about 14 ms on every CLI process, for a
//! result that only changes when the spec does. The result is a pure function
//! of the bytes of the files the expansion read and of the parser itself, so
//! the cache stores both as its key:
//!
//! * the content hash of every file the expansion read, in read order; and
//! * a hash of `slaspec_parse.rs`, so editing the parser invalidates entries
//!   written by an older parser instead of serving its output.
//!
//! A stale or damaged entry is never an error: every failure path returns
//! `None` and the caller re-parses.

use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use rustc_hash::FxHasher;

use super::slaspec_parse::ParsedRegister;

/// Bump when the on-disk layout below changes.
const LAYOUT: u32 = 1;
const MAGIC: &[u8; 4] = b"FRD1";

/// Directory entries live in, or `None` when no cache location is known.
///
/// Unit tests of the public parse entry point run uncached so they neither
/// depend on nor write into the developer's cache.
pub(crate) fn default_cache_dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let root = match std::env::var_os("FISSION_SLEIGH_CACHE_DIR") {
        Some(path) => PathBuf::from(path),
        None => fission_core::resource_roots::fission_cache_dir()?,
    };
    Some(root.join("register-defs"))
}

/// Cache file for one entry spec inside `dir`.
pub(crate) fn entry_path(dir: &Path, entry_spec: &Path) -> PathBuf {
    let canonical = fs::canonicalize(entry_spec).unwrap_or_else(|_| entry_spec.to_path_buf());
    let stem = entry_spec
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("spec");
    let mut hasher = FxHasher::default();
    hasher.write(canonical.as_os_str().as_encoded_bytes());
    dir.join(format!("{stem}.{:016x}.v{LAYOUT}.bin", hasher.finish()))
}

fn parser_fingerprint() -> u64 {
    content_hash(include_bytes!("slaspec_parse.rs"))
}

pub(crate) fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write_usize(bytes.len());
    hasher.write(bytes);
    hasher.finish()
}

/// Load the registers stored at `path` if every file they were derived from
/// still has the content it had then.
pub(crate) fn load(path: &Path) -> Option<Vec<ParsedRegister>> {
    let bytes = fs::read(path).ok()?;
    let (body, tail) = bytes.split_at(bytes.len().checked_sub(8)?);
    if content_hash(body) != u64::from_le_bytes(tail.try_into().ok()?) {
        return None;
    }
    let mut reader = Reader { bytes: body };
    if reader.take(4)? != MAGIC || reader.u64()? != parser_fingerprint() {
        return None;
    }
    for _ in 0..reader.u32()? {
        let source = PathBuf::from(reader.string()?);
        let expected = reader.u64()?;
        if content_hash(&fs::read(&source).ok()?) != expected {
            return None;
        }
    }
    let count = reader.u32()? as usize;
    let mut registers = Vec::with_capacity(count);
    for _ in 0..count {
        registers.push(ParsedRegister {
            name: reader.string()?,
            offset: reader.u64()?,
            size: reader.u32()?,
            slot_index: usize::try_from(reader.u64()?).ok()?,
            base_offset: reader.u64()?,
        });
    }
    reader.bytes.is_empty().then_some(registers)
}

/// Store `registers` keyed by `sources`: each file read during expansion with
/// the hash of the bytes that were actually parsed (not re-read here, so a file
/// edited mid-parse cannot be recorded under content the result never saw).
/// Best effort: a read-only or missing cache location just means the next run
/// parses again.
pub(crate) fn store(path: &Path, sources: &[(PathBuf, u64)], registers: &[ParsedRegister]) {
    let Some(bytes) = encode(sources, registers) else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    // Write beside the target and rename, so a concurrent reader sees either
    // the previous complete entry or this one, never a prefix.
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&tmp, bytes).is_err() || fs::rename(&tmp, path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

fn encode(sources: &[(PathBuf, u64)], registers: &[ParsedRegister]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&parser_fingerprint().to_le_bytes());
    out.extend_from_slice(&u32::try_from(sources.len()).ok()?.to_le_bytes());
    for (source, hash) in sources {
        put_string(&mut out, source.to_str()?)?;
        out.extend_from_slice(&hash.to_le_bytes());
    }
    out.extend_from_slice(&u32::try_from(registers.len()).ok()?.to_le_bytes());
    for reg in registers {
        put_string(&mut out, &reg.name)?;
        out.extend_from_slice(&reg.offset.to_le_bytes());
        out.extend_from_slice(&reg.size.to_le_bytes());
        out.extend_from_slice(&u64::try_from(reg.slot_index).ok()?.to_le_bytes());
        out.extend_from_slice(&reg.base_offset.to_le_bytes());
    }
    let checksum = content_hash(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    Some(out)
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Option<()> {
    out.extend_from_slice(&u32::try_from(value.len()).ok()?.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Some(())
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.bytes.split_at_checked(len)?;
        self.bytes = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slaspec_parse::parse_registers_cached;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch_dir(tag: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fission-slaspec-cache-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(registers: &[ParsedRegister]) -> Vec<&str> {
        registers.iter().map(|r| r.name.as_str()).collect()
    }

    fn write_spec(dir: &Path, sinc_registers: &str) -> PathBuf {
        fs::write(
            dir.join("regs.sinc"),
            format!("define register offset=0 size=4 [ {sinc_registers} ];\n"),
        )
        .unwrap();
        let entry = dir.join("main.slaspec");
        fs::write(&entry, "@include \"regs.sinc\"\n").unwrap();
        entry
    }

    #[test]
    fn second_parse_is_served_from_the_entry_and_equals_the_first() {
        let spec_dir = scratch_dir("spec");
        let cache_dir = scratch_dir("cache");
        let entry = write_spec(&spec_dir, "r0 r1 r2");
        let first = parse_registers_cached(&entry, Some(&cache_dir)).unwrap();
        let path = entry_path(&cache_dir, &entry);
        assert!(path.exists(), "first parse must write an entry");
        assert_eq!(load(&path).as_deref(), Some(first.as_slice()));
        assert_eq!(
            parse_registers_cached(&entry, Some(&cache_dir)).unwrap(),
            first
        );
        assert_eq!(names(&first), ["r0", "r1", "r2"]);
    }

    #[test]
    fn editing_an_included_file_invalidates_the_entry() {
        let spec_dir = scratch_dir("spec");
        let cache_dir = scratch_dir("cache");
        let entry = write_spec(&spec_dir, "r0 r1");
        parse_registers_cached(&entry, Some(&cache_dir)).unwrap();
        let path = entry_path(&cache_dir, &entry);
        assert!(load(&path).is_some());

        write_spec(&spec_dir, "r0 r1 r2");
        assert!(load(&path).is_none(), "stale entry must not load");
        let reparsed = parse_registers_cached(&entry, Some(&cache_dir)).unwrap();
        assert_eq!(names(&reparsed), ["r0", "r1", "r2"]);
        assert_eq!(load(&path).as_deref(), Some(reparsed.as_slice()));
    }

    #[test]
    fn a_damaged_entry_falls_back_to_parsing() {
        let spec_dir = scratch_dir("spec");
        let cache_dir = scratch_dir("cache");
        let entry = write_spec(&spec_dir, "r0 r1");
        let first = parse_registers_cached(&entry, Some(&cache_dir)).unwrap();
        let path = entry_path(&cache_dir, &entry);

        let good = fs::read(&path).unwrap();
        // Truncated, flipped byte, and empty: none may load or panic.
        for damaged in [
            good[..good.len() / 2].to_vec(),
            {
                let mut flipped = good.clone();
                flipped[good.len() / 2] ^= 0xff;
                flipped
            },
            Vec::new(),
        ] {
            fs::write(&path, damaged).unwrap();
            assert!(load(&path).is_none());
            assert_eq!(
                parse_registers_cached(&entry, Some(&cache_dir)).unwrap(),
                first
            );
        }
    }

    #[test]
    fn an_unwritable_cache_location_does_not_fail_the_parse() {
        let spec_dir = scratch_dir("spec");
        let entry = write_spec(&spec_dir, "r0");
        // A file where the directory should be: create_dir_all fails.
        let blocker = scratch_dir("blocker").join("not-a-dir");
        fs::write(&blocker, b"x").unwrap();
        let regs = parse_registers_cached(&entry, Some(&blocker.join("sub"))).unwrap();
        assert_eq!(names(&regs), ["r0"]);
    }

    #[test]
    fn a_real_spec_round_trips_through_the_entry_unchanged() {
        let root = crate::apply::sleigh_languages_root();
        let spec = crate::ldefs::global_language_slaspec_index(&root)["x86:LE:64:default"].clone();
        let cache_dir = scratch_dir("cache");
        let uncached = parse_registers_cached(&spec, None).unwrap();
        let first = parse_registers_cached(&spec, Some(&cache_dir)).unwrap();
        let cached = parse_registers_cached(&spec, Some(&cache_dir)).unwrap();
        assert_eq!(first, uncached);
        assert_eq!(cached, uncached);
        assert!(load(&entry_path(&cache_dir, &spec)).is_some());
    }
}
