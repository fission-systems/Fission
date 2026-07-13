//! MinGW pseudo-relocation list discovery for PE images.
//!
//! Scans `__RUNTIME_PSEUDO_RELOC_LIST__` and v1/v2 pseudo-reloc entries to emit
//! CFG label leaders and relocation use-sites without Ghidra runtime dependency.

use crate::loader::types::RelocationEntry;

const PSEUDO_RELOC_LIST_SYMBOL: &str = "__RUNTIME_PSEUDO_RELOC_LIST__";
const PSEUDO_RELOC_LIST_END_SYMBOL: &str = "__RUNTIME_PSEUDO_RELOC_LIST_END__";
const RUNTIME_RELOCATOR_SYMBOL: &str = "_pei386_runtime_relocator";

const V2_MAGIC1: u32 = 0;
const V2_MAGIC2: u32 = 0;
const V2_VERSION: u32 = 1;

#[derive(Debug, Default)]
pub(super) struct MingwPseudoRelocFacts {
    pub cfg_label_leaders: Vec<u64>,
    pub relocation_use_sites: Vec<u64>,
}

pub(super) fn scan_mingw_pseudo_relocs<F>(
    image_base: u64,
    is_64bit: bool,
    global_symbols: &std::collections::HashMap<u64, String>,
    view_bytes: F,
    likely_mingw: bool,
) -> MingwPseudoRelocFacts
where
    F: Fn(u64, usize) -> Option<Vec<u8>>,
{
    if !likely_mingw {
        return MingwPseudoRelocFacts::default();
    }

    let mut facts = MingwPseudoRelocFacts::default();
    let list_va = global_symbols
        .iter()
        .find_map(|(addr, name)| is_pseudo_reloc_list_symbol(name).then_some(*addr));

    let Some(list_symbol_va) = list_va else {
        if let Some((addr, _)) = global_symbols
            .iter()
            .find(|(_, name)| name.as_str() == RUNTIME_RELOCATOR_SYMBOL)
        {
            facts.cfg_label_leaders.push(*addr);
        }
        return facts;
    };

    facts.cfg_label_leaders.push(list_symbol_va);

    let list_start = read_pointer(&view_bytes, list_symbol_va, is_64bit)
        .filter(|ptr| *ptr >= image_base && *ptr != list_symbol_va && view_bytes(*ptr, 8).is_some())
        .unwrap_or(list_symbol_va);
    facts.cfg_label_leaders.push(list_start);

    if let Some((addr, _)) = global_symbols
        .iter()
        .find(|(_, name)| name.as_str() == PSEUDO_RELOC_LIST_END_SYMBOL)
    {
        facts.cfg_label_leaders.push(*addr);
    }
    if let Some((addr, _)) = global_symbols
        .iter()
        .find(|(_, name)| name.as_str() == RUNTIME_RELOCATOR_SYMBOL)
    {
        facts.cfg_label_leaders.push(*addr);
    }

    let mut offset = 0usize;
    if is_v2_header(&view_bytes, list_start) {
        offset = 12;
    }

    loop {
        let entry_addr = list_start.saturating_add(offset as u64);
        let raw = match view_bytes(entry_addr, 8) {
            Some(bytes) if bytes.len() >= 8 => bytes,
            _ => break,
        };
        let word0 = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        let word1 = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        if word0 == 0 && word1 == 0 {
            facts.cfg_label_leaders.push(entry_addr);
            break;
        }

        let use_site = image_base.saturating_add(word1 as u64);
        if use_site != image_base {
            facts.relocation_use_sites.push(use_site);
        }

        offset = offset.saturating_add(8);
        if offset > 1024 * 1024 {
            break;
        }
    }

    facts.cfg_label_leaders.sort_unstable();
    facts.cfg_label_leaders.dedup();
    facts.relocation_use_sites.sort_unstable();
    facts.relocation_use_sites.dedup();
    facts
}

fn is_pseudo_reloc_list_symbol(name: &str) -> bool {
    name == PSEUDO_RELOC_LIST_SYMBOL
        || name.trim_start_matches('_') == "RUNTIME_PSEUDO_RELOC_LIST__"
}

