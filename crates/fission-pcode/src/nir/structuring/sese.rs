use crate::fast_hash::FastMap as HashMap;
use crate::nir::structuring::cfg_analysis::{DomTree, PostDomTree};
use crate::nir::structuring::CollapseRule;
use crate::nir::structuring::graph::StructureNode;
use crate::nir::structuring::regions::{RegionKind, RegionProof};
use crate::nir::types::HirStmt;
use crate::nir::PreviewBuilder;
use crate::nir::support::MlilPreviewError;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeseRegion {
    pub(crate) entry: usize,
    pub(crate) exit: usize,
    pub(crate) members: HashSet<usize>,
    pub(crate) children: Vec<SeseRegion>,
}

pub(crate) struct SeseRegionTree {
    pub(crate) root: SeseRegion,
}

/// Computes RPO (Reverse Post-Order) mapping for node indexing order checking.
pub(crate) fn compute_rpo_map(successors: &[Vec<usize>]) -> Vec<usize> {
    let mut rpo_map = vec![usize::MAX; successors.len()];
    if successors.is_empty() {
        return rpo_map;
    }
    let post_order = crate::nir::structuring::cfg_analysis::util::compute_rpo(0, successors, successors.len());
    for (pos, &n) in post_order.iter().enumerate() {
        if n < rpo_map.len() {
            rpo_map[n] = pos;
        }
    }
    rpo_map
}

/// Identifies all valid SESE regions in the CFG.
pub(crate) fn find_sese_regions(
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
            // Check basic dominance and post-dominance requirements.
            if !dom.dominates(u, v) {
                continue;
            }
            // Ensure u precedes v in RPO.
            if rpo_map[u] >= rpo_map[v] {
                continue;
            }

            // Collect reachable members from u to v (excluding v itself).
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

            // A SESE region must contain u and eventually reach v.
            if !reaches_exit || members.is_empty() {
                continue;
            }

            // Side-entry check: no node in members (except u) can have predecessors outside members.
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

            // Side-exit check: no node in members can have successors outside (members U {v}).
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
pub(crate) fn build_sese_tree(mut regions: Vec<SeseRegion>, total_nodes: usize) -> SeseRegionTree {
    // Sort regions by member count (ascending) to build from inside out.
    regions.sort_by_key(|r| r.members.len());

    let mut nested_regions: Vec<SeseRegion> = Vec::new();

    for region in regions {
        let mut new_region = region;
        let mut children = Vec::new();
        let mut remaining = Vec::new();

        // Find which existing nested regions are strictly contained inside new_region.
        for r in nested_regions {
            if r.members.is_subset(&new_region.members) && r.members.len() < new_region.members.len() {
                children.push(r);
            } else {
                remaining.push(r);
            }
        }

        new_region.children = children;
        nested_regions = remaining;
        nested_regions.push(new_region);
    }

    // The remaining top-level nested regions are direct children of the global root region.
    let global_members: HashSet<usize> = (0..total_nodes).collect();
    let global_root = SeseRegion {
        entry: 0,
        exit: total_nodes,
        members: global_members,
        children: nested_regions,
    };

    SeseRegionTree { root: global_root }
}

/// Recursively structures the SESE tree in a bottom-up manner.
pub(crate) fn sese_structure_region(
    builder: &mut PreviewBuilder,
    region: &SeseRegion,
    results: &mut HashMap<(usize, usize), Vec<HirStmt>>,
    total_nodes: usize,
) -> Result<(), MlilPreviewError> {
    let is_root = region.entry == 0 && region.exit == total_nodes;

    // Bottom-up recursion: structure children first.
    for child in &region.children {
        if let Err(err) = sese_structure_region(builder, child, results, total_nodes) {
            match builder.build_linear_sese_child_fallback(child.entry, child.exit) {
                Ok(body) => {
                    builder.telemetry.structuring.sese_child_localized_linear_count += 1;
                    results.insert((child.entry, child.exit), body);
                }
                Err(_) => return Err(err),
            }
        }
    }

    // Collect child results to map their entries to their corresponding structured body, exit, and proof.
    let mut child_map = HashMap::default();
    for child in &region.children {
        if let Some(body) = results.get(&(child.entry, child.exit)) {
            // Map the child's entry block to the structured statements, exit block, and proof details.
            let proof = RegionProof::structured(
                RegionKind::Sequence,
                child.entry,
                child.exit,
                None,
            );
            child_map.insert(child.entry, (body.clone(), child.exit, proof));
        }
    }

    // Structure the current region using the builder.
    match builder.build_sese_region_body(region.entry, region.exit, child_map) {
        Ok(body) => {
            results.insert((region.entry, region.exit), body);
            Ok(())
        }
        Err(err) if is_root => Err(err),
        Err(err) => match builder.build_linear_sese_child_fallback(region.entry, region.exit) {
            Ok(body) => {
                builder.telemetry.structuring.sese_child_localized_linear_count += 1;
                results.insert((region.entry, region.exit), body);
                Ok(())
            }
            Err(_) => Err(err),
        },
    }
}

/// Main entrypoint for SESE region-based structuring.
pub(crate) fn structure_cfg_via_sese(
    builder: &mut PreviewBuilder,
    total_nodes: usize,
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    let dom = builder.cfg_fact_cache().dominators().clone();
    let postdom = builder.cfg_fact_cache().postdominators().clone();
    
    // Find SESE regions and build SESE hierarchy.
    let regions = find_sese_regions(&builder.successors, &builder.predecessors, &dom, &postdom);
    let tree = build_sese_tree(regions, total_nodes);

    let mut results = HashMap::default();
    sese_structure_region(builder, &tree.root, &mut results, total_nodes)?;

    let final_body = results
        .remove(&(0, total_nodes))
        .unwrap_or_default();
        
    Ok(final_body)
}
