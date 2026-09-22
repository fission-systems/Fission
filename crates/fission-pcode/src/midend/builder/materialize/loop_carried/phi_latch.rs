use super::*;
use std::collections::BTreeSet;

impl<'a> PreviewBuilder<'a> {
    /// Name for a loop-body definition whose value reaches the loop head only
    /// through the backedge phi: a value reloaded at the latch and read at the
    /// top of the next iteration (`movzx edx, byte [key + i + 1]` read back as
    /// `movsx edx, dl`).
    ///
    /// `prove_loop_carried_register_update` cannot see this shape. It proves
    /// updates (`x = f(x)`), and here the head overwrites the register before
    /// the latch reloads it. Cross-block reads resolve by dominance, and a
    /// latch never dominates its head, so the head kept reading the preheader
    /// definition's name while the reload went to a name nothing read -- the
    /// loop compared every character against the first one.
    ///
    /// The scalar SSA already knows the answer exactly: this definition is a
    /// backedge operand of a loop-head phi, and the phi's entry operands carry
    /// a name. Taking that name makes the reload assign the variable the head
    /// reads. A read between the head's overwrite and the latch has a different
    /// SSA value, so nothing outside the phi's own class is renamed.
    pub(in crate::midend::builder) fn loop_head_phi_latch_binding_name(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        if !Self::is_loop_carried_register_update_candidate(output) {
            return None;
        }
        let block_idx = self.address_to_index.get(&block.start_address).copied()?;
        // Only a value that some real op reads through a phi needs a shared name.
        if !self.output_is_read_by_phi(Some(block_idx), op_idx) {
            return None;
        }
        let site = fission_midend_core::ir::SsaOpSite {
            block: block_idx as u32,
            op: op_idx as u32,
        };
        let pieces = self.scalar_ssa.operation_outputs.get(&site)?.clone();
        let mut chosen: Option<String> = None;
        let loop_bodies = self
            .loop_bodies
            .iter()
            .filter(|loop_body| loop_body.body.contains(&block_idx))
            .cloned()
            .collect::<Vec<_>>();
        for loop_body in loop_bodies {
            let Some(phis) = self.scalar_ssa.phis.get(&(loop_body.head as u32)).cloned() else {
                continue;
            };
            for phi in phis {
                let (latch, entry): (Vec<&fission_midend_core::ir::SsaPhiOperand>, Vec<_>) = phi
                    .operands
                    .iter()
                    .partition(|operand| loop_body.body.contains(&(operand.predecessor as usize)));
                let carried_by_this_definition = latch
                    .iter()
                    .any(|operand| pieces.iter().any(|piece| piece.value == operand.value));
                if entry.is_empty() || !carried_by_this_definition {
                    continue;
                }
                // Thread only when the head genuinely *consumes* the carried
                // value: it reads the phi's storage before redefining it. A
                // head that redefines the register first (e.g. `rax =
                // cur->next` in a list walk) merely reuses the register for an
                // unrelated value -- the phi is an artifact of shared storage,
                // not a carried scalar, and threading the latch onto the entry
                // name mis-binds it (observed rewriting the list unlink in
                // `___w64_mingwthr_remove_key_dtor` to read a global).
                let storage_varnode = Varnode {
                    space_id: phi.storage.space_id,
                    offset: phi.storage.offset,
                    size: phi.storage.size,
                    is_constant: false,
                    constant_val: 0,
                };
                let head_key = VarnodeKey::from(&storage_varnode);
                if !self.loop_phi_output_read_before_redefinition(&loop_body, &head_key) {
                    continue;
                }
                let mut names = BTreeSet::new();
                for operand in entry {
                    let value = self.scalar_ssa.value(operand.value)?;
                    let fission_midend_core::ir::SsaValueDefinition::Operation(definition) =
                        value.definition
                    else {
                        return None;
                    };
                    let definition_op = self
                        .pcode
                        .blocks
                        .get(definition.block as usize)?
                        .ops
                        .get(definition.op as usize)?
                        .clone();
                    let definition_output = definition_op.output.as_ref()?.clone();
                    let name = self.loop_phi_entry_binding_name(
                        definition,
                        &definition_op,
                        &definition_output,
                    )?;
                    names.insert(name.clone());
                }
                let mut names = names.into_iter();
                let (Some(name), None) = (names.next(), names.next()) else {
                    return None;
                };
                // The head read resolves to the entry name by dominance, so
                // threading the latch onto it is only sound when that name is a
                // private scalar. A `tmp_<addr>` name is an absolute-addressed
                // global's identity (a register that loaded a global inherits
                // the global's name); assigning the latch cursor to it would
                // overwrite the global's meaning -- observed corrupting
                // `___w64_mingwthr_remove_key_dtor`, where the entry value was
                // a load of `__mingwthr_cs_init`.
                if !Self::is_threadable_scalar_temp_name(&name) {
                    return None;
                }
                if chosen.as_ref().is_some_and(|previous| *previous != name) {
                    return None;
                }
                chosen = Some(name);
            }
        }
        chosen.filter(|name| self.temps.get(name.as_str()).is_some())
    }

