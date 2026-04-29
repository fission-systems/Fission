use crate::loader::reader::ByteReader;
use crate::prelude::*;

// Initial Identification (first 16 bytes)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElfIdent {
    pub class: u8,  // 1=32, 2=64
    pub endian: u8, // 1=Little, 2=Big
    pub version: u8,
    pub os_abi: u8,
    pub abi_version: u8,
}

// --- 64-bit Structures ---

#[derive(Debug, Clone)]
pub struct Elf64Header {
    pub ident: ElfIdent, // Already read, but needed for alignment if reading whole struct.
    // Note: We might read Ident first separately to decide endianness.
    // But if we use explicit endian reader, we can read this.
    pub type_: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

#[derive(Debug, Clone)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

#[derive(Debug, Clone)]
pub struct Elf64Shdr {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
}

#[derive(Debug, Clone)]
pub struct Elf64Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

// --- 32-bit Structures ---

#[derive(Debug, Clone)]
pub struct Elf32Header {
    pub ident: ElfIdent,
    pub type_: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u32,
    pub phoff: u32,
    pub shoff: u32,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

#[derive(Debug, Clone)]
pub struct Elf32Phdr {
    pub p_type: u32,
    pub p_offset: u32,
    pub p_vaddr: u32,
    pub p_paddr: u32,
    pub p_filesz: u32,
    pub p_memsz: u32,
    pub p_flags: u32,
    pub p_align: u32,
}

#[derive(Debug, Clone)]
pub struct Elf32Shdr {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u32,
    pub sh_addr: u32,
    pub sh_offset: u32,
    pub sh_size: u32,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u32,
    pub sh_entsize: u32,
}

#[derive(Debug, Clone)]
pub struct Elf32Sym {
    pub st_name: u32,
    pub st_value: u32,
    pub st_size: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
}

impl ElfIdent {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let reader = ByteReader::little(bytes);
        if reader.slice(0, 4)? != b"\x7fELF" {
            return Err(err!(loader, "Invalid ELF magic"));
        }
        Ok(Self {
            class: reader.u8(4)?,
            endian: reader.u8(5)?,
            version: reader.u8(6)?,
            os_abi: reader.u8(7)?,
            abi_version: reader.u8(8)?,
        })
    }
}

impl Elf64Header {
    pub fn parse(bytes: &[u8], reader: &ByteReader<'_>) -> Result<Self> {
        let ident = ElfIdent::parse(bytes)?;
        Ok(Self {
            ident,
            type_: reader.u16(16)?,
            machine: reader.u16(18)?,
            version: reader.u32(20)?,
            entry: reader.u64(24)?,
            phoff: reader.u64(32)?,
            shoff: reader.u64(40)?,
            flags: reader.u32(48)?,
            ehsize: reader.u16(52)?,
            phentsize: reader.u16(54)?,
            phnum: reader.u16(56)?,
            shentsize: reader.u16(58)?,
            shnum: reader.u16(60)?,
            shstrndx: reader.u16(62)?,
        })
    }
}

impl Elf64Phdr {
    pub const SIZE: usize = 56;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            p_type: reader.u32(offset)?,
            p_flags: reader.u32(offset + 4)?,
            p_offset: reader.u64(offset + 8)?,
            p_vaddr: reader.u64(offset + 16)?,
            p_paddr: reader.u64(offset + 24)?,
            p_filesz: reader.u64(offset + 32)?,
            p_memsz: reader.u64(offset + 40)?,
            p_align: reader.u64(offset + 48)?,
        })
    }
}

impl Elf64Shdr {
    pub const SIZE: usize = 64;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            sh_name: reader.u32(offset)?,
            sh_type: reader.u32(offset + 4)?,
            sh_flags: reader.u64(offset + 8)?,
            sh_addr: reader.u64(offset + 16)?,
            sh_offset: reader.u64(offset + 24)?,
            sh_size: reader.u64(offset + 32)?,
            sh_link: reader.u32(offset + 40)?,
            sh_info: reader.u32(offset + 44)?,
            sh_addralign: reader.u64(offset + 48)?,
            sh_entsize: reader.u64(offset + 56)?,
        })
    }
}

impl Elf64Sym {
    pub const SIZE: usize = 24;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            st_name: reader.u32(offset)?,
            st_info: reader.u8(offset + 4)?,
            st_other: reader.u8(offset + 5)?,
            st_shndx: reader.u16(offset + 6)?,
            st_value: reader.u64(offset + 8)?,
            st_size: reader.u64(offset + 16)?,
        })
    }
}

impl Elf32Header {
    pub fn parse(bytes: &[u8], reader: &ByteReader<'_>) -> Result<Self> {
        let ident = ElfIdent::parse(bytes)?;
        Ok(Self {
            ident,
            type_: reader.u16(16)?,
            machine: reader.u16(18)?,
            version: reader.u32(20)?,
            entry: reader.u32(24)?,
            phoff: reader.u32(28)?,
            shoff: reader.u32(32)?,
            flags: reader.u32(36)?,
            ehsize: reader.u16(40)?,
            phentsize: reader.u16(42)?,
            phnum: reader.u16(44)?,
            shentsize: reader.u16(46)?,
            shnum: reader.u16(48)?,
            shstrndx: reader.u16(50)?,
        })
    }
}

impl Elf32Phdr {
    pub const SIZE: usize = 32;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            p_type: reader.u32(offset)?,
            p_offset: reader.u32(offset + 4)?,
            p_vaddr: reader.u32(offset + 8)?,
            p_paddr: reader.u32(offset + 12)?,
            p_filesz: reader.u32(offset + 16)?,
            p_memsz: reader.u32(offset + 20)?,
            p_flags: reader.u32(offset + 24)?,
            p_align: reader.u32(offset + 28)?,
        })
    }
}

impl Elf32Shdr {
    pub const SIZE: usize = 40;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            sh_name: reader.u32(offset)?,
            sh_type: reader.u32(offset + 4)?,
            sh_flags: reader.u32(offset + 8)?,
            sh_addr: reader.u32(offset + 12)?,
            sh_offset: reader.u32(offset + 16)?,
            sh_size: reader.u32(offset + 20)?,
            sh_link: reader.u32(offset + 24)?,
            sh_info: reader.u32(offset + 28)?,
            sh_addralign: reader.u32(offset + 32)?,
            sh_entsize: reader.u32(offset + 36)?,
        })
    }
}

impl Elf32Sym {
    pub const SIZE: usize = 16;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            st_name: reader.u32(offset)?,
            st_value: reader.u32(offset + 4)?,
            st_size: reader.u32(offset + 8)?,
            st_info: reader.u8(offset + 12)?,
            st_other: reader.u8(offset + 13)?,
            st_shndx: reader.u16(offset + 14)?,
        })
    }
}
