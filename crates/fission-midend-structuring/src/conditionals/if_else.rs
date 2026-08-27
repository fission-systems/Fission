//! If/else and postdom-follow if/else free functions.

use super::{forward_join_idx_from_address, shared_forward_linear_exit};
use crate::HashMap;
use crate::HashSet;
use crate::cfg_analysis::{CommonPostdominator, ImmPostDomTree};
use crate::host::{StructuredChildMap, StructuringHost};
use crate::linear_types::{LinearExit, LoweredTerminator};
use crate::regions::RegionProof;
use crate::sese_driver::build_sese_region_body_for_members;
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::PreHirStmt;
use fission_midend_prehir::util::negate_expr;

/// Count the explicit control-flow debt retained by a structured candidate.
/// Nested scopes count because both NIR and HIR render each retained goto.
pub fn count_explicit_gotos(stmts: &[PreHirStmt]) -> usize {
    fn count_stmt(stmt: &PreHirStmt) -> usize {
        match stmt {
            PreHirStmt::Goto(_) => 1,
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => count_explicit_gotos(body),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => count_explicit_gotos(then_body) + count_explicit_gotos(else_body),
            PreHirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| count_explicit_gotos(&case.body))
                    .sum::<usize>()
                    + count_explicit_gotos(default)
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Label(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => 0,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexArmPlan {
    pub entry: usize,
    pub members: Vec<usize>,
}

impl ComplexArmPlan {
    fn scan_end(&self) -> Option<usize> {
        self.members.last().copied()?.checked_add(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualExitIfElsePlan {
    pub first_arm: ComplexArmPlan,
    pub second_arm: ComplexArmPlan,
    /// Nodes reachable from both successors. They remain outside the emitted
    /// conditional and are surfaced once by the enclosing reconstruction.
    pub shared_tail: Vec<usize>,
    pub skip_to: usize,
}

fn closed_arm_members(
    successors: &[Vec<usize>],
    start: usize,
    region_exit: usize,
) -> Option<HashSet<usize>> {
    let mut members = HashSet::default();
    let mut pending = vec![start];
    while let Some(node) = pending.pop() {
        if node >= region_exit || !members.insert(node) {
            continue;
        }
        for &succ in successors.get(node)? {
            if succ >= region_exit {
                return None;
            }
            pending.push(succ);
        }
    }
    Some(members)
}

fn sorted_members(members: HashSet<usize>) -> Vec<usize> {
    let mut members: Vec<_> = members.into_iter().collect();
    members.sort_unstable();
    members
}

/// Prove a two-arm conditional whose only common postdominator is the virtual
/// function exit. This phase is CFG-only: it performs no statement,
/// terminator, expression, type, or materialization lowering.
pub fn plan_virtual_exit_if_else(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    postdom: &ImmPostDomTree,
    idx: usize,
    fallthrough_idx: usize,
    region_exit: usize,
) -> Option<VirtualExitIfElsePlan> {
    let succs = successors.get(idx)?;
    if succs.len() != 2 || !succs.contains(&fallthrough_idx) {
        return None;
    }
    let other_idx = succs
        .iter()
        .copied()
        .find(|&succ| succ != fallthrough_idx)?;
    if !matches!(
        postdom.nearest_common_postdominator_target(&[fallthrough_idx, other_idx]),
        Some(CommonPostdominator::VirtualExit)
    ) {
        return None;
    }

    let first_reachable = closed_arm_members(successors, fallthrough_idx, region_exit)?;
    let second_reachable = closed_arm_members(successors, other_idx, region_exit)?;
    if first_reachable.contains(&idx) || second_reachable.contains(&idx) {
        return None;
    }

    let shared: HashSet<usize> = first_reachable
        .intersection(&second_reachable)
        .copied()
        .collect();
    let first_members: HashSet<usize> = first_reachable.difference(&shared).copied().collect();
    let second_members: HashSet<usize> = second_reachable.difference(&shared).copied().collect();
    if !first_members.contains(&fallthrough_idx)
        || !second_members.contains(&other_idx)
        || !first_members.is_disjoint(&second_members)
    {
        return None;
    }

    for (entry, members) in [
        (fallthrough_idx, &first_members),
        (other_idx, &second_members),
    ] {
        for &node in members {
            for &pred in predecessors.get(node)? {
                if !members.contains(&pred) && !(node == entry && pred == idx) {
                    return None;
                }
            }
            if successors
                .get(node)?
                .iter()
                .any(|succ| !members.contains(succ) && !shared.contains(succ))
            {
                return None;
            }
        }
    }

    let all_members: HashSet<usize> = first_members
        .iter()
        .chain(second_members.iter())
        .chain(shared.iter())
        .copied()
        .collect();
    if all_members.len() != region_exit.saturating_sub(idx + 1)
        || !(idx + 1..region_exit).all(|node| all_members.contains(&node))
    {
        return None;
    }
    for &node in &shared {
        if predecessors
            .get(node)?
            .iter()
            .any(|pred| !all_members.contains(pred))
            || successors
                .get(node)?
                .iter()
                .any(|succ| !shared.contains(succ))
        {
            return None;
        }
    }

    let shared_tail = sorted_members(shared);
    let skip_to = shared_tail.first().copied().unwrap_or(region_exit);
    Some(VirtualExitIfElsePlan {
        first_arm: ComplexArmPlan {
            entry: fallthrough_idx,
            members: sorted_members(first_members),
        },
        second_arm: ComplexArmPlan {
            entry: other_idx,
            members: sorted_members(second_members),
        },
        shared_tail,
        skip_to,
    })
}

/// Lower a pre-proven virtual-exit if/else. Callers must execute this only on
/// isolated host state after stable-first admission has selected the candidate.
pub fn lower_virtual_exit_if_else_committed(
    host: &mut impl StructuringHost,
    idx: usize,
    plan: VirtualExitIfElsePlan,
    first_children: StructuredChildMap,
    second_children: StructuredChildMap,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let cond_prefix = host.lower_block_stmts(idx)?;
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target: Some(false_target),
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };
    let true_idx = host.find_block_index_by_address(true_target);
    let false_idx = host.find_block_index_by_address(false_target);
    let expected = [plan.first_arm.entry, plan.second_arm.entry];
    if !true_idx.is_some_and(|target| expected.contains(&target))
        || !false_idx.is_some_and(|target| expected.contains(&target))
        || true_idx == false_idx
    {
        return Ok(None);
    }

    let fallthrough = host.fallthrough_index(idx);
    let cond = if true_idx == fallthrough {
        cond
    } else if false_idx == fallthrough {
        negate_expr(cond)
    } else {
        return Ok(None);
    };

    fn lower_arm(
        host: &mut impl StructuringHost,
        arm: &ComplexArmPlan,
        child_map: StructuredChildMap,
    ) -> Result<Option<std::rc::Rc<Vec<PreHirStmt>>>, MlilPreviewError> {
        let Some(scan_end) = arm.scan_end() else {
            return Ok(None);
        };
        let members: HashSet<usize> = arm.members.iter().copied().collect();
        let (body, achieved_exit, extra_members) =
            build_sese_region_body_for_members(host, arm.entry, scan_end, child_map, &members)?;
        if achieved_exit != scan_end || extra_members.iter().any(|member| !members.contains(member))
        {
            return Ok(None);
        }
        Ok(Some(std::rc::Rc::new(body)))
    }

    let Some(first_body) = lower_arm(host, &plan.first_arm, first_children)? else {
        return Ok(None);
    };
    let Some(second_body) = lower_arm(host, &plan.second_arm, second_children)? else {
        return Ok(None);
    };
    // `cond` is normalized so true means the lexical fallthrough arm.
    let stmt = PreHirStmt::If {
        cond,
        then_body: first_body,
        else_body: second_body,
    };
    if cond_prefix.is_empty() {
        Ok(Some((stmt, plan.skip_to)))
    } else {
        let mut wrapped = cond_prefix;
        wrapped.push(stmt);
        Ok(Some((
            PreHirStmt::Block(std::rc::Rc::new(wrapped)),
            plan.skip_to,
        )))
    }
}

/// Follow a linear single-predecessor chain to a Return within `[start, follow)`.
pub fn try_lower_return_chain_arm(
    host: &mut impl StructuringHost,
    start_idx: usize,
    follow_idx: usize,
) -> Result<Option<(Vec<PreHirStmt>, usize)>, MlilPreviewError> {
    let mut body: Vec<PreHirStmt> = Vec::new();
    let mut visited: HashSet<usize> = HashSet::default();
    let mut idx = start_idx;
    loop {
        if idx >= follow_idx || !visited.insert(idx) {
            return Ok(None);
        }
        body.extend(host.lower_block_stmts(idx)?);
        match host.lower_block_terminator(idx)? {
            LoweredTerminator::Return(expr) => {
                body.push(PreHirStmt::Return(expr));
                return Ok(Some((body, follow_idx)));
            }
            LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                let Some(next_idx) = host.find_block_index_by_address(target) else {
                    return Ok(None);
                };
                if next_idx == follow_idx {
                    return Ok(None);
                }
                if next_idx >= follow_idx {
                    return Ok(None);
                }
                if !host.can_inline_linear_successor(idx, next_idx, &visited) {
                    return Ok(None);
                }
                idx = next_idx;
            }
            _ => return Ok(None),
        }
    }
}

/// Lower a diamond if/else when both arms share a forward linear exit.
pub fn try_lower_if_else(
    host: &mut impl StructuringHost,
    idx: usize,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let cond_prefix = host.lower_block_stmts(idx)?;
    if idx + 2 >= host.block_count() {
        return Ok(None);
    }
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target: Some(false_target),
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };

    let Some(next_idx) = host.fallthrough_index(idx) else {
        return Ok(None);
    };
    let next_addr = host.block_target_key(next_idx);

    let (cond, then_idx, else_idx) = if true_target == next_addr {
        let Some(else_idx) = forward_join_idx_from_address(host, idx, false_target) else {
            return Ok(None);
        };
        (cond, next_idx, else_idx)
    } else if false_target == next_addr {
        let Some(then_idx) = forward_join_idx_from_address(host, idx, true_target) else {
            return Ok(None);
        };
        (negate_expr(cond), next_idx, then_idx)
    } else {
        return Ok(None);
    };

    let Some(exit) = shared_forward_linear_exit(host, idx, then_idx, else_idx)? else {
        return Ok(None);
    };
    let Some((then_body, then_skip)) = host.lower_linear_body(then_idx, exit)? else {
        return Ok(None);
    };
    let Some((else_body, else_skip)) = host.lower_linear_body(else_idx, exit)? else {
        return Ok(None);
    };
    let skip_to = match exit {
        LinearExit::Join(join_idx) => join_idx,
        LinearExit::Return | LinearExit::End => then_skip.max(else_skip),
    };
    let stmt = PreHirStmt::If {
        cond,
        then_body,
        else_body,
    };
    if cond_prefix.is_empty() {
        Ok(Some((stmt, skip_to)))
    } else {
        let mut wrapped = cond_prefix;
        wrapped.push(stmt);
        Ok(Some((
            PreHirStmt::Block(std::rc::Rc::new(wrapped)),
            skip_to,
        )))
    }
}

/// Postdominance-guided if-then-else using a precomputed follow block.
pub fn try_reduce_if_else_with_follow(
    host: &mut impl StructuringHost,
    idx: usize,
    follow: Option<usize>,
) -> Result<Option<(PreHirStmt, usize)>, MlilPreviewError> {
    let Some(follow_idx) = follow else {
        return Ok(None);
    };
    if follow_idx <= idx || follow_idx >= host.block_count() {
        return Ok(None);
    }

    let cond_prefix = host.lower_block_stmts(idx)?;

    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target: Some(false_target),
    } = host.lower_block_terminator(idx)?
    else {
        return Ok(None);
    };

    let Some(next_idx) = host.fallthrough_index(idx) else {
        return Ok(None);
    };
    let next_addr = host.block_target_key(next_idx);

    let (cond, then_idx, else_idx) = if true_target == next_addr {
        let Some(else_idx) = forward_join_idx_from_address(host, idx, false_target) else {
            return Ok(None);
        };
        (cond, next_idx, else_idx)
    } else if false_target == next_addr {
        let Some(then_idx) = forward_join_idx_from_address(host, idx, true_target) else {
            return Ok(None);
        };
        (negate_expr(cond), next_idx, then_idx)
    } else {
        return Ok(None);
    };

    let exit = LinearExit::Join(follow_idx);

    if then_idx >= follow_idx || else_idx >= follow_idx {
        return Ok(None);
    }

    let (then_body, _) = match host.lower_linear_body(then_idx, exit)? {
        Some((body, skip)) => (body, skip),
        None => match try_lower_return_chain_arm(host, then_idx, follow_idx)? {
            Some((body, skip)) => (std::rc::Rc::new(body), skip),
            None => return Ok(None),
        },
    };
    let (else_body, _) = match host.lower_linear_body(else_idx, exit)? {
        Some((body, skip)) => (body, skip),
        None => match try_lower_return_chain_arm(host, else_idx, follow_idx)? {
            Some((body, skip)) => (std::rc::Rc::new(body), skip),
            None => return Ok(None),
        },
    };

    let stmt = PreHirStmt::If {
        cond,
        then_body,
        else_body,
    };
    if cond_prefix.is_empty() {
        Ok(Some((stmt, follow_idx)))
    } else {
        let mut wrapped = cond_prefix;
        wrapped.push(stmt);
        Ok(Some((
            PreHirStmt::Block(std::rc::Rc::new(wrapped)),
            follow_idx,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn predecessors(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
        let mut result = vec![Vec::new(); successors.len()];
        for (from, succs) in successors.iter().enumerate() {
            for &to in succs {
                result[to].push(from);
            }
        }
        result
    }

    #[test]
    fn plans_closed_arms_with_distinct_returns() {
        let successors = vec![vec![1, 4], vec![2], vec![3], vec![], vec![5], vec![]];
        let predecessors = predecessors(&successors);
        let postdom = ImmPostDomTree::compute(&successors, &predecessors);
        assert_eq!(
            plan_virtual_exit_if_else(&successors, &predecessors, &postdom, 0, 1, 6),
            Some(VirtualExitIfElsePlan {
                first_arm: ComplexArmPlan {
                    entry: 1,
                    members: vec![1, 2, 3]
                },
                second_arm: ComplexArmPlan {
                    entry: 4,
                    members: vec![4, 5]
                },
                shared_tail: vec![],
                skip_to: 6,
            })
        );
    }

    #[test]
    fn plans_interleaved_arms_and_shared_terminal_tail() {
        let successors = vec![
            vec![1, 4],
            vec![2],
            vec![3, 6],
            vec![],
            vec![5],
            vec![3, 6],
            vec![],
        ];
        let predecessors = predecessors(&successors);
        let postdom = ImmPostDomTree::compute(&successors, &predecessors);
        assert_eq!(
            plan_virtual_exit_if_else(&successors, &predecessors, &postdom, 0, 1, 7),
            Some(VirtualExitIfElsePlan {
                first_arm: ComplexArmPlan {
                    entry: 1,
                    members: vec![1, 2]
                },
                second_arm: ComplexArmPlan {
                    entry: 4,
                    members: vec![4, 5]
                },
                shared_tail: vec![3, 6],
                skip_to: 3,
            })
        );
    }

    #[test]
    fn rejects_side_entry_into_an_arm() {
        let successors = vec![vec![1, 4, 2], vec![2], vec![3], vec![], vec![5], vec![]];
        let predecessors = predecessors(&successors);
        let postdom = ImmPostDomTree::compute(&successors, &predecessors);
        assert_eq!(
            plan_virtual_exit_if_else(&successors, &predecessors, &postdom, 0, 1, 6),
            None
        );
    }

    #[test]
    fn rejects_real_join_instead_of_virtual_exit() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = predecessors(&successors);
        let postdom = ImmPostDomTree::compute(&successors, &predecessors);
        assert_eq!(
            plan_virtual_exit_if_else(&successors, &predecessors, &postdom, 0, 1, 4),
            None
        );
    }
}
