//! Fission Constants
//!
//! Magic bytes, offsets, and other constants used throughout the codebase.

// ============================================================================
// PE (Portable Executable) Constants
// ============================================================================

/// DOS MZ magic bytes "MZ"
pub const DOS_MAGIC: u16 = 0x5A4D;

/// PE signature "PE\0\0"
pub const PE_SIGNATURE: u32 = 0x00004550;

/// PE32 optional header magic
pub const PE32_MAGIC: u16 = 0x010B;

/// PE32+ (64-bit) optional header magic
pub const PE64_MAGIC: u16 = 0x020B;

/// Offset to PE header pointer in DOS header
pub const PE_HEADER_OFFSET_LOCATION: usize = 0x3C;

// PE Section Characteristics
/// Section contains executable code
pub const IMAGE_SCN_CNT_CODE: u32 = 0x00000020;
/// Section can be executed as code
pub const IMAGE_SCN_MEM_EXECUTE: u32 = 0x20000000;
/// Section can be read
pub const IMAGE_SCN_MEM_READ: u32 = 0x40000000;
/// Section can be written to
pub const IMAGE_SCN_MEM_WRITE: u32 = 0x80000000;

// PE Machine Types
/// x86 (i386)
pub const IMAGE_FILE_MACHINE_I386: u16 = 0x014C;
/// x64 (AMD64)
pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
/// ARM
pub const IMAGE_FILE_MACHINE_ARM: u16 = 0x01C0;
/// ARM64
pub const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;

// ============================================================================
// ELF (Executable and Linkable Format) Constants
// ============================================================================

/// ELF magic bytes
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// ELF 32-bit class
pub const ELFCLASS32: u8 = 1;
/// ELF 64-bit class
pub const ELFCLASS64: u8 = 2;

/// ELF little-endian
pub const ELFDATA2LSB: u8 = 1;
/// ELF big-endian
pub const ELFDATA2MSB: u8 = 2;

// ELF Machine Types
/// x86
pub const EM_386: u16 = 3;
/// x86-64
pub const EM_X86_64: u16 = 62;
/// ARM
pub const EM_ARM: u16 = 40;
/// AArch64
pub const EM_AARCH64: u16 = 183;

// ============================================================================
// Mach-O Constants
// ============================================================================

/// Mach-O 32-bit magic
pub const MH_MAGIC: u32 = 0xFEEDFACE;
/// Mach-O 64-bit magic
pub const MH_MAGIC_64: u32 = 0xFEEDFACF;
/// Mach-O fat binary magic
pub const FAT_MAGIC: u32 = 0xCAFEBABE;
/// Mach-O fat binary magic (64-bit)
pub const FAT_MAGIC_64: u32 = 0xCAFEBABF;

// ============================================================================
// .NET / CLR Constants
// ============================================================================

/// .NET metadata signature "BSJB"
pub const DOTNET_METADATA_SIGNATURE: u32 = 0x424A5342;

/// CLI header size
pub const CLI_HEADER_SIZE: usize = 72;

// .NET metadata table IDs
pub const TABLE_MODULE: u8 = 0x00;
pub const TABLE_TYPEREF: u8 = 0x01;
pub const TABLE_TYPEDEF: u8 = 0x02;
pub const TABLE_FIELD: u8 = 0x04;
pub const TABLE_METHODDEF: u8 = 0x06;
pub const TABLE_PARAM: u8 = 0x08;
pub const TABLE_MEMBERREF: u8 = 0x0A;
pub const TABLE_ASSEMBLY: u8 = 0x20;
pub const TABLE_ASSEMBLYREF: u8 = 0x23;

/// Total number of possible .NET metadata tables
/// The .NET metadata spec defines 64 possible table types (0x00-0x3F)
pub const DOTNET_TABLE_COUNT: usize = 64;

// ============================================================================
// Archive/Packer Signatures
// ============================================================================

/// ZIP file magic
pub const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// RAR file magic
pub const RAR_MAGIC: [u8; 7] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];

/// UPX magic string
pub const UPX_MAGIC: &[u8] = b"UPX!";