    /// A partial-register update may be represented by a narrow definition
    /// followed immediately by a wider zero/sign-extension of the same
    /// storage. Scalar SSA can attach the loop-head phi to the wider
    /// definition, while the narrow definition is the one that the return
    /// path and the next arithmetic operation read. Reuse the wider
    /// definition's phi-latch binding instead of falling back to a hardware
    /// name for the narrow alias.
    pub(super) fn loop_head_phi_latch_binding_name_for_widened_alias(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        if output.is_constant || output.size >= self.options.pointer_size {
            return None;
        }
        let candidates = block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter_map(|(candidate_idx, candidate)| {
                if !matches!(
                    candidate.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                ) {
                    return None;
                }
                let input = candidate.inputs.first()?;
                let candidate_output = candidate.output.as_ref()?;
                if input.is_constant
                    || candidate_output.is_constant
                    || input.space_id != output.space_id
                    || input.offset != output.offset
                    || input.size != output.size
                    || candidate_output.space_id != output.space_id
                    || candidate_output.offset != output.offset
                    || candidate_output.size <= output.size
                {
                    return None;
                }
                Some((candidate_idx, candidate_output.clone()))
            })
            .collect::<Vec<_>>();

        candidates
            .into_iter()
            .find_map(|(candidate_idx, candidate_output)| {
                self.loop_head_phi_latch_binding_name(block, candidate_idx, &candidate_output)
            })
    }

    /// Resolve the name of a phi entry definition even when loop materialization
    /// reaches the latch before the defining block.  Ordinary materialization
    /// remains the source of truth: reserve its normal temporary binding and
    /// place the reservation in the same merge-name table used by later join
    /// recovery.  Proven entry parameters are deliberately not converted into
    /// temporaries; the caller's existing threadable-name gate rejects those
    /// formal names.
    pub(super) fn loop_carried_seed_binding_name(
        &mut self,
        output: &Varnode,
        loop_head: usize,
    ) -> Option<String> {
        let (definition, definition_op) = self.lookup_def_site(output)?;
        if self.current_lowering_site == Some(definition) {
            return None;
        }
        // A definition in the proven loop body is not the first-iteration
        // seed.  It is an in-loop passthrough/redefinition and may represent
        // an incoming formal value (for example a widening of ECX before its
        // shift update).  Reserving it as a private seed would incorrectly
        // outrank the ABI parameter binding.
        if self
            .loop_bodies
            .iter()
            .any(|body| body.head == loop_head && body.body.contains(&definition.block_idx))
        {
            return None;
        }
        let definition_output = definition_op.output.as_ref()?;
        if !self.varnode_aliases_value(definition_output, output)
            && !self.varnode_aliases_value(output, definition_output)
        {
            return None;
        }
        let name = self.loop_phi_entry_binding_name(
            fission_midend_core::ir::SsaOpSite {
                block: definition.block_idx as u32,
                op: definition.op_idx as u32,
            },
            &definition_op,
            definition_output,
        );
        name
    }

