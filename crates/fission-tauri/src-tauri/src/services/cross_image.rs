use fission_core::common::types::FunctionInfo;
use fission_loader::loader::LoadedBinary;
use iced_x86::{Decoder, DecoderOptions, FlowControl, OpKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const MAX_SIBLING_MODULES: usize = 24;
const MAX_SIBLING_BYTES: u64 = 200 * 1024 * 1024;
const MAX_WRAPPER_SIZE: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropagationReason {
    ImportExport,
    Thunk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropagatedRename {
    pub name: String,
    pub reason: PropagationReason,
}

#[derive(Debug, Clone)]
struct SiblingModule {
    module_name: String,
    exports: HashSet<String>,
    strong_names: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
struct RenameCandidates {
    accepted: Option<PropagatedRename>,
    conflict: bool,
}

pub fn collect_folder_propagated_renames(
    current: &LoadedBinary,
    folder: &Path,
) -> HashMap<u64, PropagatedRename> {
    let siblings = load_sibling_modules(current, folder);
    collect_propagated_renames_from_binaries(current, &siblings)
}

fn load_sibling_modules(current: &LoadedBinary, folder: &Path) -> Vec<SiblingModule> {
    let current_path = Path::new(&current.path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&current.path));
    let mut sibling_paths = collect_candidate_module_paths(folder);
    sibling_paths.retain(|path| path != &current_path);

    sibling_paths.sort();
    sibling_paths.truncate(MAX_SIBLING_MODULES);

    sibling_paths
        .into_iter()
        .filter_map(|path| LoadedBinary::from_file(&path).ok())
        .map(|binary| summarize_sibling_module(&binary))
        .collect()
}

fn collect_candidate_module_paths(folder: &Path) -> Vec<PathBuf> {
    let mut candidates = collect_modules_in_dir(folder);
    let plugins_dir = folder.join("plugins");
    if plugins_dir.is_dir() {
        candidates.extend(collect_modules_in_dir(&plugins_dir));
    }
    candidates
}

fn collect_modules_in_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "exe" | "dll"))
                .unwrap_or(false)
        })
        .filter(|path| {
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            !file_name.starts_with("qt")
        })
        .filter(|path| {
            std::fs::metadata(path)
                .map(|meta| meta.len() <= MAX_SIBLING_BYTES)
                .unwrap_or(false)
        })
        .filter_map(|path| path.canonicalize().ok().or(Some(path)))
        .collect()
}