pub(super) fn mingw_pseudo_reloc_entries(use_sites: &[u64]) -> Vec<RelocationEntry> {
    use_sites
        .iter()
        .map(|&address| RelocationEntry {
            address,
            r_type: if address > u32::MAX as u64 { 10 } else { 3 },
            size: 4,
            addend: 0,
            symbol_name: None,
        })
        .collect()
}

fn is_v2_header<F>(view_bytes: &F, addr: u64) -> bool
where
    F: Fn(u64, usize) -> Option<Vec<u8>>,
{
    let raw = match view_bytes(addr, 12) {
        Some(bytes) if bytes.len() >= 12 => bytes,
        _ => return false,
    };
    let magic1 = u32::from_le_bytes(raw[0..4].try_into().unwrap());
    let magic2 = u32::from_le_bytes(raw[4..8].try_into().unwrap());
    let version = u32::from_le_bytes(raw[8..12].try_into().unwrap());
    magic1 == V2_MAGIC1 && magic2 == V2_MAGIC2 && version == V2_VERSION
}

fn read_pointer<F>(view_bytes: &F, addr: u64, is_64bit: bool) -> Option<u64>
where
    F: Fn(u64, usize) -> Option<Vec<u8>>,
{
    if is_64bit {
        let raw = view_bytes(addr, 8)?;
        Some(u64::from_le_bytes(raw.try_into().ok()?))
    } else {
        let raw = view_bytes(addr, 4)?;
        Some(u32::from_le_bytes(raw.try_into().ok()?) as u64)
    }
}

pub(super) fn is_likely_mingw_pe(
    rich_header_records: Option<&[crate::loader::types::RichHeaderRecord]>,
    global_symbols: &std::collections::HashMap<u64, String>,
) -> bool {
    if global_symbols.values().any(|name| {
        name.contains("PSEUDO_RELOC")
            || name.contains("__mingw")
            || name == RUNTIME_RELOCATOR_SYMBOL
    }) {
        return true;
    }
    rich_header_records.is_some_and(|records| {
        records.iter().any(|record| {
            matches!(record.product_id, 0x0103..=0x0105) || record.build_number >= 0x0100
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_pseudo_reloc_list_emits_use_sites_and_end_label() {
        let image_base = 0x1400_0000;
        let list_rva = 0x5000u64;
        let target_rva = 0x1200u64;
        let mut bytes = vec![0u8; 0x6000];
        let list_start = image_base + list_rva;
        let use_site = image_base + target_rva;

        bytes[list_rva as usize..list_rva as usize + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[list_rva as usize + 4..list_rva as usize + 8]
            .copy_from_slice(&(target_rva as u32).to_le_bytes());
        bytes[list_rva as usize + 8..list_rva as usize + 12].copy_from_slice(&0u32.to_le_bytes());
        bytes[list_rva as usize + 12..list_rva as usize + 16].copy_from_slice(&0u32.to_le_bytes());

        let mut globals = std::collections::HashMap::new();
        globals.insert(list_start, PSEUDO_RELOC_LIST_SYMBOL.to_string());

        let view = |addr: u64, len: usize| {
            let start = addr.saturating_sub(image_base) as usize;
            let end = start.saturating_add(len);
            bytes.get(start..end).map(|slice| slice.to_vec())
        };

        let facts = scan_mingw_pseudo_relocs(image_base, true, &globals, &view, true);
        assert!(facts.relocation_use_sites.contains(&use_site));
        assert!(facts.cfg_label_leaders.contains(&list_start));
    }

    #[test]
    fn refptr_symbol_is_not_treated_as_pseudo_reloc_list() {
        let image_base = 0x1400_0000;
        let refptr = image_base + 0x4000;
        let mut globals = std::collections::HashMap::new();
        globals.insert(refptr, "_refptr___RUNTIME_PSEUDO_RELOC_LIST__".to_string());

        let view = |_addr: u64, len: usize| Some(vec![0; len]);
        let facts = scan_mingw_pseudo_relocs(image_base, true, &globals, &view, true);
        assert!(facts.relocation_use_sites.is_empty());
        assert!(facts.cfg_label_leaders.is_empty());
    }
}
