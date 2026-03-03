use binrw::BinRead;

// Load command types (from mach-o/loader.h)
pub const LC_SEGMENT: u32 = 0x1;
pub const LC_SYMTAB: u32 = 0x2;
pub const LC_DYSYMTAB: u32 = 0xB;
pub const LC_SEGMENT_64: u32 = 0x19;
pub const LC_MAIN: u32 = 0x80000028; // LC_REQ_DYLD | 0x28
/// LC_FUNCTION_STARTS: compressed table of function start addresses.
/// Ghidra's MachoFunctionStartsAnalyzer uses this to discover all functions
/// defined in a Mach-O binary, including those not exported or symbolicated.
pub const LC_FUNCTION_STARTS: u32 = 0x26;

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
pub struct DysymtabCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub ilocalsym: u32,
    pub nlocalsym: u32,
    pub iextdefsym: u32,
    pub nextdefsym: u32,
    pub iundefsym: u32,
    pub nundefsym: u32,
    pub tocoff: u32,
    pub ntoc: u32,
    pub modtaboff: u32,
    pub nmodtab: u32,
    pub extrefsymoff: u32,
    pub nextrefsyms: u32,
    pub indirectsymoff: u32,
    pub nindirectsyms: u32,
    pub extreloff: u32,
    pub nextrel: u32,
    pub locreloff: u32,
    pub nlocrel: u32,
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

#[derive(BinRead, Debug, Clone)]
pub struct EntryPointCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub entryoff: u64,  // file (__TEXT) offset of main()
    pub stacksize: u64, // initial stack size (usually 0)
}

/// Generic linkedit-data command (LC_FUNCTION_STARTS, LC_CODE_SIGNATURE, …).
/// The actual data sits at `dataoff` bytes from the start of the file.
#[derive(BinRead, Debug, Clone)]
pub struct LinkeditDataCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub dataoff: u32,  // file offset of the data blob
    pub datasize: u32, // size of the data blob
}
