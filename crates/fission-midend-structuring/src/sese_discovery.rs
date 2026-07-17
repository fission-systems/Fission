//! SESE region discovery and hierarchical structuring (ADR 0012 free owner).

use crate::cfg_analysis::util::compute_rpo;
use crate::cfg_analysis::{DomTree, PostDomTree};
use crate::host::StructuringHost;
use crate::linear_recovery::build_linear_sese_child_fallback;
use crate::regions::{RegionKind, RegionProof};
use crate::sese_driver::build_sese_region_body;
use fission_midend_core::ir::{HirStmt, MlilPreviewError};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeseRegion {
    pub entry: usize,
    pub exit: usize,
    pub members: HashSet<usize>,
    pub children: Vec<SeseRegion>,
}

pub struct SeseRegionTree {
    pub root: SeseRegion,
}

/// Computes RPO (Reverse Post-Order) mapping for node indexing order checking.
pub fn compute_rpo_map(successors: &[Vec<usize>]) -> Vec<usize> {
    let mut rpo_map = vec![usize::MAX; successors.len()];
    if successors.is_empty() {
        return rpo_map;
    }
    let post_order = compute_rpo(0, successors, successors.len());
    for (pos, &n) in post_order.iter().enumerate() {
        if n < rpo_map.len() {
            rpo_map[n] = pos;
        }
    }
    rpo_map
}

/// Identifies all valid SESE regions in the CFG.
pub fn find_sese_regions(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    dom: &DomTree,
    postdom: &PostDomTree,
) -> Vec<SeseRegion> {
    let n = successors.len();
    let rpo_map = compute_rpo_map(successors);
    let mut regions = Vec::new();

    for u in 0..n {
        let Some(pdom_set) = postdom.postdominators().get(&u) else {
            continue;
        };
        for &v in pdom_set.iter() {
            if u == v {
                continue;
            }
            if !dom.dominates(u, v) {
                continue;
            }
            if rpo_map[u] >= rpo_map[v] {
                continue;
            }

            let mut members = HashSet::new();
            let mut queue = vec![u];
            let mut visited = HashSet::new();
            visited.insert(u);
            let mut reaches_exit = false;

            while let Some(curr) = queue.pop() {
                if curr == v {
                    reaches_exit = true;
                    continue;
                }
                members.insert(curr);
                if let Some(succs) = successors.get(curr) {
                    for &succ in succs {
                        if visited.insert(succ) {
                            queue.push(succ);
                        }
                    }
                }
            }

            if !reaches_exit || members.is_empty() {
                continue;
            }

            let mut side_entry = false;
            for &w in &members {
                if w == u {
                    continue;
                }
                if let Some(preds) = predecessors.get(w) {
                    for &p in preds {
                        if !members.contains(&p) {
                            side_entry = true;
                            break;
                        }
                    }
                }
                if side_entry {
                    break;
                }
            }
            if side_entry {
                continue;
            }

            let mut side_exit = false;
            for &w in &members {
                if let Some(succs) = successors.get(w) {
                    for &s in succs {
                        if s != v && !members.contains(&s) {
                            side_exit = true;
                            break;
                        }
                    }
                }
                if side_exit {
                    break;
                }
            }
            if side_exit {
                continue;
            }

            regions.push(SeseRegion {
                entry: u,
                exit: v,
                members,
                children: Vec::new(),
            });
        }
    }
    regions
}

/// Builds the hierarchical SESE region tree from a flat list of SESE regions.
pub fn build_sese_tree(mut regions: Vec<SeseRegion>, total_nodes: usize) -> SeseRegionTree {
    regions.sort_by_key(|r| r.members.len());

    let mut nested_regions: Vec<SeseRegion> = Vec::new();

    for region in regions {
        let mut new_region = region;
        let mut children = Vec::new();
        let mut remaining = Vec::new();

        for r in nested_regions {
            if r.members.is_subset(&new_region.members)
                && r.members.len() < new_region.members.len()
            {
                children.push(r);
            } else {
                remaining.push(r);
            }
        }

        new_region.children = children;
        nested_regions = remaining;
        nested_regions.push(new_region);
    }

    let global_members: HashSet<usize> = (0..total_nodes).collect();
    let global_root = SeseRegion {
        entry: 0,
        exit: total_nodes,
        members: global_members,
        children: nested_regions,
    };

    SeseRegionTree { root: global_root }
}

/// Recursively structures the SESE tree bottom-up.
pub fn sese_structure_region(
    host: &mut impl StructuringHost,
    region: &SeseRegion,
    results: &mut HashMap<(usize, usize), Vec<HirStmt>>,
    total_nodes: usize,
) -> Result<(), MlilPreviewError> {
    let is_root = region.entry == 0 && region.exit == total_nodes;

    for child in &region.children {
        if let Err(err) = sese_structure_region(host, child, results, total_nodes) {
            match build_linear_sese_child_fallback(host, child.entry, child.exit) {
                Ok(body) => {
                    host.bump_sese_child_localized_linear();
                    results.insert((child.entry, child.exit), body);
                }
                Err(_) => return Err(err),
            }
        }
    }

    let mut child_map = HashMap::new();
    for child in &region.children {
        if let Some(body) = results.get(&(child.entry, child.exit)) {
            let proof =
                RegionProof::structured(RegionKind::Sequence, child.entry, child.exit, None);
            child_map.insert(child.entry, (body.clone(), child.exit, proof));
        }
    }

    match build_sese_region_body(host, region.entry, region.exit, child_map) {
        Ok(body) => {
            results.insert((region.entry, region.exit), body);
            Ok(())
        }
        Err(err) if is_root => Err(err),
        Err(err) => match build_linear_sese_child_fallback(host, region.entry, region.exit) {
            Ok(body) => {
                host.bump_sese_child_localized_linear();
                results.insert((region.entry, region.exit), body);
                Ok(())
            }
            Err(_) => Err(err),
        },
    }
}

/// Main entrypoint for SESE region-based structuring.
pub fn structure_cfg_via_sese(
    host: &mut impl StructuringHost,
    total_nodes: usize,
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    let dom = host.cfg_facts().dominators().clone();
    let postdom = host.cfg_facts().postdominators().clone();

    let regions = find_sese_regions(host.successors(), host.predecessors(), &dom, &postdom);
    let tree = build_sese_tree(regions, total_nodes);

    let mut results = HashMap::new();
    sese_structure_region(host, &tree.root, &mut results, total_nodes)?;

    Ok(results.remove(&(0, total_nodes)).unwrap_or_default())
}
