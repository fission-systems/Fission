//! Loops inside a single instruction.
//!
//! Some SLEIGH constructors iterate: x86's bit scans (`BSR`, `BSF`, `TZCNT`,
//! `LZCNT`) step an index and branch back to an earlier op of the same
//! instruction until a bit is found. The instruction stays in one basic block,
//! so neither the CFG nor scalar SSA sees the back edge: SSA resolves every
//! read of the index to the definition before it in op order, and the loop
//! body runs once. `BSR` of any non-zero 32-bit value came out as 30.
//!
//! The loop is lowered here, where the block's ops become statements:
//! the region between the back edge's target and the back edge becomes
//! `while (1)`, a branch to the op after the back edge becomes `break`, and
//! each temporary carried around the loop lives in one variable that every
//! read inside the instruction refers to.
//!
//! Only a loop whose body writes nothing but temporaries is taken. A
//! register or memory write inside it would be read through SSA from outside
//! the instruction, and SSA cannot see the loop; such an instruction keeps the
//! old lowering.

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct InstructionLoop {
    /// First op of the loop body: the back edge's target.
    pub(crate) head: usize,
    /// The unconditional branch back to `head`.
    pub(crate) back: usize,
    /// Temporaries read in one iteration and written for the next, and the
    /// variable each one lives in.
    pub(crate) carried: HashMap<VarnodeKey, String>,
}

impl<'a> PreviewBuilder<'a> {
    /// Find every instruction loop of the function and name its carried
    /// temporaries.
    ///
    /// Runs once, when the builder is created. Structuring forks isolated
    /// builders that each allocate temps from the counter they inherited, so a
    /// name first handed out inside one fork can be handed out again by a
    /// sibling for an unrelated value; allocating here, before any fork,
    /// gives every fork the same names.
    pub(crate) fn prepare_instruction_loops(&mut self) {
        if !self.function_has_instruction_loops() {
            return;
        }
        for block_idx in 0..self.pcode.blocks.len() {
            let mut addresses: Vec<u64> = self.pcode.blocks[block_idx]
                .ops
                .iter()
                .map(|op| op.address)
                .collect();
            addresses.dedup();
            for address in addresses {
                let found = self.find_instruction_loop(block_idx, address);
                self.instruction_loops.insert((block_idx, address), found);
            }
        }
    }

    /// The loop of the instruction at `address` in `block_idx`, if it has one
    /// this lowering can take.
    pub(crate) fn instruction_loop(
        &mut self,
        block_idx: usize,
        address: u64,
    ) -> Option<InstructionLoop> {
        if !self.function_has_instruction_loops() {
            return None;
        }
        self.instruction_loops
            .get(&(block_idx, address))
            .cloned()
            .flatten()
    }

    /// The variable a loop-carried temporary of the instruction at `address`
    /// lives in.
    pub(crate) fn instruction_loop_var(
        &mut self,
        block_idx: usize,
        address: u64,
        vn: &Varnode,
    ) -> Option<String> {
        if !is_unique_space_id(vn.space_id) {
            return None;
        }
        self.instruction_loop(block_idx, address)?
            .carried
            .get(&VarnodeKey::from(vn))
            .cloned()
    }

    /// Whether any op of the function branches back inside its instruction;
    /// decided once so functions without such an instruction pay nothing.
    fn function_has_instruction_loops(&mut self) -> bool {
        if let Some(known) = self.instruction_loops_present {
            return known;
        }
        let present = self.pcode.blocks.iter().any(|block| {
            block.ops.iter().enumerate().any(|(idx, op)| {
                op.opcode == PcodeOpcode::Branch
                    && op.inputs.first().is_some_and(|target| {
                        crate::midend::cfg::same_block_backward_branch_target_op_idx(
                            block, idx, op, target,
                        )
                        .is_some()
                    })
            })
        });
        self.instruction_loops_present = Some(present);
        present
    }

    fn find_instruction_loop(&mut self, block_idx: usize, address: u64) -> Option<InstructionLoop> {
        let block = self.pcode.blocks.get(block_idx)?;
        let start = block.ops.iter().position(|op| op.address == address)?;
        let end = start
            + block.ops[start..]
                .iter()
                .take_while(|op| op.address == address)
                .count();
        let local_target = |pos: usize| -> Option<usize> {
            let op = &block.ops[pos];
            let seq =
                crate::midend::cfg::instruction_local_branch_target_seq(op, op.inputs.first()?)?;
            (start..end).find(|&candidate| block.ops[candidate].seq_num == seq)
        };

        // Exactly one back edge, an unconditional branch.
        let mut back_edges = (start..end).filter_map(|pos| {
            let op = &block.ops[pos];
            (op.opcode == PcodeOpcode::Branch && op.inputs.len() == 1)
                .then(|| local_target(pos))
                .flatten()
                .filter(|&target| target <= pos)
                .map(|head| (head, pos))
        });
        let (head, back) = back_edges.next()?;
        if back_edges.next().is_some() || back + 1 >= end {
            return None;
        }
        let exit = back + 1;

        let mut writes: HashMap<VarnodeKey, Vec<usize>> = HashMap::default();
        let mut reads: HashMap<VarnodeKey, Vec<usize>> = HashMap::default();
        for pos in head..back {
            let op = &block.ops[pos];
            match op.opcode {
                PcodeOpcode::CBranch => {
                    let target = local_target(pos)?;
                    if !(target > pos && target <= back || target == exit) {
                        return None;
                    }
                }
                PcodeOpcode::Branch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Call
                | PcodeOpcode::CallInd
                | PcodeOpcode::CallOther
                | PcodeOpcode::Return
                | PcodeOpcode::Store => return None,
                _ => {}
            }
            if let Some(output) = &op.output {
                if !is_unique_space_id(output.space_id) {
                    return None;
                }
                writes
                    .entry(VarnodeKey::from(output))
                    .or_default()
                    .push(pos);
            }
            for input in op
                .inputs
                .iter()
                .filter(|vn| is_unique_space_id(vn.space_id))
            {
                reads.entry(VarnodeKey::from(input)).or_default().push(pos);
            }
        }

        // Carried: some iteration reads the value a later op of the same
        // iteration writes, so the next iteration must see the new one.
        let mut carried_keys: Vec<(VarnodeKey, u32)> = writes
            .iter()
            .filter(|(key, written)| {
                reads.get(*key).is_some_and(|read| {
                    let last_write = written.iter().copied().max().unwrap_or(0);
                    read.iter().any(|&r| r <= last_write)
                })
            })
            .filter_map(|(key, written)| {
                let size = block.ops[written[0]].output.as_ref()?.size;
                Some((key.clone(), size))
            })
            .collect();
        if carried_keys.is_empty() {
            return None;
        }
        // Names are handed out in a fixed order so output is deterministic.
        carried_keys.sort_by_key(|(key, _)| (key.space_id, key.offset, key.size));

        let mut carried = HashMap::default();
        for (key, size) in carried_keys {
            let ty = type_from_size(size, false);
            let name = self.next_unused_temp_binding_name(&ty);
            self.temps.insert(
                name.clone(),
                PreHirBinding {
                    name: name.clone(),
                    ty,
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            );
            carried.insert(key, name);
        }
        Some(InstructionLoop {
            head,
            back,
            carried,
        })
    }
}
