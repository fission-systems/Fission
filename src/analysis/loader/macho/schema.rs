use binrw::BinRead;

// Constants (copied from mach-o/loader.h concepts)
pub const MH_MAGIC: u32 = 0xFEEDFACE;
pub const MH_CIGAM: u32 = 0xCEFAEDFE;
pub const MH_MAGIC_64: u32 = 0xFEEDFACF;
pub const MH_CIGAM_64: u32 = 0xCFFAEDFE;

pub const LC_SEGMENT: u32 = 0x1;
pub const LC_SYMTAB: u32 = 0x2;
pub const LC_SEGMENT_64: u32 = 0x19;

#[derive(BinRead, Debug, Clone)]
pub struct MachHeader64 {
    pub magic: u32,
    pub cputype: i32,
    pub cpusubtype: i32,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
    pub flags: u32,
    pub reserved: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct MachHeader32 {
    pub magic: u32,
    pub cputype: i32,
    pub cpusubtype: i32,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
    pub flags: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct LoadCommand {
    pub cmd: u32,
    pub cmdsize: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct SegmentCommand64 {
    pub cmd: u32,
    pub cmdsize: u32,
    #[br(count = 16)]
    pub segname: Vec<u8>,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
    pub maxprot: i32,
    pub initprot: i32,
    pub nsects: u32,
    pub flags: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct Section64 {
    #[br(count = 16)]
    pub sectname: Vec<u8>,
    #[br(count = 16)]
    pub segname: Vec<u8>,
    pub addr: u64,
    pub size: u64,
    pub offset: u32,
    pub align: u32,
    pub reloff: u32,
    pub nreloc: u32,
    pub flags: u32,
    pub reserved1: u32,
    pub reserved2: u32,
    pub reserved3: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct SegmentCommand32 {
    pub cmd: u32,
    pub cmdsize: u32,
    #[br(count = 16)]
    pub segname: Vec<u8>,
    pub vmaddr: u32,
    pub vmsize: u32,
    pub fileoff: u32,
    pub filesize: u32,
    pub maxprot: i32,
    pub initprot: i32,
    pub nsects: u32,
    pub flags: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct Section32 {
    #[br(count = 16)]
    pub sectname: Vec<u8>,
    #[br(count = 16)]
    pub segname: Vec<u8>,
    pub addr: u32,
    pub size: u32,
    pub offset: u32,
    pub align: u32,
    pub reloff: u32,
    pub nreloc: u32,
    pub flags: u32,
    pub reserved1: u32,
    pub reserved2: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct SymtabCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub symoff: u32,
    pub nsyms: u32,
    pub stroff: u32,
    pub strsize: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct Nlist64 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u64,
}

#[derive(BinRead, Debug, Clone)]
pub struct Nlist32 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u32,
}
