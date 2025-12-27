//! GDT (Ghidra Data Type) File Parser
//! 
//! Parses Ghidra .gdt files to extract Windows type definitions.
//! GDT format: Java Serialization header + embedded ZIP with FOLDER_ITEM DB

use std::io::{Read, Write, Cursor};
use std::fs::File;
use std::path::Path;

use flate2::read::DeflateDecoder;

/// GDT file parsing errors
#[derive(Debug)]
pub enum GdtError {
    Io(std::io::Error),
    InvalidHeader,
    ZipNotFound,
    DecompressFailed,
    InvalidDb,
}

impl From<std::io::Error> for GdtError {
    fn from(e: std::io::Error) -> Self {
        GdtError::Io(e)
    }
}

/// Extracted type information from GDT
#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub size: u32,
    pub category: String,
}

/// GDT file parser
pub struct GdtParser {
    data: Vec<u8>,
}

impl GdtParser {
    /// Load a GDT file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, GdtError> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        Ok(Self { data })
    }
    
    /// Load from bytes
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }
    
    /// Verify Java serialization header (AC ED 00 05)
    pub fn verify_header(&self) -> bool {
        self.data.len() >= 4 &&
        self.data[0] == 0xAC &&
        self.data[1] == 0xED &&
        self.data[2] == 0x00 &&
        self.data[3] == 0x05
    }
    
    /// Find ZIP local header (PK 03 04)
    fn find_zip_local(&self) -> Option<usize> {
        let pattern = [0x50, 0x4B, 0x03, 0x04]; // PK\x03\x04
        self.data.windows(4)
            .position(|w| w == pattern)
    }
    
    /// Find ZIP data descriptor (PK 07 08)
    fn find_zip_descriptor(&self, start: usize) -> Option<usize> {
        let pattern = [0x50, 0x4B, 0x07, 0x08];
        self.data[start..].windows(4)
            .position(|w| w == pattern)
            .map(|p| p + start)
    }
    
    /// Read little-endian u16
    fn read_le16(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.data[offset], self.data[offset + 1]])
    }
    
    /// Read little-endian u32
    fn read_le32(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
    
    /// Extract and decompress the FOLDER_ITEM payload
    pub fn extract_db(&self) -> Result<Vec<u8>, GdtError> {
        if !self.verify_header() {
            return Err(GdtError::InvalidHeader);
        }
        
        // Find ZIP local header
        let local_off = self.find_zip_local().ok_or(GdtError::ZipNotFound)?;
        
        if local_off + 30 > self.data.len() {
            return Err(GdtError::ZipNotFound);
        }
        
        // Parse ZIP local header
        let method = self.read_le16(local_off + 8);
        let name_len = self.read_le16(local_off + 26) as usize;
        let extra_len = self.read_le16(local_off + 28) as usize;
        let comp_start = local_off + 30 + name_len + extra_len;
        
        // Find data descriptor
        let desc_off = self.find_zip_descriptor(comp_start).ok_or(GdtError::ZipNotFound)?;
        
        if desc_off + 16 > self.data.len() {
            return Err(GdtError::ZipNotFound);
        }
        
        // Get uncompressed size from descriptor
        let uncomp_size = self.read_le32(desc_off + 12) as usize;
        
        // Must be deflate compression
        if method != 8 {
            return Err(GdtError::DecompressFailed);
        }
        
        // Decompress
        let comp_data = &self.data[comp_start..desc_off];
        let mut decoder = DeflateDecoder::new(Cursor::new(comp_data));
        let mut decompressed = Vec::with_capacity(uncomp_size);
        decoder.read_to_end(&mut decompressed).map_err(|_| GdtError::DecompressFailed)?;
        
        Ok(decompressed)
    }
    
    /// Extract type names from the decompressed DB
    pub fn extract_type_names(db: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        let s = String::from_utf8_lossy(db);
        
        // Find uppercase identifiers (likely type names)
        for word in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if word.len() >= 5 && 
               word.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) &&
               word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
                if !names.contains(&word.to_string()) {
                    names.push(word.to_string());
                }
            }
        }
        
        names.sort();
        names.dedup();
        names
    }
    
    /// Extract structure-like type names (with underscore prefix or known patterns)
    pub fn extract_struct_names(db: &[u8]) -> Vec<String> {
        let all_names = Self::extract_type_names(db);
        
        all_names.into_iter()
            .filter(|name| {
                // Filter for likely structure names
                name.starts_with('_') ||
                name.starts_with("IMAGE_") ||
                name.starts_with("PROCESS_") ||
                name.starts_with("THREAD_") ||
                name.starts_with("MEMORY_") ||
                name.starts_with("SECURITY_") ||
                name.starts_with("TOKEN_") ||
                name.starts_with("CONTEXT") ||
                name.starts_with("EXCEPTION_") ||
                name.starts_with("RTL_") ||
                name.starts_with("LDR_") ||
                name.starts_with("UNICODE_") ||
                name.starts_with("LIST_ENTRY") ||
                name.starts_with("SOCKADDR") ||
                name.starts_with("OVERLAPPED") ||
                name.starts_with("CRITICAL_SECTION") ||
                name.starts_with("STARTUPINFO") ||
                name.starts_with("WIN32_FIND") ||
                name == "FILETIME" ||
                name == "GUID" ||
                name == "LARGE_INTEGER" ||
                name == "POINT" ||
                name == "RECT" ||
                name == "SIZE" ||
                name == "MSG" ||
                name == "WSADATA" ||
                name == "PEB" ||
                name == "TEB"
            })
            .collect()
    }
}

/// DB header information
#[derive(Debug)]
pub struct DbHeader {
    pub signature: String,
    pub id: u64,
    pub version: u32,
    pub page_size: u32,
    pub page_count: usize,
}

impl DbHeader {
    /// Parse DB header from decompressed data
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 28 {
            return None;
        }
        
        let signature = String::from_utf8_lossy(&data[0..8]).to_string();
        let id = u64::from_be_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        let version = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let page_size = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        
        let page_count = if page_size > 0 {
            data.len() / page_size as usize
        } else {
            0
        };
        
        Some(Self {
            signature,
            id,
            version,
            page_size,
            page_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verify_header() {
        let valid = vec![0xAC, 0xED, 0x00, 0x05, 0x00, 0x00];
        let parser = GdtParser::from_bytes(valid);
        assert!(parser.verify_header());
        
        let invalid = vec![0x00, 0x00, 0x00, 0x00];
        let parser = GdtParser::from_bytes(invalid);
        assert!(!parser.verify_header());
    }
}
