use binrw::BinRead;

// Initial Identification (first 16 bytes)
#[derive(BinRead, Debug, Clone, Copy, PartialEq)]
#[br(magic = b"\x7fELF")]
pub struct ElfIdent {
    pub class: u8,  // 1=32, 2=64
    pub endian: u8, // 1=Little, 2=Big
    pub version: u8,
    pub os_abi: u8,
    pub abi_version: u8,
    #[br(pad_before = 7)] // padding
    pub _pad: (),
}

// --- 64-bit Structures ---

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
pub struct Elf64Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

// --- 32-bit Structures ---

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
pub struct Elf32Sym {
    pub st_name: u32,
    pub st_value: u32,
    pub st_size: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
}
