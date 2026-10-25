//! Call graph analysis built from cross-references.
//!
//! Edges are aggregated from [`super::xrefs::XrefDatabase`] entries whose [`super::xrefs::XrefType`]
//! is [`super::xrefs::XrefType::Call`]. This includes conditional calls (`call` on conditional
//! flow) when Sleigh classifies them as call targets; jump-only tail edges are excluded.

use rustc_hash::FxHashMap;

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_signatures::CallGraphView;

use super::external_symbol::ExternalSymbolIndex;
use super::xref_index::{XrefIndex, XrefKind, resolve_enclosing_function};
use super::xrefs::{XrefDatabase, XrefType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallEdge {
    pub addr: u64,
    pub count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    callers: FxHashMap<u64, Vec<CallEdge>>,
    callees: FxHashMap<u64, Vec<CallEdge>>,
    total_call_sites: usize,
}

/// Name-aware view over the canonical static-analysis call graph.
///
/// The graph owns only address edges. Function names remain owned by the
/// loader/program metadata, so relation matching receives a short-lived view
/// instead of making the graph responsible for a second name table.
pub struct NamedCallGraphView<'a> {
    graph: &'a CallGraph,
    names: FxHashMap<u64, &'a str>,
    external_symbols: Option<&'a ExternalSymbolIndex>,
}

impl<'a> NamedCallGraphView<'a> {
    pub fn new(graph: &'a CallGraph, functions: &'a [FunctionInfo]) -> Self {
        Self::with_externals(graph, functions, None)
    }

    pub fn with_externals(
        graph: &'a CallGraph,
        functions: &'a [FunctionInfo],
        external_symbols: Option<&'a ExternalSymbolIndex>,
    ) -> Self {
        let mut names = FxHashMap::default();
        names.reserve(functions.len());
        for function in functions {
            if !function.name.is_empty() {
                names.insert(function.address, function.name.as_str());
            }
        }
        Self {
            graph,
            names,
            external_symbols,
        }
    }

    pub fn from_binary(
        graph: &'a CallGraph,
        binary: &'a LoadedBinary,
        external_symbols: Option<&'a ExternalSymbolIndex>,
    ) -> Self {
        let mut names = FxHashMap::default();
        names.reserve(
            binary.functions.len()
                + binary.iat_symbols.len()
                + binary.global_symbols.len()
                + binary.relocation_symbols.len(),
        );
        for function in &binary.functions {
            if !function.name.is_empty() {
                names.insert(function.address, function.name.as_str());
            }
        }
        for (&va, name) in &binary.iat_symbols {
            if !name.is_empty() {
                names.entry(va).or_insert(name.as_str());
            }
        }
        for (&va, name) in &binary.global_symbols {
            if !name.is_empty() {
                names.entry(va).or_insert(name.as_str());
            }
        }
        for (&va, name) in &binary.relocation_symbols {
            if !name.is_empty() {
                names.entry(va).or_insert(name.as_str());
            }
        }
        Self {
            graph,
            names,
            external_symbols,
        }
    }

    pub fn name_at(&self, address: u64) -> Option<&str> {
        if let Some(&name) = self.names.get(&address) {
            return Some(name);
        }
        if let Some(ext) = self.external_symbols {
            if let Some(identity) = ext.identity_for_va(address) {
                if !identity.symbol_key.is_empty() {
                    return Some(identity.symbol_key.as_str());
                }
            }
        }
        None
    }

    fn matches_name(&self, address: u64, name: &str) -> bool {
        if let Some(target) = self.name_at(address) {
            if target == name {
                return true;
            }
            if let Some((_, sym)) = target.split_once('!') {
                if sym.trim() == name {
                    return true;
                }
            }
            if let Some(sym) = target.strip_prefix("__imp_") {
                let trimmed = sym.strip_prefix('_').unwrap_or(sym);
                if trimmed == name {
                    return true;
                }
            }
            if target.strip_prefix('_') == Some(name) || name.strip_prefix('_') == Some(target) {
                return true;
            }
        }
        if let Some(ext) = self.external_symbols {
            if let Some(id) = ext.identity_for_va(address) {
                if id.symbol_key == name
                    || id.symbol_key.strip_prefix('_') == Some(name)
                    || name.strip_prefix('_') == Some(id.symbol_key.as_str())
                {
                    return true;
                }
            }
        }
        false
    }
}

impl CallGraphView for NamedCallGraphView<'_> {
    fn has_callee_named(&self, function: u64, name: &str) -> bool {
        self.graph
            .callees_of(function)
            .iter()
            .any(|edge| self.matches_name(edge.addr, name))
    }

    fn has_caller_named(&self, function: u64, name: &str) -> bool {
        self.graph
            .callers_of(function)
            .iter()
            .any(|edge| self.matches_name(edge.addr, name))
    }
}

