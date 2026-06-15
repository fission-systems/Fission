//! Binary-level control-flow facts for Sleigh lift / CFG construction.
//!
//! Merges loader labels, xref flow references (jump-only, call excluded per Ghidra BBM),
//! relocation-derived indirect targets, and Ghidra noreturn metadata into a single sliceable
//! fact bundle consumed by `DecodeMemoryContext`.

mod decode_context;

use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use fission_core::core::ghidra_no_return::{binary_format_to_ghidra_format, ghidra_no_return_index};
use fission_loader::loader::{LoadedBinary, RelocationEntry};
use fission_sleigh::runtime::{DecodeMemoryContext, RuntimeSleighFrontend};
use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::analysis::xrefs::{XrefDatabase, XrefType};

pub use decode_context::{decode_memory_context_for, function_max_bytes};

const FACTS_CACHE_CAPACITY: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFlowFacts {
    pub label_leaders: Vec<u64>,
    pub flow_edges: Vec<(u64, u64)>,
    pub indirect_targets: Vec<u64>,
    pub noreturn_callsites: Vec<u64>,
    pub function_extents: HashMap<u64, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionControlFlowFacts {
    pub function_address: u64,
    pub labels: Vec<u64>,
    pub flow_edges: Vec<(u64, u64)>,
    pub indirect_targets: Vec<u64>,
    pub noreturn_callsites: Vec<u64>,
}

static FACTS_CACHE: OnceLock<Mutex<LruCache<String, ControlFlowFacts>>> = OnceLock::new();

fn facts_cache() -> &'static Mutex<LruCache<String, ControlFlowFacts>> {
    FACTS_CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(FACTS_CACHE_CAPACITY).expect("non-zero cache capacity"),
        ))
    })
}

/// Return cached control-flow facts for `binary`, assembling on first use.
pub fn control_flow_facts_for(binary: &LoadedBinary) -> ControlFlowFacts {
    let hash = binary.hash.clone();
    if let Ok(mut cache) = facts_cache().lock() {
        if let Some(facts) = cache.get(&hash) {
            return facts.clone();
        }
    }

    let frontend = binary
        .load_spec()
        .and_then(|load_spec| RuntimeSleighFrontend::new_for_load_spec(load_spec).ok());
    let facts = ControlFlowFacts::assemble(binary, frontend.as_ref());

    if let Ok(mut cache) = facts_cache().lock() {
        cache.put(hash, facts.clone());
    }
    facts
}

impl ControlFlowFacts {
    pub fn assemble(
        binary: &LoadedBinary,
        frontend: Option<&RuntimeSleighFrontend>,
    ) -> Self {
        let label_leaders = collect_label_leaders(binary);
        let function_extents = collect_function_extents(binary);
        let indirect_targets = collect_indirect_targets(binary);

        let mut flow_edges = Vec::new();
        let mut noreturn_callsites = BTreeSet::new();

        if let Some(frontend) = frontend {
            let xref_db = XrefDatabase::build_with_frontend(binary, frontend);
            let ghidra_format = binary_format_to_ghidra_format(&binary.format);
            let compiler_key = ghidra_no_return_compiler_key(binary);
            let no_return_idx = ghidra_no_return_index();

            for xref in xref_db.iter() {
                match xref.xref_type {
                    XrefType::Jump => {
                        flow_edges.push((xref.from_addr, xref.to_addr));
                    }
                    XrefType::Call => {
                        if ghidra_format.is_some_and(|fmt| {
                            resolve_call_target_name(binary, xref.to_addr).is_some_and(|(symbol, library)| {
                                no_return_idx.is_no_return(fmt, compiler_key, library, &symbol)
                            })
                        }) {
                            noreturn_callsites.insert(xref.from_addr);
                        }
                    }
                    XrefType::Data | XrefType::DataRead | XrefType::DataWrite => {}
                }
            }
        }

        flow_edges.sort_unstable();
        flow_edges.dedup();

        Self {
            label_leaders: label_leaders.into_iter().collect(),
            flow_edges,
            indirect_targets: indirect_targets.into_iter().collect(),
            noreturn_callsites: noreturn_callsites.into_iter().collect(),
            function_extents,
        }
    }

