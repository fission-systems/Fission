use crate::prelude::*;
// use bytecheck::CheckBytes; removed as it was causing a warning
use fission_disasm::DisasmEngine;
use rkyv::{Archive, Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// ============================================================================
// rkyv Wrappers for Arc<T> types (COW optimization)
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
        unsafe {
            rkyv::vec::ArchivedVec::resolve_from_slice(field.as_slice(), pos, resolver, out);
        }
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
        _deserializer: &mut D,
    ) -> std::result::Result<Arc<Vec<u8>>, D::Error> {
        let vec: Vec<u8> = field.as_slice().to_vec();
        Ok(Arc::new(vec))
    }
}

/// rkyv wrapper for `Arc<Vec<FunctionInfo>>` - functions list
pub struct ArcFunctionsWrapper;

/// rkyv wrapper for `Arc<Vec<SectionInfo>>` - sections list  
pub struct ArcSectionsWrapper;

/// rkyv wrapper for `Arc<HashMap<u64, String>>` - symbol maps
pub struct ArcSymbolMapWrapper;

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

/// Inner data structure containing all binary information.
/// This is wrapped in Arc for O(1) cloning with COW semantics.
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct LoadedBinaryInner {
    /// Original file path
    pub path: String,
    /// Binary data hash (Blake3) for caching and identification
    pub hash: String,
    /// Raw bytes of the file
    pub data: Vec<u8>,
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
    pub functions_sorted: bool,
}

/// Parsed binary information with O(1) clone via Arc.
///
/// This wrapper provides Copy-on-Write semantics:
/// - Clone is O(1) - only increments Arc reference count
/// - Modifications use `Arc::make_mut` to clone only when needed
/// - All fields are accessed through the inner Arc
#[derive(Debug, Clone)]
pub struct LoadedBinary {
    inner: Arc<LoadedBinaryInner>,
}

impl LoadedBinary {
    /// Create a new LoadedBinary from inner data
    pub fn from_inner(inner: LoadedBinaryInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Get immutable reference to inner data
    #[inline]
    pub fn inner(&self) -> &LoadedBinaryInner {
        &self.inner
    }

    /// Get mutable reference with COW semantics
    /// Clones the inner data only if there are other references
    #[inline]
    pub fn inner_mut(&mut self) -> &mut LoadedBinaryInner {
        Arc::make_mut(&mut self.inner)
    }

    /// Check if this is the only reference (for debugging)
    #[inline]
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
}

// Deref allows direct field access: binary.path instead of binary.inner().path
impl std::ops::Deref for LoadedBinary {
    type Target = LoadedBinaryInner;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// DerefMut provides COW semantics: modifying binary.path clones if needed
impl std::ops::DerefMut for LoadedBinary {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.inner)
    }
}

/// Builder for LoadedBinary
pub struct LoadedBinaryBuilder {
    path: String,
    hash: String,
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
        let hash = blake3::hash(&data).to_hex().to_string();
        Self {
            path,
            hash,
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

        // Build indices
        let mut function_addr_index = std::collections::HashMap::new();
        let mut function_name_index = std::collections::HashMap::new();
        for (idx, func) in functions.iter().enumerate() {
            function_addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                function_name_index.insert(func.name.clone(), idx);
            }
        }

        let inner = LoadedBinaryInner {
            path: self.path,
            hash: self.hash,
            data: self.data,
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
            function_addr_index,
            function_name_index,
            functions_sorted: true,
        };

        Ok(LoadedBinary::from_inner(inner))
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
    pub fn discover_internal_functions(&mut self) {
        use std::collections::HashSet;

        // Create disassembler for this binary's architecture
        let engine = match DisasmEngine::new(self.is_64bit) {
            Ok(e) => e,
            Err(_) => return,
        };

        // Pre-compute executable section ranges for fast range checking
        let executable_ranges: Vec<(u64, u64)> = self
            .sections
            .iter()
            .filter(|s| s.is_executable)
            .map(|s| (s.virtual_address, s.virtual_address + s.virtual_size))
            .collect();

        // Helper closure to check if target is in executable range
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
                if self.function_addr_index.contains_key(&target) {
                    continue;
                }

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
    }

    /// Rebuild function lookup indices after modifying the functions vector
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();

        // Collect data first to avoid borrow issues
        let entries: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .map(|(idx, func)| (idx, func.address, func.name.clone()))
            .collect();

        for (idx, addr, name) in entries {
            self.function_addr_index.insert(addr, idx);
            if !name.is_empty() {
                self.function_name_index.insert(name, idx);
            }
        }
    }

    // ========================================================================
    // Binary Patching
    // ========================================================================

    /// Patch bytes at a file offset
    /// Returns the original bytes that were replaced
    ///
    /// Uses Copy-on-Write semantics at the LoadedBinary level:
    /// If the LoadedBinary is cloned (via Arc), this modification
    /// will trigger a clone of the entire inner structure.
    pub fn patch_bytes(&mut self, offset: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + new_bytes.len();

        if end > self.data.len() {
            return None;
        }

        // Save original bytes
        let original = self.data[offset..end].to_vec();

        // Apply patch - DerefMut triggers COW at LoadedBinary level
        self.data[offset..end].copy_from_slice(new_bytes);

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
        std::fs::write(path, &self.data)?;
        Ok(())
    }

    /// Apply a quick patch at a file offset
    pub fn apply_quick_patch(&mut self, offset: u64, patch_type: QuickPatch) -> Option<Vec<u8>> {
        let bytes = patch_type.bytes();
        self.patch_bytes(offset, &bytes)
    }

    /// Apply a quick patch at a virtual address
    pub fn apply_quick_patch_va(&mut self, va: u64, patch_type: QuickPatch) -> Option<Vec<u8>> {
        let bytes = patch_type.bytes();
        self.patch_bytes_va(va, &bytes)
    }
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
