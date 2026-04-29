use super::{FunctionInfo, LoadedBinary};

fn is_generic_function_name(name: &str) -> bool {
    name.starts_with("FUN_") || name.starts_with("sub_")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalViewCounts {
    pub functions: usize,
    pub imports: usize,
    pub exports: usize,
}

pub fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    if is_generic_function_name(candidate) != is_generic_function_name(current) {
        return !is_generic_function_name(candidate);
    }
    candidate.len() > current.len()
}

fn function_provenance_rank(func: &FunctionInfo) -> u8 {
    if func.is_import || func.is_thunk_like {
        return 0;
    }
    match func.kind.as_deref() {
        Some("undefined_external" | "import" | "import_thunk") => 0,
        Some("debug_symbol") => 2,
        Some("entry" | "code") => 4,
        _ if func.is_export => 5,
        _ if is_generic_function_name(&func.name) => 1,
        _ => 3,
    }
}

fn dedupe_exact_functions<'a>(functions: Vec<&'a FunctionInfo>) -> Vec<&'a FunctionInfo> {
    let mut deduped: Vec<&'a FunctionInfo> = Vec::with_capacity(functions.len());
    for func in functions {
        match deduped.last_mut() {
            Some(current) if current.address == func.address => {
                let candidate_rank = function_provenance_rank(func);
                let current_rank = function_provenance_rank(current);
                if candidate_rank > current_rank
                    || (candidate_rank == current_rank
                        && prefer_function_name(&func.name, &current.name))
                {
                    *current = func;
                }
            }
            _ => deduped.push(func),
        }
    }
    deduped
}

fn should_filter_internal_candidate(func: &FunctionInfo, covering_end: u64) -> bool {
    if func.is_import
        || func.is_thunk_like
        || matches!(
            func.kind.as_deref(),
            Some("undefined_external" | "import" | "import_thunk" | "debug_symbol")
        )
    {
        return true;
    }
    if func.is_export || func.address >= covering_end {
        return false;
    }
    if !is_generic_function_name(&func.name) {
        return false;
    }
    let extent_end = func.address.saturating_add(func.size.max(1));
    extent_end <= covering_end
}

pub fn is_runtime_wrapper_zero_size(func: &FunctionInfo) -> bool {
    func.size == 0 && func.name == "register_frame_ctor"
}

/// Return the canonical user-facing function list shared by CLI and GUI.
///
/// This excludes true imports, undefined externals, import thunks, debug-only
/// symbols, and covered generic internal labels. It does not hide zero-size
/// runtime wrappers; batch decompile seed selection applies that final policy.
pub fn canonical_functions_sorted(binary: &LoadedBinary) -> Vec<&FunctionInfo> {
    let deduped = dedupe_exact_functions(binary.functions_sorted());
    let mut filtered = Vec::with_capacity(deduped.len());
    let mut covering_end = 0u64;

    for func in deduped {
        if should_filter_internal_candidate(func, covering_end) {
            continue;
        }
        if !func.is_import && func.size > 0 {
            covering_end = covering_end.max(func.address.saturating_add(func.size));
        }
        filtered.push(func);
    }

    filtered
}

pub fn canonical_imports_sorted(binary: &LoadedBinary) -> Vec<&FunctionInfo> {
    binary.imports().collect()
}

pub fn canonical_exports_sorted(binary: &LoadedBinary) -> Vec<&FunctionInfo> {
    binary.exports().collect()
}

pub fn canonical_view_counts(binary: &LoadedBinary) -> CanonicalViewCounts {
    CanonicalViewCounts {
        functions: canonical_functions_sorted(binary).len(),
        imports: canonical_imports_sorted(binary).len(),
        exports: canonical_exports_sorted(binary).len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    fn func(
        name: &str,
        address: u64,
        size: u64,
        kind: Option<&str>,
        origin: Option<&str>,
    ) -> FunctionInfo {
        FunctionInfo {
            name: name.to_string(),
            address,
            size,
            is_export: false,
            is_import: false,
            origin: origin.map(str::to_string),
            kind: kind.map(str::to_string),
            source_section: Some(".text".to_string()),
            external_library: None,
            is_thunk_like: false,
        }
    }

    fn test_binary(functions: Vec<FunctionInfo>) -> LoadedBinary {
        LoadedBinaryBuilder::new("view.bin".to_string(), DataBuffer::Heap(vec![0x90; 0x100]))
            .format("TEST")
            .entry_point(0x1000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x100,
                file_offset: 0,
                file_size: 0x100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_functions(functions)
            .build()
            .expect("test binary builds")
    }

    #[test]
    fn canonical_functions_exclude_import_external_thunk_and_debug_only_symbols() {
        let mut import = func(
            "KERNEL32!ExitProcess",
            0x5000,
            0,
            Some("import"),
            Some("pe-import-table"),
        );
        import.is_import = true;
        import.external_library = Some("KERNEL32.dll".to_string());

        let mut undefined_external = func(
            "puts",
            0x6000,
            0,
            Some("undefined_external"),
            Some("elf-dynsym"),
        );
        undefined_external.is_import = true;

        let mut thunk = func(
            "__stubs_printf",
            0x1010,
            6,
            Some("import_thunk"),
            Some("macho-indirect-symbols"),
        );
        thunk.is_thunk_like = true;

        let debug = func(
            "debug_only_label",
            0x1020,
            4,
            Some("debug_symbol"),
            Some("dwarf"),
        );
        let code = func(
            "real_function",
            0x1000,
            16,
            Some("code"),
            Some("elf-symtab"),
        );

        let binary = test_binary(vec![code, import, undefined_external, thunk, debug]);
        let functions = canonical_functions_sorted(&binary);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "real_function");
        assert_eq!(canonical_imports_sorted(&binary).len(), 2);
        assert_eq!(canonical_exports_sorted(&binary).len(), 0);
    }

    #[test]
    fn canonical_functions_dedupe_same_address_by_provenance() {
        let generic = func("sub_1000", 0x1000, 8, Some("code"), Some("synthetic"));
        let named = func("named_symbol", 0x1000, 8, Some("code"), Some("elf-symtab"));
        let mut exported = func(
            "exported_api",
            0x1000,
            8,
            Some("export"),
            Some("pe-export-table"),
        );
        exported.is_export = true;

        let binary = test_binary(vec![generic, named, exported]);
        let functions = canonical_functions_sorted(&binary);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "exported_api");
        assert!(functions[0].is_export);
    }

    #[test]
    fn canonical_view_counts_match_canonical_views() {
        let code = func(
            "real_function",
            0x1000,
            16,
            Some("code"),
            Some("elf-symtab"),
        );
        let mut import = func(
            "puts",
            0x6000,
            0,
            Some("undefined_external"),
            Some("elf-dynsym"),
        );
        import.is_import = true;
        let mut export = func("api", 0x1020, 8, Some("export"), Some("pe-export-table"));
        export.is_export = true;

        let binary = test_binary(vec![code, import, export]);
        let counts = canonical_view_counts(&binary);

        assert_eq!(counts.functions, canonical_functions_sorted(&binary).len());
        assert_eq!(counts.imports, canonical_imports_sorted(&binary).len());
        assert_eq!(counts.exports, canonical_exports_sorted(&binary).len());
        assert_eq!(
            counts,
            CanonicalViewCounts {
                functions: 2,
                imports: 1,
                exports: 1
            }
        );
    }
}