impl CallGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn callers_of(&self, addr: u64) -> &[CallEdge] {
        self.callers.get(&addr).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn callees_of(&self, addr: u64) -> &[CallEdge] {
        self.callees.get(&addr).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn total_call_sites(&self) -> usize {
        self.total_call_sites
    }

    pub fn build_from_xrefs(
        functions: &[FunctionInfo],
        xref_db: &XrefDatabase,
        fallback_range: u64,
    ) -> Self {
        let mut functions = functions.to_vec();
        functions.sort_by_key(|func| func.address);

        let fallback_range = fallback_range.max(1);
        let mut callers_map: FxHashMap<u64, FxHashMap<u64, usize>> = FxHashMap::default();
        let mut callees_map: FxHashMap<u64, FxHashMap<u64, usize>> = FxHashMap::default();
        let mut total_call_sites = 0usize;

        for xref in xref_db.iter() {
            if xref.xref_type != XrefType::Call {
                continue;
            }

            let caller = match find_function_addr(&functions, xref.from_addr, fallback_range) {
                Some(addr) => addr,
                None => continue,
            };

            let callee = find_function_addr(&functions, xref.to_addr, fallback_range)
                .unwrap_or(xref.to_addr);

            callers_map
                .entry(callee)
                .or_default()
                .entry(caller)
                .and_modify(|count| *count += 1)
                .or_insert(1);

            callees_map
                .entry(caller)
                .or_default()
                .entry(callee)
                .and_modify(|count| *count += 1)
                .or_insert(1);

            total_call_sites += 1;
        }

        let callers = finalize_edges(callers_map);
        let callees = finalize_edges(callees_map);

        Self {
            callers,
            callees,
            total_call_sites,
        }
    }

    /// Same aggregation as [`Self::build_from_xrefs`], but from an
    /// already-computed [`XrefIndex`] (the canonical merged loader +
    /// disassembly index the Xrefs panel/API builds and caches) instead of
    /// a raw [`XrefDatabase`]. Building a fresh `XrefDatabase` here would
    /// mean a second whole-binary disassembly pass identical to the one
    /// `XrefIndex` already paid for -- this reuses that result instead.
    pub fn build_from_xref_index(
        functions: &[FunctionInfo],
        index: &XrefIndex,
        fallback_range: u64,
    ) -> Self {
        let mut functions = functions.to_vec();
        functions.sort_by_key(|func| func.address);

        let fallback_range = fallback_range.max(1);
        let mut callers_map: FxHashMap<u64, FxHashMap<u64, usize>> = FxHashMap::default();
        let mut callees_map: FxHashMap<u64, FxHashMap<u64, usize>> = FxHashMap::default();
        let mut total_call_sites = 0usize;

        for record in &index.refs {
            if record.kind != XrefKind::Call {
                continue;
            }
            let Some(to_addr) = record.target.address else {
                continue;
            };

            let caller =
                match resolve_enclosing_function(&functions, record.source.address, fallback_range)
                {
                    Some(addr) => addr,
                    None => continue,
                };
            let callee =
                resolve_enclosing_function(&functions, to_addr, fallback_range).unwrap_or(to_addr);

            callers_map
                .entry(callee)
                .or_default()
                .entry(caller)
                .and_modify(|count| *count += 1)
                .or_insert(1);

            callees_map
                .entry(caller)
                .or_default()
                .entry(callee)
                .and_modify(|count| *count += 1)
                .or_insert(1);

            total_call_sites += 1;
        }

        let callers = finalize_edges(callers_map);
        let callees = finalize_edges(callees_map);

        Self {
            callers,
            callees,
            total_call_sites,
        }
    }
}

fn finalize_edges(map: FxHashMap<u64, FxHashMap<u64, usize>>) -> FxHashMap<u64, Vec<CallEdge>> {
    let mut out = FxHashMap::default();
    for (addr, edges) in map {
        let mut list: Vec<CallEdge> = edges
            .into_iter()
            .map(|(addr, count)| CallEdge { addr, count })
            .collect();
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.addr.cmp(&b.addr)));
        out.insert(addr, list);
    }
    out
}

