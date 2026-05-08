//! [`XrefIndex`] construction and queries.

use fission_loader::loader::function_view::canonical_exports_sorted;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::DecodedFlowKind;
use rustc_hash::{FxHashMap, FxHashSet};

use super::model::{
    FunctionXrefsSummary, XrefEvidence, XrefId, XrefIndexSummary, XrefKind, XrefRecord, XrefSource,
    XrefSourceCategory, XrefSourceLayer, XrefTarget,
};
use crate::analysis::xrefs::{XrefDatabase, XrefType};

#[derive(Debug, Default)]
pub struct XrefIndexBuilder {
    pending: Vec<XrefRecordDraft>,
}

#[derive(Debug, Clone)]
struct XrefRecordDraft {
    source: XrefSource,
    target: XrefTarget,
    kind: XrefKind,
    confidence: fission_loader::Confidence,
    evidence: XrefEvidence,
}

/// Canonical merged cross-reference index (loader + disassembly + future layers).
#[derive(Debug, Clone)]
pub struct XrefIndex {
    pub refs: Vec<XrefRecord>,
    by_source: FxHashMap<u64, Vec<XrefId>>,
    by_target: FxHashMap<u64, Vec<XrefId>>,
}

impl XrefIndexBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_record(
        &mut self,
        source: XrefSource,
        target: XrefTarget,
        kind: XrefKind,
        confidence: fission_loader::Confidence,
        evidence: XrefEvidence,
    ) {
        self.pending.push(XrefRecordDraft {
            source,
            target,
            kind,
            confidence,
            evidence,
        });
    }

    pub fn finish(self) -> XrefIndex {
        let mut refs = Vec::with_capacity(self.pending.len());
        let mut by_source: FxHashMap<u64, Vec<XrefId>> = FxHashMap::default();
        let mut by_target: FxHashMap<u64, Vec<XrefId>> = FxHashMap::default();

        for (idx, draft) in self.pending.into_iter().enumerate() {
            let id = idx as XrefId;
            let rec = XrefRecord {
                id,
                source: draft.source.clone(),
                target: draft.target.clone(),
                kind: draft.kind,
                confidence: draft.confidence,
                evidence: draft.evidence,
            };

            by_source.entry(rec.source.address).or_default().push(id);
            if let Some(va) = rec.target.address {
                by_target.entry(va).or_default().push(id);
            }

            refs.push(rec);
        }

        XrefIndex {
            refs,
            by_source,
            by_target,
        }
    }
}

fn xref_kind_snake(kind: XrefKind) -> &'static str {
    match kind {
        XrefKind::Call => "call",
        XrefKind::Jump => "jump",
        XrefKind::ConditionalJump => "conditional_jump",
        XrefKind::DataRead => "data_read",
        XrefKind::DataWrite => "data_write",
        XrefKind::StringRef => "string_ref",
        XrefKind::ImportRef => "import_ref",
        XrefKind::ExportRef => "export_ref",
        XrefKind::GlobalSymbol => "global_symbol",
        XrefKind::Relocation => "relocation",
        XrefKind::Unknown => "unknown",
    }
}

fn xref_layer_snake(layer: XrefSourceLayer) -> &'static str {
    match layer {
        XrefSourceLayer::Loader => "loader",
        XrefSourceLayer::Relocation => "relocation",
        XrefSourceLayer::SymbolTable => "symbol_table",
        XrefSourceLayer::Disassembly => "disassembly",
        XrefSourceLayer::Pcode => "pcode",
        XrefSourceLayer::StaticAnalyzer => "static_analyzer",
        XrefSourceLayer::DebugInfo => "debug_info",
        XrefSourceLayer::Manual => "manual",
    }
}

