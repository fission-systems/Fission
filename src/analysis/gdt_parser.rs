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
        
        // Structure record format (based on observation and Ghidra DB schema):
        // [Name String]
        // [Comment String]
        // [Is Union (1 byte)]
        // [Category ID (8 bytes)]
        // [Length (4 bytes)]
        // [Alignment (4 bytes)]
        // [Num Components (4 bytes)]
        
        let mut i = 0;
        while i + 30 < db.len() {
            // 1. Try to read Name
            // Check for valid string length (1-127 for now)
            let name_len = db[i];
            if name_len > 0 && name_len < 0x80 {
                let name_len = name_len as usize;
                
                // Check bounds for Name + Min Record Size (Comment=0, Union=1, Cat=8, Len=4, Align=4, Count=4 = 21 bytes)
                if i + 1 + name_len + 21 <= db.len() {
                     // Check Name Charset
                    let name_bytes = &db[i + 1..i + 1 + name_len];
                    if name_bytes.iter().all(|&b| b.is_ascii_graphic()) {
                         if let Ok(name) = std::str::from_utf8(name_bytes) {
                             if Self::is_windows_struct_name(name) {
                                 let mut curr = i + 1 + name_len;
                                 
                                 // 2. Skip Comment String
                                 let comment_len = db[curr];
                                 if comment_len < 0x80 { // Handle simple strings only for now
                                     curr += 1 + comment_len as usize;
                                     
                                     // 3. Skip Is Union (1 byte)
                                     /* let is_union = db[curr] != 0; */
                                     curr += 1;
                                     
                                     // 4. Skip Category ID (8 bytes)
                                     /* let cat_id = u64::from_be_bytes(...) */
                                     curr += 8;
                                     
                                     // 5. Read Length, Alignment, Count
                                     if curr + 12 <= db.len() {
                                         let size = u32::from_be_bytes([db[curr], db[curr+1], db[curr+2], db[curr+3]]);
                                         let align = u32::from_be_bytes([db[curr+4], db[curr+5], db[curr+6], db[curr+7]]);
                                         let field_count = u32::from_be_bytes([db[curr+8], db[curr+9], db[curr+10], db[curr+11]]);
                                         
                                         // Filter out suspicious values and potential Typedefs
                                         // Typedefs have ID (8 bytes) where Structure has Size(4) + Align(4)
                                         // If we read a Typedef as Structure:
                                         //   Size = ID_High
                                         //   Align = ID_Low
                                         // IDs are often large or odd, while Align MUST be power of 2.
                                         
                                         let is_power_of_2 = align > 0 && (align & (align - 1)) == 0;
                                         
                                         // Heuristic: Small values are likely real sizes. Large values are likely IDs.
                                         let is_valid_layout = size > 0 && size < 65536 && align > 0 && align <= 64;
                                         
                                         if is_valid_layout {
                                             structures.push(StructDef {
                                                 name: name.to_string(),
                                                 size,
                                                 alignment: align,
                                                 field_count,
                                                 fields: Vec::new(),
                                             });
                                         } else {
                                             // Still capture the name, but mark size as 0 (opaque)
                                             structures.push(StructDef {
                                                 name: name.to_string(),
                                                 size: 0,
                                                 alignment: 0,
                                                 field_count: 0,
                                                 fields: Vec::new(),
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
    
    
    /// Extract field definitions from Component Data Types table
    /// Based on RE findings:
    /// RecordID(4) + ParentID(4) + Offset(4) + Unknown(4) + TypeID(4) + NameLen(4) + Name + Comment(4) + Size(4) + Ordinal(4)
    pub fn extract_fields(db: &[u8]) -> Vec<FieldDef> {
        let mut fields = Vec::new();
        
        // Component record structure (all BE u32):
        // +0:  Record ID (4 bytes)
        // +4:  Parent Structure ID (4 bytes)
        // +8:  Field Offset within struct (4 bytes)
        // +12: Unknown (4 bytes)
        // +16: Type ID (4 bytes)
        // +20: Name Length (4 bytes)
        // +24: Name (variable, name_len bytes)
        // +24+name_len: Comment marker (0xFFFFFFFF if null)
        // +24+name_len+4: Component Size (4 bytes)
        // +24+name_len+8: Ordinal (4 bytes)
        
        let mut i = 0;
        while i + 36 < db.len() {
            // Look for potential Component record pattern:
            // 1. Reasonable Parent ID (< 0x10000)
            // 2. Reasonable Offset (< 0x10000)
            // 3. Reasonable Name Length (1-64)
            
            let parent_id = u32::from_be_bytes([db[i+4], db[i+5], db[i+6], db[i+7]]);
            let field_offset = u32::from_be_bytes([db[i+8], db[i+9], db[i+10], db[i+11]]);
            let name_len = u32::from_be_bytes([db[i+20], db[i+21], db[i+22], db[i+23]]) as usize;
            
            // Sanity checks
            if parent_id > 0 && parent_id < 0x100000 &&
               field_offset < 0x100000 &&
               name_len > 0 && name_len < 64 &&
               i + 24 + name_len + 12 < db.len() {
                
                // Check if name is valid ASCII
                let name_start = i + 24;
                let name_bytes = &db[name_start..name_start + name_len];
                
                if name_bytes.iter().all(|&b| b.is_ascii_graphic() || b == b'_') {
                    if let Ok(name) = std::str::from_utf8(name_bytes) {
                        // Check for comment marker (0xFFFFFFFF)
                        let comment_pos = name_start + name_len;
                        if db[comment_pos] == 0xFF && db[comment_pos+1] == 0xFF &&
                           db[comment_pos+2] == 0xFF && db[comment_pos+3] == 0xFF {
                            
                            let comp_size = u32::from_be_bytes([
                                db[comment_pos+4], db[comment_pos+5],
                                db[comment_pos+6], db[comment_pos+7]
                            ]);
                            let ordinal = u32::from_be_bytes([
                                db[comment_pos+8], db[comment_pos+9],
                                db[comment_pos+10], db[comment_pos+11]
                            ]);
                            
                            // Additional sanity: size should be reasonable (1-8192 bytes)
                            if comp_size > 0 && comp_size <= 8192 && ordinal < 10000 {
                                fields.push(FieldDef {
                                    name: name.to_string(),
                                    offset: field_offset,
                                    size: comp_size,
                                    type_name: String::new(),
                                    parent_id,
                                    ordinal,
                                });
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        
        // Sort by parent_id then ordinal for proper grouping
        fields.sort_by(|a, b| {
            a.parent_id.cmp(&b.parent_id)
                .then(a.ordinal.cmp(&b.ordinal))
        });
        
        fields
    }
    
    /// Extract fields grouped by parent structure ID
    pub fn extract_fields_grouped(db: &[u8]) -> std::collections::HashMap<u32, Vec<FieldDef>> {
        let fields = Self::extract_fields(db);
        let mut grouped: std::collections::HashMap<u32, Vec<FieldDef>> = std::collections::HashMap::new();
        
        for field in fields {
            grouped.entry(field.parent_id).or_default().push(field);
        }
        
        grouped
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
    pub parent_id: u32,
    pub ordinal: u32,
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