fn find_function_addr(functions: &[FunctionInfo], addr: u64, fallback_range: u64) -> Option<u64> {
    resolve_enclosing_function(functions, addr, fallback_range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::FunctionInfo;
    use crate::analysis::xrefs::{OPERAND_INDEX_MNEMONIC, Xref, XrefDatabase, XrefType};
    use fission_sleigh::runtime::DecodedFlowKind;

    fn sample_functions() -> Vec<FunctionInfo> {
        vec![
            FunctionInfo {
                name: "caller".into(),
                address: 0x1000,
                size: 0x100,
                is_export: false,
                is_import: false,
                ..Default::default()
            },
            FunctionInfo {
                name: "callee".into(),
                address: 0x2000,
                size: 0x50,
                is_export: false,
                is_import: false,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn callgraph_counts_only_call_xrefs() {
        let mut db = XrefDatabase::new();
        db.add_xref(Xref {
            from_addr: 0x1004,
            to_addr: 0x2000,
            xref_type: XrefType::Call,
            operand_index: OPERAND_INDEX_MNEMONIC,
            sleigh_kind: None,
            flow_kind: Some(DecodedFlowKind::Call),
        });
        db.add_xref(Xref {
            from_addr: 0x1008,
            to_addr: 0x2050,
            xref_type: XrefType::Jump,
            operand_index: OPERAND_INDEX_MNEMONIC,
            sleigh_kind: None,
            flow_kind: Some(DecodedFlowKind::Jump),
        });

        let functions = sample_functions();
        let g = CallGraph::build_from_xrefs(&functions, &db, 0x40);
        assert_eq!(g.total_call_sites(), 1);
        let callees = g.callees_of(0x1000);
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].addr, 0x2000);

        let view = NamedCallGraphView::new(&g, &functions);
        let signature = fission_signatures::FunctionSignature::from_hex("caller", "e8 ?? ?? ?? ??")
            .with_callees(&["callee"]);
        let validation = fission_signatures::validate_relation(&signature, 0x1000, &view);
        assert!(validation.passed);
        assert_eq!(validation.matched_callees, vec!["callee"]);
    }

    #[test]
    fn named_callgraph_view_resolves_external_import_callees() {
        let mut db = XrefDatabase::new();
        // Call from 0x1004 inside function at 0x1000 to external IAT slot 0x3000
        db.add_xref(Xref {
            from_addr: 0x1004,
            to_addr: 0x3000,
            xref_type: XrefType::Call,
            operand_index: OPERAND_INDEX_MNEMONIC,
            sleigh_kind: None,
            flow_kind: Some(DecodedFlowKind::Call),
        });

        let functions = sample_functions();
        let g = CallGraph::build_from_xrefs(&functions, &db, 0x40);

        let binary = fission_loader::loader::LoadedBinaryBuilder::new(
            "test_bin".to_string(),
            fission_loader::loader::DataBuffer::Heap(vec![0u8; 0x1000]),
        )
        .add_iat_symbol(0x3000, "msvcrt.dll!malloc".to_string())
        .build()
        .expect("build binary");

        let view = NamedCallGraphView::from_binary(&g, &binary, None);
        assert!(view.has_callee_named(0x1000, "malloc"));
        assert!(view.has_callee_named(0x1000, "msvcrt.dll!malloc"));

        let ext = crate::analysis::external_symbol::build_external_symbol_index(&binary);
        let view_with_ext = NamedCallGraphView::with_externals(&g, &functions, Some(&ext));
        assert!(view_with_ext.has_callee_named(0x1000, "malloc"));
    }

    #[test]
    fn find_function_addr_unsized_function_bounded_by_next_function() {
        let functions = vec![
            FunctionInfo {
                name: "unsized_func".into(),
                address: 0x1000,
                size: 0,
                is_export: false,
                is_import: false,
                ..Default::default()
            },
            FunctionInfo {
                name: "next_func".into(),
                address: 0x1500,
                size: 0x100,
                is_export: false,
                is_import: false,
                ..Default::default()
            },
        ];

        // Address beyond 64-byte cap but within unsized function bounds
        assert_eq!(find_function_addr(&functions, 0x1080, 0x40), Some(0x1000));
        assert_eq!(find_function_addr(&functions, 0x14ff, 0x40), Some(0x1000));
        // Address at next function
        assert_eq!(find_function_addr(&functions, 0x1500, 0x40), Some(0x1500));
    }

    #[test]
    fn find_function_addr_duplicate_entry_prefers_sized() {
        let functions = vec![
            FunctionInfo {
                name: "unsized_dup".into(),
                address: 0x1000,
                size: 0,
                ..Default::default()
            },
            FunctionInfo {
                name: "sized_dup".into(),
                address: 0x1000,
                size: 0x200,
                ..Default::default()
            },
        ];

        assert_eq!(find_function_addr(&functions, 0x1100, 0x40), Some(0x1000));
    }
}
