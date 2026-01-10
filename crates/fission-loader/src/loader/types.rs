use crate::prelude::*;
use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// ============================================================================
// rkyv Wrapper for Arc<Vec<u8>>
// ============================================================================

/// Custom rkyv wrapper for `Arc<Vec<u8>>` that serializes as `Vec<u8>`.
///
/// This enables efficient cloning of LoadedBinary while maintaining
/// compatibility with rkyv serialization (e.g., for snapshots).
pub struct ArcVecWrapper;

impl rkyv::with::ArchiveWith<Arc<Vec<u8>>> for ArcVecWrapper {
    type Archived = rkyv::vec::ArchivedVec<u8>;
    type Resolver = rkyv::vec::VecResolver;

    #[inline]
    unsafe fn resolve_with(
        field: &Arc<Vec<u8>>,
        pos: usize,
        resolver: Self::Resolver,
        out: *mut Self::Archived,
    ) {
        rkyv::vec::ArchivedVec::resolve_from_slice(field.as_slice(), pos, resolver, out);
    }
}

impl<S: rkyv::ser::Serializer + rkyv::ser::ScratchSpace + ?Sized>
    rkyv::with::SerializeWith<Arc<Vec<u8>>, S> for ArcVecWrapper
{
    fn serialize_with(
        field: &Arc<Vec<u8>>,
        serializer: &mut S,
    ) -> std::result::Result<Self::Resolver, S::Error> {
        rkyv::vec::ArchivedVec::serialize_from_slice(field.as_slice(), serializer)
    }
}

impl<D: rkyv::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::vec::ArchivedVec<u8>, Arc<Vec<u8>>, D> for ArcVecWrapper
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<u8>,
        deserializer: &mut D,
    ) -> std::result::Result<Arc<Vec<u8>>, D::Error> {
        // ArchivedVec<u8> can be converted to a slice directly, then to Vec
        let vec: Vec<u8> = field.as_slice().to_vec();
        Ok(Arc::new(vec))
    }
}

/// Information about a function found in the binary
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct FunctionInfo {
    /// Function name (may be empty for unnamed functions)
    pub name: String,
    /// Virtual address of the function
    pub address: u64,
    /// Size in bytes (0 if unknown)
    pub size: u64,
    /// Whether this is an exported function
    pub is_export: bool,
    /// Whether this is an imported function (stub)
    pub is_import: bool,
}

/// Information about a section in the binary
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct SectionInfo {
    /// Section name
    pub name: String,
    /// Virtual address
    pub virtual_address: u64,
    /// Size in memory
    pub virtual_size: u64,
    /// Offset in file
    pub file_offset: u64,
    /// Size in file
    pub file_size: u64,
    /// Is this section executable?
    pub is_executable: bool,
    /// Is this section readable?
    pub is_readable: bool,
    /// Is this section writable?
    pub is_writable: bool,
}

/// Parsed binary information
///
/// Note: The `data` field uses `Arc<Vec<u8>>` for efficient cloning.
/// When cloning a LoadedBinary, only metadata is copied while the raw bytes
/// are shared via reference counting. This enables cheap "copy-on-write"
/// semantics: patching operations will only clone the data when necessary.
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct LoadedBinary {
    /// Original file path
    pub path: String,
    /// Raw bytes of the file (Arc for cheap clone, COW via Arc::make_mut)
    #[with(ArcVecWrapper)]
    pub data: Arc<Vec<u8>>,
    /// Detected architecture (e.g., "x86:LE:64:default")
    pub arch_spec: String,
    /// Entry point address
    pub entry_point: u64,
    /// Image base address
    pub image_base: u64,
    /// All discovered functions (kept sorted by address for efficient access)
    pub functions: Vec<FunctionInfo>,
    /// All sections
    pub sections: Vec<SectionInfo>,
    /// Is this a 64-bit binary?
    pub is_64bit: bool,
    /// Does the image contain a CLR (.NET) runtime header?
    pub is_dotnet: bool,
    /// Reported CLR metadata version string (e.g. "v4.0.30319")
    pub dotnet_runtime_version: Option<String>,
    /// Binary format (PE, ELF, Mach-O)
    pub format: String,
    /// IAT address to symbol name mapping for decompiler output
    pub iat_symbols: std::collections::HashMap<u64, String>,
    /// Global data symbol mapping (address -> name) for decompiler output
    pub global_symbols: std::collections::HashMap<u64, String>,
    /// Index of functions by address for O(1) lookup
    pub function_addr_index: std::collections::HashMap<u64, usize>,
    /// Index of functions by name for O(1) lookup
    pub function_name_index: std::collections::HashMap<String, usize>,
    /// Flag indicating functions are sorted by address
    /// This is set during build() and after discover_internal_functions()
    /// Note: This is a runtime-only flag, serialized as false by default
    #[with(rkyv::with::Skip)]
    functions_sorted: bool,
}