    pub fn decode_context_for(
        &self,
        binary: &LoadedBinary,
        entry_address: u64,
        max_bytes: usize,
    ) -> DecodeMemoryContext {
        let limit_addr = function_limit_addr(binary, entry_address, max_bytes);

        let mut block_entry_hints: Vec<u64> = self
            .label_leaders
            .iter()
            .copied()
            .filter(|addr| (entry_address..limit_addr).contains(addr))
            .collect();
        if block_entry_hints.is_empty() {
            block_entry_hints =
                binary.cfg_block_entry_hints_in_range(entry_address, limit_addr);
        }

        let mut flow_leaders: BTreeSet<u64> = self
            .flow_edges
            .iter()
            .filter_map(|(from, to)| {
                if *from >= entry_address
                    && *from < limit_addr
                    && *to >= entry_address
                    && *to < limit_addr
                {
                    Some(*to)
                } else {
                    None
                }
            })
            .collect();

        for leader in &block_entry_hints {
            flow_leaders.remove(leader);
        }

        let flow_edges: Vec<(u64, u64)> = self
            .flow_edges
            .iter()
            .copied()
            .filter(|(from, to)| {
                *from >= entry_address
                    && *from < limit_addr
                    && *to >= entry_address
                    && *to < limit_addr
            })
            .collect();

        let mut jump_table_targets: Vec<u64> = self
            .indirect_targets
            .iter()
            .copied()
            .filter(|addr| (entry_address..limit_addr).contains(addr))
            .collect();
        jump_table_targets.extend(collect_relocation_targets_in_range(
            binary,
            entry_address,
            limit_addr,
        ));
        jump_table_targets.sort_unstable();
        jump_table_targets.dedup();

        let noreturn_callsites: Vec<u64> = self
            .noreturn_callsites
            .iter()
            .copied()
            .filter(|addr| (entry_address..limit_addr).contains(addr))
            .collect();

        DecodeMemoryContext {
            relative_address_bases: relative_address_bases(binary, entry_address),
            jump_table_targets,
            block_entry_hints,
            flow_leaders: flow_leaders.into_iter().collect(),
            flow_edges,
            noreturn_callsites,
        }
    }

    pub fn facts_for_function(
        &self,
        binary: &LoadedBinary,
        entry_address: u64,
        max_bytes: usize,
    ) -> FunctionControlFlowFacts {
        let ctx = self.decode_context_for(binary, entry_address, max_bytes);
        FunctionControlFlowFacts {
            function_address: entry_address,
            labels: ctx.block_entry_hints,
            flow_edges: ctx.flow_edges,
            indirect_targets: ctx.jump_table_targets,
            noreturn_callsites: ctx.noreturn_callsites,
        }
    }
}

fn collect_label_leaders(binary: &LoadedBinary) -> BTreeSet<u64> {
    let mut leaders = BTreeSet::new();
    leaders.extend(
        binary
            .cfg_label_leaders
            .iter()
            .copied()
            .filter(|addr| is_executable_address(binary, *addr)),
    );
    for func in &binary.functions {
        if func.is_import {
            continue;
        }
        if is_executable_address(binary, func.address) {
            leaders.insert(func.address);
        }
    }
    leaders
}

fn is_executable_address(binary: &LoadedBinary, address: u64) -> bool {
    binary.executable_sections().iter().any(|section| {
        let end = section
            .virtual_address
            .saturating_add(section.virtual_size.max(section.file_size));
        address >= section.virtual_address && address < end
    })
}

fn pe_relocation_is_indirect_candidate(reloc: &RelocationEntry) -> bool {
    matches!(reloc.r_type, 1 | 2 | 3 | 10) && reloc.size > 0
}

fn relocation_use_sites(binary: &LoadedBinary) -> Vec<u64> {
    let inner = binary.inner();
    let mut sites: BTreeSet<u64> = inner.relocation_symbols.keys().copied().collect();
    for reloc in &inner.relocations {
        if pe_relocation_is_indirect_candidate(reloc) {
            sites.insert(reloc.address);
        }
    }
    sites.into_iter().collect()
}

fn read_reloc_target_at_use_site(
    binary: &LoadedBinary,
    use_site: u64,
    little_endian: bool,
) -> Option<u64> {
    if let Some(raw_8) = binary.view_bytes(use_site, 8) {
        let val = if little_endian {
            u64::from_le_bytes(raw_8.try_into().ok()?)
        } else {
            u64::from_be_bytes(raw_8.try_into().ok()?)
        };
        if val != 0 {
            return Some(val);
        }
    }
    if let Some(raw_4) = binary.view_bytes(use_site, 4) {
        let val_32 = if little_endian {
            u32::from_le_bytes(raw_4.try_into().ok()?)
        } else {
            u32::from_be_bytes(raw_4.try_into().ok()?)
        };
        if val_32 != 0 {
            return Some(val_32 as u64);
        }
    }
    None
}

