//! Call graph analysis built from cross-references.

use std::collections::HashMap;

use fission_loader::loader::FunctionInfo;

use super::xrefs::{XrefDatabase, XrefType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallEdge {
    pub addr: u64,
    pub count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    callers: HashMap<u64, Vec<CallEdge>>,
    callees: HashMap<u64, Vec<CallEdge>>,
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
        let mut callers_map: HashMap<u64, HashMap<u64, usize>> = HashMap::new();
        let mut callees_map: HashMap<u64, HashMap<u64, usize>> = HashMap::new();
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

fn finalize_edges(map: HashMap<u64, HashMap<u64, usize>>) -> HashMap<u64, Vec<CallEdge>> {
    let mut out = HashMap::new();
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
