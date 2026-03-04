// use bytecheck::CheckBytes; removed as it was causing a warning
use rkyv::{Archive, Deserialize, Serialize};
use std::sync::Arc;

// Re-export common types from fission-core to maintain API compatibility
pub use fission_core::common::types::{FunctionInfo, SectionInfo};

#[path = "types_builder.rs"]
mod builder_methods;
#[path = "types_discovery.rs"]
mod discovery;
#[path = "types_patching.rs"]
mod patching;
#[path = "types_query.rs"]
mod query;
#[path = "types_string_utils.rs"]
mod string_utils;
pub use string_utils::{extract_cstring, extract_fixed_string};

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