fn collect_function_extents(binary: &LoadedBinary) -> HashMap<u64, u64> {
    let mut entries: Vec<u64> = binary.functions.iter().map(|f| f.address).collect();
    entries.sort_unstable();
    entries.dedup();

    let mut extents = HashMap::new();
    for (idx, &entry) in entries.iter().enumerate() {
        let end = if let Some(func) = binary.function_at_exact(entry) {
            if func.size > 0 {
                entry.saturating_add(func.size as u64)
            } else if let Some(&next) = entries.get(idx + 1) {
                next
            } else {
                section_upper_bound(binary, entry)
            }
        } else if let Some(&next) = entries.get(idx + 1) {
            next
        } else {
            section_upper_bound(binary, entry)
        };
        extents.insert(entry, end);
    }
    extents
}

fn section_upper_bound(binary: &LoadedBinary, address: u64) -> u64 {
    for section in &binary.sections {
        let start = section.virtual_address;
        let end = start.saturating_add(section.virtual_size.max(section.file_size));
        if address >= start && address < end {
            return end;
        }
    }
    address.saturating_add(256 * 1024)
}

fn collect_indirect_targets(binary: &LoadedBinary) -> BTreeSet<u64> {
    let little_endian = !binary.inner().arch_spec.contains("BE");
    let mut targets = BTreeSet::new();

    for use_site in relocation_use_sites(binary) {
        if let Some(val) = read_reloc_target_at_use_site(binary, use_site, little_endian) {
            targets.insert(val);
        }
    }

    targets
}

fn collect_relocation_targets_in_range(
    binary: &LoadedBinary,
    start: u64,
    end: u64,
) -> Vec<u64> {
    let little_endian = !binary.inner().arch_spec.contains("BE");
    let mut targets = Vec::new();

    for use_site in relocation_use_sites(binary) {
        if use_site < start || use_site >= end {
            continue;
        }
        if let Some(val) = read_reloc_target_at_use_site(binary, use_site, little_endian) {
            if val >= start && val < end && !targets.contains(&val) {
                targets.push(val);
            }
        }
    }

    targets
}

fn relative_address_bases(binary: &LoadedBinary, entry_address: u64) -> Vec<u64> {
    let inner = binary.inner();
    let mut relative_address_bases = Vec::new();
    for section in &inner.sections {
        let start = section.virtual_address;
        let end = start.saturating_add(section.virtual_size);
        if entry_address >= start && entry_address < end && !relative_address_bases.contains(&start) {
            relative_address_bases.push(start);
        }
    }
    if inner.image_base != 0 && !relative_address_bases.contains(&inner.image_base) {
        relative_address_bases.push(inner.image_base);
    }
    relative_address_bases
}

fn function_limit_addr(binary: &LoadedBinary, entry_address: u64, max_bytes: usize) -> u64 {
    let inner = binary.inner();
    if let Some(&idx) = inner.function_addr_index.get(&entry_address) {
        if let Some(info) = inner.functions.get(idx) {
            if info.size > 0 {
                return entry_address.saturating_add(info.size as u64);
            }
        }
    }

    let mut next_addr = entry_address.saturating_add(max_bytes as u64);
    for info in &inner.functions {
        if info.address > entry_address && info.address < next_addr {
            next_addr = info.address;
        }
    }
    next_addr
}

fn ghidra_no_return_compiler_key(binary: &LoadedBinary) -> Option<&'static str> {
    let lang = binary
        .identity_report
        .as_ref()?
        .summary
        .likely_language
        .as_deref()?;
    match lang.to_ascii_lowercase().as_str() {
        "go" | "golang" => Some("golang"),
        "rust" => Some("rustc"),
        _ => None,
    }
}

fn resolve_call_target_name(binary: &LoadedBinary, target_addr: u64) -> Option<(String, Option<&str>)> {
    resolve_call_target_name_direct(binary, target_addr).or_else(|| {
        let ptr_size = if binary.is_64bit { 8 } else { 4 };
        let raw = binary.view_bytes(target_addr, ptr_size)?;
        let pointed = if binary.is_64bit {
            u64::from_le_bytes(raw.try_into().ok()?)
        } else {
            u32::from_le_bytes(raw.try_into().ok()?) as u64
        };
        if pointed == 0 {
            return None;
        }
        resolve_call_target_name_direct(binary, pointed)
    })
}

