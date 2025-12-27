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
    
    /// Extract structure definitions with field information
    pub fn extract_structures(db: &[u8]) -> Vec<StructDef> {
        let mut structures = Vec::new();
        
        // Pattern: structure record with fields
        // Structure record format:
        // [len][name][padding][category_id][size_be32][align_be32][field_count_be32]
        
        let mut i = 0;
        while i + 20 < db.len() {
            // Look for structure name pattern (starts with _ and uppercase)
            if db[i] > 0 && db[i] < 64 {  // reasonable name length
                let name_len = db[i] as usize;
                if i + 1 + name_len + 16 < db.len() {
                    // Check if name starts with _ or uppercase
                    if db[i + 1] == b'_' || (db[i + 1] >= b'A' && db[i + 1] <= b'Z') {
                        if let Ok(name) = std::str::from_utf8(&db[i + 1..i + 1 + name_len]) {
                            // Check if it looks like a Windows structure name
                            if Self::is_windows_struct_name(name) {
                                // Try to parse structure info after the name
                                let after_name = i + 1 + name_len;
                                
                                // Skip to find size/alignment info (look for pattern)
                                // Format: [padding to align][category?][size_be32][align_be32][field_count_be32]
                                if after_name + 16 < db.len() {
                                    // Skip padding zeros
                                    let mut j = after_name;
                                    while j < after_name + 10 && j < db.len() && db[j] == 0 {
                                        j += 1;
                                    }
                                    
                                    // Try to read size (should be reasonable value < 10000)
                                    if j + 12 < db.len() {
                                        let size = u32::from_be_bytes([db[j], db[j+1], db[j+2], db[j+3]]);
                                        let align = u32::from_be_bytes([db[j+4], db[j+5], db[j+6], db[j+7]]);
                                        let field_count = u32::from_be_bytes([db[j+8], db[j+9], db[j+10], db[j+11]]);
                                        
                                        // Sanity check
                                        if size > 0 && size < 100000 && align <= 16 && field_count < 500 {
                                            structures.push(StructDef {
                                                name: name.to_string(),
                                                size,
                                                alignment: align,
                                                field_count,
                                                fields: Vec::new(), // Fields parsed separately
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        
        // Deduplicate by name
        structures.sort_by(|a, b| a.name.cmp(&b.name));
        structures.dedup_by(|a, b| a.name == b.name);
        
        structures
    }
    
    /// Check if name looks like a Windows structure
    fn is_windows_struct_name(name: &str) -> bool {
        // Must be at least 4 chars
        if name.len() < 4 {
            return false;
        }
        
        // Check for common patterns
        name.starts_with('_') ||
        name.starts_with("IMAGE_") ||
        name.starts_with("PROCESS_") ||
        name.starts_with("THREAD_") ||
        name.starts_with("MEMORY_") ||
        name.starts_with("SECURITY_") ||
        name.starts_with("TOKEN_") ||
        name.starts_with("RTL_") ||
        name.starts_with("LDR_") ||
        name.starts_with("LIST_") ||
        name.starts_with("UNICODE_") ||
        name.starts_with("EXCEPTION_") ||
        name.starts_with("CONTEXT") ||
        name.starts_with("CRITICAL_") ||
        name.starts_with("OVERLAPPED") ||
        name.starts_with("SOCKADDR") ||
        name.starts_with("STARTUPINFO") ||
        name.starts_with("WIN32_") ||
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
    }
    
    /// Extract field definitions from DB
    pub fn extract_fields(db: &[u8]) -> Vec<FieldDef> {
        let mut fields = Vec::new();
        
        // Field record format:
        // [name_len][field_name][ff ff ff][offset_be32][size_be32][...]
        
        let mut i = 0;
        while i + 20 < db.len() {
            // Look for field name followed by ffff marker
            if db[i] > 2 && db[i] < 64 {  // reasonable name length
                let name_len = db[i] as usize;
                if i + 1 + name_len + 10 < db.len() {
                    // Check for ffff marker after name
                    let marker_start = i + 1 + name_len;
                    if marker_start + 3 < db.len() && 
                       db[marker_start] == 0xff && 
                       db[marker_start + 1] == 0xff &&
                       db[marker_start + 2] == 0xff {
                        if let Ok(name) = std::str::from_utf8(&db[i + 1..i + 1 + name_len]) {
                            // Check if it looks like a field name (camelCase or has common prefixes)
                            if Self::is_field_name(name) {
                                let after_marker = marker_start + 4;
                                if after_marker + 8 < db.len() {
                                    let offset = u32::from_be_bytes([
                                        db[after_marker], db[after_marker+1], 
                                        db[after_marker+2], db[after_marker+3]
                                    ]);
                                    let size = u32::from_be_bytes([
                                        db[after_marker+4], db[after_marker+5],
                                        db[after_marker+6], db[after_marker+7]
                                    ]);
                                    
                                    // Sanity check
                                    if offset < 100000 && size <= 8 && size > 0 {
                                        fields.push(FieldDef {
                                            name: name.to_string(),
                                            offset,
                                            size,
                                            type_name: String::new(), // Would need more parsing
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        
        fields
    }
    
    /// Check if name looks like a structure field
    fn is_field_name(name: &str) -> bool {
        if name.len() < 2 || name.len() > 50 {
            return false;
        }
        // Field names are typically camelCase or have prefixes like dw, lp, h, p, etc.
        let first = name.chars().next().unwrap();
        first.is_ascii_lowercase() ||
        name.starts_with("dw") ||
        name.starts_with("lp") ||
        name.starts_with("cb") ||
        name.starts_with("sz") ||
        name.starts_with("n") ||
        name.starts_with("h") ||
        name.starts_with("p") ||
        name.starts_with("b") ||
        name.starts_with("f") ||
        name.starts_with("c") ||
        name.starts_with("u") ||
        name.starts_with("w") ||
        (first.is_ascii_uppercase() && name.len() > 2)
    }
}

/// Structure definition extracted from GDT
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub size: u32,
    pub alignment: u32,
    pub field_count: u32,
    pub fields: Vec<FieldDef>,
}

/// Field definition within a structure
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub type_name: String,
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
