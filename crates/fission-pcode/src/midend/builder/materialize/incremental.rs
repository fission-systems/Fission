use crate::midend::builder::*;
use crate::midend::support::*;
use crate::midend::ir::*;
use crate::pcode::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn run_incremental_heritage(&mut self) {
        if !self.locals.is_empty() {
            return;
        }

        let mut rsp_accesses = Vec::new();
        let mut rbp_accesses = Vec::new();

        for (block_idx, block) in self.pcode.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                match op.opcode {
                    PcodeOpcode::Load => {
                        if op.inputs.len() < 2 {
                            continue;
                        }
                        let ptr = &op.inputs[1];
                        if let Some((base, offset)) = self
                            .resolve_stack_address_from_memory_op(op)
                            .or_else(|| self.resolve_stack_address(ptr))
                        {
                            let size = op.output.as_ref().map(|out| out.size).unwrap_or(0);
                            if size > 0 {
                                match base {
                                    StackBase::Rsp => rsp_accesses.push((offset, size)),
                                    StackBase::Rbp => rbp_accesses.push((offset, size)),
                                }
                            }
                        }
                    }
                    PcodeOpcode::Store => {
                        if op.inputs.len() < 3 {
                            continue;
                        }
                        if self.is_callee_saved_push_store(op)
                            || self.is_call_return_scaffold_store(block, op_idx, op)
                            || self.x86_32_store_is_recovered_call_arg(block, op_idx)
                        {
                            continue;
                        }
                        let ptr = &op.inputs[1];
                        if let Some((base, offset)) = self
                            .resolve_stack_address_from_memory_op(op)
                            .or_else(|| self.resolve_stack_address(ptr))
                        {
                            if let Some(val) = op.inputs.get(2) {
                                let size = val.size;
                                if size > 0 {
                                    match base {
                                        StackBase::Rsp => rsp_accesses.push((offset, size)),
                                        StackBase::Rbp => rbp_accesses.push((offset, size)),
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let refined_rsp = refine_partitions(&rsp_accesses);
        let refined_rbp = refine_partitions(&rbp_accesses);

        for (offset, size) in refined_rsp {
            self.register_refined_slot(StackBase::Rsp, offset, size);
        }

        for (offset, size) in refined_rbp {
            self.register_refined_slot(StackBase::Rbp, offset, size);
        }

        self.invalidate_materialization_dependent_caches();
    }

    fn register_refined_slot(&mut self, base: StackBase, offset: i64, size: u32) {
        let ty = type_from_size(size, false);
        let origin = self.classify_stack_slot_origin(base, offset);
        if let NirBindingOrigin::ParamIndex(index) = origin {
            self.ensure_incoming_stack_param_binding(index, ty);
            return;
        }
        let kind_name = match origin {
            NirBindingOrigin::HomeSlot(home_offset) => format!("home_{home_offset:x}"),
            NirBindingOrigin::OutgoingArgSlot(arg_offset) => format!("arg_out_{arg_offset:x}"),
            NirBindingOrigin::ReturnScaffold => format!("ret_scaffold_{:x}", offset.unsigned_abs()),
            _ => match base {
                StackBase::Rbp if offset > 0 => format!("param_{:x}", offset),
                StackBase::Rbp => format!("local_{:x}", offset.unsigned_abs()),
                StackBase::Rsp => format!("local_{:x}", self.rsp_local_display_offset(offset)),
            },
        };

        if self.locals.contains_key(&offset) {
            return;
        }

        let id = self.locals_next_id;
        self.locals_next_id += 1;
        let name = self.unique_stack_slot_binding_name(&kind_name, id);
        self.locals.insert(
            offset,
            StackSlot {
                id,
                name,
                ty,
                origin,
            },
        );
    }
}

pub(super) fn refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    if accesses.is_empty() {
        return Vec::new();
    }

    let mut boundaries = HashSet::default();
    for &(offset, size) in accesses {
        boundaries.insert(offset);
        boundaries.insert(offset + size as i64);
    }
    let mut sorted_boundaries: Vec<i64> = boundaries.into_iter().collect();
    sorted_boundaries.sort_unstable();

    let mut valid_boundaries = Vec::new();
    if let Some(&first) = sorted_boundaries.first() {
        valid_boundaries.push(first);
    }
    for &b in &sorted_boundaries[1..sorted_boundaries.len().saturating_sub(1)] {
        let mut spanned = false;
        for &(offset, size) in accesses {
            let access_start = offset;
            let access_end = offset + size as i64;
            if access_start < b && b < access_end {
                spanned = true;
                break;
            }
        }
        if !spanned {
            valid_boundaries.push(b);
        }
    }
    if sorted_boundaries.len() > 1 {
        if let Some(&last) = sorted_boundaries.last() {
            valid_boundaries.push(last);
        }
    }

    let mut intervals = Vec::new();
    for i in 0..valid_boundaries.len() - 1 {
        let start = valid_boundaries[i];
        let end = valid_boundaries[i + 1];
        let mut covered = false;
        for &(offset, size) in accesses {
            let access_start = offset;
            let access_end = offset + size as i64;
            if access_start <= start && end <= access_end {
                covered = true;
                break;
            }
        }
        if covered {
            intervals.push((start, end));
        }
    }

    let mut merged_intervals = Vec::new();
    let mut i = 0;
    while i < intervals.len() {
        if i + 1 < intervals.len() {
            let (start_a, end_a) = intervals[i];
            let (start_b, end_b) = intervals[i + 1];
            let size_a = end_a - start_a;
            let size_b = end_b - start_b;
            if ((size_a == 1 && size_b == 3) || (size_a == 3 && size_b == 1))
                && start_a.rem_euclid(4) == 0
            {
                merged_intervals.push((start_a, end_b));
                i += 2;
                continue;
            }
        }
        merged_intervals.push(intervals[i]);
        i += 1;
    }

    merged_intervals
        .into_iter()
        .map(|(start, end)| (start, (end - start) as u32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disjoint_partitions() {
        let accesses = vec![(0, 2), (2, 2)];
        let res = refine_partitions(&accesses);
        assert_eq!(res, vec![(0, 2), (2, 2)]);
    }

    #[test]
    fn test_spanned_partitions() {
        let accesses = vec![(0, 4), (0, 2), (2, 2)];
        let res = refine_partitions(&accesses);
        assert_eq!(res, vec![(0, 4)]);
    }

    #[test]
    fn test_1_3_merge_aligned() {
        let accesses = vec![(0, 1), (1, 3)];
        let res = refine_partitions(&accesses);
        assert_eq!(res, vec![(0, 4)]);

        let accesses2 = vec![(0, 3), (3, 1)];
        let res2 = refine_partitions(&accesses2);
        assert_eq!(res2, vec![(0, 4)]);
    }

    #[test]
    fn test_1_3_merge_unaligned() {
        let accesses = vec![(2, 1), (3, 3)];
        let res = refine_partitions(&accesses);
        assert_eq!(res, vec![(2, 1), (3, 3)]);
    }
}