    fn loop_phi_entry_binding_name(
        &mut self,
        definition: fission_midend_core::ir::SsaOpSite,
        definition_op: &PcodeOp,
        definition_output: &Varnode,
    ) -> Option<String> {
        let materialized_key = MaterializedVarnodeKey::new(definition_output, definition_op);
        if let Some(name) = self.materialized_vns.get(&materialized_key).cloned() {
            return Some(name);
        }

        let merge_key = (
            definition.block as usize,
            VarnodeKey::from(definition_output),
        );
        if let Some(name) = self.explicit_merge_bindings.get(&merge_key).cloned()
            && self.temps.contains_key(&name)
        {
            return Some(name);
        }

        // A widening p-code op writes the same logical scalar as its narrow
        // input, even though scalar SSA represents the output as a fresh
        // value. Reserve the input definition's binding first so an entry
        // seed such as `r8d = value; r8 = zext(r8d)` cannot acquire a second
        // name when a loop latch is materialized before the seed block.
        if matches!(
            definition_op.opcode,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
        ) && let Some(input) = definition_op.inputs.first()
            && !input.is_constant
            && input.space_id == definition_output.space_id
            && input.offset == definition_output.offset
            && input.size < definition_output.size
        {
            let source_site = LoweringSite {
                block_idx: definition.block as usize,
                op_idx: definition.op as usize,
            };
            let (source_site, source_op) = self.with_lowering_site(source_site, |this| {
                this.lookup_def_site(input)
                    .map(|(site, op)| (site, op.clone()))
            })?;
            let source_output = source_op.output.as_ref()?.clone();
            if VarnodeKey::from(&source_output) != VarnodeKey::from(input) {
                return None;
            }
            let source_key = MaterializedVarnodeKey::new(&source_output, &source_op);
            let name = if let Some(name) = self.materialized_vns.get(&source_key).cloned() {
                name
            } else if let Some(name) = self
                .explicit_merge_bindings
                .get(&(source_site.block_idx, VarnodeKey::from(&source_output)))
                .cloned()
            {
                name
            } else if self
                .abi_state()
                .param_slot_for_varnode(&source_output)
                .is_some_and(|index| index < self.entry_arity)
                && !self.definition_has_internal_seed_input(&source_op, &source_output)
            {
                self.register_param(&source_output)?
            } else {
                self.ensure_temp_binding_for_output(&source_op, &source_output, true)
                    .name
            };
            self.materialized_vns.insert(materialized_key, name.clone());
            self.invalidate_materialization_dependent_caches();
            return Some(name);
        }

        if self
            .abi_state()
            .param_slot_for_varnode(definition_output)
            .is_some_and(|index| index < self.entry_arity)
            && !self.definition_has_internal_seed_input(definition_op, definition_output)
        {
            return self.register_param(definition_output);
        }

        let name = self
            .ensure_temp_binding_for_output(definition_op, definition_output, true)
            .name;
        self.explicit_merge_bindings
            .entry(merge_key)
            .or_insert_with(|| name.clone());
        self.invalidate_materialization_dependent_caches();
        Some(name)
    }

    fn definition_has_internal_seed_input(&self, op: &PcodeOp, output: &Varnode) -> bool {
        op.inputs.iter().any(|input| input.is_constant)
            || (!matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            ) && op
                .inputs
                .iter()
                .any(|input| self.varnode_aliases_value(input, output)))
    }

    /// Whether the value entering the loop head through the phi is genuinely
    /// consumed: following the CFG forward from the head, some op reads the
    /// storage before any op redefines (kills) it. A head that redefines the
    /// register before any read (a list walk's `rax = cur->next`) leaves the
    /// incoming phi value dead -- the phi is only an artifact of shared
    /// storage, and its entry name must not be threaded. The head block itself
    /// need not be the reader: the minimal case reads the carried character in
    /// the block *after* the head, still before the latch reloads it.
    fn loop_phi_output_read_before_redefinition(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        key: &VarnodeKey,
    ) -> bool {
        let mut queue = std::collections::VecDeque::from([loop_body.head]);
        let mut visited = HashSet::default();
        while let Some(block_idx) = queue.pop_front() {
            if !loop_body.body.contains(&block_idx) || !visited.insert(block_idx) {
                continue;
            }
            let Some(block) = self.pcode.blocks.get(block_idx) else {
                continue;
            };
            let mut killed = false;
            for op in &block.ops {
                if Self::op_reads_varnode_key(op, key) {
                    return true;
                }
                if Self::op_kills_varnode_definition(op, key) {
                    killed = true;
                    break;
                }
            }
            if killed {
                continue;
            }
            queue.extend(
                self.successors
                    .get(block_idx)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        false
    }

    /// A private, unaliased scalar temp -- the only name shape it is safe to
    /// thread a backedge value onto. Excludes `tmp_<addr>` (absolute-global
    /// identity), `param_`, hardware register names, and structured names,
    /// any of which can be independently live as something other than this
    /// loop's carried value.
    fn is_threadable_scalar_temp_name(name: &str) -> bool {
        ["uVar", "xVar", "iVar", "bVar", "fVar"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            && name[4..].bytes().all(|byte| byte.is_ascii_digit())
    }
}