/// PyInstaller archive magic
pub const PYINSTALLER_MAGIC: &[u8] = b"MEI\x0C\x0B\x0A\x09\x08";

// ============================================================================
// Version Information
// ============================================================================

/// Fission version
pub const FISSION_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Fission name
pub const FISSION_NAME: &str = "Fission";

/// Fission tagline
pub const FISSION_TAGLINE: &str = "Split the Binary, Fuse the Power.";

// ============================================================================
// Size Constants
// ============================================================================

/// 1 KB
pub const KB: usize = 1024;
/// 1 MB
pub const MB: usize = 1024 * 1024;
/// 1 GB
pub const GB: usize = 1024 * 1024 * 1024;

/// Page size (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Maximum function body size for analysis / disassembly (64 KB)
pub const MAX_FUNCTION_SIZE: usize = 64 * KB;

/// Maximum supported binary size (1 GB)
pub const MAX_BINARY_SIZE: usize = GB;

// ============================================================================
// Application / Config
// ============================================================================

/// Default configuration filename
pub const CONFIG_FILENAME: &str = "fission.toml";

/// Application data directory name
pub const APP_DIR_NAME: &str = "fission";

/// Decompiler disk-cache sub-directory
pub const DECOMP_CACHE_DIR_NAME: &str = "decomp";

/// Settings filename stored in the OS app-data directory (Tauri)
pub const SETTINGS_FILENAME: &str = "settings.json";

/// Default plugin search directory (relative)
pub const PLUGIN_DIR_NAME: &str = "plugins";

// ============================================================================
// Decompiler Defaults
// ============================================================================

/// Default decompilation timeout in milliseconds (30 s)
pub const DEFAULT_DECOMP_TIMEOUT_MS: u64 = 30_000;

/// Default in-memory (L1) LRU cache entry count for decompiler results
pub const DEFAULT_L1_CACHE_SIZE: usize = 100;

/// Default decompiler memory limit passed to native engine (10 MB)
pub const DEFAULT_DECOMP_MEMORY_LIMIT: usize = 10 * MB;

// ============================================================================
// Scan / Decode Limits
// ============================================================================

/// Maximum bytes read for hex-view / instruction decode per request
pub const MAX_HEX_READ: usize = 4_096;

/// Maximum bytes scanned per section (e.g., string search, 256 KB)
pub const MAX_SCAN_PER_SECTION: usize = 256 * KB;

/// Maximum function body bytes decoded for CFG / xref outgoing scan (64 KB)
pub const MAX_XREF_DECODE: usize = 65_536;

/// Maximum incoming cross-reference results returned per query
pub const MAX_XREF_INCOMING: usize = 2_000;

/// Maximum outgoing cross-reference results returned per query
pub const MAX_XREF_OUTGOING: usize = 4_000;

// ============================================================================
// Analysis Defaults
// ============================================================================

/// Disassembly byte read window (1 KB)
pub const DISASM_READ_WINDOW: usize = KB;

/// Minimum printable string length for string extraction
pub const MIN_STRING_LENGTH: usize = 4;

/// Maximum instructions per function before aborting decompilation / analysis
pub const MAX_INSTRUCTIONS_PER_FUNCTION: u32 = 100_000;

// ============================================================================
// Fallback Strings
// ============================================================================

/// Fallback library name when an import's owning DLL is unknown
pub const UNKNOWN_LIBRARY: &str = "unknown";

/// Fallback compiler/format identifier
pub const DEFAULT_COMPILER_ID: &str = "default";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dos_magic() {
        let bytes = [0x4D, 0x5A]; // "MZ"
        let magic = u16::from_le_bytes(bytes);
        assert_eq!(magic, DOS_MAGIC);
    }

    #[test]
    fn test_elf_magic() {
        assert_eq!(ELF_MAGIC[0], 0x7F);
        assert_eq!(ELF_MAGIC[1], b'E');
    }

    #[test]
    fn test_sizes() {
        assert_eq!(KB, 1024);
        assert_eq!(MB, 1024 * 1024);
    }
}