/// Builder for LoadedBinary
pub struct LoadedBinaryBuilder {
    path: String,
    data: Vec<u8>,
    arch_spec: String,
    entry_point: u64,
    image_base: u64,
    functions: Vec<FunctionInfo>,
    sections: Vec<SectionInfo>,
    is_64bit: bool,
    is_dotnet: bool,
    dotnet_runtime_version: Option<String>,
    format: String,
    iat_symbols: std::collections::HashMap<u64, String>,
    global_symbols: std::collections::HashMap<u64, String>,
}

impl LoadedBinaryBuilder {
    pub fn new(path: String, data: Vec<u8>) -> Self {
        Self {
            path,
            data,
            arch_spec: "unknown".to_string(),
            entry_point: 0,
            image_base: 0,
            functions: Vec::new(),
            sections: Vec::new(),
            is_64bit: false,
            is_dotnet: false,
            dotnet_runtime_version: None,
            format: "unknown".to_string(),
            iat_symbols: std::collections::HashMap::new(),
            global_symbols: std::collections::HashMap::new(),
        }
    }

    pub fn arch_spec(mut self, arch_spec: impl Into<String>) -> Self {
        self.arch_spec = arch_spec.into();
        self
    }

    pub fn entry_point(mut self, entry_point: u64) -> Self {
        self.entry_point = entry_point;
        self
    }

    pub fn image_base(mut self, image_base: u64) -> Self {
        self.image_base = image_base;
        self
    }

    pub fn is_64bit(mut self, is_64bit: bool) -> Self {
        self.is_64bit = is_64bit;
        self
    }

    pub fn is_dotnet(mut self, is_dotnet: bool) -> Self {
        self.is_dotnet = is_dotnet;
        self
    }

    pub fn dotnet_runtime_version(mut self, version: Option<String>) -> Self {
        self.dotnet_runtime_version = version;
        self
    }

    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    pub fn add_function(mut self, function: FunctionInfo) -> Self {
        self.functions.push(function);
        self
    }

    pub fn add_functions(mut self, functions: impl IntoIterator<Item = FunctionInfo>) -> Self {
        self.functions.extend(functions);
        self
    }

    pub fn add_section(mut self, section: SectionInfo) -> Self {
        self.sections.push(section);
        self
    }

    pub fn add_sections(mut self, sections: impl IntoIterator<Item = SectionInfo>) -> Self {
        self.sections.extend(sections);
        self
    }

    pub fn add_iat_symbol(mut self, va: u64, name: String) -> Self {
        self.iat_symbols.insert(va, name);
        self
    }

    pub fn add_iat_symbols(mut self, symbols: std::collections::HashMap<u64, String>) -> Self {
        self.iat_symbols.extend(symbols);
        self
    }

    pub fn add_global_symbol(mut self, va: u64, name: String) -> Self {
        self.global_symbols.insert(va, name);
        self
    }

    pub fn add_global_symbols(mut self, symbols: std::collections::HashMap<u64, String>) -> Self {
        self.global_symbols.extend(symbols);
        self
    }

    pub fn build(self) -> Result<LoadedBinary> {
        // Sort functions by address during build for efficient sorted access
        let mut functions = self.functions;
        functions.sort_by_key(|f| f.address);

        let mut binary = LoadedBinary {
            path: self.path,
            data: Arc::new(self.data),
            arch_spec: self.arch_spec,
            entry_point: self.entry_point,
            image_base: self.image_base,
            functions,
            sections: self.sections,
            is_64bit: self.is_64bit,
            is_dotnet: self.is_dotnet,
            dotnet_runtime_version: self.dotnet_runtime_version,
            format: self.format,
            iat_symbols: self.iat_symbols,
            global_symbols: self.global_symbols,
            function_addr_index: std::collections::HashMap::new(),
            function_name_index: std::collections::HashMap::new(),
            functions_sorted: true,
        };

        // Build indices
        for (idx, func) in binary.functions.iter().enumerate() {
            binary.function_addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                binary.function_name_index.insert(func.name.clone(), idx);
            }
        }

        Ok(binary)
    }
}

impl LoadedBinary {
    /// Sort sections by virtual address for binary search
    pub fn sort_sections(&mut self) {
        self.sections.sort_by_key(|s| s.virtual_address);
    }

