//! Binary format signatures and magic numbers
//!
//! Well-known constants for identifying and parsing executable file formats.
//! References:
//! - PE: Microsoft PE/COFF Specification
//! - ELF: System V ABI specification
//! - Mach-O: Apple Mach-O file format reference

// =============================================================================
// PE (Portable Executable) Format - Windows
// =============================================================================

/// DOS header signature "MZ" (0x5A4D)
pub const PE_DOS_SIGNATURE: u16 = 0x5A4D;

/// DOS header signature (alternative: "ZM")
pub const PE_DOS_SIGNATURE_ZM: u16 = 0x4D5A;

/// PE signature "PE\0\0" (0x00004550)
pub const PE_SIGNATURE: u32 = 0x00004550;

/// PE optional header magic for 32-bit executable
pub const PE_OPTIONAL_HEADER_MAGIC_PE32: u16 = 0x010B;

/// PE optional header magic for 64-bit executable
pub const PE_OPTIONAL_HEADER_MAGIC_PE32_PLUS: u16 = 0x020B;

/// PE optional header magic for ROM image
pub const PE_OPTIONAL_HEADER_MAGIC_ROM: u16 = 0x0107;

// =============================================================================
// ELF (Executable and Linkable Format) - Unix/Linux
// =============================================================================

/// ELF magic number: 0x7F 'E' 'L' 'F'
pub const ELF_MAGIC: u32 = 0x7F454C46;

/// ELF magic number (alternative byte order)
pub const ELF_MAGIC_BE: u32 = 0x464c457f;

/// ELF magic bytes as array
pub const ELF_MAGIC_BYTES: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// ELF class: 32-bit objects
pub const ELF_CLASS_32: u8 = 1;

/// ELF class: 64-bit objects
pub const ELF_CLASS_64: u8 = 2;

/// ELF data encoding: little endian
pub const ELF_DATA_LITTLE_ENDIAN: u8 = 1;

/// ELF data encoding: big endian
pub const ELF_DATA_BIG_ENDIAN: u8 = 2;

/// ELF version: current version
pub const ELF_VERSION_CURRENT: u8 = 1;

// ELF file types
/// ELF type: relocatable file
pub const ELF_TYPE_REL: u16 = 1;

/// ELF type: executable file
pub const ELF_TYPE_EXEC: u16 = 2;

/// ELF type: shared object file
pub const ELF_TYPE_DYN: u16 = 3;

/// ELF type: core file
pub const ELF_TYPE_CORE: u16 = 4;

// =============================================================================
// Mach-O Format - macOS/iOS
// =============================================================================

/// Mach-O magic for 32-bit big endian
pub const MACHO_MAGIC_32_BE: u32 = 0xfeedface;

/// Mach-O magic for 32-bit little endian
pub const MACHO_MAGIC_32_LE: u32 = 0xcefaedfe;

/// Mach-O magic for 64-bit big endian
pub const MACHO_MAGIC_64_BE: u32 = 0xfeedfacf;

/// Mach-O magic for 64-bit little endian
pub const MACHO_MAGIC_64_LE: u32 = 0xcffaedfe;

/// Mach-O fat binary magic (universal binary)
pub const MACHO_FAT_MAGIC: u32 = 0xcafebabe;

/// Mach-O fat binary magic (reverse byte order)
pub const MACHO_FAT_CIGAM: u32 = 0xbebafeca;

// CPU types
/// Mach-O CPU type: x86
pub const MACHO_CPU_TYPE_X86: i32 = 7;

/// Mach-O CPU type: x86_64
pub const MACHO_CPU_TYPE_X86_64: i32 = 0x01000007;

/// Mach-O CPU type: ARM
pub const MACHO_CPU_TYPE_ARM: i32 = 12;

/// Mach-O CPU type: ARM64
pub const MACHO_CPU_TYPE_ARM64: i32 = 0x0100000C;

// =============================================================================
// Archive Formats
// =============================================================================

/// AR archive signature "!<arch>\n"
pub const AR_MAGIC: &[u8; 8] = b"!<arch>\n";

/// COFF archive signature
pub const COFF_ARCHIVE_SIGNATURE: &[u8; 8] = b"!<arch>\n";

// =============================================================================
// Other Binary Formats
// =============================================================================

/// Java class file magic 0xCAFEBABE
pub const JAVA_CLASS_MAGIC: u32 = 0xCAFEBABE;

/// DEX (Dalvik Executable) magic
pub const DEX_MAGIC: [u8; 4] = [b'd', b'e', b'x', 0x0A];

/// DEX version magic (complete with version)
pub const DEX_MAGIC_WITH_VERSION: &[u8; 8] = b"dex\n039\0";

// =============================================================================
// Compression/Container Formats
// =============================================================================

/// ZIP archive signature (PK\x03\x04)
pub const ZIP_SIGNATURE: u32 = 0x04034b50;

/// GZIP magic number
pub const GZIP_MAGIC: u16 = 0x8B1F;

/// ZLIB magic
pub const ZLIB_MAGIC: u16 = 0x9C78;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pe_signatures() {
        assert_eq!(PE_DOS_SIGNATURE, 0x5A4D);
        assert_eq!(PE_SIGNATURE, 0x00004550);
    }

    #[test]
    fn test_elf_magic() {
        assert_eq!(ELF_MAGIC, 0x7F454C46);
        assert_eq!(ELF_MAGIC_BYTES, [0x7F, b'E', b'L', b'F']);
    }

    #[test]
    fn test_macho_magic() {
        // 64-bit little endian is most common on modern macOS
        assert_eq!(MACHO_MAGIC_64_LE, 0xcffaedfe);
    }
}
