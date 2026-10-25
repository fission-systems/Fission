//! Binary-level control-flow facts for Sleigh lift / CFG construction.
//!
//! Merges loader labels, xref flow references (jump-only, call excluded per Ghidra BBM),
//! relocation-derived indirect targets, and Ghidra noreturn metadata into a single sliceable
//! fact bundle consumed by `DecodeMemoryContext`.

mod decode_context;

use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

use fission_core::core::ghidra_no_return::{
    binary_format_to_ghidra_format, ghidra_no_return_index,
};
use fission_loader::loader::{LoadedBinary, RelocationEntry};
use fission_sleigh::runtime::{DecodeMemoryContext, RuntimeSleighFrontend};
use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::analysis::noreturn::{DecodeSeeds, no_return_functions_with_seeds};
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
    /// LSDA landing pads (see `collect_lsda_landing_pads`) -- unlike
    /// `label_leaders`, these need to seed `DecodeMemoryContext::
    /// additional_decode_entries` too, so kept as a distinct set rather
    /// than only merged into `label_leaders`.
    pub lsda_landing_pads: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionControlFlowFacts {
    pub function_address: u64,
    pub labels: Vec<u64>,
    pub flow_edges: Vec<(u64, u64)>,
    pub indirect_targets: Vec<u64>,
    pub noreturn_callsites: Vec<u64>,
}

struct CachedControlFlowFacts {
    facts: ControlFlowFacts,
    storage: fission_loader::loader::LoadedBinaryStorageToken,
}

static FACTS_CACHE: OnceLock<
    Mutex<LruCache<String, std::sync::Arc<OnceLock<CachedControlFlowFacts>>>>,
> = OnceLock::new();

fn facts_cache()
-> &'static Mutex<LruCache<String, std::sync::Arc<OnceLock<CachedControlFlowFacts>>>> {
    FACTS_CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(FACTS_CACHE_CAPACITY).expect("non-zero cache capacity"),
        ))
    })
}

/// Return cached control-flow facts for `binary`, assembling on first use.
///
/// FID signature matching (`ingest_signature_matches_with_databases`) calls
/// this once per candidate function, in parallel across every rayon worker,
/// for the SAME binary -- so the very first call after a cache miss used to
/// be a thundering herd: the lock was released between the miss check and
/// the `assemble` call, so every one of those parallel callers independently
/// saw "not cached yet" and each ran the full whole-binary xref/no-return
/// analysis at once, all fighting over the same CPU cores simultaneously.
/// Confirmed via a real 6500-function binary's first decompile taking
/// anywhere from ~50s to 30+ minutes across otherwise-identical runs --
/// exactly the variance a scheduling-dependent stampede produces, not a
/// per-function cost. Each binary hash now maps to a shared
/// `Arc<OnceLock<..>>`; concurrent callers for the same binary block on that
/// one slot's `get_or_init` instead of redoing the work, so only one thread
/// ever actually computes it.
pub fn control_flow_facts_for(binary: &LoadedBinary) -> ControlFlowFacts {
    with_control_flow_facts(binary, |view| view.facts.clone())
}