impl XrefIndex {
    #[must_use]
    pub fn refs_to_address(&self, va: u64) -> Vec<&XrefRecord> {
        self.by_target
            .get(&va)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.refs.get(*id as usize))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn refs_from_address(&self, va: u64) -> Vec<&XrefRecord> {
        self.by_source
            .get(&va)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.refs.get(*id as usize))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn summary(&self) -> XrefIndexSummary {
        let mut s = XrefIndexSummary {
            total: self.refs.len(),
            relocation_note: None,
            ..Default::default()
        };

        for r in &self.refs {
            *s.by_kind
                .entry(xref_kind_snake(r.kind).to_string())
                .or_insert(0) += 1;
            *s.by_layer
                .entry(xref_layer_snake(r.evidence.layer).to_string())
                .or_insert(0) += 1;
            match r.kind {
                XrefKind::ImportRef => s.imports += 1,
                XrefKind::ExportRef => s.exports += 1,
                XrefKind::Relocation => s.relocations += 1,
                XrefKind::StringRef => s.strings += 1,
                XrefKind::GlobalSymbol => s.globals += 1,
                XrefKind::Call => {
                    s.calls += 1;
                }
                XrefKind::Jump | XrefKind::ConditionalJump => s.jumps += 1,
                XrefKind::DataRead | XrefKind::DataWrite => s.data += 1,
                XrefKind::Unknown => {}
            }
        }
        s
    }

    /// Bucket outgoing / incoming xref ids per function entry VA (`fallback_range` mirrors callgraph).
    #[must_use]
    pub fn function_summaries(
        &self,
        binary: &LoadedBinary,
        fallback_range: u64,
    ) -> FxHashMap<u64, FunctionXrefsSummary> {
        let mut funcs: Vec<FunctionInfo> = binary.functions.clone();
        funcs.sort_by_key(|f| f.address);

        let string_addrs: FxHashSet<u64> = binary.string_map.keys().copied().collect();
        let iat_addrs: FxHashSet<u64> = binary.iat_symbols.keys().copied().collect();

        let mut map: FxHashMap<u64, FunctionXrefsSummary> = FxHashMap::default();
        for f in &funcs {
            map.insert(
                f.address,
                FunctionXrefsSummary {
                    function_address: f.address,
                    ..Default::default()
                },
            );
        }

        for rec in &self.refs {
            let src_fn = resolve_enclosing_function(&funcs, rec.source.address, fallback_range);
            let tgt_va = rec.target.address;
            let tgt_fn =
                tgt_va.and_then(|va| resolve_enclosing_function(&funcs, va, fallback_range));

            match rec.kind {
                XrefKind::Call => {
                    if let Some(fa) = src_fn {
                        map.entry(fa).or_default().calls_out.push(rec.id);
                    }
                    if let Some(tfa) = tgt_fn {
                        map.entry(tfa).or_default().callers.push(rec.id);
                    }
                    if let Some(va) = tgt_va {
                        if iat_addrs.contains(&va) {
                            if let Some(fa) = src_fn {
                                let v = map.entry(fa).or_default();
                                if !v.imports_used.contains(&rec.id) {
                                    v.imports_used.push(rec.id);
                                }
                            }
                        }
                    }
                }
                XrefKind::Jump | XrefKind::ConditionalJump => {
                    if let Some(fa) = src_fn {
                        map.entry(fa).or_default().jumps_out.push(rec.id);
                    }
                }
                XrefKind::DataRead | XrefKind::DataWrite => {
                    if let Some(fa) = src_fn {
                        if let Some(va) = tgt_va {
                            if string_addrs.contains(&va) {
                                map.entry(fa).or_default().strings.push(rec.id);
                                continue;
                            }
                        }
                        map.entry(fa).or_default().globals_read.push(rec.id);
                    }
                }
                XrefKind::StringRef => {
                    if let Some(fa) = src_fn {
                        map.entry(fa).or_default().strings.push(rec.id);
                    }
                }
                XrefKind::ImportRef => {
                    if let Some(fa) = src_fn {
                        map.entry(fa).or_default().imports_used.push(rec.id);
                    }
                }
                _ => {}
            }
        }

        map
    }

    #[must_use]
    pub fn function_summary_for(
        &self,
        binary: &LoadedBinary,
        function_entry: u64,
        fallback_range: u64,
    ) -> Option<FunctionXrefsSummary> {
        self.function_summaries(binary, fallback_range)
            .remove(&function_entry)
    }
}

#[must_use]
pub fn resolve_enclosing_function(
    functions: &[FunctionInfo],
    addr: u64,
    fallback_range: u64,
) -> Option<u64> {
    if functions.is_empty() {
        return None;
    }
    let fallback_range = fallback_range.max(1);
    let idx = match functions.binary_search_by_key(&addr, |f| f.address) {
        Ok(i) => i,
        Err(i) => i.checked_sub(1)?,
    };
    let func = &functions[idx];
    let size = if func.size > 0 {
        func.size
    } else {
        fallback_range
    };
    let end = func.address.saturating_add(size);
    if addr >= func.address && addr < end {
        Some(func.address)
    } else {
        None
    }
}

