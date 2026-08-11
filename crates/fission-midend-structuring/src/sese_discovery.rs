//! SESE region discovery and hierarchical structuring (ADR 0012 free owner).

use crate::cfg_analysis::util::compute_rpo;
use crate::cfg_analysis::{DomTree, PostDomTree};
use crate::host::StructuringHost;
use crate::linear_recovery::build_linear_sese_child_fallback;
use crate::regions::{RegionKind, RegionProof};
use crate::sese_driver::build_sese_region_body;
use fission_midend_core::ir::{MlilPreviewError};
use fission_midend_prehir::{PreHirStmt};
use crate::HashMap;
use crate::HashSet;

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

            let mut members = HashSet::default();
            let mut queue = vec![u];
            let mut visited = HashSet::default();
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

/// Partitions `candidates` into (accepted, rejected) against `bound_members`,
/// where `accepted` is a pairwise-*disjoint* subset of the candidates that
/// are themselves proper subsets of `bound_members`.
///
/// A valid SESE region *family* is laminar: any two regions are either
/// disjoint or one nests inside the other -- never a partial overlap
/// ("crossing"). `find_sese_regions` doesn't guarantee this for
/// irreducible/convergent CFGs (e.g. several switch-case blocks that all
/// funnel into the same postdominator can produce two SESE-valid (u, v)
/// pairs that share members without either containing the other). Admitting
/// both as siblings would structure the shared blocks twice, once under each
/// subtree, and then stitch the two conflicting results back together as if
/// they were sequential -- silently corrupting the output (observed on a
/// real sample-set binary: a do-while loop's real successor, a switch
/// dispatcher, got replaced by an unrelated block from the crossing region,
/// and the dispatcher itself vanished from the output entirely). A rejected
/// candidate falls back to its parent's own linear structuring instead of a
/// nested if/while shape -- a cosmetic loss (more gotos), not a correctness
/// one.
fn select_disjoint_subset(
    candidates: Vec<SeseRegion>,
    bound_members: &HashSet<usize>,
) -> (Vec<SeseRegion>, Vec<SeseRegion>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut claimed: HashSet<usize> = HashSet::default();

    for r in candidates {
        if r.members.is_subset(bound_members)
            && r.members.len() < bound_members.len()
            && r.members.is_disjoint(&claimed)
        {
            claimed.extend(r.members.iter().copied());
            accepted.push(r);
        } else {
            rejected.push(r);
        }
    }

    (accepted, rejected)
}

/// Builds the hierarchical SESE region tree from a flat list of SESE regions.
pub fn build_sese_tree(mut regions: Vec<SeseRegion>, total_nodes: usize) -> SeseRegionTree {
    regions.sort_by_key(|r| r.members.len());

    let mut nested_regions: Vec<SeseRegion> = Vec::new();

    for region in regions {
        let mut new_region = region;
        let (children, remaining) = select_disjoint_subset(nested_regions, &new_region.members);
        new_region.children = children;
        nested_regions = remaining;
        nested_regions.push(new_region);
    }

    let global_members: HashSet<usize> = (0..total_nodes).collect();
    // The synthetic global root isn't itself a `regions` entry, so its
    // children never went through the loop above -- apply the same
    // crossing-rejection here, or a pair of top-level leftover regions that
    // cross each other (and never happened to both fit under some other
    // enclosing candidate) would slip through unfiltered.
    let (root_children, _unclaimed) = select_disjoint_subset(nested_regions, &global_members);
    let global_root = SeseRegion {
        entry: 0,
        exit: total_nodes,
        members: global_members,
        children: root_children,
    };

    SeseRegionTree { root: global_root }
}

/// Recursively structures the SESE tree bottom-up.
pub fn sese_structure_region(
    host: &mut impl StructuringHost,
    region: &SeseRegion,
    results: &mut HashMap<(usize, usize), Vec<PreHirStmt>>,
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

    let mut child_map = HashMap::default();
    for child in &region.children {
        // `results` entries are write-once-read-once: this loop, in the
        // child's immediate (and only) parent, is the sole consumer --
        // `structure_cfg_via_sese`'s own top-level read only ever takes the
        // root entry. `remove` takes ownership instead of cloning the whole
        // recursively-nested PreHirStmt body, which profiling showed was a
        // second (smaller) instance of the same clone-heavy pattern already
        // fixed in `lower_linear_body_cached`.
        if let Some(body) = results.remove(&(child.entry, child.exit)) {
            let proof =
                RegionProof::structured(RegionKind::Sequence, child.entry, child.exit, None);
            child_map.insert(child.entry, (body, child.exit, proof));
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
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    let dom = host.cfg_facts().dominators().clone();
    let postdom = host.cfg_facts().postdominators().clone();

    let regions = find_sese_regions(host.successors(), host.predecessors(), &dom, &postdom);
    let tree = build_sese_tree(regions, total_nodes);

    let mut results = HashMap::default();
    sese_structure_region(host, &tree.root, &mut results, total_nodes)?;

    Ok(results.remove(&(0, total_nodes)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(entry: usize, exit: usize, members: &[usize]) -> SeseRegion {
        SeseRegion {
            entry,
            exit,
            members: members.iter().copied().collect(),
            children: Vec::new(),
        }
    }

    fn assert_no_crossing_siblings(children: &[SeseRegion]) {
        for i in 0..children.len() {
            for j in (i + 1)..children.len() {
                assert!(
                    children[i].members.is_disjoint(&children[j].members),
                    "sibling regions ({}, {}) and ({}, {}) overlap without nesting: {:?} vs {:?}",
                    children[i].entry,
                    children[i].exit,
                    children[j].entry,
                    children[j].exit,
                    children[i].members,
                    children[j].members,
                );
            }
        }
        for child in children {
            assert_no_crossing_siblings(&child.children);
        }
    }

    /// Regression test for the bin_000.elf finding: two SESE-valid regions
    /// that partially overlap without either containing the other must
    /// never both become children of the same parent -- doing so causes
    /// their shared blocks to be structured twice under two different
    /// subtrees, and the caller stitches the (conflicting) results back
    /// together as if they were sequential, silently dropping or
    /// misattaching real control flow.
    #[test]
    fn build_sese_tree_rejects_crossing_sibling_regions() {
        let crossing_a = region(1, 39, &[1, 2, 3, 4, 5]);
        let crossing_b = region(0, 3, &[0, 1, 2, 5]);
        let enclosing = region(0, 39, &[0, 1, 2, 3, 4, 5]);
        let tree = build_sese_tree(vec![crossing_a, crossing_b, enclosing], 40);
        assert_no_crossing_siblings(std::slice::from_ref(&tree.root));
    }

    /// Same invariant, but for a crossing pair that never fits under any
    /// other candidate region and so falls all the way through to the
    /// synthetic global root -- that assembly step must apply the same
    /// rejection, not just the per-region loop above.
    #[test]
    fn build_sese_tree_rejects_crossing_regions_at_the_root() {
        let crossing_a = region(1, 5, &[1, 2, 3]);
        let crossing_b = region(0, 3, &[0, 1, 2]);
        let tree = build_sese_tree(vec![crossing_a, crossing_b], 6);
        assert_no_crossing_siblings(std::slice::from_ref(&tree.root));
    }
}
