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
        for loop_body in self
            .loop_bodies
            .iter()
            .filter(|loop_body| loop_body.body.contains(&block_idx))
        {
            let Some(phis) = self.scalar_ssa.phis.get(&(loop_body.head as u32)) else {
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
                if !self.loop_phi_output_read_before_redefinition(loop_body, &head_key) {
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
                        .get(definition.op as usize)?;
                    let definition_output = definition_op.output.as_ref()?;
                    let name = self.materialized_vns.get(&MaterializedVarnodeKey::new(
                        definition_output,
                        definition_op,
                    ))?;
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