#[must_use]
fn section_containing_va(binary: &LoadedBinary, va: u64) -> Option<String> {
    binary.sections.iter().find_map(|sec| {
        let end = sec.virtual_address.saturating_add(sec.virtual_size);
        if va >= sec.virtual_address && va < end {
            Some(sec.name.clone())
        } else {
            None
        }
    })
}

/// Loader-derived xref seeds (imports/exports/strings/globals).
pub fn push_loader_seeds(builder: &mut XrefIndexBuilder, binary: &LoadedBinary) {
    let sorted_funcs = {
        let mut v = binary.functions.clone();
        v.sort_by_key(|f| f.address);
        v
    };

    for (&slot_va, name) in binary.iat_symbols.iter() {
        builder.push_record(
            XrefSource {
                address: slot_va,
                category: XrefSourceCategory::Instruction {
                    enclosing_function: resolve_enclosing_function(&sorted_funcs, slot_va, 0x40),
                },
            },
            XrefTarget {
                address: Some(slot_va),
                symbol: Some(name.clone()),
            },
            XrefKind::ImportRef,
            fission_loader::Confidence::High,
            XrefEvidence {
                layer: XrefSourceLayer::Loader,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: Some(name.clone()),
                note: Some("IAT/import thunk slot".into()),
            },
        );
    }

    for export in canonical_exports_sorted(binary) {
        builder.push_record(
            XrefSource {
                address: export.address,
                category: XrefSourceCategory::LoaderMetadata {
                    label: "export_entry".into(),
                },
            },
            XrefTarget {
                address: Some(export.address),
                symbol: Some(export.name.clone()),
            },
            XrefKind::ExportRef,
            fission_loader::Confidence::High,
            XrefEvidence {
                layer: XrefSourceLayer::Loader,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: Some(export.name.clone()),
                note: Some("Exported symbol".into()),
            },
        );
    }

    for (&addr, content) in binary.string_map.iter() {
        let section = section_containing_va(binary, addr).unwrap_or_else(|| "unknown".into());
        let preview: String = content.chars().take(48).collect();
        builder.push_record(
            XrefSource {
                address: addr,
                category: XrefSourceCategory::LoaderMetadata {
                    label: "string_literal".into(),
                },
            },
            XrefTarget {
                address: Some(addr),
                symbol: None,
            },
            XrefKind::StringRef,
            fission_loader::Confidence::Medium,
            XrefEvidence {
                layer: XrefSourceLayer::Loader,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: None,
                note: Some(format!("section={section}; preview={preview:?}")),
            },
        );
    }

    for (&addr, name) in binary.global_symbols.iter() {
        let section = section_containing_va(binary, addr).unwrap_or_else(|| "unknown".into());
        builder.push_record(
            XrefSource {
                address: addr,
                category: XrefSourceCategory::Data { section },
            },
            XrefTarget {
                address: Some(addr),
                symbol: Some(name.clone()),
            },
            XrefKind::GlobalSymbol,
            fission_loader::Confidence::High,
            XrefEvidence {
                layer: XrefSourceLayer::SymbolTable,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: Some(name.clone()),
                note: Some("global_symbols map".into()),
            },
        );
    }

    for (&addr, name) in binary.inner().relocation_symbols.iter() {
        builder.push_record(
            XrefSource {
                address: addr,
                category: XrefSourceCategory::Instruction {
                    enclosing_function: resolve_enclosing_function(&sorted_funcs, addr, 0x40),
                },
            },
            XrefTarget {
                address: None,
                symbol: Some(name.clone()),
            },
            XrefKind::Relocation,
            fission_loader::Confidence::High,
            XrefEvidence {
                layer: XrefSourceLayer::Relocation,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: Some(name.clone()),
                note: Some("relocation symbol use-site".into()),
            },
        );
    }
}

