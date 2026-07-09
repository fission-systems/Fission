//! Guest-facing virtual file system with optional host-file backing.
//!
//! Used by Linux open/openat/read and (when seeded) by the dynlink path so
//! `ld.so` can open the main binary and libraries without a real host FS tree.

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Represents a single file inside the Virtual File System.
#[derive(Clone, Debug)]
pub struct SimFile {
    pub name: String,
    pub content: Vec<u8>,
    pub cursor: usize,
    /// Optional host path for re-open / diagnostics.
    pub host_path: Option<PathBuf>,
}

impl SimFile {
    pub fn new(name: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            content,
            cursor: 0,
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
        let start = self.cursor;
        let end = start + to_read;
        self.cursor = end;
        self.content[start..end].to_vec()
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let start = self.cursor;
        let required_len = start + data.len();
        if self.content.len() < required_len {
            self.content.resize(required_len, 0);
        }
        self.content[start..required_len].copy_from_slice(data);
        self.cursor += data.len();
        data.len()
    }

    pub fn seek(&mut self, offset: usize) {
        self.cursor = offset.min(self.content.len());
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
        if let Some(host) = self.host_aliases.get(name).or_else(|| self.host_aliases.get(base)) {
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
        if let Some(file) = self.files.get_mut(&fd) {
            Ok(file.read(count))
        } else {
            bail!("Bad file descriptor: {}", fd)
        }
    }

    pub fn write(&mut self, fd: u64, data: &[u8]) -> Result<usize> {
        if let Some(file) = self.files.get_mut(&fd) {
            Ok(file.write(data))
        } else {
            bail!("Bad file descriptor: {}", fd)
        }
    }

    pub fn close(&mut self, fd: u64) -> Result<()> {
        if fd <= 2 {
            // Don't close stdio.
            return Ok(());
        }
        if self.files.remove(&fd).is_some() {
            Ok(())
        } else {
            bail!("Bad file descriptor: {}", fd)
        }
    }

    pub fn seek(&mut self, fd: u64, offset: usize) -> Result<usize> {
        if let Some(file) = self.files.get_mut(&fd) {
            file.seek(offset);
            Ok(file.cursor)
        } else {
            bail!("Bad file descriptor: {}", fd)
        }
    }

    pub fn file_size(&self, fd: u64) -> Option<usize> {
        self.files.get(&fd).map(|f| f.size())
    }

    /// Read a slice from an open FD without advancing the cursor (for mmap).
    pub fn peek(&self, fd: u64, offset: usize, len: usize) -> Result<Vec<u8>> {
        let file = self
            .files
            .get(&fd)
            .ok_or_else(|| anyhow::anyhow!("Bad file descriptor: {fd}"))?;
        if offset >= file.content.len() {
            return Ok(vec![0; len]);
        }
        let end = (offset + len).min(file.content.len());
        let mut out = file.content[offset..end].to_vec();
        if out.len() < len {
            out.resize(len, 0);
        }
        Ok(out)
    }
}

impl Default for SimVFS {
    fn default() -> Self {
        Self::new()
    }
}
