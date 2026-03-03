//! Memory-related constants
//!
//! Common memory sizes, alignments, and offsets used throughout the codebase.

// =============================================================================
// Common Memory Sizes
// =============================================================================

/// 1 Kilobyte
pub const KB: usize = 1024;

/// 1 Megabyte
pub const MB: usize = 1024 * KB;

/// 1 Gigabyte  
pub const GB: usize = 1024 * MB;

/// Default page size (4KB) - common on x86/x64
pub const PAGE_SIZE_4K: usize = 4 * KB;

/// Large page size (2MB) - x86/x64 huge pages
pub const PAGE_SIZE_2M: usize = 2 * MB;

/// Default buffer size for reading binary data
pub const DEFAULT_BUFFER_SIZE: usize = 64 * KB;

/// Maximum reasonable string length for decompiled output
pub const MAX_DECOMPILED_STRING_LENGTH: usize = 10 * MB;

// =============================================================================
// Memory Alignment
// =============================================================================

/// Pointer alignment on 32-bit systems
pub const POINTER_ALIGN_32: usize = 4;

/// Pointer alignment on 64-bit systems
pub const POINTER_ALIGN_64: usize = 8;

/// Cache line size (common on modern x86/x64)
pub const CACHE_LINE_SIZE: usize = 64;

/// SIMD alignment (128-bit SSE)
pub const SIMD_ALIGN_128: usize = 16;

/// SIMD alignment (256-bit AVX)
pub const SIMD_ALIGN_256: usize = 32;

/// SIMD alignment (512-bit AVX-512)
pub const SIMD_ALIGN_512: usize = 64;

// =============================================================================
// Common Structure Offsets (Go Runtime)
// =============================================================================

/// Go runtime moduledata offset (varies by Go version)
pub const GO_MODULEDATA_OFFSET: u64 = 0x08;

/// Go runtime pclntab offset
pub const GO_PCLNTAB_OFFSET: u64 = 0x10;

/// Go runtime type information offset
pub const GO_TYPEINFO_OFFSET: u64 = 0x18;

// =============================================================================
// Address Ranges
// =============================================================================

/// Minimum user-mode address (typical on Windows x64)
pub const MIN_USER_ADDRESS_X64: u64 = 0x0000_0000_0001_0000;

/// Maximum user-mode address (typical on Windows x64)
pub const MAX_USER_ADDRESS_X64: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Kernel space start (typical on Linux x64)
pub const KERNEL_SPACE_START_X64: u64 = 0xFFFF_8000_0000_0000;

/// NULL pointer
pub const NULL_PTR: u64 = 0x0000_0000_0000_0000;

// =============================================================================
// Special Values
// =============================================================================

/// Sign extension mask for 32-bit signed values in 64-bit
pub const SIGN_EXTEND_32: u64 = 0xFFFF_FFFF_8000_0000;

/// Sign extension mask for 16-bit signed values in 64-bit
pub const SIGN_EXTEND_16: u64 = 0xFFFF_FFFF_FFFF_8000;

/// Sign extension mask for 8-bit signed values in 64-bit
pub const SIGN_EXTEND_8: u64 = 0xFFFF_FFFF_FFFF_FF80;

/// Maximum signed 32-bit integer
pub const MAX_I32: i64 = i32::MAX as i64;

/// Minimum signed 32-bit integer
pub const MIN_I32: i64 = i32::MIN as i64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sizes() {
        assert_eq!(KB, 1024);
        assert_eq!(MB, 1024 * 1024);
        assert_eq!(GB, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_page_sizes() {
        assert_eq!(PAGE_SIZE_4K, 4096);
        assert_eq!(PAGE_SIZE_2M, 2 * 1024 * 1024);
    }

    #[test]
    fn test_alignment() {
        assert_eq!(POINTER_ALIGN_64, 8);
        assert_eq!(CACHE_LINE_SIZE, 64);
    }
}
