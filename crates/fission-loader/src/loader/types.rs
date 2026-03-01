use crate::prelude::*;
// use bytecheck::CheckBytes; removed as it was causing a warning
use fission_disasm::DisasmEngine;
use rkyv::{Archive, Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// ============================================================================
// rkyv Wrappers for Arc<T> types (COW optimization)
// ============================================================================

/// Unified buffer that can be either on the heap or memory-mapped from a file.
///
/// This allows Fission to handle multi-gigabyte binaries without loading
/// them entirely into RAM, while still supporting in-memory buffers
/// (e.g., from snapshots or unpacking).
#[derive(Debug)]
pub enum DataBuffer {
    Heap(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl DataBuffer {
    /// Get the content as a byte slice
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Heap(v) => v.as_slice(),
            Self::Mapped(m) => m,
        }
    }

    /// Convert to a mutable Vec<u8> (triggers copy if mapped)
    pub fn to_mut_vec(&mut self) -> &mut Vec<u8> {
        if let Self::Mapped(_) = self {
            let vec = self.as_slice().to_vec();
            *self = Self::Heap(vec);
        }
        match self {
            Self::Heap(v) => v,
            _ => unreachable!(),
        }
    }
}

impl Clone for DataBuffer {
    fn clone(&self) -> Self {
        match self {
            Self::Heap(v) => Self::Heap(v.clone()),
            Self::Mapped(m) => Self::Heap(m.to_vec()),
        }
    }
}

impl rkyv::Archive for DataBuffer {
    type Archived = ();
    type Resolver = ();
    #[inline]
    unsafe fn resolve(&self, _pos: usize, _resolver: Self::Resolver, _out: *mut Self::Archived) {}
}

impl<S: rkyv::ser::Serializer + ?Sized> rkyv::Serialize<S> for DataBuffer {
    #[inline]
    fn serialize(&self, _serializer: &mut S) -> std::result::Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: rkyv::Fallible + ?Sized> rkyv::Deserialize<DataBuffer, D> for () {
    #[inline]
    fn deserialize(&self, _deserializer: &mut D) -> std::result::Result<DataBuffer, D::Error> {
        unreachable!("DataBuffer should be deserialized via ArcDataWrapper")
    }
}

/// Custom rkyv wrapper for `Arc<DataBuffer>` that serializes as `Vec<u8>`.
pub struct ArcDataWrapper;

impl rkyv::with::ArchiveWith<Arc<DataBuffer>> for ArcDataWrapper {
    type Archived = rkyv::vec::ArchivedVec<u8>;
    type Resolver = rkyv::vec::VecResolver;

    #[inline]
    unsafe fn resolve_with(
        field: &Arc<DataBuffer>,
        pos: usize,
        resolver: Self::Resolver,
        out: *mut Self::Archived,
    ) {
        // SAFETY: The caller guarantees that out points to valid memory
        unsafe {
            let out_vec = &mut *out;
            rkyv::vec::ArchivedVec::resolve_from_slice(field.as_slice(), pos, resolver, out_vec);
        }
    }
}

impl<S: rkyv::ser::Serializer + rkyv::ser::ScratchSpace + ?Sized>
    rkyv::with::SerializeWith<Arc<DataBuffer>, S> for ArcDataWrapper
{
    fn serialize_with(
        field: &Arc<DataBuffer>,
        serializer: &mut S,
    ) -> std::result::Result<Self::Resolver, S::Error> {
        rkyv::vec::ArchivedVec::serialize_from_slice(field.as_slice(), serializer)
    }
}

impl<D: rkyv::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::vec::ArchivedVec<u8>, Arc<DataBuffer>, D> for ArcDataWrapper
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<u8>,
        _deserializer: &mut D,
    ) -> std::result::Result<Arc<DataBuffer>, D::Error> {
        let vec: Vec<u8> = field.as_slice().to_vec();
        Ok(Arc::new(DataBuffer::Heap(vec)))
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

/// Information about an inferred field in a type
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct InferredFieldInfo {
    /// Field name
    pub name: String,
    /// Field type (may be mangled or simplified)
    pub type_name: String,
    /// Offset from struct base
    pub offset: u32,
    /// Size in bytes (0 if unknown)
    pub size: u32,
}

/// Information about an inferred type (class/struct) from metadata
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct InferredTypeInfo {
    /// Type name (demangled if possible)
    pub name: String,
    /// Mangled name (for lookup)
    pub mangled_name: String,
    /// Kind of type (class, struct, enum)
    pub kind: String,
    /// Fields in this type
    pub fields: Vec<InferredFieldInfo>,
    /// Total size of type (0 if unknown)
    pub size: u32,
    /// Associated metadata address (if any)
    pub metadata_address: u64,
}

// ============================================================================
// DWARF Debug Information Types
// ============================================================================

/// Location of a variable extracted from DWARF DW_AT_location
#[derive(Debug, Clone)]
pub enum DwarfLocation {
    /// Stack offset relative to frame base (DW_OP_fbreg)
    StackOffset(i64),
    /// CPU register (DW_OP_reg*)
    Register(String),
    /// Complex or unparsed location expression
    Unknown,
}

/// Parameter information extracted from DWARF DW_TAG_formal_parameter
#[derive(Debug, Clone)]
pub struct DwarfParamInfo {
    /// Parameter name from DW_AT_name
    pub name: String,
    /// Type name resolved from DW_AT_type
    pub type_name: String,
    /// Parameter location (register or stack)
    pub location: DwarfLocation,
}

/// Local variable information from DWARF DW_TAG_variable
#[derive(Debug, Clone)]
pub struct DwarfLocalVar {
    /// Variable name from DW_AT_name
    pub name: String,
    /// Type name resolved from DW_AT_type
    pub type_name: String,
    /// Variable location
    pub location: DwarfLocation,
}

/// Function information extracted from DWARF DW_TAG_subprogram
#[derive(Debug, Clone)]
pub struct DwarfFunctionInfo {
    /// Function address (DW_AT_low_pc)
    pub address: u64,
    /// Function name (DW_AT_name or DW_AT_linkage_name)
    pub name: String,
    /// Return type resolved from DW_AT_type
    pub return_type: Option<String>,
    /// Parameters in declaration order
    pub params: Vec<DwarfParamInfo>,
    /// Local variables
    pub local_vars: Vec<DwarfLocalVar>,
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
    /// Raw bytes of the file (COW enabled ArcDataBuffer)
    #[with(ArcDataWrapper)]
    pub data: Arc<DataBuffer>,
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
    /// Inferred types from metadata analysis (Swift, Go, etc.)
    pub inferred_types: Vec<InferredTypeInfo>,
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
    /// DWARF debug information for functions (params, locals, return types).
    /// Keyed by function address for O(1) lookup during post-processing.
    /// Not serialized — rebuilt on each load from debug sections.
    pub dwarf_functions: std::collections::HashMap<u64, DwarfFunctionInfo>,
}

impl LoadedBinary {
    /// Create a new LoadedBinary from inner data
    pub fn from_inner(inner: LoadedBinaryInner) -> Self {
        Self {
            inner: Arc::new(inner),
            dwarf_functions: std::collections::HashMap::new(),
        }
    }

    /// Get immutable reference to inner data
    #[inline]
    pub fn inner(&self) -> &LoadedBinaryInner {
        &self.inner
    }

    /// Get Ghidra-compatible compiler ID based on detections
    pub fn get_ghidra_compiler_id(&self) -> Option<String> {
        let detection = crate::detector::detect(self);
        let is_pe = self.format.to_ascii_uppercase().starts_with("PE");
        detection
            .compiler()
            .map(|d| match d.name.to_lowercase().as_str() {
                "microsoft visual c++" | "msvc" => "windows".to_string(),
                "gcc" | "mingw" => {
                    if is_pe {
                        "windows".to_string()
                    } else {
                        "gcc".to_string()
                    }
                }
                "clang" => "clang".to_string(),
                _ => "default".to_string(),
            })
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
    data: DataBuffer,
    arch_spec: String,
    entry_point: u64,
    image_base: u64,
    functions: Vec<FunctionInfo>,
    sections: Vec<SectionInfo>,
    is_64bit: bool,
    format: String,
    iat_symbols: std::collections::HashMap<u64, String>,
    global_symbols: std::collections::HashMap<u64, String>,
}

impl LoadedBinaryBuilder {
    pub fn new(path: String, data: DataBuffer) -> Self {
        let hash = blake3::hash(data.as_slice()).to_hex().to_string();
        Self {
            path,
            hash,
            data,
            arch_spec: "x86:LE:64:default".to_string(), // Default
            entry_point: 0,
            image_base: 0,
            functions: Vec::new(),
            sections: Vec::new(),
            is_64bit: false,
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

        // Build indices and demangle names
        let mut function_addr_index = std::collections::HashMap::new();
        let mut function_name_index = std::collections::HashMap::new();
        for (idx, func) in functions.iter_mut().enumerate() {
            if !func.name.is_empty() {
                // Apply demangling
                let demangled = crate::loader::demangle::demangle(&func.name);
                if demangled != func.name {
                    func.name = demangled;
                }
                function_name_index.insert(func.name.clone(), idx);
            }
            function_addr_index.insert(func.address, idx);
        }

        // Demangle IAT symbols
        let mut iat_symbols = std::collections::HashMap::new();
        for (addr, name) in self.iat_symbols {
            let demangled = crate::loader::demangle::demangle(&name);
            iat_symbols.insert(addr, demangled);
        }

        // Demangle Global symbols
        let mut global_symbols = std::collections::HashMap::new();
        for (addr, name) in self.global_symbols {
            let demangled = crate::loader::demangle::demangle(&name);
            global_symbols.insert(addr, demangled);
        }

        let inner = LoadedBinaryInner {
            path: self.path,
            hash: self.hash,
            data: Arc::new(self.data),
            arch_spec: self.arch_spec,
            entry_point: self.entry_point,
            image_base: self.image_base,
            functions,
            sections: self.sections,
            is_64bit: self.is_64bit,
            format: self.format,
            iat_symbols,
            global_symbols,
            function_addr_index,
            function_name_index,
            functions_sorted: true,
            inferred_types: Vec::new(),
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
        self.view_bytes(address, size).map(|s| s.to_vec())
    }

    /// Get a slice of bytes at a given address (zero-copy)
    pub fn view_bytes(&self, address: u64, size: usize) -> Option<&[u8]> {
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
            let file_offset = section.file_offset as usize + offset_in_section as usize;

            if file_offset + size <= self.data.as_slice().len() {
                return Some(&self.data.as_slice()[file_offset..file_offset + size]);
            }
        }
        None
    }

    /// Read a pointer at the given address
    pub fn read_ptr(&self, address: u64) -> Result<u64> {
        let size = if self.is_64bit { 8 } else { 4 };
        let bytes = self.get_bytes(address, size).ok_or_else(|| {
            FissionError::loader(format!("Could not read pointer at 0x{:x}", address))
        })?;

        let ptr = if self.is_64bit {
            u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]))
        } else {
            u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4])) as u64
        };

        Ok(ptr)
    }

    /// Get executable sections only
    pub fn executable_sections(&self) -> Vec<&SectionInfo> {
        self.sections.iter().filter(|s| s.is_executable).collect()
    }

    /// Iterate over imported functions.
    pub fn imports(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter().filter(|f| f.is_import)
    }

    /// Iterate over exported functions.
    pub fn exports(&self) -> impl Iterator<Item = &FunctionInfo> {
        self.functions.iter().filter(|f| f.is_export)
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
             Sections: {}\n\
             Functions: {}",
            if self.is_64bit { "64-bit" } else { "32-bit" },
            self.format,
            self.entry_point,
            self.image_base,
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
            .map(|s| s.virtual_address + s.virtual_size)
            .max()
            .unwrap_or(0);

        let mut mapped = vec![0u8; (max_va_end - self.image_base) as usize];
        let binary_data = self.inner().data.as_slice();

        for section in &self.sections {
            if section.file_size == 0 || section.file_offset as usize >= binary_data.len() {
                continue;
            }

            let start = section.file_offset as usize;
            let end = std::cmp::min(start + section.file_size as usize, binary_data.len());
            let size = end - start;

            let dest_start = (section.virtual_address - self.image_base) as usize;
            if dest_start + size <= mapped.len() {
                mapped[dest_start..dest_start + size].copy_from_slice(&binary_data[start..end]);
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
            if start + size > self.data.as_slice().len() {
                continue;
            }
            let bytes = &self.data.as_slice()[start..start + size];

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

    /// Discover functions by scanning for common prologue patterns and CALL targets
    ///
    /// This is useful when the control flow is obfuscated (e.g., indirect calls)
    /// and standard call-graph usage fails to find all functions.
    pub fn discover_functions_by_prologue(&mut self) -> usize {
        let mut count = 0;
        let mut candidates = std::collections::HashSet::new();

        // Define common prologues
        let patterns: &[&[u8]] = if self.is_64bit {
            &[
                &[0x55, 0x48, 0x89, 0xe5],       // push rbp; mov rbp, rsp
                &[0x48, 0x83, 0xec],             // sub rsp, X
                &[0x48, 0x81, 0xec],             // sub rsp, X (32-bit imm)
                &[0x55, 0x48, 0x8d, 0x2c, 0x24], // push rbp; lea rbp, [rsp] (Win/Ghidra)
                &[0x40, 0x55, 0x48, 0x8b, 0xec], // push rbp; mov rbp, rsp (Win/REX)
                &[0x48, 0x8b, 0xc4],             // mov rax, rsp (Win/Ghidra)
            ]
        } else {
            &[
                &[0x55, 0x89, 0xe5], // push ebp; mov ebp, esp (GCC)
                &[0x55, 0x8b, 0xec], // push ebp; mov ebp, esp (MSVC)
                &[0x83, 0xec],       // sub esp, X (minimal)
                &[0x81, 0xec],       // sub esp, X (32-bit imm)
            ]
        };

        // Pre-calculate executable ranges to validate call targets
        let mut exec_ranges = Vec::new();
        for section in &self.sections {
            if section.is_executable {
                exec_ranges.push((
                    section.virtual_address,
                    section.virtual_address + section.virtual_size,
                ));
            }
        }

        // Scan executable sections
        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            let start = section.file_offset as usize;
            let end = (section.file_offset + section.file_size) as usize;
            if end > self.data.as_slice().len() {
                continue;
            }

            // Limit search to a reasonable size to prevent excessive memory usage or hangs
            let search_limit = (512 * 1024) // 512KB limit
                .min(self.data.as_slice().len() - start);
            let data = &self.data.as_slice()[start..start + search_limit];
            let va_start = section.virtual_address;

            for i in 0..data.len() {
                // 1. Prologue Matching
                if i + 4 <= data.len() {
                    let window = &data[i..];
                    for pat in patterns {
                        if window.starts_with(pat) {
                            let potential_addr = va_start + i as u64;
                            if !self.function_addr_index.contains_key(&potential_addr) {
                                candidates.insert(potential_addr);
                            }
                            break;
                        }
                    }
                }

                // 2. CALL Target Discovery (0xE8 rel32)
                // This finds functions even if they have obfuscated prologues
                if i + 5 <= data.len() && data[i] == 0xE8 {
                    // Read relative offset (i32)
                    let rel_bytes = [data[i + 1], data[i + 2], data[i + 3], data[i + 4]];
                    let rel = i32::from_le_bytes(rel_bytes);

                    let call_insn_addr = va_start + i as u64;
                    // Target = NextIP + Rel = (Addr + 5) + Rel
                    let target_addr = (call_insn_addr.wrapping_add(5)).wrapping_add(rel as u64);

                    // Validate target is within executable memory
                    let is_valid = exec_ranges
                        .iter()
                        .any(|(s, e)| target_addr >= *s && target_addr < *e);

                    if is_valid {
                        // Keep within 32/64 bit limits
                        let addr_masked = if self.is_64bit {
                            target_addr
                        } else {
                            target_addr & 0xFFFFFFFF
                        };

                        if !self.function_addr_index.contains_key(&addr_masked) {
                            candidates.insert(addr_masked);
                        }
                    }
                }
            }
        }

        // Register valid candidates
        for addr in candidates {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}_scanned", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
            count += 1;
        }

        if count > 0 {
            self.functions.sort_by_key(|f| f.address);
            self.functions_sorted = true;
            self.rebuild_function_indices();
        }

        count
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

        if end > self.data.as_slice().len() {
            return None;
        }

        // Save original bytes
        let original = self.data.as_slice()[offset..end].to_vec();

        // Apply patch - ensure we have a mutable Heap buffer (COW)
        let data_mut = Arc::make_mut(&mut self.data);
        let vec = data_mut.to_mut_vec();
        vec[offset..end].copy_from_slice(new_bytes);

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

        if end > self.data.as_slice().len() {
            return None;
        }

        Some(self.data.as_slice()[offset..end].to_vec())
    }

    /// Save the (potentially patched) binary to a file
    pub fn save_as<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        std::fs::write(path, self.data.as_slice())?;
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