fn summarize_sibling_module(binary: &LoadedBinary) -> SiblingModule {
    let module_name = Path::new(&binary.path)
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let exports = binary
        .exports()
        .map(|func| func.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();

    let strong_names = binary
        .functions_iter()
        .filter(|func| is_strong_name(&func.name))
        .map(|func| func.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();

    SiblingModule {
        module_name,
        exports,
        strong_names,
    }
}

fn collect_propagated_renames_from_binaries(
    current: &LoadedBinary,
    siblings: &[SiblingModule],
) -> HashMap<u64, PropagatedRename> {
    let mut candidates = HashMap::<u64, RenameCandidates>::new();

    for func in current.functions_iter() {
        if func.is_import || !is_weak_name(&func.name) {
            continue;
        }
        if let Some(candidate) = detect_wrapper_candidate(current, func, siblings) {
            insert_candidate(&mut candidates, func.address, candidate);
        }
    }

    candidates
        .into_iter()
        .filter_map(|(address, state)| (!state.conflict).then_some((address, state.accepted?)))
        .collect()
}

pub fn apply_propagated_renames(
    current: &LoadedBinary,
    renamed_functions: &mut HashMap<u64, String>,
    manual_renamed_functions: &HashSet<u64>,
    auto_renamed_functions: &mut HashMap<u64, PropagationReason>,
    propagated: HashMap<u64, PropagatedRename>,
) -> usize {
    let mut applied = 0usize;

    for (address, candidate) in propagated {
        if manual_renamed_functions.contains(&address) {
            continue;
        }

        let Some(current_func) = current.function_at_exact(address) else {
            continue;
        };

        let current_name = renamed_functions
            .get(&address)
            .map(String::as_str)
            .unwrap_or(&current_func.name);
        let current_auto = auto_renamed_functions.contains_key(&address);

        if !current_auto && !is_weak_name(current_name) {
            continue;
        }

        renamed_functions.insert(address, candidate.name.clone());
        auto_renamed_functions.insert(address, candidate.reason);
        applied += 1;
    }

    applied
}

fn detect_wrapper_candidate(
    current: &LoadedBinary,
    func: &FunctionInfo,
    siblings: &[SiblingModule],
) -> Option<PropagatedRename> {
    if func.size == 0 || func.size > MAX_WRAPPER_SIZE {
        return None;
    }

    let bytes = current.view_bytes(func.address, func.size as usize)?;
    let bitness = if current.is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, bytes, func.address, DecoderOptions::NONE);
    let instr = decoder.decode();
    if instr.is_invalid() {
        return None;
    }

    match instr.flow_control() {
        FlowControl::UnconditionalBranch | FlowControl::Call => {
            if matches!(
                instr.op0_kind(),
                OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
            ) {
                let target = instr.near_branch_target();
                return resolve_target_name(current, target, siblings);
            }

            if matches!(instr.op0_kind(), OpKind::Memory) {
                let target = instr.memory_displacement64();
                return resolve_import_symbol(current.iat_symbols.get(&target), siblings);
            }
        }
        FlowControl::IndirectBranch | FlowControl::IndirectCall => {
            if matches!(instr.op0_kind(), OpKind::Memory) {
                let target = instr.memory_displacement64();
                return resolve_import_symbol(current.iat_symbols.get(&target), siblings);
            }
        }
        _ => {}
    }

    None
}

fn resolve_target_name(
    current: &LoadedBinary,
    target: u64,
    siblings: &[SiblingModule],
) -> Option<PropagatedRename> {
    if let Some(import_symbol) = current.iat_symbols.get(&target) {
        return resolve_import_symbol(Some(import_symbol), siblings);
    }

    let target_func = current.function_at_exact(target)?;
    if target_func.is_import {
        return resolve_import_symbol(Some(&target_func.name), siblings);
    }

    if is_weak_name(&target_func.name) {
        return None;
    }

    Some(PropagatedRename {
        name: target_func.name.clone(),
        reason: PropagationReason::Thunk,
    })
}

fn resolve_import_symbol(
    import_symbol: Option<&String>,
    siblings: &[SiblingModule],
) -> Option<PropagatedRename> {
    let import_symbol = import_symbol?;
    let (module_name, symbol_name) = parse_import_symbol(import_symbol)?;
    let sibling = siblings
        .iter()
        .find(|sibling| sibling.module_name.eq_ignore_ascii_case(&module_name));

    if let Some(sibling) = sibling {
        if sibling.exports.contains(symbol_name) {
            return Some(PropagatedRename {
                name: symbol_name.to_string(),
                reason: PropagationReason::ImportExport,
            });
        }

        if sibling.strong_names.contains(symbol_name) {
            return Some(PropagatedRename {
                name: symbol_name.to_string(),
                reason: PropagationReason::Thunk,
            });
        }
    }

    if symbol_name.starts_with("Ordinal_") || symbol_name.starts_with("func_") {
        return None;
    }

    Some(PropagatedRename {
        name: symbol_name.to_string(),
        reason: PropagationReason::Thunk,
    })
}

fn insert_candidate(
    candidates: &mut HashMap<u64, RenameCandidates>,
    address: u64,
    candidate: PropagatedRename,
) {
    let entry = candidates.entry(address).or_default();
    match &entry.accepted {
        Some(existing) if existing.name != candidate.name => {
            entry.accepted = None;
            entry.conflict = true;
        }
        Some(_) => {}
        None if !entry.conflict => {
            entry.accepted = Some(candidate);
        }
        None => {}
    }
}

fn parse_import_symbol(import_symbol: &str) -> Option<(&str, &str)> {
    let (module, symbol) = import_symbol.split_once('!')?;
    let module = module.trim();
    let symbol = symbol.trim();
    if module.is_empty() || symbol.is_empty() {
        return None;
    }
    Some((module, symbol))
}

fn is_weak_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty()
        || trimmed.starts_with("sub_")
        || trimmed.starts_with("FUN_")
        || trimmed.starts_with("func_")
        || trimmed.starts_with("Ordinal_")
        || trimmed.starts_with("j_")
        || trimmed.starts_with("thunk_")
        || trimmed.starts_with("nullsub_")
        || trimmed.starts_with("loc_")
        || trimmed.starts_with("LAB_")
}

