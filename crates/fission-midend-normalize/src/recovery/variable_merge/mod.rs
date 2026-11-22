//! Recover shared variable identities from stack views, copies, and live ranges.

mod copy_aliases;
mod copy_barriers;
mod copy_conflicts;
mod join_copies;
mod live_ranges;
mod merge_eligibility;
mod merge_types;
mod stack_views;
mod variable_reads;

#[cfg(test)]
mod tests;

use crate::prelude::*;
use crate::{HashMap, HashSet};
use copy_aliases::{collect_direct_copies, sorted_var_pair, transitive_copy_aliases};
use copy_barriers::collect_copy_merge_barrier_vars;
use copy_conflicts::collect_copy_merge_conflicts;
use fission_midend_prehir::util::rename_vars_in_stmts;
use join_copies::collect_dominant_copy_join_merges;
use live_ranges::LiveRangeCollector;
use merge_eligibility::{
    is_eligible_for_speculative_merge, is_hardware_register_variable, is_sp_or_bp_name,
    name_priority,
};
use merge_types::{force_unify_types_for_merge, unify_types_for_merge};
pub use stack_views::rewrite_stack_view_as_array_element;
use stack_views::{get_stack_span_from_parts, spans_overlap};
use variable_reads::{collect_cooccurring_var_pairs, collect_read_vars};