/// Maps [`XrefDatabase`] rows into canonical records (`XrefSourceLayer::Disassembly`).
pub fn push_disassembly_layer(
    builder: &mut XrefIndexBuilder,
    binary: &LoadedBinary,
    db: &XrefDatabase,
) {
    let mut sorted: Vec<FunctionInfo> = binary.functions.clone();
    sorted.sort_by_key(|f| f.address);

    for x in db.iter() {
        let enclosing_fn = resolve_enclosing_function(&sorted, x.from_addr, 0x100);

        let (kind, note_suffix) = match x.xref_type {
            XrefType::Call => (XrefKind::Call, "call"),
            XrefType::Jump => {
                if matches!(x.flow_kind, Some(DecodedFlowKind::ConditionalJump)) {
                    (XrefKind::ConditionalJump, "conditional_jump")
                } else {
                    (XrefKind::Jump, "jump")
                }
            }
            XrefType::Data => (XrefKind::DataRead, "data"),
        };

        let mut note = format!("operand_index={}; {note_suffix}", x.operand_index);
        if let Some(sk) = x.sleigh_kind {
            note.push_str(&format!("; sleigh_kind={sk:?}"));
        }

        builder.push_record(
            XrefSource {
                address: x.from_addr,
                category: XrefSourceCategory::Instruction {
                    enclosing_function: enclosing_fn,
                },
            },
            XrefTarget {
                address: Some(x.to_addr),
                symbol: None,
            },
            kind,
            fission_loader::Confidence::Medium,
            XrefEvidence {
                layer: XrefSourceLayer::Disassembly,
                instruction_mnemonic: None,
                pcode_op: None,
                relocation_kind: None,
                symbol_name: None,
                note: Some(note),
            },
        );
    }
}

/// Build full index: loader seeds + optional disassembly (`include_disassembly` requires load_spec).
#[must_use]
pub fn build_xref_index(binary: &LoadedBinary, include_disassembly: bool) -> XrefIndex {
    let mut b = XrefIndexBuilder::new();
    push_loader_seeds(&mut b, binary);
    if include_disassembly && binary.load_spec().is_some() {
        let db = XrefDatabase::build_from_binary(binary);
        push_disassembly_layer(&mut b, binary, &db);
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::xref_index::model::XrefKind;
    use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn empty_builder_finishes() {
        let idx = XrefIndexBuilder::new().finish();
        assert_eq!(idx.refs.len(), 0);
        assert_eq!(idx.summary().total, 0);
    }

    #[test]
    fn loader_seeds_iat_and_export() {
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
            .add_iat_symbol(0x140002000, "KERNEL32.ExitProcess".into())
            .add_function(FunctionInfo {
                name: "exported_main".into(),
                address: 0x140001000,
                size: 32,
                is_export: true,
                is_import: false,
                ..Default::default()
            })
            .build()
            .expect("build");

        let idx = build_xref_index(&bin, false);
        let sum = idx.summary();
        assert!(sum.imports >= 1);
        assert!(sum.exports >= 1);
        assert!(idx.refs.iter().any(|r| {
            r.kind == XrefKind::ImportRef && r.confidence == fission_loader::Confidence::High
        }));
    }

    #[test]
    fn loader_seeds_relocation_symbols() {
        let mut relocation_symbols = std::collections::HashMap::new();
        relocation_symbols.insert(0x10003a8, "control_sink".to_string());

        let bin = LoadedBinaryBuilder::new("reloc.o".to_string(), DataBuffer::Heap(vec![0u8; 256]))
            .format("ELF64")
            .entry_point(0x100000)
            .image_base(0x100000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x100000,
                virtual_size: 0x400,
                file_offset: 0,
                file_size: 0x100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_function(FunctionInfo {
                name: "run_control_flow".into(),
                address: 0x100140,
                size: 0x300,
                is_export: true,
                is_import: false,
                ..Default::default()
            })
            .add_relocation_symbols(relocation_symbols)
            .build()
            .expect("build");

        let idx = build_xref_index(&bin, false);
        let sum = idx.summary();
        assert_eq!(sum.relocations, 1);
        assert_eq!(sum.by_kind.get("relocation"), Some(&1));
        assert!(idx.refs.iter().any(|r| {
            r.kind == XrefKind::Relocation
                && r.source.address == 0x10003a8
                && r.target.symbol.as_deref() == Some("control_sink")
                && r.evidence.layer == XrefSourceLayer::Relocation
        }));
    }
}