fn is_strong_name(name: &str) -> bool {
    !is_weak_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::common::types::SectionInfo;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use std::path::Path;

    fn synthetic_binary(
        path: &str,
        functions: Vec<FunctionInfo>,
        iat_symbols: HashMap<u64, String>,
    ) -> LoadedBinary {
        let mut bytes = vec![0u8; 0x3000];
        let jump_from = 0x1000u64;
        let jump_to = 0x2000u64;
        let rel = (jump_to as i64 - (jump_from as i64 + 5)) as i32;
        bytes[0] = 0xE9;
        bytes[1..5].copy_from_slice(&rel.to_le_bytes());

        LoadedBinaryBuilder::new(path.to_string(), DataBuffer::Heap(bytes))
            .format("PE")
            .is_64bit(true)
            .image_base(0)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x3000,
                file_offset: 0,
                file_size: 0x3000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_functions(functions)
            .add_iat_symbols(iat_symbols)
            .build()
            .unwrap()
    }

    #[test]
    fn import_wrapper_generates_candidate_when_sibling_exports_match() {
        let current = synthetic_binary(
            "/tmp/host.exe",
            vec![
                FunctionInfo {
                    name: "sub_1000".to_string(),
                    address: 0x1000,
                    size: 5,
                    is_export: false,
                    is_import: false,
                },
                FunctionInfo {
                    name: "a.dll!KnownFunc".to_string(),
                    address: 0x2000,
                    size: 0,
                    is_export: false,
                    is_import: true,
                },
            ],
            HashMap::from([(0x2000, "a.dll!KnownFunc".to_string())]),
        );
        let sibling = summarize_sibling_module(&synthetic_binary(
            "/tmp/a.dll",
            vec![FunctionInfo {
                name: "KnownFunc".to_string(),
                address: 0x5000,
                size: 16,
                is_export: true,
                is_import: false,
            }],
            HashMap::new(),
        ));

        let propagated = collect_propagated_renames_from_binaries(&current, &[sibling]);
        let Some(candidate) = propagated.get(&0x1000) else {
            panic!("expected propagated rename")
        };
        assert_eq!(candidate.name, "KnownFunc");
        assert_eq!(candidate.reason, PropagationReason::ImportExport);
    }

    #[test]
    fn manual_rename_is_not_overwritten() {
        let current = synthetic_binary(
            "/tmp/host.exe",
            vec![FunctionInfo {
                name: "sub_1000".to_string(),
                address: 0x1000,
                size: 5,
                is_export: false,
                is_import: false,
            }],
            HashMap::new(),
        );
        let mut renamed = HashMap::from([(0x1000, "ManualName".to_string())]);
        let manual = HashSet::from([0x1000]);
        let mut auto = HashMap::new();
        let propagated = HashMap::from([(
            0x1000,
            PropagatedRename {
                name: "KnownFunc".to_string(),
                reason: PropagationReason::Thunk,
            },
        )]);

        let applied =
            apply_propagated_renames(&current, &mut renamed, &manual, &mut auto, propagated);

        assert_eq!(applied, 0);
        assert_eq!(renamed.get(&0x1000).map(String::as_str), Some("ManualName"));
        assert!(auto.is_empty());
    }

    #[test]
    fn conflicting_candidates_are_dropped() {
        let mut candidates = HashMap::new();
        insert_candidate(
            &mut candidates,
            0x1000,
            PropagatedRename {
                name: "NameA".to_string(),
                reason: PropagationReason::Thunk,
            },
        );
        insert_candidate(
            &mut candidates,
            0x1000,
            PropagatedRename {
                name: "NameB".to_string(),
                reason: PropagationReason::ImportExport,
            },
        );

        let state = candidates.get(&0x1000).expect("candidate state");
        assert!(state.conflict);
        assert!(state.accepted.is_none());
    }

    #[test]
    fn ida76sp1_smoke_collects_some_renames_when_corpus_exists() {
        let sample = Path::new("/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.dll");
        if !sample.exists() {
            return;
        }

        let binary = LoadedBinary::from_file(sample).expect("load ida64.dll");
        let folder = sample.parent().expect("ida76sp1 folder");
        let propagated = collect_folder_propagated_renames(&binary, folder);

        assert!(
            !propagated.is_empty(),
            "expected some propagated names from ida76sp1 siblings"
        );
    }

    #[test]
    fn plugin_scope_candidates_include_plugins_subdir() {
        let sample = Path::new("/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.dll");
        if !sample.exists() {
            return;
        }

        let folder = sample.parent().expect("ida76sp1 folder");
        let candidates = collect_candidate_module_paths(folder);
        assert!(
            candidates.iter().any(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().eq_ignore_ascii_case("hexrays.dll"))
                    .unwrap_or(false)
            }),
            "expected plugins/hexrays.dll to be included in propagation scope"
        );
    }

    #[test]
    fn weak_name_heuristics_cover_wrappers_and_ordinals() {
        for name in [
            "sub_401000",
            "FUN_140001000",
            "func_1234",
            "Ordinal_12",
            "j_KnownFunc",
            "thunk_Reset",
            "nullsub_1",
            "loc_401020",
            "LAB_42",
        ] {
            assert!(is_weak_name(name), "{name} should be weak");
        }
        assert!(is_strong_name("KnownFunc"));
        assert!(is_strong_name("?Reset@Widget@@QEAAXXZ"));
    }
}