    /// Get bytes at a given address using binary search for O(log N) lookup
    pub fn get_bytes(&self, address: u64, size: usize) -> Option<Vec<u8>> {
        // Binary search to find the section containing this address
        // Sections must be sorted by virtual_address (done during parsing)
        let idx = self.sections.binary_search_by(|section| {
            if address < section.virtual_address {
                std::cmp::Ordering::Greater
            } else if address >= section.virtual_address + section.virtual_size {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });

        if let Ok(idx) = idx {
            let section = &self.sections[idx];
            let offset_in_section = address - section.virtual_address;
            let file_offset = section.file_offset + offset_in_section;
            let end = (file_offset as usize + size).min(self.data.len());
            let start = file_offset as usize;

            if start < self.data.len() {
                return Some(self.data[start..end].to_vec());
            }
        }
        None
    }

    /// Get executable sections only
    pub fn executable_sections(&self) -> Vec<&SectionInfo> {
        self.sections.iter().filter(|s| s.is_executable).collect()
    }

    /// Get functions sorted by address
    ///
    /// Performance: Returns references to already-sorted functions when possible.
    /// Functions are sorted during build() and discover_internal_functions(),
    /// avoiding redundant sorting on each call.
    pub fn functions_sorted(&self) -> Vec<&FunctionInfo> {
        if self.functions_sorted {
            // Functions are already sorted, just return references
            self.functions.iter().collect()
        } else {
            // Fallback: sort on demand (should rarely happen after build)
            let mut funcs: Vec<_> = self.functions.iter().collect();
            funcs.sort_by_key(|f| f.address);
            funcs
        }
    }

    /// Get iterator over functions (already sorted by address)
    ///
    /// Performance: Always zero-allocation since it returns a slice iterator.
    /// Prefer this over functions_sorted() to avoid Vec allocation when
    /// only iteration is needed.
    #[inline]
    pub fn functions_iter(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter()
    }

    /// Find a function by name using O(1) HashMap lookup
    pub fn find_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.function_name_index
            .get(name)
            .and_then(|&idx| self.functions.get(idx))
    }

    /// Find function at exact address using O(1) HashMap lookup
    pub fn function_at(&self, address: u64) -> Option<&FunctionInfo> {
        // First try exact address match using the index (O(1))
        if let Some(&idx) = self.function_addr_index.get(&address) {
            return self.functions.get(idx);
        }

        // Fall back to range check for addresses within function bounds (O(N))
        // This handles addresses inside a function body (not at the start)
        // We check >= f.address to be safe in case the index is inconsistent
        self.functions
            .iter()
            .find(|f| f.size > 0 && address >= f.address && address < f.address + f.size)
    }

