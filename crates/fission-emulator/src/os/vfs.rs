use std::collections::HashMap;
use anyhow::{Result, bail};

/// Represents a single file inside the Virtual File System.
/// A SimFile can either be an in-memory buffer (symbolic or concrete) or a passthrough to a host file.
#[derive(Clone, Debug)]
pub struct SimFile {
    pub name: String,
    pub content: Vec<u8>,
    pub cursor: usize,
}

impl SimFile {
    pub fn new(name: impl Into<String>, content: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            content,
            cursor: 0,
        }
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
        self.cursor = offset;
    }
}

/// The Virtual File System (SimVFS) mapping FDs to SimFiles.
#[derive(Clone, Debug)]
pub struct SimVFS {
    pub files: HashMap<u64, SimFile>,
    pub next_fd: u64,
}

impl SimVFS {
    pub fn new() -> Self {
        let mut files = HashMap::new();
        // Initialize stdio
        files.insert(0, SimFile::new("stdin", Vec::new()));
        files.insert(1, SimFile::new("stdout", Vec::new()));
        files.insert(2, SimFile::new("stderr", Vec::new()));
        
        Self { files, next_fd: 3 }
    }

    pub fn open(&mut self, name: &str, content: Vec<u8>) -> u64 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.files.insert(fd, SimFile::new(name, content));
        fd
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
        if self.files.remove(&fd).is_some() {
            Ok(())
        } else {
            bail!("Bad file descriptor: {}", fd)
        }
    }
}

impl Default for SimVFS {
    fn default() -> Self {
        Self::new()
    }
}
