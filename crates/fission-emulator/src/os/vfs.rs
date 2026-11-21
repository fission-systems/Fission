//! Guest-facing virtual file system with optional host-file backing.
//!
//! Used by Linux open/openat/read and (when seeded) by the dynlink path so
//! `ld.so` can open the main binary and libraries without a real host FS tree.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The largest a simulated file grows. Its bytes are host memory, and where a
/// write lands is the guest's choice (`lseek` reaches any offset), so without
/// a limit one byte written a terabyte in asks the host for the terabyte.
/// Linux has the same notion -- `RLIMIT_FSIZE`, answered with `EFBIG`.
pub const MAX_FILE_SIZE: usize = 1 << 30;

/// Why a file operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VfsError {
    #[error("bad file descriptor: {0}")]
    BadFd(u64),
    #[error("write would grow the file past {MAX_FILE_SIZE} bytes")]
    TooLarge,
}

impl VfsError {
    /// The Linux `errno` this is reported as.
    pub fn errno(self) -> i64 {
        match self {
            VfsError::BadFd(_) => 9,  // EBADF
            VfsError::TooLarge => 27, // EFBIG
        }
    }
}

type Result<T> = std::result::Result<T, VfsError>;

/// Represents a single file inside the Virtual File System.
#[derive(Clone, Debug)]
pub struct SimFile {
    pub name: String,
    pub content: Vec<u8>,
    pub cursor: usize,
    pub eof: bool,
    /// Optional host path for re-open / diagnostics.
    pub host_path: Option<PathBuf>,
}

impl SimFile {
    pub fn new(name: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            content,
            cursor: 0,
            eof: false,
            host_path: None,
        }
    }

    pub fn with_host(mut self, path: PathBuf) -> Self {
        self.host_path = Some(path);
        self
    }

    pub fn read(&mut self, count: usize) -> Vec<u8> {
        let remaining = self.content.len().saturating_sub(self.cursor);
        let to_read = std::cmp::min(count, remaining);
        if count > 0 && to_read < count {
            self.eof = true;
        }
        if to_read == 0 {
            return Vec::new();
        }
        let start = self.cursor;
        let end = start + to_read;
        self.cursor = end;
        self.content[start..end].to_vec()
    }

    /// Store `data` at the cursor and advance it, growing the file (with
    /// zeros in any gap) when the write ends past its end. A write that would
    /// end past [`MAX_FILE_SIZE`] stores nothing and moves nothing.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        let start = self.cursor;
        let end = start
            .checked_add(data.len())
            .filter(|end| *end <= MAX_FILE_SIZE)
            .ok_or(VfsError::TooLarge)?;
        if data.is_empty() {
            return Ok(0);
        }
        if self.content.len() < end {
            self.content.resize(end, 0);
        }
        self.content[start..end].copy_from_slice(data);
        self.cursor = end;
        Ok(data.len())
    }

    pub fn seek(&mut self, offset: usize) {
        self.cursor = offset;
        self.eof = false;
    }

    pub fn clear_eof(&mut self) {
        self.eof = false;
    }

    pub fn size(&self) -> usize {
        self.content.len()
    }
}

/// The Virtual File System (SimVFS) mapping FDs to SimFiles.
#[derive(Clone, Debug)]
pub struct SimVFS {
    pub files: HashMap<u64, SimFile>,
    pub next_fd: u64,
    /// Guest path (or basename) → preloaded content (e.g. main binary for ld.so).
    pub path_seeds: HashMap<String, Vec<u8>>,
    /// Guest path → host path for lazy open.
    pub host_aliases: HashMap<String, PathBuf>,
    /// Explicit guest symlinks available to `readlinkat`.
    symlinks: BTreeMap<String, String>,
}

impl SimVFS {
    pub fn new() -> Self {
        let mut files = HashMap::new();
        files.insert(0, SimFile::new("stdin", Vec::new()));
        files.insert(1, SimFile::new("stdout", Vec::new()));
        files.insert(2, SimFile::new("stderr", Vec::new()));
        Self {
            files,
            next_fd: 3,
            path_seeds: HashMap::new(),
            host_aliases: HashMap::new(),
            symlinks: BTreeMap::new(),
        }
    }

    /// Preload a guest-visible path with bytes (in-memory).
    pub fn seed_path(&mut self, guest_path: impl Into<String>, content: Vec<u8>) {
        let g = guest_path.into();
        self.path_seeds.insert(g.clone(), content.clone());
        // Also alias basename for open("./prog") style.
        if let Some(base) = Path::new(&g).file_name().and_then(|s| s.to_str()) {
            self.path_seeds.entry(base.to_string()).or_insert(content);
        }
    }

    /// Seed a guest symlink target without consulting the host filesystem.
    pub fn seed_symlink(&mut self, guest_path: impl Into<String>, target: impl Into<String>) {
        self.symlinks.insert(guest_path.into(), target.into());
    }

    /// Read the target of an explicitly seeded guest symlink.
    pub fn readlink(&self, guest_path: &str) -> Option<String> {
        self.symlinks.get(guest_path).cloned()
    }

    /// Map a guest path to a host filesystem file (read on open).
    pub fn alias_host(&mut self, guest_path: impl Into<String>, host: impl Into<PathBuf>) {
        let g = guest_path.into();
        let h = host.into();
        self.host_aliases.insert(g.clone(), h.clone());
        if let Some(base) = Path::new(&g).file_name().and_then(|s| s.to_str()) {
            self.host_aliases.entry(base.to_string()).or_insert(h);
        }
    }