fn resolve_call_target_name_direct(
    binary: &LoadedBinary,
    target_addr: u64,
) -> Option<(String, Option<&str>)> {
    if let Some(iat_name) = binary.iat_symbols.get(&target_addr) {
        let symbol = iat_name
            .rsplit_once('!')
            .map(|(_, sym)| sym.to_string())
            .unwrap_or_else(|| iat_name.clone());
        let library = iat_name
            .rsplit_once('!')
            .map(|(lib, _)| lib.strip_suffix(".dll").unwrap_or(lib))
            .or_else(|| binary.function_at_exact(target_addr).and_then(|f| f.external_library.as_deref()));
        return Some((symbol, library));
    }

    if let Some(name) = binary.global_symbols.get(&target_addr) {
        return Some((name.clone(), None));
    }

    if let Some(name) = binary.relocation_symbols.get(&target_addr) {
        return Some((name.clone(), None));
    }

    if let Some(func) = binary.function_at_exact(target_addr) {
        let library = func.external_library.as_deref();
        return Some((func.name.clone(), library));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder};

    #[test]
    fn label_leaders_include_function_entries_and_loader_labels() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![0; 0x1000]))
            .format("PE")
            .is_64bit(true)
            .image_base(0x1000)
            .add_section(fission_loader::loader::SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x1000,
                file_offset: 0,
                file_size: 0x1000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .cfg_label_leaders([0x1005])
            .add_function(FunctionInfo {
                name: "main".to_string(),
                address: 0x1000,
                size: 32,
                ..Default::default()
            })
            .build()
            .expect("build");

        let facts = ControlFlowFacts::assemble(&binary, None);
        assert!(facts.label_leaders.contains(&0x1000));
        assert!(facts.label_leaders.contains(&0x1005));
        assert_eq!(facts.function_extents.get(&0x1000).copied(), Some(0x1020));
    }

    #[test]
    fn decode_context_slices_labels_and_flow_edges() {
        let facts = ControlFlowFacts {
            label_leaders: vec![0x1000, 0x1010, 0x2000],
            flow_edges: vec![(0x1000, 0x1010), (0x1010, 0x3000)],
            indirect_targets: vec![0x1020],
            noreturn_callsites: vec![0x1008],
            function_extents: HashMap::from([(0x1000, 0x1100)]),
        };

        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .image_base(0x1000)
            .add_section(fission_loader::loader::SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x1000,
                file_offset: 0,
                file_size: 0,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

        let ctx = facts.decode_context_for(&binary, 0x1000, 0x100);
        assert_eq!(ctx.block_entry_hints, vec![0x1000, 0x1010]);
        assert_eq!(ctx.flow_edges, vec![(0x1000, 0x1010)]);
        assert_eq!(ctx.flow_leaders, Vec::<u64>::new());
        assert_eq!(ctx.jump_table_targets, vec![0x1020]);
        assert_eq!(ctx.noreturn_callsites, vec![0x1008]);
    }

    #[test]
    fn pe_relocation_use_sites_union_with_relocation_symbols() {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, RelocationEntry};

        let mut data = vec![0u8; 0x2000];
        data[0x1200..0x1208].copy_from_slice(&0x1400_1500u64.to_le_bytes());
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(data))
            .format("PE")
            .is_64bit(true)
            .image_base(0x1400_0000)
            .add_section(fission_loader::loader::SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1400_1000,
                virtual_size: 0x1000,
                file_offset: 0x1000,
                file_size: 0x1000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_relocations(vec![RelocationEntry {
                address: 0x1400_1200,
                r_type: 10,
                size: 8,
                addend: 0,
                symbol_name: None,
            }])
            .build()
            .expect("build");

        let facts = ControlFlowFacts::assemble(&binary, None);
        assert!(facts.indirect_targets.contains(&0x1400_1500));
    }

    #[test]
    fn noreturn_resolves_one_hop_iat_pointer() {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
        use std::collections::HashMap;

        let imp_slot = 0x1400_3000u64;
        let mut data = vec![0u8; 0x4000];
        data[0x3000..0x3008].copy_from_slice(&0x1400_4000u64.to_le_bytes());

        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(data))
            .format("PE")
            .is_64bit(true)
            .image_base(0x1400_0000)
            .add_section(fission_loader::loader::SectionInfo {
                name: ".data".to_string(),
                virtual_address: 0x1400_3000,
                virtual_size: 0x2000,
                file_offset: 0x3000,
                file_size: 0x2000,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            })
            .add_iat_symbols(HashMap::from([(
                0x1400_4000,
                "KERNEL32.dll!ExitProcess".to_string(),
            )]))
            .build()
            .expect("build");

        let resolved = resolve_call_target_name(&binary, imp_slot);
        assert_eq!(
            resolved.map(|(sym, _)| sym),
            Some("ExitProcess".to_string())
        );
    }
}
