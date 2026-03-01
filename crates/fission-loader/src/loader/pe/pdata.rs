use super::*;

pub(super) fn parse_pdata(
    loader: &PeLoaderImpl<'_>,
    pdata_rva: u32,
    pdata_size: u32,
    image_base: u64,
) -> Result<Vec<crate::loader::types::FunctionInfo>> {
    let mut functions = Vec::new();

    let pdata_offset = match loader.rva_to_file_offset(pdata_rva, image_base) {
        Some(off) => off,
        None => return Ok(functions),
    };

    let entry_count = (pdata_size / 12) as usize;

    for i in 0..entry_count {
        let entry_offset = pdata_offset + (i * 12) as u64;

        if entry_offset + 12 > loader.data.len() as u64 {
            break;
        }

        let begin_rva = u32::from_le_bytes([
            loader.data[entry_offset as usize],
            loader.data[(entry_offset + 1) as usize],
            loader.data[(entry_offset + 2) as usize],
            loader.data[(entry_offset + 3) as usize],
        ]);

        let end_rva = u32::from_le_bytes([
            loader.data[(entry_offset + 4) as usize],
            loader.data[(entry_offset + 5) as usize],
            loader.data[(entry_offset + 6) as usize],
            loader.data[(entry_offset + 7) as usize],
        ]);

        if begin_rva == 0 || begin_rva >= end_rva {
            continue;
        }

        let func_addr = image_base + begin_rva as u64;
        let func_size = (end_rva - begin_rva) as u64;

        functions.push(crate::loader::types::FunctionInfo {
            name: format!("FUN_0x{:x}", func_addr),
            address: func_addr,
            size: func_size,
            is_export: false,
            is_import: false,
        });
    }

    Ok(functions)
}
