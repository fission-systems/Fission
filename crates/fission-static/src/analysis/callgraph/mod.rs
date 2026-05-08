//! Call graph analysis built from cross-references.
//!
//! Edges are aggregated from [`super::xrefs::XrefDatabase`] entries whose [`super::xrefs::XrefType`]
//! is [`super::xrefs::XrefType::Call`]. This includes conditional calls (`call` on conditional
//! flow) when Sleigh classifies them as call targets; jump-only tail edges are excluded.

use rustc_hash::FxHashMap;

use fission_loader::loader::FunctionInfo;

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
    if functions.is_empty() {
        return None;
    }

    let idx = match functions.binary_search_by_key(&addr, |func| func.address) {
        Ok(index) => index,
        Err(index) => index.checked_sub(1)?,
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

        let g = CallGraph::build_from_xrefs(&sample_functions(), &db, 0x40);
        assert_eq!(g.total_call_sites(), 1);
        let callees = g.callees_of(0x1000);
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].addr, 0x2000);
    }
}
