use binrw::BinRead;

#[derive(BinRead, Debug, Clone)]
#[br(magic = b"MZ")]
pub struct PeFile {
    pub dos_header: DosHeader,

    // The DOS header contains the offset to the NT headers (e_lfanew).
    // We seek to that position to read the NT headers.
    #[br(seek_before = binrw::io::SeekFrom::Start(dos_header.e_lfanew as u64))]
    pub nt_headers: NtHeaders,

    // Reading sections requires knowing the NumberOfSections from FileHeader
    // and following the OptionalHeader.
    // robustly seek to Start of Optional Header + SizeOfOptionalHeader
    #[br(seek_before = binrw::io::SeekFrom::Start(
        dos_header.e_lfanew as u64 + 4 + 20 + nt_headers.file_header.size_of_optional_header as u64
    ))]
    #[br(count = nt_headers.file_header.number_of_sections)]
    pub section_headers: Vec<SectionHeader>,
}

#[derive(BinRead, Debug, Clone)]
pub struct DosHeader {
    #[br(pad_before = 58)]
    // e_cblp through e_oemid, total 60 bytes minus parsed fields (magic 2 bytes)
    pub e_lfanew: u32,
}

#[derive(BinRead, Debug, Clone)]
#[br(magic = b"PE\0\0")]
pub struct NtHeaders {
    pub file_header: FileHeader,
    pub optional_header: OptionalHeader,
}

#[derive(BinRead, Debug, Clone)]
pub struct FileHeader {
    pub machine: u16,
    pub number_of_sections: u16,
    pub time_date_stamp: u32,
    pub pointer_to_symbol_table: u32,
    pub number_of_symbols: u32,
    pub size_of_optional_header: u16,
    pub characteristics: u16,
}

#[derive(BinRead, Debug, Clone)]
pub enum OptionalHeader {
    #[br(magic(0x10bu16))]
    Pe32(OptionalHeader32),

    #[br(magic(0x20bu16))]
    Pe32Plus(OptionalHeader64),
}

#[derive(BinRead, Debug, Clone)]
pub struct OptionalHeader32 {
    pub major_linker_version: u8,
    pub minor_linker_version: u8,
    pub size_of_code: u32,
    pub size_of_initialized_data: u32,
    pub size_of_uninitialized_data: u32,
    pub address_of_entry_point: u32,
    pub base_of_code: u32,
    pub base_of_data: u32,
    pub image_base: u32,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub major_os_version: u16,
    pub minor_os_version: u16,
    pub major_image_version: u16,
    pub minor_image_version: u16,
    pub major_subsystem_version: u16,
    pub minor_subsystem_version: u16,
    pub win32_version_value: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub check_sum: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub size_of_stack_reserve: u32,
    pub size_of_stack_commit: u32,
    pub size_of_heap_reserve: u32,
    pub size_of_heap_commit: u32,
    pub loader_flags: u32,
    pub number_of_rva_and_sizes: u32,
    #[br(count = number_of_rva_and_sizes)]
    pub data_directories: Vec<DataDirectory>,
}

#[derive(BinRead, Debug, Clone)]
pub struct OptionalHeader64 {
    pub major_linker_version: u8,
    pub minor_linker_version: u8,
    pub size_of_code: u32,
    pub size_of_initialized_data: u32,
    pub size_of_uninitialized_data: u32,
    pub address_of_entry_point: u32,
    pub base_of_code: u32,
    pub image_base: u64,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub major_os_version: u16,
    pub minor_os_version: u16,
    pub major_image_version: u16,
    pub minor_image_version: u16,
    pub major_subsystem_version: u16,
    pub minor_subsystem_version: u16,
    pub win32_version_value: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub check_sum: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub size_of_stack_reserve: u64,
    pub size_of_stack_commit: u64,
    pub size_of_heap_reserve: u64,
    pub size_of_heap_commit: u64,
    pub loader_flags: u32,
    pub number_of_rva_and_sizes: u32,
    #[br(count = number_of_rva_and_sizes)]
    pub data_directories: Vec<DataDirectory>,
}

#[derive(BinRead, Debug, Clone)]
pub struct DataDirectory {
    pub virtual_address: u32,
    pub size: u32,
}

#[derive(BinRead, Debug, Clone)]
pub struct SectionHeader {
    #[br(parse_with = parse_sstring)]
    pub name: String,
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub size_of_raw_data: u32,
    pub pointer_to_raw_data: u32,
    pub pointer_to_relocations: u32,
    pub pointer_to_linenumbers: u32,
    pub number_of_relocations: u16,
    pub number_of_linenumbers: u16,
    pub characteristics: u32,
}

// Helper to parse fixed-length null-padded strings (8 bytes for section names)
fn parse_sstring<R: binrw::io::Read + binrw::io::Seek>(
    reader: &mut R,
    _: binrw::Endian,
    _: (),
) -> binrw::BinResult<String> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(8);
    Ok(String::from_utf8_lossy(&bytes[..len]).to_string())
}

#[derive(BinRead, Debug, Clone)]
pub struct ExportDirectory {
    pub characteristics: u32,
    pub time_date_stamp: u32,
    pub major_version: u16,
    pub minor_version: u16,
    pub name_rva: u32,
    pub base: u32,
    pub number_of_functions: u32,
    pub number_of_names: u32,
    pub address_of_functions: u32,     // RVA -> Array of u32 (RVAs)
    pub address_of_names: u32,         // RVA -> Array of u32 (RVAs pointing to strings)
    pub address_of_name_ordinals: u32, // RVA -> Array of u16
}

#[derive(BinRead, Debug, Clone)]
pub struct ImportDescriptor {
    pub original_first_thunk: u32, // RVA to ILT (Import Lookup Table)
    pub time_date_stamp: u32,
    pub forwarder_chain: u32,
    pub name: u32,        // RVA to DLL name string
    pub first_thunk: u32, // RVA to IAT (Import Address Table)
}
