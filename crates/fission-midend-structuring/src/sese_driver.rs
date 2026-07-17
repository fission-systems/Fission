//! SESE / multiblock driver free functions.
//!
//! Collapse-rule dispatch uses free `try_lower_*` entry points. Full SESE body
//! construction still uses host hooks for graph overlay + residual recovery.

use crate::host::StructuringHost;
use crate::conditionals::{
    try_lower_if, try_lower_if_else, try_lower_short_circuit_if, try_reduce_if_else_with_follow,
};
use crate::loops::{
    try_lower_dowhile, try_lower_for, try_lower_infloop, try_lower_infloop_with_break,
    try_lower_multiblock_dowhile, try_lower_multiblock_infloop, try_lower_while,
};
use crate::switch::try_lower_switch;
use crate::linear_types::structuring_diag_enabled;
use crate::guarded_tail::promote_guarded_tail_regions_until_stable;
use fission_midend_core::ir::{HirStmt, MlilPreviewError};

/// Collapse rule tags (Ghidra ActionStructureTransform analog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseRule {
    Switch,
    ForLoop,
    DoWhile,
    WhileDo,
    InfLoopBreak,
    InfLoop,
    Conditional,
    Sequence,
    Unstructured,
}

impl CollapseRule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Switch => "switch",
            Self::ForLoop => "for",
            Self::DoWhile => "do-while",
            Self::WhileDo => "while",
            Self::InfLoopBreak => "infloop-break",
            Self::InfLoop => "infloop",
            Self::Conditional => "conditional",
            Self::Sequence => "sequence",
            Self::Unstructured => "unstructured",
        }
    }
}

/// Active collapse rule order (matches pcode ACTIVE_COLLAPSE_RULES).
pub const ACTIVE_COLLAPSE_RULES: [CollapseRule; 9] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
    CollapseRule::Sequence,
    CollapseRule::Unstructured,
];

/// Ideal-rule subset for SESE tier-1 collapse.
pub const IDEAL_COLLAPSE_RULES: [CollapseRule; 7] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
];

/// Apply one collapse rule at `idx` via free-function `try_lower_*` owners.
pub fn apply_collapse_rule(
    host: &mut impl StructuringHost,
    rule: CollapseRule,
    idx: usize,
    follow: Option<usize>,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    match rule {
        CollapseRule::Switch => try_lower_switch(host, idx),
        CollapseRule::ForLoop => try_lower_for(host, idx),
        CollapseRule::DoWhile => {
            let mut dw = try_lower_dowhile(host, idx)?;
            if dw.is_none() {
                dw = try_lower_multiblock_dowhile(host, idx)?;
            }
            Ok(dw)
        }
        CollapseRule::WhileDo => try_lower_while(host, idx),
        CollapseRule::InfLoopBreak => try_lower_infloop_with_break(host, idx),
        CollapseRule::InfLoop => {
            let mut inf = try_lower_infloop(host, idx);
            if inf.is_err() || matches!(inf, Ok(None)) {
                inf = try_lower_multiblock_infloop(host, idx);
            }
            inf
        }
        CollapseRule::Conditional => {
            let mut cond = try_lower_short_circuit_if(host, idx);
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_reduce_if_else_with_follow(host, idx, follow);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if_else(host, idx);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if(host, idx);
            }
            cond
        }
        CollapseRule::Sequence | CollapseRule::Unstructured => Ok(None),
    }
}

/// Promote guarded-tail regions to a fixed point (free entry).
pub fn promote_guarded_tails(host: &mut impl StructuringHost, body: &mut Vec<HirStmt>) {
    promote_guarded_tail_regions_until_stable(host, body);
    if structuring_diag_enabled() {
        // keep quiet unless already enabled elsewhere
    }
}