/// Project immutable cached facts while retaining their initialization slot.
fn with_control_flow_facts<R>(
    binary: &LoadedBinary,
    project: impl FnOnce(AssembledControlFlowFacts<'_>) -> R,
) -> R {
    with_control_flow_facts_and_seeds(binary, None, project)
}

/// Capture only in the winning cold initializer; no traces enter the global cache.
pub(crate) fn prepare_fid_decode_seeds(binary: &LoadedBinary) -> DecodeSeeds {
    let mut seeds = DecodeSeeds::new();
    with_control_flow_facts_and_seeds(binary, Some(&mut seeds), |_| ());
    seeds
}

fn with_control_flow_facts_and_seeds<R>(
    binary: &LoadedBinary,
    seeds: Option<&mut DecodeSeeds>,
    project: impl FnOnce(AssembledControlFlowFacts<'_>) -> R,
) -> R {
    let hash = binary.hash.clone();

    let slot = {
        let mut cache = facts_cache()
            .lock()
            .expect("control-flow facts cache poisoned");
        if let Some(slot) = cache.get(&hash) {
            slot.clone()
        } else {
            let slot = std::sync::Arc::new(OnceLock::new());
            cache.put(hash, slot.clone());
            slot
        }
    };

    let cached = slot.get_or_init(|| {
        let storage = binary.storage_token();
        let frontend = binary
            .load_spec()
            .and_then(|load_spec| RuntimeSleighFrontend::new_for_load_spec(load_spec).ok());
        CachedControlFlowFacts {
            facts: ControlFlowFacts::assemble_with_seeds(binary, frontend.as_ref(), seeds),
            storage,
        }
    });
    project(AssembledControlFlowFacts {
        facts: &cached.facts,
        storage: &cached.storage,
    })
}

impl ControlFlowFacts {
    pub fn assemble(binary: &LoadedBinary, frontend: Option<&RuntimeSleighFrontend>) -> Self {
        Self::assemble_with_seeds(binary, frontend, None)
    }

    fn assemble_with_seeds(
        binary: &LoadedBinary,
        frontend: Option<&RuntimeSleighFrontend>,
        seeds: Option<&mut DecodeSeeds>,
    ) -> Self {
        let lsda_landing_pads = collect_lsda_landing_pads(binary);
        let mut label_leaders = collect_label_leaders(binary);
        label_leaders.extend(lsda_landing_pads.iter().copied());
        let function_extents = collect_function_extents(binary);
        let indirect_targets = collect_indirect_targets(binary);

        let mut flow_edges = collect_lsda_edges(binary);
        let mut noreturn_callsites = BTreeSet::new();

        if let Some(frontend) = frontend {
            let xref_db = XrefDatabase::build_with_frontend(binary, frontend);
            let ghidra_format = binary_format_to_ghidra_format(&binary.format);
            let compiler_key = ghidra_no_return_compiler_key(binary);
            let no_return_idx = ghidra_no_return_index();
            // Transitively-computed no-return functions (a local wrapper
            // around a known no-return like `abort`, propagated to fixpoint)
            // -- see `analysis::noreturn`. Checked alongside the by-name
            // lookup below, not instead of it: that lookup already covers
            // every case this set also covers (it seeds from the same
            // index), but keeping both is a smaller, more obviously
            // behavior-preserving change than replacing the existing check.
            let computed_no_return = no_return_functions_with_seeds(binary, seeds);

            for xref in xref_db.iter() {
                match xref.xref_type {
                    XrefType::Jump => {
                        flow_edges.push((xref.from_addr, xref.to_addr));
                    }
                    XrefType::Call => {
                        let by_name = ghidra_format.is_some_and(|fmt| {
                            resolve_call_target_name(binary, xref.to_addr).is_some_and(
                                |(symbol, library)| {
                                    no_return_idx.is_no_return(fmt, compiler_key, library, &symbol)
                                },
                            )
                        });
                        if by_name || computed_no_return.contains(&xref.to_addr) {
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
            lsda_landing_pads: lsda_landing_pads.into_iter().collect(),
        }
    }

    pub fn decode_context_for(
        &self,
        binary: &LoadedBinary,
        entry_address: u64,
        max_bytes: usize,
    ) -> DecodeMemoryContext {
        let limit_addr = function_limit_addr(binary, entry_address, max_bytes);
        FlowFactSlices::from_facts(self).decode_context_for(
            binary,
            entry_address,
            limit_addr,
            collect_relocation_targets_in_range(binary, entry_address, limit_addr),
        )
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

/// A view obtained only from the immutable, fully assembled cache slot.
/// All address vectors (and edge sources) are sorted; indirect targets already
/// include every successfully read relocation target in the binary.
struct AssembledControlFlowFacts<'a> {
    facts: &'a ControlFlowFacts,
    storage: &'a fission_loader::loader::LoadedBinaryStorageToken,
}

impl AssembledControlFlowFacts<'_> {
    fn decode_context_for(
        &self,
        binary: &LoadedBinary,
        entry_address: u64,
        max_bytes: usize,
    ) -> DecodeMemoryContext {
        let limit_addr = function_limit_addr(binary, entry_address, max_bytes);
        let facts = self.facts;
        let slices = FlowFactSlices {
            label_leaders: sorted_range(&facts.label_leaders, entry_address, limit_addr, |v| *v),
            flow_edges: sorted_range(&facts.flow_edges, entry_address, limit_addr, |v| v.0),
            indirect_targets: sorted_range(
                &facts.indirect_targets,
                entry_address,
                limit_addr,
                |v| *v,
            ),
            noreturn_callsites: sorted_range(
                &facts.noreturn_callsites,
                entry_address,
                limit_addr,
                |v| *v,
            ),
            lsda_landing_pads: sorted_range(
                &facts.lsda_landing_pads,
                entry_address,
                limit_addr,
                |v| *v,
            ),
        };
        // The general query's use-site-bounded relocation reads form a subset
        // of the assembled indirect targets. The final union is sorted and
        // deduplicated, so reading/sorting all use sites again adds no target.
        let relocation_targets = if self.storage.matches(binary) {
            Vec::new()
        } else {
            // The public patch APIs retain the content hash. A stale cache
            // entry must keep the old query's reads from the current bytes.
            collect_relocation_targets_in_range(binary, entry_address, limit_addr)
        };
        slices.decode_context_for(binary, entry_address, limit_addr, relocation_targets)
    }
}

fn sorted_range<T>(items: &[T], start: u64, end: u64, key: impl Fn(&T) -> u64) -> &[T] {
    let first = items.partition_point(|item| key(item) < start);
    let last = first + items[first..].partition_point(|item| key(item) < end);
    &items[first..last]
}

struct FlowFactSlices<'a> {
    label_leaders: &'a [u64],
    flow_edges: &'a [(u64, u64)],
    indirect_targets: &'a [u64],
    noreturn_callsites: &'a [u64],
    lsda_landing_pads: &'a [u64],
}

impl<'a> FlowFactSlices<'a> {
    fn from_facts(facts: &'a ControlFlowFacts) -> Self {
        Self {
            label_leaders: &facts.label_leaders,
            flow_edges: &facts.flow_edges,
            indirect_targets: &facts.indirect_targets,
            noreturn_callsites: &facts.noreturn_callsites,
            lsda_landing_pads: &facts.lsda_landing_pads,
        }
    }

    fn decode_context_for(
        &self,
        binary: &LoadedBinary,
        entry_address: u64,
        limit_addr: u64,
        relocation_targets: Vec<u64>,
    ) -> DecodeMemoryContext {
        let mut block_entry_hints: Vec<u64> = self
            .label_leaders
            .iter()
            .copied()
            .filter(|addr| (entry_address..limit_addr).contains(addr))
            .collect();
        if block_entry_hints.is_empty() {
            block_entry_hints = binary.cfg_block_entry_hints_in_range(entry_address, limit_addr);
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
        jump_table_targets.extend(relocation_targets);
        jump_table_targets.sort_unstable();
        jump_table_targets.dedup();

        let additional_decode_entries: Vec<u64> = self
            .lsda_landing_pads
            .iter()
            .copied()
            .filter(|addr| (entry_address..limit_addr).contains(addr))
            .collect();

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
            additional_decode_entries,
            readonly_windows: readonly_windows_for(binary),
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

/// LSDA landing pads (see `fission_loader::loader::gcc_lsda`) are only
/// reachable via the C++ personality routine unwinding into them when a
/// `throw` propagates through a covered call site -- no ordinary jump/call
/// instruction targets them, so without this they're invisible to both the
/// decoder (never disassembled as a block leader) and structuring (treated
/// as unreachable dead code and dropped). Confirmed on a real `try`/`catch`
/// fixture: the `catch` body was silently missing from decompiled output
/// entirely before this, `guarded()` in `x64_dyn_lsda_test.elf` showing only
/// the non-throwing path.
fn collect_lsda_landing_pads(binary: &LoadedBinary) -> BTreeSet<u64> {
    binary
        .eh_lsda
        .values()
        .flat_map(|info| info.call_sites.iter())
        .filter_map(|call_site| call_site.landing_pad)
        .filter(|addr| is_executable_address(binary, *addr))
        .collect()
}

/// One edge per LSDA call site with a landing pad, from the start of the
/// covered instruction range to the landing pad -- not tied to the exact
/// throwing instruction (the LSDA doesn't identify one specifically within
/// the range), just enough for reachability propagation to keep the landing
/// pad's block alive instead of pruning it as dead code.
fn collect_lsda_edges(binary: &LoadedBinary) -> Vec<(u64, u64)> {
    binary
        .eh_lsda
        .values()
        .flat_map(|info| info.call_sites.iter())
        .filter_map(|call_site| call_site.landing_pad.map(|lp| (call_site.start, lp)))
        .collect()
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

static READONLY_WINDOWS_CACHE: OnceLock<
    Mutex<LruCache<String, std::sync::Arc<Vec<(u64, std::sync::Arc<[u8]>)>>>>,
> = OnceLock::new();

/// Non-executable readable sections, for the jump-table reader.
///
/// A switch table is data, not code, so on x86-64 GCC `-O0` it lives in
/// `.rodata` while the `jmp` that reads it lives in `.text`. The reader used
/// to look only inside the function being decoded and gave up when the table
/// was not there, so every case of every such `switch` stayed undecoded.
///
/// Executable sections are excluded: those bytes already reach the reader as
/// the function's own window, and offering them twice would let a run of
/// instructions be read as a table. Cached and `Arc`-shared because a decode
/// context is built per function and these are whole sections.
fn readonly_windows_for(binary: &LoadedBinary) -> Vec<(u64, std::sync::Arc<[u8]>)> {
    let cache = READONLY_WINDOWS_CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(FACTS_CACHE_CAPACITY).expect("non-zero cache capacity"),
        ))
    });
    if let Some(hit) = cache
        .lock()
        .expect("readonly-window cache poisoned")
        .get(&binary.hash)
        .cloned()
    {
        return (*hit).clone();
    }
    let mut windows: Vec<(u64, std::sync::Arc<[u8]>)> = Vec::new();
    for section in &binary.sections {
        // Not executable, and not writable: a jump table is immutable data.
        // Allowing every readable section let `.got`, `.data` and `.bss` be
        // read as tables, and a page of function pointers passes "two entries
        // that land inside this function" easily enough to send the decoder
        // into the middle of an instruction.
        if section.is_executable || section.is_writable || !section.is_readable {
            continue;
        }
        let size = section
            .file_size
            .min(section.virtual_size.max(section.file_size));
        let Ok(size) = usize::try_from(size) else {
            continue;
        };
        if size == 0 || section.virtual_address == 0 {
            continue;
        }
        if let Some(bytes) = binary.view_bytes(section.virtual_address, size) {
            windows.push((section.virtual_address, std::sync::Arc::from(bytes)));
        }
    }
    let shared = std::sync::Arc::new(windows);
    cache
        .lock()
        .expect("readonly-window cache poisoned")
        .put(binary.hash.clone(), shared.clone());
    (*shared).clone()
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
    let little_endian = binary.is_little_endian();
    let mut targets = BTreeSet::new();

    for use_site in relocation_use_sites(binary) {
        if let Some(val) = read_reloc_target_at_use_site(binary, use_site, little_endian) {
            targets.insert(val);
        }
    }

    targets
}

fn collect_relocation_targets_in_range(binary: &LoadedBinary, start: u64, end: u64) -> Vec<u64> {
    let little_endian = binary.is_little_endian();
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
        if entry_address >= start && entry_address < end && !relative_address_bases.contains(&start)
        {
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

pub(crate) fn ghidra_no_return_compiler_key(binary: &LoadedBinary) -> Option<&'static str> {
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

fn resolve_call_target_name(
    binary: &LoadedBinary,
    target_addr: u64,
) -> Option<(String, Option<&str>)> {
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
            .or_else(|| {
                binary
                    .function_at_exact(target_addr)
                    .and_then(|f| f.external_library.as_deref())
            });
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

    fn seed_fixture(hash: &str) -> LoadedBinary {
        let mut binary =
            LoadedBinaryBuilder::new("seed-test.bin".into(), DataBuffer::Heap(vec![0xc3; 32]))
                .format("ELF")
                .arch_spec("x86:LE:64:default")
                .is_64bit(true)
                .image_base(0x400000)
                .add_section(fission_loader::loader::SectionInfo {
                    name: ".text".into(),
                    virtual_address: 0x401000,
                    virtual_size: 32,
                    file_offset: 0,
                    file_size: 32,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_function(FunctionInfo {
                    name: "seed".into(),
                    address: 0x401000,
                    size: 1,
                    ..Default::default()
                })
                .build()
                .unwrap();
        binary.hash = hash.into();
        binary
    }

    #[test]
    fn transient_decode_seeds_are_cold_transaction_owned_and_warm_preparation_is_empty() {
        let binary = seed_fixture("transient-cold-cf-seed-fixture");
        let seeds = prepare_fid_decode_seeds(&binary);
        assert!(seeds.contains_key(&0x401000));
        let facts = control_flow_facts_for(&binary);
        assert!(prepare_fid_decode_seeds(&binary).is_empty());
        drop(seeds);
        assert_eq!(control_flow_facts_for(&binary), facts);
    }

    #[test]
    fn transient_decode_seeds_do_not_recompute_warm_no_return_or_ordinary_cf() {
        let binary = seed_fixture("transient-warm-noret-seed-fixture");
        let known = crate::analysis::noreturn::no_return_functions_for(&binary);
        assert!(prepare_fid_decode_seeds(&binary).is_empty());
        assert_eq!(
            crate::analysis::noreturn::no_return_functions_for(&binary),
            known
        );
        let other = seed_fixture("transient-ordinary-cf-seed-fixture");
        let _ = control_flow_facts_for(&other);
        assert!(prepare_fid_decode_seeds(&other).is_empty());
    }

    #[test]
    #[ignore = "local real-input diagnostic; not a timing control or CI fixture"]
    fn transient_decode_seed_local_window_probe() {
        let input = std::env::var("FISSION_SEED_PROBE_INPUT").expect("explicit local input");
        let binary = fission_loader::loader::LoadedBinary::from_file(input).unwrap();
        let seeds = prepare_fid_decode_seeds(&binary);
        let frontend =
            RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().unwrap()).unwrap();
        let mut exact_windows = 0;
        for func in binary.functions.iter().filter(|f| !f.is_import) {
            let address = frontend
                .normalize_low_bit_code_address(func.address)
                .address;
            let old_limit =
                super::super::noreturn::no_return_scan_max_bytes(&binary, address, 4096);
            let new_limit = function_max_bytes(&binary, address, 4096);
            if binary.view_bytes(address, old_limit) == binary.view_bytes(address, new_limit) {
                exact_windows += 1;
            }
        }
        eprintln!(
            "captured_functions={} exact_windows={} candidate_functions={}",
            seeds.len(),
            exact_windows,
            binary.functions.iter().filter(|f| !f.is_import).count()
        );
        for seed in seeds.values() {
            eprintln!("{seed:?}");
        }
    }

    #[test]
    fn cached_projections_share_facts_and_preserve_owned_api_and_decode_context() {
        let binary =
            LoadedBinaryBuilder::new("projection.bin".into(), DataBuffer::Heap(vec![7; 128]))
                .format("PE")
                .is_64bit(true)
                .image_base(0x1000)
                .add_function(FunctionInfo {
                    name: "first".into(),
                    address: 0x1000,
                    size: 0,
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "next".into(),
                    address: 0x1040,
                    size: 16,
                    ..Default::default()
                })
                .build()
                .expect("projection fixture");
        let expected = control_flow_facts_for(&binary);
        let first = with_control_flow_facts(&binary, |view| view.facts as *const _ as usize);
        let second = with_control_flow_facts(&binary, |view| view.facts as *const _ as usize);
        assert_eq!(first, second);
        let mut owned = control_flow_facts_for(&binary);
        owned.label_leaders.clear();
        owned.function_extents.clear();
        owned.flow_edges.push((1, 2));
        assert_eq!(control_flow_facts_for(&binary), expected);
        for address in [0x1000, 0x1008, 0x1040] {
            assert_eq!(
                decode_memory_context_for(&binary, address, 64),
                expected.decode_context_for(&binary, address, 64)
            );
        }
        assert_eq!(function_max_bytes(&binary, 0x1000, 8), 64);
        assert!(with_control_flow_facts(&binary, |facts| {
            control_flow_facts_for(&binary) == *facts.facts
        }));
    }

    #[test]
    fn sorted_ranges_preserve_all_filter_results_and_duplicate_order() {
        let values: Vec<_> = (0..24u64).map(|i| i / 3).collect();
        for start in 0..10 {
            for end in 0..10 {
                let expected: Vec<_> = values
                    .iter()
                    .copied()
                    .filter(|v| *v >= start && *v < end)
                    .collect();
                assert_eq!(sorted_range(&values, start, end, |v| *v), expected);
            }
        }
        assert!(sorted_range::<u64>(&[], 0, u64::MAX, |v| *v).is_empty());
    }

    #[test]
    fn assembled_range_queries_match_general_queries_including_relocations() {
        use fission_loader::loader::{RelocationEntry, SectionInfo};

        for big_endian in [false, true] {
            let mut data = vec![0u8; 256];
            for offset in [8, 16] {
                let value = if big_endian {
                    0x1020u64.to_be_bytes()
                } else {
                    0x1020u64.to_le_bytes()
                };
                data[offset..offset + 8].copy_from_slice(&value);
            }
            // Preserve the existing helper's abstention on a truncated
            // eight-byte view at the end of the mapped bytes.
            let narrow = if big_endian {
                0x1030u32.to_be_bytes()
            } else {
                0x1030u32.to_le_bytes()
            };
            data[252..].copy_from_slice(&narrow);
            let binary = LoadedBinaryBuilder::new("ranges.bin".into(), DataBuffer::Heap(data))
                .format("PE")
                .arch_spec(if big_endian {
                    "test:BE:64"
                } else {
                    "test:LE:64"
                })
                .is_64bit(true)
                .image_base(0x1000)
                .add_section(SectionInfo {
                    name: ".text".into(),
                    virtual_address: 0x1000,
                    virtual_size: 256,
                    file_offset: 0,
                    file_size: 256,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_relocations(
                    [8, 16, 252]
                        .into_iter()
                        .map(|offset| RelocationEntry {
                            address: 0x1000 + offset,
                            r_type: 10,
                            size: if offset == 252 { 4 } else { 8 },
                            addend: 0,
                            symbol_name: None,
                        })
                        .collect::<Vec<_>>(),
                )
                .build()
                .expect("range fixture");
            let mut facts = ControlFlowFacts::assemble(&binary, None);
            assert_eq!(facts.indirect_targets, vec![0x1020]);
            facts.label_leaders = vec![0x1000, 0x1010, 0x1010, 0x1040, 0x2000];
            facts.flow_edges = vec![
                (0, 0x1020),
                (0x1000, 0x1010),
                (0x1010, 0x1020),
                (0x1010, 0x1020),
                (0x1020, 0x2000),
                (0x2000, 0x1010),
            ];
            facts.noreturn_callsites = vec![0x1008, 0x1040, 0x1040, 0x2000];
            facts.lsda_landing_pads = vec![0x1020, 0x1040, 0x2000];
            let storage = binary.storage_token();
            let assembled = AssembledControlFlowFacts {
                facts: &facts,
                storage: &storage,
            };
            for entry in [
                0,
                0xfff,
                0x1000,
                0x1008,
                0x1020,
                0x10fc,
                0x1100,
                0x2000,
                u64::MAX,
            ] {
                for size in [0, 1, 7, 31, 64, 256, 512, usize::MAX] {
                    assert_eq!(
                        assembled.decode_context_for(&binary, entry, size),
                        facts.decode_context_for(&binary, entry, size),
                        "big_endian={big_endian} entry={entry:x} size={size}"
                    );
                }
            }
            // An ordinary public record need not be sorted or complete.
            // Its general query must still discover the relocation targets.
            facts.label_leaders.reverse();
            facts.noreturn_callsites.reverse();
            facts.indirect_targets.clear();
            let general = facts.decode_context_for(&binary, 0x1000, 256);
            assert_eq!(
                general.block_entry_hints,
                vec![0x1040, 0x1010, 0x1010, 0x1000]
            );
            assert_eq!(general.noreturn_callsites, vec![0x1040, 0x1040, 0x1008]);
            assert_eq!(general.jump_table_targets, vec![0x1020]);
        }
    }

    #[test]
    fn assembled_queries_rescan_relocations_after_storage_mutation() {
        use fission_loader::loader::{RelocationEntry, SectionInfo};

        let mut data = vec![0u8; 128];
        data[8..16].copy_from_slice(&0x1020u64.to_le_bytes());
        let mut binary =
            LoadedBinaryBuilder::new("patched-range.bin".into(), DataBuffer::Heap(data))
                .format("PE")
                .is_64bit(true)
                .image_base(0x1000)
                .add_section(SectionInfo {
                    name: ".text".into(),
                    virtual_address: 0x1000,
                    virtual_size: 128,
                    file_offset: 0,
                    file_size: 128,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_relocations(vec![RelocationEntry {
                    address: 0x1008,
                    r_type: 10,
                    size: 8,
                    addend: 0,
                    symbol_name: None,
                }])
                .build()
                .expect("patched range fixture");
        let facts = ControlFlowFacts::assemble(&binary, None);
        let storage = binary.storage_token();
        let assembled = AssembledControlFlowFacts {
            facts: &facts,
            storage: &storage,
        };
        assert_eq!(facts.indirect_targets, vec![0x1020]);
        let hash = binary.hash.clone();
        assert!(
            binary
                .patch_bytes_va(0x1008, &0x1040u64.to_le_bytes())
                .is_some()
        );
        assert_eq!(binary.hash, hash);
        assert!(!storage.matches(&binary));
        let expected = facts.decode_context_for(&binary, 0x1000, 128);
        assert_eq!(expected.jump_table_targets, vec![0x1020, 0x1040]);
        assert_eq!(assembled.decode_context_for(&binary, 0x1000, 128), expected);
    }

    #[test]
    fn label_leaders_include_function_entries_and_loader_labels() {
        let binary =
            LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![0; 0x1000]))
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

    /// An LSDA landing pad has no ordinary jump/call predecessor -- see
    /// `fission_loader::loader::gcc_lsda` -- so `ControlFlowFacts::assemble`
    /// must surface it through both `label_leaders` (so the decoder actually
    /// disassembles it) and `flow_edges` (so reachability propagation keeps
    /// it, rather than pruning it as dead code), and `decode_context_for`
    /// must further expose it via `additional_decode_entries` -- the field
    /// that actually seeds the decode worklist (see `fission-sleigh`'s
    /// `lift_raw_pcode_function_with_context_and_memory_context`).
    #[test]
    fn assemble_surfaces_lsda_landing_pads_as_leaders_and_edges() {
        let mut binary =
            LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![0; 0x1000]))
                .format("ELF64")
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
                .add_function(FunctionInfo {
                    name: "guarded".to_string(),
                    address: 0x1000,
                    size: 0x100,
                    ..Default::default()
                })
                .build()
                .expect("build");
        binary.eh_lsda.insert(
            0x1000,
            fission_loader::loader::gcc_lsda::LsdaInfo {
                lp_start: 0x1000,
                call_sites: vec![fission_loader::loader::gcc_lsda::LsdaCallSite {
                    start: 0x1020,
                    end: 0x1030,
                    landing_pad: Some(0x1050),
                    action_chain: vec![1],
                }],
                type_table: vec![],
            },
        );

        let facts = ControlFlowFacts::assemble(&binary, None);
        assert!(facts.label_leaders.contains(&0x1050));
        assert!(facts.flow_edges.contains(&(0x1020, 0x1050)));
        assert_eq!(facts.lsda_landing_pads, vec![0x1050]);

        let ctx = facts.decode_context_for(&binary, 0x1000, 0x100);
        assert_eq!(ctx.additional_decode_entries, vec![0x1050]);
        assert!(ctx.block_entry_hints.contains(&0x1050));
    }

    #[test]
    fn decode_context_slices_labels_and_flow_edges() {
        let facts = ControlFlowFacts {
            label_leaders: vec![0x1000, 0x1010, 0x2000],
            flow_edges: vec![(0x1000, 0x1010), (0x1010, 0x3000)],
            indirect_targets: vec![0x1020],
            noreturn_callsites: vec![0x1008],
            function_extents: HashMap::from([(0x1000, 0x1100)]),
            lsda_landing_pads: Vec::new(),
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

    #[test]
    fn collect_indirect_targets_uses_canonical_big_endian_for_powerpc_alias() {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

        let mut data = vec![0u8; 0x3000];
        data[0x1200..0x1208].copy_from_slice(&0x1400_1500u64.to_be_bytes());

        let binary = LoadedBinaryBuilder::new("ppc.elf".to_string(), DataBuffer::Heap(data))
            .format("ELF")
            .architecture(fission_core::architecture::ArchitectureDescriptor {
                processor: "PowerPC".into(),
                endian: "big".into(),
                bitness: 64,
                variant: "default".into(),
                abi: None,
                raw_machine: "ppc64".into(),
            })
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
}
