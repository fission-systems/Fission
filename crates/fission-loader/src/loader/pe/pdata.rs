use super::*;

/// `UNW_FLAG_CHAININFO` in the flags of the `UNWIND_INFO` at `unwind_rva`.
fn unwind_info_is_chained(loader: &PeLoaderImpl<'_>, unwind_rva: u32, image_base: u64) -> bool {
    const UNW_FLAG_CHAININFO: u8 = 0x4;
    loader
        .rva_to_file_offset(unwind_rva, image_base)
        .and_then(|offset| loader.data.get(offset as usize))
        // Version in bits 0-2, flags in bits 3-7.
        .is_some_and(|first| (first >> 3) & UNW_FLAG_CHAININFO != 0)
}

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

        // A RUNTIME_FUNCTION whose unwind info is chained describes a fragment
        // of another function (a cold or split block), not a function of its
        // own: registering it plants a phantom entry inside its parent.
        let unwind_rva = u32::from_le_bytes([
            loader.data[(entry_offset + 8) as usize],
            loader.data[(entry_offset + 9) as usize],
            loader.data[(entry_offset + 10) as usize],
            loader.data[(entry_offset + 11) as usize],
        ]);
        if unwind_info_is_chained(loader, unwind_rva & !1, image_base) {
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
            origin: Some("pe-pdata".to_string()),
            kind: Some("code".to_string()),
            source_section: Some(".pdata".to_string()),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        });
    }

    Ok(functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::types::SectionInfo;

    const IMAGE_BASE: u64 = 0x40_0000;

    /// `.pdata` at RVA 0x1000 with two entries; `UNWIND_INFO` for them at
    /// RVA 0x1100 and 0x1108. The second entry's info carries `flags`.
    fn pdata_with_second_entry_flags(flags: u8) -> Vec<u8> {
        let mut data = vec![0u8; 0x200];
        let entry = |data: &mut Vec<u8>, at: usize, begin: u32, end: u32, unwind: u32| {
            data[at..at + 4].copy_from_slice(&begin.to_le_bytes());
            data[at + 4..at + 8].copy_from_slice(&end.to_le_bytes());
            data[at + 8..at + 12].copy_from_slice(&unwind.to_le_bytes());
        };
        entry(&mut data, 0, 0x2000, 0x2040, 0x1100);
        entry(&mut data, 12, 0x2040, 0x2060, 0x1108);
        data[0x100] = 1; // version 1, no flags
        data[0x108] = 1 | (flags << 3);
        data
    }

    fn discovered(flags: u8) -> Vec<u64> {
        let data = pdata_with_second_entry_flags(flags);
        let sections = [SectionInfo {
            name: ".pdata".to_string(),
            virtual_address: IMAGE_BASE + 0x1000,
            virtual_size: 0x200,
            file_offset: 0,
            file_size: 0x200,
            is_executable: false,
            is_readable: true,
            is_writable: false,
        }];
        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };
        parse_pdata(&loader, 0x1000, 24, IMAGE_BASE)
            .unwrap()
            .into_iter()
            .map(|function| function.address)
            .collect()
    }

    #[test]
    fn an_unchained_entry_is_a_function() {
        assert_eq!(
            discovered(0),
            vec![IMAGE_BASE + 0x2000, IMAGE_BASE + 0x2040]
        );
    }

    #[test]
    fn a_chained_unwind_entry_is_a_fragment_not_a_function() {
        // UNW_FLAG_CHAININFO
        assert_eq!(discovered(0x4), vec![IMAGE_BASE + 0x2000]);
        // CHAININFO together with the handler flags is still chained.
        assert_eq!(discovered(0x4 | 0x1), vec![IMAGE_BASE + 0x2000]);
    }
}
