use crate::loader::reader::ByteReader;
use crate::prelude::*;

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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct MachHeader32 {
    pub magic: u32,
    pub cputype: i32,
    pub cpusubtype: i32,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct LoadCommand {
    pub cmd: u32,
    pub cmdsize: u32,
}

#[derive(Debug, Clone)]
pub struct SegmentCommand64 {
    pub cmd: u32,
    pub cmdsize: u32,
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

#[derive(Debug, Clone)]
pub struct Section64 {
    pub sectname: Vec<u8>,
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

#[derive(Debug, Clone)]
pub struct SegmentCommand32 {
    pub cmd: u32,
    pub cmdsize: u32,
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

#[derive(Debug, Clone)]
pub struct Section32 {
    pub sectname: Vec<u8>,
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

#[derive(Debug, Clone)]
pub struct SymtabCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub symoff: u32,
    pub nsyms: u32,
    pub stroff: u32,
    pub strsize: u32,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Nlist64 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u64,
}

#[derive(Debug, Clone)]
pub struct Nlist32 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u32,
}

#[derive(Debug, Clone)]
pub struct EntryPointCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub entryoff: u64,  // file (__TEXT) offset of main()
    pub stacksize: u64, // initial stack size (usually 0)
}

/// Generic linkedit-data command (LC_FUNCTION_STARTS, LC_CODE_SIGNATURE, …).
/// The actual data sits at `dataoff` bytes from the start of the file.
#[derive(Debug, Clone)]
pub struct LinkeditDataCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub dataoff: u32,  // file offset of the data blob
    pub datasize: u32, // size of the data blob
}

impl MachHeader64 {
    pub fn parse(reader: &ByteReader<'_>) -> Result<Self> {
        Ok(Self {
            magic: reader.u32(0)?,
            cputype: reader.i32(4)?,
            cpusubtype: reader.i32(8)?,
            filetype: reader.u32(12)?,
            ncmds: reader.u32(16)?,
            sizeofcmds: reader.u32(20)?,
            flags: reader.u32(24)?,
            reserved: reader.u32(28)?,
        })
    }
}

impl MachHeader32 {
    pub fn parse(reader: &ByteReader<'_>) -> Result<Self> {
        Ok(Self {
            magic: reader.u32(0)?,
            cputype: reader.i32(4)?,
            cpusubtype: reader.i32(8)?,
            filetype: reader.u32(12)?,
            ncmds: reader.u32(16)?,
            sizeofcmds: reader.u32(20)?,
            flags: reader.u32(24)?,
        })
    }
}

impl LoadCommand {
    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
        })
    }
}

impl SegmentCommand64 {
    pub const SIZE: usize = 72;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            segname: reader.slice(offset + 8, 16)?.to_vec(),
            vmaddr: reader.u64(offset + 24)?,
            vmsize: reader.u64(offset + 32)?,
            fileoff: reader.u64(offset + 40)?,
            filesize: reader.u64(offset + 48)?,
            maxprot: reader.i32(offset + 56)?,
            initprot: reader.i32(offset + 60)?,
            nsects: reader.u32(offset + 64)?,
            flags: reader.u32(offset + 68)?,
        })
    }
}

impl Section64 {
    pub const SIZE: usize = 80;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            sectname: reader.slice(offset, 16)?.to_vec(),
            segname: reader.slice(offset + 16, 16)?.to_vec(),
            addr: reader.u64(offset + 32)?,
            size: reader.u64(offset + 40)?,
            offset: reader.u32(offset + 48)?,
            align: reader.u32(offset + 52)?,
            reloff: reader.u32(offset + 56)?,
            nreloc: reader.u32(offset + 60)?,
            flags: reader.u32(offset + 64)?,
            reserved1: reader.u32(offset + 68)?,
            reserved2: reader.u32(offset + 72)?,
            reserved3: reader.u32(offset + 76)?,
        })
    }
}

impl SegmentCommand32 {
    pub const SIZE: usize = 56;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            segname: reader.slice(offset + 8, 16)?.to_vec(),
            vmaddr: reader.u32(offset + 24)?,
            vmsize: reader.u32(offset + 28)?,
            fileoff: reader.u32(offset + 32)?,
            filesize: reader.u32(offset + 36)?,
            maxprot: reader.i32(offset + 40)?,
            initprot: reader.i32(offset + 44)?,
            nsects: reader.u32(offset + 48)?,
            flags: reader.u32(offset + 52)?,
        })
    }
}

impl Section32 {
    pub const SIZE: usize = 68;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            sectname: reader.slice(offset, 16)?.to_vec(),
            segname: reader.slice(offset + 16, 16)?.to_vec(),
            addr: reader.u32(offset + 32)?,
            size: reader.u32(offset + 36)?,
            offset: reader.u32(offset + 40)?,
            align: reader.u32(offset + 44)?,
            reloff: reader.u32(offset + 48)?,
            nreloc: reader.u32(offset + 52)?,
            flags: reader.u32(offset + 56)?,
            reserved1: reader.u32(offset + 60)?,
            reserved2: reader.u32(offset + 64)?,
        })
    }
}

impl SymtabCommand {
    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            symoff: reader.u32(offset + 8)?,
            nsyms: reader.u32(offset + 12)?,
            stroff: reader.u32(offset + 16)?,
            strsize: reader.u32(offset + 20)?,
        })
    }
}

impl DysymtabCommand {
    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            ilocalsym: reader.u32(offset + 8)?,
            nlocalsym: reader.u32(offset + 12)?,
            iextdefsym: reader.u32(offset + 16)?,
            nextdefsym: reader.u32(offset + 20)?,
            iundefsym: reader.u32(offset + 24)?,
            nundefsym: reader.u32(offset + 28)?,
            tocoff: reader.u32(offset + 32)?,
            ntoc: reader.u32(offset + 36)?,
            modtaboff: reader.u32(offset + 40)?,
            nmodtab: reader.u32(offset + 44)?,
            extrefsymoff: reader.u32(offset + 48)?,
            nextrefsyms: reader.u32(offset + 52)?,
            indirectsymoff: reader.u32(offset + 56)?,
            nindirectsyms: reader.u32(offset + 60)?,
            extreloff: reader.u32(offset + 64)?,
            nextrel: reader.u32(offset + 68)?,
            locreloff: reader.u32(offset + 72)?,
            nlocrel: reader.u32(offset + 76)?,
        })
    }
}

impl Nlist64 {
    pub const SIZE: usize = 16;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            n_strx: reader.u32(offset)?,
            n_type: reader.u8(offset + 4)?,
            n_sect: reader.u8(offset + 5)?,
            n_desc: reader.u16(offset + 6)?,
            n_value: reader.u64(offset + 8)?,
        })
    }
}

impl Nlist32 {
    pub const SIZE: usize = 12;

    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            n_strx: reader.u32(offset)?,
            n_type: reader.u8(offset + 4)?,
            n_sect: reader.u8(offset + 5)?,
            n_desc: reader.u16(offset + 6)?,
            n_value: reader.u32(offset + 8)?,
        })
    }
}

impl EntryPointCommand {
    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            entryoff: reader.u64(offset + 8)?,
            stacksize: reader.u64(offset + 16)?,
        })
    }
}

impl LinkeditDataCommand {
    pub fn parse(reader: &ByteReader<'_>, offset: usize) -> Result<Self> {
        Ok(Self {
            cmd: reader.u32(offset)?,
            cmdsize: reader.u32(offset + 4)?,
            dataoff: reader.u32(offset + 8)?,
            datasize: reader.u32(offset + 12)?,
        })
    }
}