    /// Open by name: seed → host alias → empty buffer.
    pub fn open(&mut self, name: &str, content: Vec<u8>) -> u64 {
        let resolved = self.resolve_content(name, content);
        let fd = self.next_fd;
        self.next_fd += 1;
        let mut file = SimFile::new(name, resolved.0);
        file.host_path = resolved.1;
        self.files.insert(fd, file);
        fd
    }

    fn resolve_content(&self, name: &str, fallback: Vec<u8>) -> (Vec<u8>, Option<PathBuf>) {
        if let Some(c) = self.path_seeds.get(name) {
            return (c.clone(), self.host_aliases.get(name).cloned());
        }
        // Try absolute/relative normalization of trailing component.
        let base = Path::new(name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        if base != name {
            if let Some(c) = self.path_seeds.get(base) {
                return (c.clone(), self.host_aliases.get(base).cloned());
            }
        }
        if let Some(host) = self
            .host_aliases
            .get(name)
            .or_else(|| self.host_aliases.get(base))
        {
            if let Ok(bytes) = std::fs::read(host) {
                return (bytes, Some(host.clone()));
            }
        }
        // Direct host open when path exists (dynlink / library search).
        let p = Path::new(name);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(p) {
                return (bytes, Some(p.to_path_buf()));
            }
        }
        (fallback, None)
    }

    pub fn read(&mut self, fd: u64, count: usize) -> Result<Vec<u8>> {
        let file = self.files.get_mut(&fd).ok_or(VfsError::BadFd(fd))?;
        Ok(file.read(count))
    }

    pub fn write(&mut self, fd: u64, data: &[u8]) -> Result<usize> {
        let file = self.files.get_mut(&fd).ok_or(VfsError::BadFd(fd))?;
        file.write(data)
    }

    pub fn close(&mut self, fd: u64) -> Result<()> {
        if fd <= 2 {
            // Don't close stdio.
            return Ok(());
        }
        if self.files.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(VfsError::BadFd(fd))
        }
    }

    pub fn seek(&mut self, fd: u64, offset: usize) -> Result<usize> {
        let file = self.files.get_mut(&fd).ok_or(VfsError::BadFd(fd))?;
        file.seek(offset);
        Ok(file.cursor)
    }

    pub fn file_size(&self, fd: u64) -> Option<usize> {
        self.files.get(&fd).map(|f| f.size())
    }

    /// The bytes an open FD holds in `[offset, offset + len)`, without moving
    /// its cursor: fewer than `len` where the file ends first, none where it
    /// ends before `offset`.
    pub fn file_bytes(&self, fd: u64, offset: usize, len: usize) -> Result<&[u8]> {
        let file = self.files.get(&fd).ok_or(VfsError::BadFd(fd))?;
        let size = file.content.len();
        let start = offset.min(size);
        let end = offset.saturating_add(len).min(size);
        Ok(&file.content[start..end])
    }

    /// [`Self::file_bytes`] padded with zeros to exactly `len` bytes, which
    /// is how a file-backed mapping reads past the end of its file. `len` is
    /// a host allocation: pass one the caller has bounded.
    pub fn peek(&self, fd: u64, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = self.file_bytes(fd, offset, len)?.to_vec();
        out.resize(len, 0);
        Ok(out)
    }
}

impl Default for SimVFS {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lseek` may leave the cursor anywhere a `usize` reaches. A write there
    /// computed `cursor + len` unchecked: an overflow panic in a debug build,
    /// and in a release one a wrapped end that the slice then rejected.
    #[test]
    fn a_write_at_the_end_of_the_offset_range_fails_without_overflowing() {
        let mut vfs = SimVFS::new();
        let fd = vfs.open("scratch", Vec::new());
        vfs.seek(fd, usize::MAX - 1).unwrap();
        assert!(vfs.write(fd, b"abcd").is_err());
        // The failed write moved nothing and stored nothing.
        assert_eq!(vfs.file_size(fd), Some(0));
        assert_eq!(vfs.files[&fd].cursor, usize::MAX - 1);
    }

    /// A file is as large as the host memory behind it, so a sparse write --
    /// one byte a gigabyte past the start -- is refused rather than backed.
    #[test]
    fn a_write_far_past_the_end_is_refused_rather_than_backed_by_host_memory() {
        let mut vfs = SimVFS::new();
        let fd = vfs.open("scratch", Vec::new());
        vfs.seek(fd, MAX_FILE_SIZE).unwrap();
        assert_eq!(vfs.write(fd, b"x"), Err(VfsError::TooLarge));
        assert_eq!(vfs.file_size(fd), Some(0));

        // Positive control: the same write inside the limit is stored, with
        // the gap reading as zeros.
        vfs.seek(fd, 0x2000).unwrap();
        assert_eq!(vfs.write(fd, b"x"), Ok(1));
        assert_eq!(vfs.file_size(fd), Some(0x2001));
        assert_eq!(vfs.peek(fd, 0x1fff, 2).unwrap(), [0, b'x']);
    }

    /// The range read for a mapping added `offset + len` unchecked.
    #[test]
    fn a_range_whose_end_overflows_is_what_the_file_holds_from_there() {
        let mut vfs = SimVFS::new();
        let fd = vfs.open("scratch", b"hello".to_vec());
        assert_eq!(vfs.file_bytes(fd, 1, usize::MAX).unwrap(), b"ello");
        assert_eq!(vfs.file_bytes(fd, usize::MAX, usize::MAX).unwrap(), b"");
        assert_eq!(vfs.peek(fd, 3, 4).unwrap(), [b'l', b'o', 0, 0]);
    }
}