    /// Find function at exact address only (no range check) - O(1) lookup
    #[inline]
    pub fn function_at_exact(&self, address: u64) -> Option<&FunctionInfo> {
        self.function_addr_index
            .get(&address)
            .and_then(|&idx| self.functions.get(idx))
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "{} {} binary\n\
             Entry: 0x{:x}\n\
             Image Base: 0x{:x}\n\
             .NET: {}{}\n\
             Sections: {}\n\
             Functions: {}",
            if self.is_64bit { "64-bit" } else { "32-bit" },
            self.format,
            self.entry_point,
            self.image_base,
            if self.is_dotnet { "yes" } else { "no" },
            self.dotnet_runtime_version
                .as_ref()
                .map(|v| format!(" (runtime {v})"))
                .unwrap_or_default(),
            self.sections.len(),
            self.functions.len()
        )
    }

    /// Convert a virtual address to file offset using binary search for O(log N) lookup
    pub fn va_to_file_offset(&self, va: u64) -> Option<usize> {
        // Binary search to find the section containing this VA
        let idx = self.sections.binary_search_by(|section| {
            let section_size = if section.virtual_size > 0 {
                section.virtual_size
            } else {
                section.file_size
            };

            if va < section.virtual_address {
                std::cmp::Ordering::Greater
            } else if va >= section.virtual_address + section_size {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });

        if let Ok(idx) = idx {
            let section = &self.sections[idx];
            let offset_in_section = va - section.virtual_address;
            return Some(section.file_offset as usize + offset_in_section as usize);
        }
        None
    }

    /// Create a memory-mapped representation of the binary for the decompiler.
    /// This places each section at its virtual address offset (relative to image_base).
    /// The returned Vec starts at image_base, so loadFill(VA) can use offset = VA - image_base.
    pub fn get_memory_mapped_data(&self) -> Vec<u8> {
        // Find the maximum virtual address extent to determine buffer size
        let max_va_end = self
            .sections
            .iter()
            .map(|s| {
                let size = if s.virtual_size > 0 {
                    s.virtual_size
                } else {
                    s.file_size
                };
                s.virtual_address + size
            })
            .max()
            .unwrap_or(self.image_base);

        // Calculate required buffer size (max_va relative to image_base)
        let buffer_size = if max_va_end > self.image_base {
            (max_va_end - self.image_base) as usize
        } else {
            0
        };

        // Create zeroed buffer
        let mut mapped = vec![0u8; buffer_size];

        // IMPORTANT: Copy PE/ELF headers at offset 0
        // The headers are NOT in a section but are needed for format detection.
        // For PE, the first section typically starts at 0x1000 (after headers).
        // We copy the raw file data from 0 up to the first section's file offset.
        let first_section_offset = self
            .sections
            .iter()
            .filter(|s| s.file_offset > 0)
            .map(|s| s.file_offset as usize)
            .min()
            .unwrap_or(0x1000.min(self.data.len()));

        // Copy headers to offset 0 in memory-mapped buffer
        let header_copy_size = first_section_offset.min(self.data.len()).min(mapped.len());
        if header_copy_size > 0 {
            mapped[..header_copy_size].copy_from_slice(&self.data[..header_copy_size]);
        }

        // Map each section into the buffer at its RVA offset
        for section in &self.sections {
            let rva = section.virtual_address.saturating_sub(self.image_base);
            let file_start = section.file_offset as usize;
            let file_end = (section.file_offset + section.file_size) as usize;

            if file_end <= self.data.len() {
                let section_data = &self.data[file_start..file_end];
                let dest_start = rva as usize;
                let dest_end = dest_start + section_data.len();

                if dest_end <= mapped.len() {
                    mapped[dest_start..dest_end].copy_from_slice(section_data);
                }
            }
        }

        mapped
    }

    /// Discover internal functions by scanning executable code for CALL instructions
    /// This finds functions that are called but not exported/imported
    ///
    /// TODO: DisasmEngine moved to fission-pcode, need to refactor dependencies
    pub fn discover_internal_functions(&mut self) {
        // Temporarily disabled - DisasmEngine is in fission-pcode crate
        // Need to decide architecture: should loader depend on pcode? Or vice versa?
        return;

        /* Original implementation:
        use std::collections::HashSet;

        // Create disassembler for this binary's architecture
        let engine = match DisasmEngine::new(self.is_64bit) {
            Ok(e) => e,
            Err(_) => return,
        };

        // Pre-compute executable section ranges for fast range checking
        // This avoids O(N) iteration over sections for each discovered target
        // Note: For typical binaries with <10 executable sections, linear search is efficient.
        let executable_ranges: Vec<(u64, u64)> = self
            .sections
            .iter()
            .filter(|s| s.is_executable)
            .map(|s| (s.virtual_address, s.virtual_address + s.virtual_size))
            .collect();

        // Helper closure to check if target is in executable range
        // Uses linear search (efficient for small number of sections)
        let is_in_executable_range = |target: u64| -> bool {
            executable_ranges
                .iter()
                .any(|&(start, end)| target >= start && target < end)
        };

        // Estimate capacity based on typical function density (~1 function per 100 bytes of code)
        let total_code_size: u64 = executable_ranges.iter().map(|(s, e)| e - s).sum();
        let estimated_functions = (total_code_size / 100) as usize;
        let mut discovered: HashSet<u64> = HashSet::with_capacity(estimated_functions.max(64));

        // Scan all executable sections
        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            // Get section bytes
            let start = section.file_offset as usize;
            let size = section.file_size as usize;
            if start + size > self.data.len() {
                continue;
            }
            let bytes = &self.data[start..start + size];

            // Discover call targets in this section
            let targets = engine.discover_call_targets(bytes, section.virtual_address);

            for target in targets {
                // Use O(1) HashMap lookup instead of HashSet contains for existing functions
                // (function_addr_index is already maintained by the LoadedBinary)
                if self.function_addr_index.contains_key(&target) {
                    continue;
                }

                // Only add if not already discovered and within executable range
                if !discovered.contains(&target) && is_in_executable_range(target) {
                    discovered.insert(target);
                }
            }
        }

        // Pre-allocate space for new functions
        self.functions.reserve(discovered.len());

        // Add discovered functions
        for addr in discovered {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        // Sort functions by address
        self.functions.sort_by_key(|f| f.address);
        self.functions_sorted = true;

        // Rebuild function indices after adding new functions
        self.rebuild_function_indices();
        */
    }

    /// Rebuild function lookup indices after modifying the functions vector
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();

        for (idx, func) in self.functions.iter().enumerate() {
            self.function_addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                self.function_name_index.insert(func.name.clone(), idx);
            }
        }
    }

    // ========================================================================
    // Binary Patching
    // ========================================================================

    /// Patch bytes at a file offset
    /// Returns the original bytes that were replaced
    ///
    /// Uses Copy-on-Write semantics: if this is the only reference to the data,
    /// the patch is applied in-place. Otherwise, the data is cloned first.
    pub fn patch_bytes(&mut self, offset: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + new_bytes.len();

        if end > self.data.len() {
            return None;
        }

        // Save original bytes (before COW clone)
        let original = self.data[offset..end].to_vec();

        // Apply patch using Copy-on-Write
        // Arc::make_mut clones only if there are other references
        let data = Arc::make_mut(&mut self.data);
        data[offset..end].copy_from_slice(new_bytes);

        Some(original)
    }

    /// Patch bytes at a virtual address
    /// Converts VA to file offset and applies the patch
    pub fn patch_bytes_va(&mut self, va: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = self.va_to_file_offset(va)?;
        self.patch_bytes(offset as u64, new_bytes)
    }

    /// Get bytes at a file offset (for displaying original)
    pub fn get_bytes_at_offset(&self, offset: u64, size: usize) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + size;

        if end > self.data.len() {
            return None;
        }

        Some(self.data[offset..end].to_vec())
    }

    /// Save the (potentially patched) binary to a file
    pub fn save_as<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        std::fs::write(path, self.data.as_ref())?;
        Ok(())
    }

    // TODO: QuickPatch integration disabled - circular dependency with fission-analysis
    // Apply a quick patch at a file offset
    // pub fn apply_quick_patch(&mut self, offset: u64, patch_type: QuickPatch) -> Option<Vec<u8>>
    //
    // Apply a quick patch at a virtual address
    // pub fn apply_quick_patch_va(&mut self, va: u64, patch_type: QuickPatch) -> Option<Vec<u8>>
}