pub fn apply_variable_merge_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;

    // Keep track of parameters to avoid merging them
    let param_names: crate::HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();

    // Step 1: Merge overlapping stack variables (coalescing multiple stack-slot views)
    let mut stack_renames = Vec::new();
    let mut stack_locals = func.locals.clone();
    let mut stack_merged = vec![false; stack_locals.len()];
    let read_vars = collect_read_vars(&func.body);

    for i in 0..stack_locals.len() {
        if stack_merged[i] {
            continue;
        }
        let b1_name = stack_locals[i].name.clone();
        let b1_ty = stack_locals[i].ty.clone();
        let b1_origin = stack_locals[i].origin;
        if param_names.contains(&b1_name) {
            continue;
        }
        let Some((offset1, size1, is_derived1)) = get_stack_span_from_parts(b1_origin, &b1_ty)
        else {
            continue;
        };

        for j in (i + 1)..stack_locals.len() {
            if stack_merged[j] {
                continue;
            }
            let b2_name = stack_locals[j].name.clone();
            let b2_ty = stack_locals[j].ty.clone();
            let b2_origin = stack_locals[j].origin;
            if param_names.contains(&b2_name) {
                continue;
            }
            let Some((offset2, size2, is_derived2)) = get_stack_span_from_parts(b2_origin, &b2_ty)
            else {
                continue;
            };

            let is_slot1 = b1_name.starts_with("slot_");
            let is_slot2 = b2_name.starts_with("slot_");
            let distinct_read_stack_states =
                offset1 != offset2 && read_vars.contains(&b1_name) && read_vars.contains(&b2_name);
            let can_merge = (!is_slot1 && !is_slot2)
                && !distinct_read_stack_states
                && ((is_derived1 == is_derived2
                    && spans_overlap((offset1, size1), (offset2, size2)))
                    || (offset1 == offset2));

            if can_merge {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let (keep_idx, merge_idx, keep_name, merge_name, keep_ty, merge_ty) = if p1 >= p2 {
                    (
                        i,
                        j,
                        b1_name.clone(),
                        b2_name.clone(),
                        b1_ty.clone(),
                        b2_ty.clone(),
                    )
                } else {
                    (
                        j,
                        i,
                        b2_name.clone(),
                        b1_name.clone(),
                        b2_ty.clone(),
                        b1_ty.clone(),
                    )
                };

                stack_renames.push((merge_name, keep_name));

                let unified_ty = force_unify_types_for_merge(&keep_ty, &merge_ty);
                stack_locals[keep_idx].ty = unified_ty;

                stack_merged[merge_idx] = true;
                changed = true;
            }
        }
    }

    if !stack_renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &stack_renames);
        let renamed_from: crate::HashSet<String> =
            stack_renames.iter().map(|(from, _)| from.clone()).collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        for local in &mut func.locals {
            if let Some(updated) = stack_locals.iter().find(|l| l.name == local.name) {
                local.ty = updated.ty.clone();
            }
        }
        changed = true;
    }

    let direct_copies = collect_direct_copies(&func.body);
    let var_types: HashMap<String, NirType> = func
        .locals
        .iter()
        .chain(func.params.iter())
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect();
    let join_copy_merges = collect_dominant_copy_join_merges(&func.body, &var_types);
    let mut direct_copies = direct_copies;
    for (a, b) in join_copy_merges {
        direct_copies.insert((a.clone(), b.clone()));
        direct_copies.insert((b, a));
    }
    let local_names: HashSet<String> = func.locals.iter().map(|b| b.name.clone()).collect();
    let stack_state_vars: HashSet<String> = func
        .locals
        .iter()
        .filter(|binding| {
            matches!(
                binding.origin,
                Some(
                    NirBindingOrigin::StackOffset(_)
                        | NirBindingOrigin::HomeSlot(_)
                        | NirBindingOrigin::DerivedFromStackOffset(_)
                )
            )
        })
        .map(|binding| binding.name.clone())
        .collect();
    let mut copy_merge_barriers = collect_copy_merge_barrier_vars(&func.body, stack_state_vars);
    // A global is storage outside the function: renaming a copy of it onto
    // the global turns every later write to the copy into a store the
    // program never made (`u = g; u &= 0x51;` became `g &= 0x51;`), and
    // renaming the global onto a local drops stores that are its effect.
    copy_merge_barriers.extend(crate::pipeline::global_storage_names());
    // Pairs that appear together in one expression (e.g. `eax = ecx + edx`) must
    // never be unified — applies to both copy-alias and disjoint live-range merge.
    let mut copy_merge_blocked_pairs = collect_cooccurring_var_pairs(&func.body);
    // A direct copy proves equality only at the copy point. If either side is
    // then changed and the other side is read, the names carry distinct live
    // values and must not be globally renamed into one storage identity.
    copy_merge_blocked_pairs.extend(collect_copy_merge_conflicts(&func.body, &direct_copies));
    let copy_aliases = transitive_copy_aliases(
        &direct_copies,
        &local_names,
        &copy_merge_barriers,
        &copy_merge_blocked_pairs,
    );

    if !copy_aliases.is_empty() {
        let copy_renames: Vec<(String, String)> = copy_aliases.into_iter().collect();
        rename_vars_in_stmts(&mut func.body, &copy_renames);
        let renamed_from: crate::HashSet<String> =
            copy_renames.iter().map(|(from, _)| from.clone()).collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        changed = true;
    }

    // Step 2: Speculatively merge variables with disjoint live ranges and compatible types
    let mut live_ranges = LiveRangeCollector {
        stmt_counter: 0,
        ranges: HashMap::default(),
        reads: HashMap::default(),
        writes: HashMap::default(),
        labels: HashMap::default(),
        backedges: Vec::new(),
        control_intervals: Vec::new(),
    };
    live_ranges.visit_stmts(&func.body);

    // Apply unstructured loop backedges collected during visit
    let backedges = live_ranges.backedges.clone();
    for (loop_start, loop_end) in backedges {
        live_ranges.extend_loop_ranges(loop_start, loop_end);
    }

    let mut disjoint_renames = Vec::new();
    let mut disjoint_merged = vec![false; func.locals.len()];
    let mut current_locals = func.locals.clone();
    let mut merge_members = current_locals
        .iter()
        .map(|binding| [binding.name.clone()].into_iter().collect::<HashSet<_>>())
        .collect::<Vec<_>>();

    for i in 0..current_locals.len() {
        if disjoint_merged[i] {
            continue;
        }
        let b1_name = current_locals[i].name.clone();
        let b1_ty = current_locals[i].ty.clone();
        let b1_init = current_locals[i].initializer.is_some();
        if param_names.contains(&b1_name) || b1_init {
            continue;
        }
        if copy_merge_barriers.contains(&b1_name) {
            continue;
        }
        if !live_ranges.ranges.contains_key(&b1_name) {
            continue;
        }

        if !is_eligible_for_speculative_merge(&current_locals[i]) {
            continue;
        }
        let span1 = get_stack_span_from_parts(current_locals[i].origin, &b1_ty);

        for j in (i + 1)..current_locals.len() {
            if disjoint_merged[j] {
                continue;
            }
            if !is_eligible_for_speculative_merge(&current_locals[j]) {
                continue;
            }
            let b2_name = current_locals[j].name.clone();
            let b2_ty = current_locals[j].ty.clone();
            let b2_init = current_locals[j].initializer.is_some();
            if param_names.contains(&b2_name) || b2_init {
                continue;
            }
            if copy_merge_barriers.contains(&b2_name) {
                continue;
            }
            let Some(&(start2, end2)) = live_ranges.ranges.get(&b2_name) else {
                continue;
            };
            let Some(&(start1, end1)) = live_ranges.ranges.get(&b1_name) else {
                break;
            };

            // Disjoint Domain Restriction: at least one variable must be a
            // synthetic temporary. Architectural register bindings are never
            // candidates here; their identity may only be shared by an
            // explicit materialize/def-use join proof.
            let is_temp1 =
                current_locals[i].is_temp_like() || name_priority(&current_locals[i].name) <= 1;
            let is_temp2 =
                current_locals[j].is_temp_like() || name_priority(&current_locals[j].name) <= 1;
            if !is_temp1 && !is_temp2 {
                continue;
            }

            // Control-Flow Boundaries: Reject merges across major loop scopes or switch boundaries.
            // If one variable is loop-local and the other is not (inside != inside), reject the merge.
            let crosses_boundary = live_ranges
                .control_intervals
                .iter()
                .any(|&(c_start, c_end)| {
                    let in1 = start1 >= c_start && end1 <= c_end;
                    let in2 = start2 >= c_start && end2 <= c_end;
                    in1 != in2
                });

            if crosses_boundary {
                continue;
            }

            let span2 = get_stack_span_from_parts(current_locals[j].origin, &b2_ty);
            if let (Some((off1, _, _)), Some((off2, _, _))) = (span1, span2) {
                if off1 != off2 {
                    continue;
                }
            }

            // Same barrier as copy-alias merge: co-occurring names are not the
            // same value (saturating_add: a, b, and sum must stay distinct).
            if merge_members[i].iter().any(|left| {
                merge_members[j]
                    .iter()
                    .any(|right| copy_merge_blocked_pairs.contains(&sorted_var_pair(left, right)))
            }) {
                continue;
            }
            // Distinct architectural GPRs are storage identities, not free
            // temps. Coalescing ecx→eax via imprecise live ranges collapses
            // multi-register sequences into `eax + eax`.
            if is_hardware_register_variable(&b1_name)
                && is_hardware_register_variable(&b2_name)
                && !is_sp_or_bp_name(&b1_name)
                && !is_sp_or_bp_name(&b2_name)
            {
                continue;
            }

            let disjoint = end1 < start2 || end2 < start1;
            if !disjoint {
                continue;
            }

            if let Some(unified_ty) = unify_types_for_merge(&b1_ty, &b2_ty) {
                let p1 = name_priority(&b1_name);
                let p2 = name_priority(&b2_name);

                let is_stack1 = current_locals[i]
                    .origin
                    .map_or(false, |o| !matches!(o, NirBindingOrigin::Temp));
                let is_stack2 = current_locals[j]
                    .origin
                    .map_or(false, |o| !matches!(o, NirBindingOrigin::Temp));

                let (keep_idx, merge_idx, keep_name, merge_name) = if is_stack1 && !is_stack2 {
                    (i, j, b1_name.clone(), b2_name.clone())
                } else if !is_stack1 && is_stack2 {
                    (j, i, b2_name.clone(), b1_name.clone())
                } else if p1 > p2 {
                    (i, j, b1_name.clone(), b2_name.clone())
                } else if p1 < p2 {
                    (j, i, b2_name.clone(), b1_name.clone())
                } else {
                    if start1 <= start2 {
                        (i, j, b1_name.clone(), b2_name.clone())
                    } else {
                        (j, i, b2_name.clone(), b1_name.clone())
                    }
                };

                disjoint_renames.push((merge_name.clone(), keep_name.clone()));

                current_locals[keep_idx].ty = unified_ty;

                let keep_origin = current_locals[keep_idx].origin;
                let merge_origin = current_locals[merge_idx].origin;
                if (keep_origin.is_none() || matches!(keep_origin, Some(NirBindingOrigin::Temp)))
                    && merge_origin.is_some()
                    && !matches!(merge_origin, Some(NirBindingOrigin::Temp))
                {
                    current_locals[keep_idx].origin = merge_origin;
                }

                let (k_start, k_end) = live_ranges
                    .ranges
                    .get(&keep_name)
                    .copied()
                    .unwrap_or((0, 0));
                let (m_start, m_end) = live_ranges
                    .ranges
                    .get(&merge_name)
                    .copied()
                    .unwrap_or((0, 0));
                live_ranges
                    .ranges
                    .insert(keep_name, (k_start.min(m_start), k_end.max(m_end)));

                let merged_members = merge_members[merge_idx].clone();
                merge_members[keep_idx].extend(merged_members);

                disjoint_merged[merge_idx] = true;
                changed = true;
                if merge_idx == i {
                    break;
                }
            }
        }
    }

    if !disjoint_renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &disjoint_renames);
        let renamed_from: crate::HashSet<String> = disjoint_renames
            .iter()
            .map(|(from, _)| from.clone())
            .collect();
        func.locals
            .retain(|local| !renamed_from.contains(&local.name));
        for local in &mut func.locals {
            if let Some(updated) = current_locals.iter().find(|l| l.name == local.name) {
                local.ty = updated.ty.clone();
            }
        }
    }

    changed
}

#[derive(Clone, Debug)]
struct VarMeta {
    name: String,
    start: usize,
    end: usize,
}
