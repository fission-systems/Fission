//! Parsed import / IAT / `__imp_` naming as a canonical external key for linking and APIs.

use fission_signatures::symbol_for_win_api_database_lookup;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use fission_loader::loader::LoadedBinary;

/// Canonical import / external stub identity (library + symbol or ordinal).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalSymbolIdentity {
    /// Normalized DLL name (`kernel32.dll`) or `None` for unknown image (e.g. plain `__imp_X`).
    pub library_key: Option<String>,
    /// Flat symbol matching Win API DB rows when not an ordinal import (`CloseHandle`).
    pub symbol_key: String,
    pub ordinal: Option<u32>,
}

impl ExternalSymbolIdentity {
    /// Stable map/merge key: `lib::sym` or `lib::#ordinal` or `*::sym` without library.
    #[must_use]
    pub fn canonical_key(&self) -> String {
        let lib = self.library_key.as_deref().unwrap_or("*");
        if let Some(o) = self.ordinal {
            format!("{lib}::#{o}")
        } else if self.symbol_key.is_empty() {
            format!("{lib}::")
        } else {
            format!("{lib}::{}", self.symbol_key)
        }
    }
}

/// Index of external symbols: VA → canonical key, plus one identity per canonical key.
#[derive(Debug, Clone, Default)]
pub struct ExternalSymbolIndex {
    canonical_to_identity: FxHashMap<String, ExternalSymbolIdentity>,
    va_to_canonical: FxHashMap<u64, String>,
}

impl ExternalSymbolIndex {
    #[must_use]
    pub fn identity_for_va(&self, va: u64) -> Option<&ExternalSymbolIdentity> {
        let ck = self.va_to_canonical.get(&va)?;
        self.canonical_to_identity.get(ck)
    }

    #[must_use]
    pub fn canonical_key_for_va(&self, va: u64) -> Option<&str> {
        self.va_to_canonical.get(&va).map(String::as_str)
    }

    #[must_use]
    pub fn len_identities(&self) -> usize {
        self.canonical_to_identity.len()
    }

    #[must_use]
    pub fn len_va_mappings(&self) -> usize {
        self.va_to_canonical.len()
    }
}

#[must_use]
pub fn normalize_library_key(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let seg = raw.rsplit(['/', '\\']).next()?.trim();
    if seg.is_empty() {
        return None;
    }
    Some(seg.to_ascii_lowercase())
}

fn parse_ordinal_in_symbol_fragment(sym: &str) -> Option<u32> {
    let sym = sym.trim();
    if let Some(rest) = sym.strip_prefix("Ordinal_") {
        return rest.parse().ok();
    }
    if let Some(pos) = sym.find(":Ordinal_") {
        let tail = sym[pos + ":Ordinal_".len()..].trim();
        return tail.parse().ok();
    }
    None
}

/// Best-effort parse from loader-produced names (`kernel32.dll!ExitProcess`, `__imp_foo`).
#[must_use]
pub fn parse_external_identity_from_loader_string(
    full: &str,
    external_library_hint: Option<&str>,
) -> Option<ExternalSymbolIdentity> {
    let full = full.trim();
    if full.is_empty() {
        return None;
    }

    let (library_key, sym_fragment) = if let Some((lib, sym)) = full.split_once('!') {
        (normalize_library_key(lib), sym.trim())
    } else {
        (
            external_library_hint.and_then(normalize_library_key),
            full,
        )
    };

    let sym_fragment = sym_fragment.trim();
    if sym_fragment.is_empty() {
        return None;
    }

    if let Some(ord) = parse_ordinal_in_symbol_fragment(sym_fragment) {
        return Some(ExternalSymbolIdentity {
            library_key,
            symbol_key: String::new(),
            ordinal: Some(ord),
        });
    }

    let flat = symbol_for_win_api_database_lookup(sym_fragment).unwrap_or(sym_fragment);
    let flat = flat.trim();
    if flat.is_empty() {
        return None;
    }

    Some(ExternalSymbolIdentity {
        library_key,
        symbol_key: flat.to_string(),
        ordinal: None,
    })
}

fn register_va(
    idx: &mut ExternalSymbolIndex,
    va: u64,
    identity: ExternalSymbolIdentity,
) {
    let ck = identity.canonical_key();
    idx.canonical_to_identity
        .entry(ck.clone())
        .or_insert(identity);
    idx.va_to_canonical.entry(va).or_insert(ck);
}

/// Build external symbol facts from IAT slots, import `FunctionInfo` rows, and `__imp_` globals.
#[must_use]
pub fn build_external_symbol_index(binary: &LoadedBinary) -> ExternalSymbolIndex {
    let mut idx = ExternalSymbolIndex::default();

    for (&va, name) in binary.iat_symbols.iter() {
        if let Some(id) = parse_external_identity_from_loader_string(name, None) {
            register_va(&mut idx, va, id);
        }
    }

    for f in &binary.functions {
        if !f.is_import {
            continue;
        }
        if let Some(id) =
            parse_external_identity_from_loader_string(&f.name, f.external_library.as_deref())
        {
            register_va(&mut idx, f.address, id);
        }
    }

    for (&va, name) in binary.global_symbols.iter() {
        let n = name.as_str();
        if !(n.starts_with("__imp__") || n.starts_with("__imp_")) {
            continue;
        }
        if let Some(id) = parse_external_identity_from_loader_string(n, None) {
            register_va(&mut idx, va, id);
        }
    }

    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn pe_style_iat_and_import_function_merge() {
        let bin = LoadedBinaryBuilder::new("t.exe".to_string(), DataBuffer::Heap(vec![0u8; 256]))
            .format("PE64")
            .entry_point(0x1000)
            .image_base(0x140000000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x140001000,
                virtual_size: 128,
                file_offset: 0,
                file_size: 128,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_iat_symbol(0x140002000, "KERNEL32.dll!ExitProcess".into())
            .add_function(FunctionInfo {
                name: "kernel32.dll!ExitProcess".into(),
                address: 0x140002000,
                size: 0,
                is_export: false,
                is_import: true,
                external_library: Some("KERNEL32.dll".into()),
                ..Default::default()
            })
            .build()
            .expect("build");

        let idx = build_external_symbol_index(&bin);
        assert_eq!(idx.len_va_mappings(), 1);
        assert_eq!(idx.len_identities(), 1);
        let id = idx.identity_for_va(0x140002000).expect("iat");
        assert_eq!(id.symbol_key, "ExitProcess");
        assert_eq!(
            id.library_key.as_deref(),
            Some("kernel32.dll")
        );
    }

    #[test]
    fn global_imp_symbol_without_dll() {
        let bin = LoadedBinaryBuilder::new("obj.o".to_string(), DataBuffer::Heap(vec![0u8; 64]))
            .format("COFF")
            .add_global_symbol(0x5000, "__imp_ReadFile".into())
            .build()
            .expect("build");

        let idx = build_external_symbol_index(&bin);
        let id = idx.identity_for_va(0x5000).expect("imp");
        assert_eq!(id.symbol_key, "ReadFile");
        assert!(id.library_key.is_none());
        assert_eq!(id.canonical_key(), "*::ReadFile");
    }
}
