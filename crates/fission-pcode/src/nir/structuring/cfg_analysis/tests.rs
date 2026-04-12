use super::*;

#[test]
fn cfg_analysis_classifies_diamond_edges() {
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);

    let analysis = CfgAnalysis::analyze(&successors, &predecessors);

    assert_eq!(analysis.class_of(0, 1), Some(EdgeClass::Tree));
    assert_eq!(analysis.class_of(0, 2), Some(EdgeClass::Tree));
    assert_eq!(analysis.class_of(1, 3), Some(EdgeClass::Tree));
    assert_eq!(analysis.class_of(2, 3), Some(EdgeClass::Cross));
    assert_eq!(analysis.count_class(EdgeClass::Back), 0);

    let tree = ImmPostDomTree::compute(&successors, &predecessors);
    assert_eq!(tree.merge_point_for_two_arms(1, 2), Some(3));
}

#[test]
fn cfg_analysis_classifies_single_loop_back_edge() {
    let successors = vec![vec![1], vec![2], vec![1, 3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);

    let analysis = CfgAnalysis::analyze(&successors, &predecessors);

    assert_eq!(analysis.class_of(2, 1), Some(EdgeClass::Back));
    assert_eq!(analysis.count_class(EdgeClass::Back), 1);
}

#[test]
fn cfg_analysis_classifies_multi_header_scc_with_back_and_cross_edges() {
    let successors = vec![vec![1, 2], vec![2], vec![1, 3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);

    let analysis = CfgAnalysis::analyze(&successors, &predecessors);

    assert_eq!(analysis.class_of(2, 1), Some(EdgeClass::Back));
    assert_eq!(analysis.class_of(1, 2), Some(EdgeClass::Tree));
    assert!(analysis.count_class(EdgeClass::Back) >= 1);
}

#[test]
fn dom_tree_finds_nearest_common_dominator_for_diamond() {
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let dom = DomTree::analyze(&successors, &predecessors);

    assert!(dom.dominates(0, 1));
    assert!(dom.dominates(0, 2));
    assert_eq!(dom.nearest_common_dominator(&[1, 2]), Some(0));
    assert_eq!(dom.nearest_common_dominator(&[3, 2]), Some(0));
}

#[test]
fn postdom_tree_finds_common_postdominator_for_diamond() {
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let postdom = PostDomTree::analyze(&successors, &predecessors);

    assert_eq!(postdom.nearest_common_postdominator(&[1, 2]), Some(3));
    assert_eq!(postdom.nearest_common_postdominator(&[0, 1]), Some(3));
}

#[test]
fn imm_dom_tree_and_dominance_frontier_match_diamond_shape() {
    // 0 -> {1,2}; 1 -> 3; 2 -> 3; 3 -> []
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let imm_dom = ImmDomTree::compute(&successors, &predecessors);
    let df = DominanceFrontier::compute(&predecessors, &imm_dom);

    assert_eq!(imm_dom.immediate_dominator(1), Some(0));
    assert_eq!(imm_dom.immediate_dominator(2), Some(0));
    assert_eq!(imm_dom.immediate_dominator(3), Some(0));
    assert!(df.contains(1, 3));
    assert!(df.contains(2, 3));
    assert!(!df.contains(0, 3));
}

/// Cross-check immediate dominators against `petgraph` (reference implementation).
#[test]
fn imm_dom_matches_petgraph_diamond() {
    use petgraph::algo::dominators::simple_fast;
    use petgraph::graph::DiGraph;

    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let imm_dom = ImmDomTree::compute(&successors, &predecessors);

    let mut g = DiGraph::<(), ()>::new();
    let nodes: Vec<_> = (0..4).map(|_| g.add_node(())).collect();
    for (u, sucs) in successors.iter().enumerate() {
        for &v in sucs {
            g.add_edge(nodes[u], nodes[v], ());
        }
    }

    let doms = simple_fast(&g, nodes[0]);
    for i in 0..4 {
        let pg = doms.immediate_dominator(nodes[i]).map(|ix| ix.index());
        assert_eq!(
            imm_dom.immediate_dominator(i),
            pg,
            "immediate dominator mismatch at node {i}"
        );
    }
}

#[test]
fn dominance_frontier_empty_for_linear_chain() {
    // 0 -> 1 -> 2 -> 3 -> []
    let successors = vec![vec![1], vec![2], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let imm_dom = ImmDomTree::compute(&successors, &predecessors);
    let df = DominanceFrontier::compute(&predecessors, &imm_dom);

    for idx in 0..successors.len() {
        let frontier = df.of(idx).expect("frontier entry must exist");
        assert!(frontier.is_empty());
    }
}

#[test]
fn scc_analysis_identifies_irreducible_multi_header_component() {
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![1, 2], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let scc = SccAnalysis::analyze(&successors, &predecessors);

    assert!(scc.component_count() >= 2);
    assert_eq!(scc.irreducible_count(), 1);
    assert_eq!(scc.irreducible_header_total_count(), 2);
    let irr = &scc.irreducible_components()[0];
    assert_eq!(irr.headers, vec![1, 2]);
}

#[test]
fn scc_analysis_does_not_mark_single_header_loop_irreducible() {
    let successors = vec![vec![1], vec![2], vec![1, 3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let scc = SccAnalysis::analyze(&successors, &predecessors);

    assert_eq!(scc.irreducible_count(), 0);
}

// ── ImmPostDomTree (Cooper algorithm) tests ────────────────────────────────

#[test]
fn imm_postdom_diamond_follow_is_join() {
    // 0 → {1, 2}; 1 → 3; 2 → 3; 3 → []
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let ipd = ImmPostDomTree::compute(&successors, &predecessors);

    // Follow block of the branch at 0 should be 3 (join point).
    assert_eq!(ipd.nearest_common_postdominator(&[1, 2]), Some(3));
    // idom of 1 and 2 is 3.
    assert_eq!(ipd.immediate_postdominator(1), Some(3));
    assert_eq!(ipd.immediate_postdominator(2), Some(3));
    // idom of 3 is itself (exit node has no strict postdominator).
    assert_eq!(ipd.immediate_postdominator(3), None);
}

#[test]
fn imm_postdom_linear_chain() {
    // 0 → 1 → 2 → 3 → []
    let successors = vec![vec![1], vec![2], vec![3], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let ipd = ImmPostDomTree::compute(&successors, &predecessors);

    assert_eq!(ipd.immediate_postdominator(0), Some(1));
    assert_eq!(ipd.immediate_postdominator(2), Some(3));
    assert_eq!(ipd.immediate_postdominator(3), None);
}

#[test]
fn imm_postdom_nested_diamond() {
    // 0 → {1, 2}; 1 → {3, 4}; 3 → 5; 4 → 5; 2 → 5; 5 → []
    let successors = vec![
        vec![1, 2], // 0
        vec![3, 4], // 1
        vec![5],    // 2
        vec![5],    // 3
        vec![5],    // 4
        vec![],     // 5
    ];
    let predecessors = build_predecessor_index_map(&successors);
    let ipd = ImmPostDomTree::compute(&successors, &predecessors);

    // Follow for outer branch (0): common postdom of {1,2} = 5.
    assert_eq!(ipd.nearest_common_postdominator(&[1, 2]), Some(5));
    // Follow for inner branch (1): common postdom of {3,4} = 5.
    assert_eq!(ipd.nearest_common_postdominator(&[3, 4]), Some(5));
}

#[test]
fn imm_postdom_single_node_is_none() {
    let successors: Vec<Vec<usize>> = vec![vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let ipd = ImmPostDomTree::compute(&successors, &predecessors);
    assert_eq!(ipd.immediate_postdominator(0), None);
}

#[test]
fn scc_analysis_reports_irreducible_membership_by_node() {
    let successors = vec![vec![1, 2], vec![3], vec![3], vec![1, 2], vec![]];
    let predecessors = build_predecessor_index_map(&successors);
    let scc = SccAnalysis::analyze(&successors, &predecessors);

    assert!(scc.is_irreducible_node(1));
    assert!(scc.is_irreducible_node(2));
    assert!(scc.is_irreducible_node(3));
    assert!(!scc.is_irreducible_node(0));
    assert!(!scc.is_irreducible_node(4));
}