// ============================================================================
// Shared Utility Functions
// ============================================================================

/// Extract a null-terminated string from a byte slice starting at the given index.
///
/// This function finds the null terminator and returns the string up to that point.
/// Invalid UTF-8 sequences are replaced with the Unicode replacement character.
///
/// # Arguments
/// * `data` - The byte slice to extract from
/// * `start` - The starting index within the slice
///
/// # Returns
/// The extracted string, or an empty string if start is out of bounds.
///
/// # Example
/// ```ignore
/// let data = b"hello\0world";
/// assert_eq!(extract_cstring(data, 0), "hello");
/// assert_eq!(extract_cstring(data, 6), "world");
/// ```
pub fn extract_cstring(data: &[u8], start: usize) -> String {
    if start >= data.len() {
        return String::new();
    }
    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|pos| start + pos)
        .unwrap_or(data.len());
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

/// Extract a null-terminated string from a fixed-size byte array.
///
/// This is useful for parsing fixed-size name fields in binary formats
/// (e.g., PE section names which are 8 bytes, Mach-O segment names which are 16 bytes).
///
/// # Arguments
/// * `bytes` - The byte slice (typically a fixed-size field)
///
/// # Returns
/// The extracted string up to the first null byte or the end of the slice.
///
/// # Example
/// ```ignore
/// let name = b".text\0\0\0";
/// assert_eq!(extract_fixed_string(name), ".text");
/// ```
pub fn extract_fixed_string(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).into_owned()
}

#[cfg(test)]
mod string_utils_tests {
    use super::*;

    #[test]
    fn test_extract_cstring_basic() {
        let data = b"hello\0world";
        assert_eq!(extract_cstring(data, 0), "hello");
        assert_eq!(extract_cstring(data, 6), "world");
    }

    #[test]
    fn test_extract_cstring_no_null() {
        let data = b"hello";
        assert_eq!(extract_cstring(data, 0), "hello");
    }

    #[test]
    fn test_extract_cstring_empty() {
        let data = b"\0hello";
        assert_eq!(extract_cstring(data, 0), "");
    }

    #[test]
    fn test_extract_cstring_out_of_bounds() {
        let data = b"hello";
        assert_eq!(extract_cstring(data, 100), "");
    }

    #[test]
    fn test_extract_fixed_string_basic() {
        let data = b".text\0\0\0";
        assert_eq!(extract_fixed_string(data), ".text");
    }

    #[test]
    fn test_extract_fixed_string_full() {
        let data = b"fullname";
        assert_eq!(extract_fixed_string(data), "fullname");
    }

    #[test]
    fn test_extract_fixed_string_empty() {
        let data = b"\0\0\0\0";
        assert_eq!(extract_fixed_string(data), "");
    }
}