/// Final SESE reconstruction scan: materialize structured child regions and
/// residual basic/unstructured nodes into a structure graph, then surface HIR.
pub fn reconstruct_sese_final_body(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
    active_child_map: &std::collections::HashMap<usize, (Vec<HirStmt>, usize, crate::regions::RegionProof)>,
    targeted: &std::collections::HashSet<u64>,
    diag: bool,
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    use crate::cleanup::child_body_has_entry_label;
    use crate::graph::{StructureEdgeFlags, StructureGraph, StructureNode, StructureNodeKind, surface_structure_graph};
    use crate::helpers::{block_label, recovered_switch_case_values};
    use crate::linear_types::LoweredTerminator;
    use crate::regions::EmitReadyDecision;
    use fission_midend_core::ir::HirSwitchCase;

    let mut graph = StructureGraph::default();
    let mut emitted_labels = std::collections::HashSet::new();
    let mut previous_node_id = None;

        let mut idx = entry;
        while idx < exit {
            let block_key = host.block_target_key(idx);
            let has_same_start_peer = host.has_same_start_address_peer(idx);
            let is_orphan_unreachable = idx != 0
                && host.predecessors()[idx].is_empty()
                && !targeted.contains(&block_key)
                && !has_same_start_peer;
            if is_orphan_unreachable {
                idx += 1;
                continue;
            }

            if let Some((child_body, child_exit, child_proof)) = active_child_map.get(&idx) {
                let mut node_statements = child_body.clone();
                let header_label = block_label(block_key);
                if (idx == 0 || targeted.contains(&block_key))
                    && emitted_labels.insert(block_key)
                    && !child_body_has_entry_label(child_body, &header_label)
                {
                    node_statements.insert(0, HirStmt::Label(header_label));
                }

                let node = StructureNode {
                    id: graph.next_node_id(),
                    kind: StructureNodeKind::Region(child_proof.kind),
                    skip_to: *child_exit,
                    statements: node_statements,
                    proof: Some(child_proof.clone()),
                };

                let node_id = graph.push(node);
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
                let next_idx = *child_exit;
                if next_idx <= idx {
                    if diag {
                        eprintln!(
                            "[DIAG] final reconstruction SESE scan: non-advancing child_exit: {} <= {}",
                            next_idx, idx
                        );
                    }
                    idx += 1;
                    continue;
                }
                idx = next_idx;
                continue;
            }

            let mut node_body = Vec::new();
            let mut explicit_edge_surface = false;
            if (idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
                node_body.push(HirStmt::Label(block_label(block_key)));
            }
            node_body.extend(host.lower_block_stmts(idx)?);
            match host.lower_block_terminator(idx)? {
                LoweredTerminator::Return(expr) => node_body.push(HirStmt::Return(expr)),
                LoweredTerminator::Goto(target) => {
                    if let Some(target_idx) = host.find_block_index_by_address(target) {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(HirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        } else if host.next_block_address(idx) != Some(target) {
                            node_body.push(HirStmt::Goto(block_label(target)));
                            explicit_edge_surface = true;
                        }
                    } else if host.next_block_address(idx) != Some(target) {
                        node_body.push(HirStmt::Goto(block_label(target)));
                        explicit_edge_surface = true;
                    }
                }
                LoweredTerminator::Fallthrough(Some(target)) => {
                    if let Some(target_idx) = host.find_block_index_by_address(target) {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, target_idx)?
                        {
                            node_body.push(HirStmt::Return(Some(expr)));
                            explicit_edge_surface = true;
                        }
                    }
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let next_addr = host.next_block_address(idx);
                    let true_idx = host.find_block_index_by_address(true_target);
                    let false_idx =
                        false_target.and_then(|target| host.find_block_index_by_address(target));
                    let true_virtual =
                        true_idx.is_some_and(|ti| crate::collapse_loop::is_virtual_goto_edge(host, idx, ti));
                    let false_virtual =
                        false_idx.is_some_and(|fi| crate::collapse_loop::is_virtual_goto_edge(host, idx, fi));
                    let mut then_body = if true_virtual || next_addr != Some(true_target) {
                        vec![HirStmt::Goto(block_label(true_target))]
                    } else {
                        Vec::new()
                    };
                    if let Some(true_idx) = true_idx {
                        if let Some(expr) =
                            host.lower_return_join_expr_for_predecessor(idx, true_idx)?
                        {
                            then_body = vec![HirStmt::Return(Some(expr))];
                        }
                    }
                    let else_body = match false_target {
                        Some(false_target) => {
                            let mut else_body = if false_virtual || Some(false_target) != next_addr
                            {
                                vec![HirStmt::Goto(block_label(false_target))]
                            } else {
                                Vec::new()
                            };
                            if let Some(false_idx) = false_idx {
                                if let Some(expr) =
                                    host.lower_return_join_expr_for_predecessor(idx, false_idx)?
                                {
                                    else_body = vec![HirStmt::Return(Some(expr))];
                                }
                            }
                            else_body
                        }
                        _ => Vec::new(),
                    };
                    node_body.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    });
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Unsupported {
                    evidence,
                    target_expr,
                } => {
                    node_body.push(host.emit_unsupported_control_surface(evidence, target_expr));
                    explicit_edge_surface = true;
                }
                LoweredTerminator::Switch {
                    expr,
                    targets,
                    default_target,
                    min_val,
                    proof,
                } => {
                    let cases: Vec<HirSwitchCase> = if let Some(proof) = proof.as_ref() {
                        if EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready {
                            proof
                                .recovered_cases
                                .iter()
                                .filter(|(_, target)| Some(*target) != default_target)
                                .map(|(value, target)| HirSwitchCase {
                                    values: vec![*value],
                                    body: vec![HirStmt::Goto(block_label(*target))],
                                })
                                .collect()
                        } else {
                            recovered_switch_case_values(
                                &targets,
                                default_target,
                                min_val,
                                Some(proof),
                            )
                            .0
                            .into_iter()
                            .map(|(value, target)| HirSwitchCase {
                                values: vec![value],
                                body: vec![HirStmt::Goto(block_label(target))],
                            })
                            .collect()
                        }
                    } else if let Some(parsed) = crate::switch::parse_switch_chain(host, idx).ok().flatten() {
                        parsed
                            .cases
                            .into_iter()
                            .filter(|(_, block_idx)| {
                                let target = host.block_target_key(*block_idx);
                                Some(target) != default_target
                            })
                            .map(|(value, block_idx)| HirSwitchCase {
                                values: vec![value],
                                body: vec![HirStmt::Goto(block_label(
                                    host.block_target_key(block_idx),
                                ))],
                            })
                            .collect()
                    } else {
                        targets
                            .into_iter()
                            .filter(|target| Some(*target) != default_target)
                            .enumerate()
                            .map(|(i, t)| HirSwitchCase {
                                values: vec![min_val + i as i64],
                                body: vec![HirStmt::Goto(block_label(t))],
                            })
                            .collect()
                    };
                    node_body.push(HirStmt::Switch {
                        expr,
                        cases,
                        default: default_target
                            .map(block_label)
                            .map(HirStmt::Goto)
                            .into_iter()
                            .collect(),
                    });
                    explicit_edge_surface = true;
                }
            }
            if explicit_edge_surface {
                let node_id = graph.next_node_id();
                let node_id = graph.push(StructureNode::unstructured(node_id, node_body, idx + 1));
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            } else {
                let node_id = graph.next_node_id();
                let node_id = graph.push(StructureNode::basic(node_id, node_body, idx + 1));
                if let Some(prev) = previous_node_id {
                    graph.push_edge(prev, node_id, StructureEdgeFlags::Plain);
                }
                previous_node_id = Some(node_id);
            }
            idx += 1;
        }


    let mut body = surface_structure_graph(graph);
    crate::guarded_tail::promote_guarded_tail_regions_until_stable(host, &mut body);
    Ok(body)
}

