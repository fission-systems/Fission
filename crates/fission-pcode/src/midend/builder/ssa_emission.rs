//! Emitted identities for complete scalar phi storage. Heritage owns the
//! values; this plan owns only their names and placement of edge copies.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub(super) struct SsaEmissionPlan {
    pub bindings: BTreeMap<SsaValueId, String>,
    pub storages: BTreeSet<SsaStorageKey>,
    pub copies: BTreeMap<(usize, usize), Vec<SsaOutOfSsaCopy>>,
    operation_bindings: BTreeMap<SsaOpSite, String>,
    entry_values: BTreeMap<(usize, SsaStorageKey), SsaValueId>,
    conditional_edges: BTreeMap<usize, SsaConditionalCopies>,
    transported_phis: BTreeSet<SsaValueId>,
}

#[derive(Debug, Clone)]
struct SsaConditionalCopies {
    condition: PreHirExpr,
    snapshot: String,
    true_successor: usize,
    false_successor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SsaEmissionError {
    MissingBinding(SsaValueId),
    InvalidEdge(usize, usize),
    UnrelatedValuesShareBinding(SsaValueId, SsaValueId),
}

impl SsaEmissionPlan {
    fn validate(
        &self,
        ssa: &NirScalarSsa,
        successors: &[Vec<usize>],
    ) -> Result<(), SsaEmissionError> {
        let mut owners = BTreeMap::<&str, SsaValueId>::new();
        for (&id, name) in &self.bindings {
            let Some(value) = ssa.value(id) else {
                return Err(SsaEmissionError::MissingBinding(id));
            };
            if let Some(&previous) = owners.get(name.as_str()) {
                let prior = ssa.value(previous).expect("validated binding owner");
                if prior.definition == SsaValueDefinition::Input
                    || value.definition == SsaValueDefinition::Input
                    || prior.storage != value.storage
                    || ssa.value_high_variables[previous.0 as usize]
                        != ssa.value_high_variables[id.0 as usize]
                {
                    return Err(SsaEmissionError::UnrelatedValuesShareBinding(previous, id));
                }
            } else {
                owners.insert(name, id);
            }
        }
        for copy in ssa.out_of_ssa_copies.iter().filter(|copy| {
            self.storages.contains(&copy.storage)
                && self.transported_phis.contains(&copy.destination)
        }) {
            let edge = (copy.predecessor as usize, copy.successor as usize);
            if !successors
                .get(edge.0)
                .is_some_and(|targets| targets.contains(&edge.1))
                || !self
                    .copies
                    .get(&edge)
                    .is_some_and(|copies| copies.contains(copy))
            {
                return Err(SsaEmissionError::InvalidEdge(edge.0, edge.1));
            }
            for id in [copy.source, copy.destination] {
                if !self.bindings.contains_key(&id) {
                    return Err(SsaEmissionError::MissingBinding(id));
                }
            }
            if successors[edge.0].len() != 1
                && self.bindings[&copy.source] != self.bindings[&copy.destination]
            {
                let Some(branch) = self.conditional_edges.get(&edge.0) else {
                    return Err(SsaEmissionError::InvalidEdge(edge.0, edge.1));
                };
                if successors[edge.0].len() != 2
                    || branch.true_successor == branch.false_successor
                    || !successors[edge.0].contains(&branch.true_successor)
                    || !successors[edge.0].contains(&branch.false_successor)
                {
                    return Err(SsaEmissionError::InvalidEdge(edge.0, edge.1));
                }
            }
        }
        Ok(())
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn ssa_conditional_snapshot(&self, block: usize) -> Option<&str> {
        self.ssa_emission
            .conditional_edges
            .get(&block)
            .map(|branch| branch.snapshot.as_str())
    }

    pub(super) fn prepare_ssa_emission(&mut self) {
        let mut trial = self.clone();
        let result = with_isolated_register_origins(|| {
            Ok::<_, MlilPreviewError>(trial.try_prepare_ssa_emission().then_some(trial))
        });
        if let Ok(Some(trial)) = result {
            *self = trial;
        }
    }

    fn try_prepare_ssa_emission(&mut self) -> bool {
        if !self.irreducible_edges.is_empty()
            || !self.virtual_block_map.is_empty()
            || self.successors != self.heritage_successors
            || super::scalar_ssa::validate_scalar_ssa_with_context(
                self.pcode,
                &self.heritage_successors,
                &self.heritage_predecessors,
                &self.scalar_ssa,
                self.options,
                self.type_context,
            )
            .is_err()
        {
            return false;
        }
        self.ssa_emission.transported_phis = self
            .scalar_ssa
            .phis
            .values()
            .flatten()
            .filter(|phi| {
                self.ssa_phi_has_operand_use(phi.output, &mut BTreeSet::new())
                    || self.ssa_storage_is_return_piece(phi.storage)
            })
            .map(|phi| phi.output)
            .collect();
        let mut candidates: BTreeSet<_> = self
            .scalar_ssa
            .phis
            .values()
            .flat_map(|phis| phis.iter())
            .filter(|phi| self.ssa_phi_has_operand_use(phi.output, &mut BTreeSet::new()))
            .map(|phi| phi.storage)
            .collect();
        // Entry and later definitions have distinct identities even without
        // a join. A phi is a transport obligation, not the condition under
        // which an immutable ABI input becomes a different value.
        candidates.extend(
            self.scalar_ssa
                .inputs
                .iter()
                .filter_map(|(&storage, &input)| {
                    (self.ssa_value_has_observable_use(input)
                // An input/return ABI-slot overlap also has implicit return
                // reads owned by return recovery. That complete correspondence
                // is not part of the no-phi input path yet.
                && !self.register_namer().is_primary_return_register(&ssa_varnode(storage))
                        && self.scalar_ssa.values.iter().any(|value| {
                            value.storage == storage
                                && matches!(value.definition, SsaValueDefinition::Operation(_))
                        }))
                    .then_some(storage)
                }),
        );
        // A view may use several disjoint pieces. Admit its entire connected
        // storage family, or keep every view on the legacy path.
        loop {
            let old = candidates.len();
            for pieces in self
                .scalar_ssa
                .operation_inputs
                .values()
                .chain(self.scalar_ssa.operation_outputs.values())
            {
                if pieces.iter().any(|piece| {
                    candidates.contains(&self.scalar_ssa.values[piece.value.0 as usize].storage)
                }) {
                    candidates.extend(
                        pieces
                            .iter()
                            .map(|piece| self.scalar_ssa.values[piece.value.0 as usize].storage),
                    );
                }
            }
            if old == candidates.len() {
                break;
            }
        }
        candidates.retain(|storage| self.ssa_storage_can_be_emitted(*storage));
        loop {
            let old = candidates.len();
            for pieces in self
                .scalar_ssa
                .operation_inputs
                .values()
                .chain(self.scalar_ssa.operation_outputs.values())
            {
                if pieces.iter().any(|piece| {
                    !candidates.contains(&self.scalar_ssa.values[piece.value.0 as usize].storage)
                }) {
                    for piece in pieces {
                        candidates.remove(&self.scalar_ssa.values[piece.value.0 as usize].storage);
                    }
                }
            }
            if old == candidates.len() {
                break;
            }
        }
        for storage in candidates {
            let values: Vec<_> = self
                .scalar_ssa
                .values
                .iter()
                .filter(|value| value.storage == storage)
                .cloned()
                .collect();
            let mut group_bindings = BTreeMap::new();
            for value in values {
                if matches!(value.definition, SsaValueDefinition::Phi { .. })
                    && !self.ssa_emission.transported_phis.contains(&value.id)
                {
                    continue;
                }
                let name = if value.definition == SsaValueDefinition::Input {
                    if !self.ssa_value_has_observable_use(value.id) {
                        continue;
                    }
                    // Admission established a genuine ABI input; a scratch
                    // register's first read is never invented as a formal.
                    self.register_param(&ssa_varnode(storage))
                        .expect("validated entry input")
                } else {
                    let high = self.scalar_ssa.value_high_variables[value.id.0 as usize];
                    if let Some(name) = group_bindings.get(&high).cloned() {
                        self.ssa_emission.bindings.insert(value.id, name);
                        continue;
                    }
                    let ty = ssa_piece_type(storage.size);
                    let preserves_load = self.scalar_ssa.values.iter().any(|member| {
                        member.storage == storage
                            && self.scalar_ssa.value_high_variables[member.id.0 as usize] == high
                            && matches!(member.definition, SsaValueDefinition::Operation(site)
                                if self.pcode.blocks[site.block as usize].ops[site.op as usize].opcode == PcodeOpcode::Load)
                    });
                    let name = if group_bindings.is_empty() {
                        self.sla_hw_name(storage.offset, storage.size)
                            .filter(|name| !self.binding_name_exists(name))
                            .unwrap_or_else(|| self.next_unused_temp_binding_name(&ty))
                    } else {
                        self.next_unused_temp_binding_name(&ty)
                    };
                    self.temps.insert(
                        name.clone(),
                        PreHirBinding {
                            name: name.clone(),
                            ty,
                            surface_type_name: None,
                            origin: Some(if preserves_load {
                                NirBindingOrigin::TempPreserved
                            } else {
                                NirBindingOrigin::Temp
                            }),
                            initializer: None,
                        },
                    );
                    record_register_origin(&name, storage.offset, storage.size);
                    group_bindings.insert(high, name.clone());
                    name
                };
                self.ssa_emission.bindings.insert(value.id, name);
            }
            self.ssa_emission.storages.insert(storage);
        }
        for (site, pieces) in self.scalar_ssa.operation_outputs.clone() {
            if pieces.len() <= 1
                || !pieces
                    .iter()
                    .all(|piece| self.ssa_emission.bindings.contains_key(&piece.value))
            {
                continue;
            }
            let output = self.pcode.blocks[site.block as usize].ops[site.op as usize]
                .output
                .as_ref()
                .expect("validated output");
            let ty = type_from_size(output.size, false);
            let name = self.next_unused_temp_binding_name(&ty);
            self.temps.insert(
                name.clone(),
                PreHirBinding {
                    name: name.clone(),
                    ty,
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::TempPreserved),
                    initializer: None,
                },
            );
            self.ssa_emission.operation_bindings.insert(site, name);
        }
        for copy in &self.scalar_ssa.out_of_ssa_copies {
            if self.ssa_emission.storages.contains(&copy.storage)
                && self
                    .ssa_emission
                    .transported_phis
                    .contains(&copy.destination)
            {
                self.ssa_emission
                    .copies
                    .entry((copy.predecessor as usize, copy.successor as usize))
                    .or_default()
                    .push(*copy);
            }
        }
        if self.ssa_emission.storages.is_empty() {
            return false;
        }
        self.prepare_ssa_entry_values();
        self.terminator_cache.clear();
        if !self.prepare_ssa_conditional_copies() {
            return false;
        }
        if self
            .ssa_emission
            .validate(&self.scalar_ssa, &self.heritage_successors)
            .is_err()
        {
            // No emitted statement may observe a partially admitted plan.
            return false;
        }
        // Validate the actual lowering, including every planned definition's
        // RHS, on the isolated builder. Unsupported materialization must not
        // escape as a partially applied naming/copy plan.
        let mut probe = self.clone();
        let pcode = self.pcode;
        let valid = with_discarded_register_origins(|| {
            let mut bodies = Vec::new();
            let mut terminators = Vec::new();
            for block in &pcode.blocks {
                bodies.push(probe.lower_block_stmts(block)?);
                terminators.push(probe.lower_block_terminator(block.index as usize)?);
            }
            Ok::<_, MlilPreviewError>(
                probe.ssa_emitted_bindings_are_initialized(&bodies, &terminators),
            )
        });
        if !matches!(valid, Ok(true)) {
            return false;
        }
        if super::debug::preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] SSA emission: storages={} bindings={} copy_edges={}",
                self.ssa_emission.storages.len(),
                self.ssa_emission.bindings.len(),
                self.ssa_emission.copies.len()
            );
        }
        !self.ssa_emission.storages.is_empty()
    }

    fn ssa_emitted_bindings_are_initialized(
        &self,
        bodies: &[Vec<PreHirStmt>],
        terminators: &[LoweredTerminator],
    ) -> bool {
        let tracked: BTreeSet<_> = self.temps.keys().cloned().collect();
        let seed: BTreeSet<_> = self
            .params
            .values()
            .map(|binding| binding.name.clone())
            .chain(
                self.temps
                    .values()
                    .filter(|binding| binding.initializer.is_some())
                    .map(|binding| binding.name.clone()),
            )
            .collect();
        let mut reachable = BTreeSet::new();
        let mut pending = vec![0];
        while let Some(block) = pending.pop() {
            if reachable.insert(block) {
                pending.extend(self.heritage_successors[block].iter().copied());
            }
        }
        let mut edge_states = BTreeMap::new();
        for &pred in &reachable {
            for &succ in &self.heritage_successors[pred] {
                edge_states.insert((pred, succ), tracked.clone());
            }
        }
        let incoming = |block: usize, edges: &BTreeMap<(usize, usize), BTreeSet<String>>| {
            if block == 0 {
                return seed.clone();
            }
            let mut state = tracked.clone();
            for &pred in &self.heritage_predecessors[block] {
                if reachable.contains(&pred) {
                    state.retain(|name| edges[&(pred, block)].contains(name));
                }
            }
            state.extend(seed.iter().cloned());
            state
        };
        loop {
            let mut changed = false;
            for &block in &reachable {
                let input = incoming(block, &edge_states);
                for &succ in &self.heritage_successors[block] {
                    let mut state = input.clone();
                    let decision = self
                        .ssa_emission
                        .conditional_edges
                        .get(&block)
                        .map(|branch| (branch.snapshot.as_str(), succ == branch.true_successor));
                    if !ssa_transfer_definitions(
                        &bodies[block],
                        &mut state,
                        &tracked,
                        decision,
                        false,
                    ) {
                        return false;
                    }
                    let edge = edge_states.get_mut(&(block, succ)).expect("reachable edge");
                    if *edge != state {
                        *edge = state;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for &block in &reachable {
            let input = incoming(block, &edge_states);
            let successors = &self.heritage_successors[block];
            for successor in successors
                .iter()
                .copied()
                .map(Some)
                .chain(successors.is_empty().then_some(None))
            {
                let mut state = input.clone();
                let decision = self
                    .ssa_emission
                    .conditional_edges
                    .get(&block)
                    .and_then(|branch| {
                        successor
                            .map(|succ| (branch.snapshot.as_str(), succ == branch.true_successor))
                    });
                if !ssa_transfer_definitions(&bodies[block], &mut state, &tracked, decision, true) {
                    if super::debug::preview_builder_diag_enabled() {
                        eprintln!(
                            "[DIAG] SSA initialization declined block={block} edge={successor:?} incoming={input:?} body={:?}",
                            bodies[block]
                        );
                    }
                    return false;
                }
                let expr = match &terminators[block] {
                    LoweredTerminator::Cond { cond, .. } => Some(cond),
                    LoweredTerminator::Switch { expr, .. } => Some(expr),
                    LoweredTerminator::Return(expr) => expr.as_ref(),
                    LoweredTerminator::Unsupported { target_expr, .. } => target_expr.as_ref(),
                    _ => None,
                };
                if expr.is_some_and(|expr| !ssa_expr_initialized(expr, &state, &tracked)) {
                    if super::debug::preview_builder_diag_enabled() {
                        eprintln!(
                            "[DIAG] SSA terminator initialization declined block={block} state={state:?} term={:?}",
                            terminators[block]
                        );
                    }
                    return false;
                }
            }
        }
        true
    }

    fn ssa_storage_is_return_piece(&self, storage: SsaStorageKey) -> bool {
        self.register_namer()
            .primary_return_registers()
            .iter()
            .any(|vn| {
                vn.space_id == storage.space_id
                    && vn.offset <= storage.offset
                    && storage.offset.saturating_add(u64::from(storage.size))
                        <= vn.offset.saturating_add(u64::from(vn.size))
            })
    }

    fn ssa_storage_can_be_emitted(&self, storage: SsaStorageKey) -> bool {
        macro_rules! reject {
            () => {{
                if super::debug::preview_builder_diag_enabled() {
                    eprintln!(
                        "[DIAG] SSA admission: function={:?} storage={:?} declined_at={}",
                        self.current_function_name,
                        storage,
                        line!()
                    );
                }
                return false;
            }};
        }
        let vn = ssa_varnode(storage);
        // Heritage phi operands enumerate CFG predecessors, not the implicit
        // function-entry edge. Until that edge has a typed incoming value,
        // an entry-block phi cannot initialize an emitted carrier.
        if self
            .scalar_ssa
            .phis
            .get(&0)
            .is_some_and(|phis| phis.iter().any(|phi| phi.storage == storage))
        {
            reject!();
        }
        if !is_register_varnode(&vn)
            || self.output_is_stack_pointer_register(&vn)
            || !(1..=8).contains(&storage.size)
        {
            reject!();
        }
        let has_calls = self.pcode.blocks.iter().any(|block| {
            block.ops.iter().any(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
            })
        });
        if has_calls {
            let preserved = self
                .pcode
                .blocks
                .iter()
                .flat_map(|block| &block.ops)
                .flat_map(|op| op.inputs.iter().chain(op.output.iter()))
                .any(|access| {
                    access.space_id == storage.space_id
                        && access.offset <= storage.offset
                        && storage.offset.saturating_add(u64::from(storage.size))
                            <= access.offset.saturating_add(u64::from(access.size))
                        && self
                            .options
                            .cspec_unaffected_offsets
                            .contains(&access.offset)
                });
            if !preserved || self.register_namer().is_primary_return_register(&vn) {
                reject!();
            }
        }
        for value in self
            .scalar_ssa
            .values
            .iter()
            .filter(|value| value.storage == storage)
        {
            match value.definition {
                SsaValueDefinition::Input if self.ssa_value_has_observable_use(value.id) => {
                    if self.suppress_entry_register_params
                        || !self
                            .abi_state()
                            .param_slot_for_varnode(&vn)
                            .is_some_and(|slot| slot < self.named_entry_param_arity())
                    {
                        reject!();
                    }
                    // Named object layouts are still attached to the ABI
                    // binding by the existing type-hint owner. A new mutable
                    // carrier needs value-specific layout/field provenance;
                    // storage identity alone does not prove that transfer.
                    // Keep that family on the established path until its
                    // complete type correspondence can be validated.
                    let has_object_layout = self
                        .abi_state()
                        .param_slot_for_varnode(&vn)
                        .and_then(|slot| {
                            self.type_context
                                .and_then(|context| context.function_hints.as_ref())
                                .and_then(|hints| hints.param_type_names.get(&slot))
                        })
                        .and_then(|name| {
                            super::type_hints::struct_base_name_for_single_pointer(name)
                        })
                        .is_some_and(|name| {
                            self.type_context
                                .is_some_and(|context| context.struct_types.contains_key(name))
                        });
                    if has_object_layout {
                        reject!();
                    }
                }
                SsaValueDefinition::Operation(site) => {
                    let block = &self.pcode.blocks[site.block as usize];
                    let op = &block.ops[site.op as usize];
                    if !op.output.as_ref().is_some_and(|out| {
                        out.space_id == storage.space_id
                            && out.offset <= storage.offset
                            && matches!(out.size, 1 | 2 | 4 | 8)
                            && storage.offset.saturating_add(u64::from(storage.size))
                                <= out.offset.saturating_add(u64::from(out.size))
                    }) || self.op_is_inside_same_block_forward_cmov_body(block, site.op as usize)
                        || matches!(
                            pcode_output_type_from_size(op.opcode, storage.size),
                            NirType::Float { .. }
                        )
                        || matches!(
                            op.opcode,
                            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                        )
                    {
                        reject!();
                    }
                }
                _ => {}
            }
        }
        for copy in self.scalar_ssa.out_of_ssa_copies.iter().filter(|copy| {
            copy.storage == storage
                && self
                    .ssa_emission
                    .transported_phis
                    .contains(&copy.destination)
        }) {
            let pred = copy.predecessor as usize;
            let Some(successors) = self.heritage_successors.get(pred) else {
                reject!();
            };
            if successors.len() != 1 {
                // A non-coalesced move needs a canonical two-way CFG branch.
                // Its expression and target correspondence are preflighted
                // on the isolated builder before any plan is applied.
                let source = self
                    .scalar_ssa
                    .value(copy.source)
                    .expect("validated source");
                if (source.definition == SsaValueDefinition::Input
                    || self.scalar_ssa.value_high_variables[copy.source.0 as usize]
                        != self.scalar_ssa.value_high_variables[copy.destination.0 as usize])
                    && !self.ssa_conditional_copy_shape(pred)
                {
                    reject!();
                }
            }
        }
        true
    }

    fn ssa_conditional_copy_shape(&self, block: usize) -> bool {
        let Some(pcode) = self.pcode.blocks.get(block) else {
            return false;
        };
        self.heritage_successors[block].len() == 2
            && self.block_terminator_index(pcode).is_some_and(|index| {
                index + 1 == pcode.ops.len()
                    && pcode.ops[index].opcode == PcodeOpcode::CBranch
                    && pcode.ops[index].inputs.len() == 2
            })
    }

    fn prepare_ssa_conditional_copies(&mut self) -> bool {
        let blocks: BTreeSet<_> = self
            .ssa_emission
            .copies
            .iter()
            .filter(|((pred, _), copies)| {
                self.heritage_successors[*pred].len() != 1
                    && copies.iter().any(|copy| {
                        self.ssa_emission.bindings.get(&copy.source)
                            != self.ssa_emission.bindings.get(&copy.destination)
                    })
            })
            .map(|(&(pred, _), _)| pred)
            .collect();
        for block in blocks {
            if !self.ssa_conditional_copy_shape(block) {
                return false;
            }
            // Establish original-site materializations before recovering the
            // condition. Recovering it first can inline an unselected load
            // that the block subsequently materializes, evaluating it twice.
            let pcode = self.pcode;
            let Ok(mut body) = self.lower_block_stmts(&pcode.blocks[block]) else {
                return false;
            };
            self.terminator_cache.remove(&block);
            let Ok(LoweredTerminator::Cond {
                cond,
                true_target,
                false_target: Some(false_target),
            }) = self.lower_block_terminator(block)
            else {
                return false;
            };
            let successors = &self.heritage_successors[block];
            let true_successor = successors
                .iter()
                .copied()
                .find(|&target| self.block_target_key(target) == true_target);
            let false_successor = successors
                .iter()
                .copied()
                .find(|&target| self.block_target_key(target) == false_target);
            let (Some(true_successor), Some(false_successor)) = (true_successor, false_successor)
            else {
                return false;
            };
            if true_successor == false_successor {
                return false;
            }
            let snapshot = self.next_unused_temp_binding_name(&NirType::Bool);
            self.temps.insert(
                snapshot.clone(),
                PreHirBinding {
                    name: snapshot.clone(),
                    ty: NirType::Bool,
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::TempPreserved),
                    initializer: None,
                },
            );
            self.ssa_emission.conditional_edges.insert(
                block,
                SsaConditionalCopies {
                    condition: cond,
                    snapshot: snapshot.clone(),
                    true_successor,
                    false_successor,
                },
            );
            // Structuring and edge copies consume the same captured decision.
            // A move may overwrite an operand of the original condition.
            self.terminator_cache.insert(
                block,
                LoweredTerminator::Cond {
                    cond: PreHirExpr::Var(snapshot),
                    true_target,
                    false_target: Some(false_target),
                },
            );
            if self.append_ssa_edge_copies(block, &mut body).is_err() {
                return false;
            }
            self.lowered_block_stmts_cache.insert(block, body);
        }
        true
    }

    fn ssa_value_has_observable_use(&self, value: SsaValueId) -> bool {
        self.ssa_value_has_observable_use_inner(value, &mut BTreeSet::new())
    }

    fn ssa_phi_has_operand_use(
        &self,
        value: SsaValueId,
        visiting: &mut BTreeSet<SsaValueId>,
    ) -> bool {
        if !visiting.insert(value) {
            return false;
        }
        if self
            .scalar_ssa
            .operation_inputs
            .values()
            .any(|pieces| pieces.iter().any(|piece| piece.value == value))
        {
            return true;
        }
        self.scalar_ssa.out_of_ssa_copies.iter().any(|copy| {
            copy.source == value && self.ssa_phi_has_operand_use(copy.destination, visiting)
        })
    }

    fn ssa_value_has_observable_use_inner(
        &self,
        value: SsaValueId,
        visiting: &mut BTreeSet<SsaValueId>,
    ) -> bool {
        if !visiting.insert(value) {
            return false;
        }
        if self.scalar_ssa.out_of_ssa_copies.iter().any(|copy| {
            copy.source == value
                && self
                    .ssa_emission
                    .transported_phis
                    .contains(&copy.destination)
        }) {
            visiting.remove(&value);
            return true;
        }
        let observable = self
            .scalar_ssa
            .operation_inputs
            .iter()
            .any(|(site, pieces)| {
                if !pieces.iter().any(|piece| piece.value == value) {
                    return false;
                }
                let op = &self.pcode.blocks[site.block as usize].ops[site.op as usize];
                if self.is_callee_saved_push_store(op) {
                    return false;
                }
                if matches!(op.opcode, PcodeOpcode::Copy | PcodeOpcode::Cast) {
                    if let Some(outputs) = self.scalar_ssa.operation_outputs.get(&SsaOpSite {
                        block: site.block,
                        op: site.op,
                    }) {
                        return outputs.iter().any(|piece| {
                            self.ssa_value_has_observable_use_inner(piece.value, visiting)
                        });
                    }
                }
                true
            });
        visiting.remove(&value);
        observable
    }

    fn prepare_ssa_entry_values(&mut self) {
        let storages: Vec<_> = self.ssa_emission.storages.iter().copied().collect();
        loop {
            let old = self.ssa_emission.entry_values.len();
            for block in 0..self.pcode.blocks.len() {
                for &storage in &storages {
                    if self
                        .ssa_emission
                        .entry_values
                        .contains_key(&(block, storage))
                    {
                        continue;
                    }
                    let phi = self
                        .scalar_ssa
                        .phis
                        .get(&(block as u32))
                        .and_then(|phis| phis.iter().find(|phi| phi.storage == storage));
                    let incoming = if let Some(phi) = phi {
                        Some(phi.output)
                    } else if block == 0 {
                        self.scalar_ssa.inputs.get(&storage).copied()
                    } else {
                        let predecessors = &self.heritage_predecessors[block];
                        let values = predecessors
                            .iter()
                            .map(|&pred| {
                                self.ssa_emitted_value_at_point(
                                    storage,
                                    LoweringSite {
                                        block_idx: pred,
                                        op_idx: self.pcode.blocks[pred].ops.len(),
                                    },
                                )
                            })
                            .collect::<Option<Vec<_>>>();
                        values.and_then(|values| {
                            values
                                .first()
                                .copied()
                                .filter(|first| values.iter().all(|value| value == first))
                        })
                    };
                    if let Some(value) = incoming {
                        self.ssa_emission
                            .entry_values
                            .insert((block, storage), value);
                    }
                }
            }
            if old == self.ssa_emission.entry_values.len() {
                break;
            }
        }
    }

    fn ssa_emitted_value_at_point(
        &self,
        storage: SsaStorageKey,
        site: LoweringSite,
    ) -> Option<SsaValueId> {
        for op in (0..site.op_idx).rev() {
            if let Some(piece) = self
                .scalar_ssa
                .operation_outputs
                .get(&SsaOpSite {
                    block: site.block_idx as u32,
                    op: op as u32,
                })
                .and_then(|pieces| {
                    pieces.iter().find(|piece| {
                        self.scalar_ssa.values[piece.value.0 as usize].storage == storage
                    })
                })
            {
                return Some(piece.value);
            }
        }
        self.ssa_emission
            .entry_values
            .get(&(site.block_idx, storage))
            .copied()
    }

    pub(super) fn ssa_emitted_read(&self, vn: &Varnode) -> Option<PreHirExpr> {
        let site = self.current_lowering_site?;
        let block = self.pcode_block_idx(site.block_idx);
        let operation = self.pcode.blocks.get(block)?.ops.get(site.op_idx);
        let pieces = operation
            .and_then(|op| {
                op.inputs
                    .iter()
                    .position(|input| VarnodeKey::from(input) == VarnodeKey::from(vn))
            })
            .and_then(|input| {
                self.scalar_ssa.operation_inputs.get(&SsaUseSite {
                    block: block as u32,
                    op: site.op_idx as u32,
                    input: input as u32,
                })
            })
            .cloned();
        let pieces = pieces.or_else(|| {
            // Synthetic ABI/return reads must have an exact reaching SSA
            // identity as well. A missing phi for an implicit join read is
            // ambiguous and never becomes a "nearest definition" guess.
            let mut pieces = Vec::new();
            let mut offset = 0;
            while offset < vn.size {
                let storage = self.ssa_emission.storages.iter().find(|storage| {
                    storage.space_id == vn.space_id
                        && storage.offset == vn.offset + u64::from(offset)
                        && storage.size <= vn.size - offset
                })?;
                let value = self.ssa_emitted_value_at_point(
                    *storage,
                    LoweringSite {
                        block_idx: block,
                        op_idx: site.op_idx,
                    },
                )?;
                pieces.push(SsaAccessPiece {
                    value,
                    byte_offset: offset,
                });
                offset += storage.size;
            }
            Some(pieces)
        })?;
        if pieces.is_empty() || vn.size > 8 {
            return None;
        }
        // A contained view of one complete producer can read its original
        // snapshot. Splitting and recombining the same value loses useful
        // width evidence without adding a new identity.
        let first = self.scalar_ssa.value(pieces[0].value)?;
        if let SsaValueDefinition::Operation(producer) = first.definition
            && pieces.iter().all(|piece| {
                self.scalar_ssa.values[piece.value.0 as usize].definition == first.definition
            })
            && let Some(output) = self.pcode.blocks[producer.block as usize].ops
                [producer.op as usize]
                .output
                .as_ref()
                .filter(|output| {
                    output.space_id == vn.space_id
                        && output.offset <= vn.offset
                        && vn
                            .offset
                            .checked_add(u64::from(vn.size))
                            .is_some_and(|end| {
                                output
                                    .offset
                                    .checked_add(u64::from(output.size))
                                    .is_some_and(|limit| end <= limit)
                            })
                })
            && let Some(name) = self.ssa_emission.operation_bindings.get(&producer)
        {
            let raw = PreHirExpr::Var(name.clone());
            if VarnodeKey::from(output) == VarnodeKey::from(vn) {
                return Some(raw);
            }
            let shift =
                self.ssa_piece_shift(output.size, (vn.offset - output.offset) as u32, vn.size);
            let view_size = (shift / 8 + vn.size).next_power_of_two();
            let viewed = PreHirExpr::Cast {
                ty: type_from_size(view_size, false),
                expr: Box::new(raw),
            };
            let shifted = if shift == 0 {
                viewed
            } else {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(viewed),
                    rhs: Box::new(PreHirExpr::Const(
                        i64::from(shift),
                        type_from_size(view_size, false),
                    )),
                    ty: type_from_size(view_size, false),
                }
            };
            return Some(ssa_piece_view(shifted, vn.size));
        }
        let ty = type_from_size(vn.size, false);
        let mut combined = None;
        for piece in &pieces {
            let value = self.scalar_ssa.value(piece.value)?;
            let name = self.ssa_emission.bindings.get(&piece.value)?;
            if pieces.len() == 1 && value.storage.size == vn.size {
                return Some(PreHirExpr::Var(name.clone()));
            }
            let narrowed = ssa_piece_view(PreHirExpr::Var(name.clone()), value.storage.size);
            let widened = PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(narrowed),
            };
            let shift = self.ssa_piece_shift(vn.size, piece.byte_offset, value.storage.size);
            let positioned = if shift == 0 {
                widened
            } else {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shl,
                    lhs: Box::new(widened),
                    rhs: Box::new(PreHirExpr::Const(i64::from(shift), ty.clone())),
                    ty: ty.clone(),
                }
            };
            combined = Some(match combined {
                None => positioned,
                Some(lhs) => PreHirExpr::Binary {
                    op: PreHirBinaryOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(positioned),
                    ty: ty.clone(),
                },
            });
        }
        combined
    }

    pub(super) fn ssa_emitted_definition(&self, block: usize, op: usize) -> Option<String> {
        let site = SsaOpSite {
            block: block as u32,
            op: op as u32,
        };
        if let Some(name) = self.ssa_emission.operation_bindings.get(&site) {
            return Some(name.clone());
        }
        let pieces = self.scalar_ssa.operation_outputs.get(&site)?;
        let [piece] = pieces.as_slice() else {
            return None;
        };
        self.ssa_emission.bindings.get(&piece.value).cloned()
    }

    fn ssa_piece_shift(&self, size: u32, offset: u32, piece_size: u32) -> u32 {
        let bytes = if self.options.is_big_endian {
            size.saturating_sub(offset.saturating_add(piece_size))
        } else {
            offset
        };
        bytes * 8
    }

    pub(super) fn ssa_definition_piece_stmts(
        &self,
        block: usize,
        op: usize,
        name: &str,
    ) -> Vec<PreHirStmt> {
        let site = SsaOpSite {
            block: block as u32,
            op: op as u32,
        };
        let Some(pieces) = self
            .scalar_ssa
            .operation_outputs
            .get(&site)
            .filter(|pieces| pieces.len() > 1)
        else {
            return Vec::new();
        };
        let size = self.pcode.blocks[block].ops[op]
            .output
            .as_ref()
            .expect("validated output")
            .size;
        pieces
            .iter()
            .map(|piece| {
                let value = &self.scalar_ssa.values[piece.value.0 as usize];
                let shift = self.ssa_piece_shift(size, piece.byte_offset, value.storage.size);
                // Bits above this physical window cannot contribute after
                // the unsigned shift and piece-width truncation. Keep only
                // the smallest supported view containing the whole window;
                // upper windows still retain the full producer width.
                let view_size = (shift / 8 + value.storage.size).next_power_of_two();
                let raw = PreHirExpr::Cast {
                    ty: type_from_size(view_size, false),
                    expr: Box::new(PreHirExpr::Var(name.into())),
                };
                let shifted = if shift == 0 {
                    raw
                } else {
                    PreHirExpr::Binary {
                        op: PreHirBinaryOp::Shr,
                        lhs: Box::new(raw),
                        rhs: Box::new(PreHirExpr::Const(
                            i64::from(shift),
                            type_from_size(size, false),
                        )),
                        ty: type_from_size(size, false),
                    }
                };
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(self.ssa_emission.bindings[&piece.value].clone()),
                    rhs: ssa_piece_view(shifted, value.storage.size),
                }
            })
            .collect()
    }

    fn ssa_edge_copy_pairs(
        &self,
        predecessor: usize,
        successor: usize,
    ) -> Vec<(String, String, NirType)> {
        self.ssa_emission
            .copies
            .get(&(predecessor, successor))
            .into_iter()
            .flatten()
            .filter_map(|copy| {
                let source = self.ssa_emission.bindings.get(&copy.source)?;
                let destination = self.ssa_emission.bindings.get(&copy.destination)?;
                (source != destination).then(|| {
                    (
                        destination.clone(),
                        source.clone(),
                        ssa_piece_type(copy.storage.size),
                    )
                })
            })
            .collect()
    }

    pub(super) fn ssa_edge_copy_stmts(
        &mut self,
        predecessor: usize,
        successor: usize,
    ) -> Vec<PreHirStmt> {
        let copies = self.ssa_edge_copy_pairs(predecessor, successor);
        schedule_parallel_copies(copies, |ty| {
            let name = self.next_unused_temp_binding_name(ty);
            self.temps.insert(
                name.clone(),
                PreHirBinding {
                    name: name.clone(),
                    ty: ty.clone(),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            );
            name
        })
    }

    pub(super) fn append_ssa_edge_copies(
        &mut self,
        block: usize,
        body: &mut Vec<PreHirStmt>,
    ) -> Result<(), MlilPreviewError> {
        let successors = self
            .heritage_successors
            .get(block)
            .cloned()
            .unwrap_or_default();
        if successors.len() == 1 {
            body.extend(self.ssa_edge_copy_stmts(block, successors[0]));
        } else if let Some(branch) = self.ssa_emission.conditional_edges.get(&block).cloned() {
            if super::debug::preview_builder_diag_enabled() {
                let first = self.ssa_edge_copy_pairs(block, branch.true_successor);
                let second = self.ssa_edge_copy_pairs(block, branch.false_successor);
                eprintln!(
                    "[DIAG] SSA parallel edge observation: block={} true={} false={} identical={}",
                    block,
                    first.len(),
                    second.len(),
                    first == second
                );
            }
            let then_body = self.ssa_edge_copy_stmts(block, branch.true_successor);
            let else_body = self.ssa_edge_copy_stmts(block, branch.false_successor);
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(branch.snapshot.clone()),
                rhs: branch.condition,
            });
            body.push(PreHirStmt::If {
                cond: PreHirExpr::Var(branch.snapshot),
                then_body: then_body.into(),
                else_body: else_body.into(),
            });
        }
        Ok(())
    }
}

fn ssa_expr_initialized(
    expr: &PreHirExpr,
    state: &BTreeSet<String>,
    tracked: &BTreeSet<String>,
) -> bool {
    let valid = |expr| ssa_expr_initialized(expr, state, tracked);
    match expr {
        PreHirExpr::Var(name) => !tracked.contains(name) || state.contains(name),
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => valid(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => valid(lhs) && valid(rhs),
        PreHirExpr::Index { base, index, .. } => valid(base) && valid(index),
        PreHirExpr::Call { args, .. } => args.iter().all(valid),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => valid(cond) && valid(then_expr) && valid(else_expr),
        PreHirExpr::Const(_, _)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::AddressOfGlobal(_) => true,
    }
}

fn ssa_transfer_definitions(
    body: &[PreHirStmt],
    state: &mut BTreeSet<String>,
    tracked: &BTreeSet<String>,
    decision: Option<(&str, bool)>,
    check_reads: bool,
) -> bool {
    for stmt in body {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if check_reads && !ssa_expr_initialized(rhs, state, tracked) {
                    return false;
                }
                let destination_exprs: Vec<&PreHirExpr> = match lhs {
                    PreHirLValue::Var(name) => {
                        state.insert(name.clone());
                        Vec::new()
                    }
                    PreHirLValue::Deref { ptr, .. } => vec![ptr],
                    PreHirLValue::Index { base, index, .. } => vec![base, index],
                    PreHirLValue::FieldAccess { base, .. } => vec![base],
                };
                if check_reads
                    && destination_exprs
                        .iter()
                        .any(|expr| !ssa_expr_initialized(expr, state, tracked))
                {
                    return false;
                }
            }
            PreHirStmt::Block(body) => {
                if !ssa_transfer_definitions(body, state, tracked, decision, check_reads) {
                    return false;
                }
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if check_reads && !ssa_expr_initialized(cond, state, tracked) {
                    return false;
                }
                if let Some((name, taken)) = decision
                    .filter(|(name, _)| matches!(cond, PreHirExpr::Var(var) if var == *name))
                {
                    let _ = name;
                    if !ssa_transfer_definitions(
                        if taken { then_body } else { else_body },
                        state,
                        tracked,
                        decision,
                        check_reads,
                    ) {
                        return false;
                    }
                } else {
                    let mut then_state = state.clone();
                    let mut else_state = state.clone();
                    if !ssa_transfer_definitions(
                        then_body,
                        &mut then_state,
                        tracked,
                        decision,
                        check_reads,
                    ) || !ssa_transfer_definitions(
                        else_body,
                        &mut else_state,
                        tracked,
                        decision,
                        check_reads,
                    ) {
                        return false;
                    }
                    then_state.retain(|name| else_state.contains(name));
                    *state = then_state;
                }
            }
            PreHirStmt::Expr(expr)
            | PreHirStmt::Return(Some(expr))
            | PreHirStmt::VaStart { va_list: expr, .. } => {
                if check_reads && !ssa_expr_initialized(expr, state, tracked) {
                    return false;
                }
            }
            PreHirStmt::Return(None) => {}
            // Block lowering should not contain independent nonlocal control;
            // those shapes need their own edge correspondence proof.
            _ => return false,
        }
    }
    true
}

fn ssa_piece_type(size: u32) -> NirType {
    type_from_size(size.next_power_of_two(), false)
}

#[cfg(test)]
mod initialization_tests {
    use super::*;

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.into()),
            rhs,
        }
    }

    #[test]
    fn previous_definition_is_required_even_when_a_later_write_exists() {
        let tracked = ["value".into()].into_iter().collect();
        let body = vec![assign("value", PreHirExpr::Var("value".into()))];
        assert!(!ssa_transfer_definitions(
            &body,
            &mut BTreeSet::new(),
            &tracked,
            None,
            true
        ));
        assert!(ssa_transfer_definitions(
            &body,
            &mut tracked.clone(),
            &tracked,
            None,
            true
        ));
    }

    #[test]
    fn conditional_initialization_belongs_only_to_its_actual_edge() {
        let tracked: BTreeSet<String> = ["decision", "carrier"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let input: BTreeSet<String> = ["decision".into()].into_iter().collect();
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Var("decision".into()),
            then_body: vec![assign("carrier", PreHirExpr::Const(7, NirType::Unknown))].into(),
            else_body: Vec::new().into(),
        }];
        for (decision, expected) in [
            (None, false),
            (Some(("decision", true)), true),
            (Some(("decision", false)), false),
        ] {
            let mut state = input.clone();
            assert!(ssa_transfer_definitions(
                &body, &mut state, &tracked, decision, true
            ));
            assert_eq!(state.contains("carrier"), expected);
        }
    }

    #[test]
    fn a_saved_copy_is_not_initialized_by_taking_its_address() {
        let tracked: BTreeSet<String> = ["saved", "address"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let mut state = BTreeSet::new();
        assert!(ssa_transfer_definitions(
            &[assign(
                "address",
                PreHirExpr::AddressOfLocal("saved".into())
            )],
            &mut state,
            &tracked,
            None,
            true
        ));
        assert!(!ssa_expr_initialized(
            &PreHirExpr::Var("saved".into()),
            &state,
            &tracked
        ));
        assert!(ssa_expr_initialized(
            &PreHirExpr::Var("address".into()),
            &state,
            &tracked
        ));
    }
}

fn ssa_piece_view(expr: PreHirExpr, size: u32) -> PreHirExpr {
    let ty = ssa_piece_type(size);
    let narrowed = PreHirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(expr),
    };
    if size.is_power_of_two() {
        narrowed
    } else {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs: Box::new(narrowed),
            rhs: Box::new(PreHirExpr::Const(
                ((1u64 << (size * 8)) - 1) as i64,
                ty.clone(),
            )),
            ty,
        }
    }
}

/// Preserve simultaneous reads on a CFG edge. Cycles snapshot an old
/// destination before any overwrite. The fresh binding belongs to that edge.
fn schedule_parallel_copies(
    mut copies: Vec<(String, String, NirType)>,
    mut fresh: impl FnMut(&NirType) -> String,
) -> Vec<PreHirStmt> {
    copies.retain(|(destination, source, _)| destination != source);
    let mut body = Vec::new();
    while !copies.is_empty() {
        if let Some(index) = copies.iter().position(|(destination, _, _)| {
            !copies.iter().any(|(_, source, _)| source == destination)
        }) {
            let (destination, source, _) = copies.remove(index);
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(destination),
                rhs: PreHirExpr::Var(source),
            });
        } else {
            let (destination, _, ty) = &copies[0];
            let saved = destination.clone();
            let temporary = fresh(ty);
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(temporary.clone()),
                rhs: PreHirExpr::Var(saved.clone()),
            });
            for (_, source, _) in &mut copies {
                if *source == saved {
                    *source = temporary.clone();
                }
            }
        }
    }
    body
}

fn ssa_varnode(storage: SsaStorageKey) -> Varnode {
    Varnode {
        space_id: storage.space_id,
        offset: storage.offset,
        size: storage.size,
        is_constant: false,
        constant_val: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_piece_windows_do_not_depend_on_unused_upper_bits() {
        for producer_size in [1u32, 2, 4, 8] {
            for offset in 0..producer_size {
                for piece_size in 1..=producer_size - offset {
                    let shift = offset * 8;
                    let view_size = (offset + piece_size).next_power_of_two();
                    let mask = |bytes| {
                        if bytes == 8 {
                            u64::MAX
                        } else {
                            (1u64 << (bytes * 8)) - 1
                        }
                    };
                    for value in [0, 1, u64::MAX, 0x1234_5678_9abc_def0, 0x8000_0000_8000_0080] {
                        let expected = (value >> shift) & mask(piece_size);
                        let actual = ((value & mask(view_size)) >> shift) & mask(piece_size);
                        assert_eq!(
                            actual, expected,
                            "window {offset}+{piece_size} of {producer_size}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn parallel_copy_cycles_and_fanout_keep_original_values() {
        let ty = type_from_size(8, false);
        let pairs = [("a", "b"), ("b", "c"), ("c", "a"), ("d", "a")];
        let body = schedule_parallel_copies(
            pairs
                .into_iter()
                .map(|(d, s)| (d.into(), s.into(), ty.clone()))
                .collect(),
            |_| "saved".into(),
        );
        let mut values = BTreeMap::from([
            ("a".to_string(), 1u64),
            ("b".into(), 2),
            ("c".into(), 3),
            ("d".into(), 4),
        ]);
        for stmt in body {
            let PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs),
                rhs: PreHirExpr::Var(rhs),
            } = stmt
            else {
                panic!("copy only");
            };
            values.insert(lhs, values[&rhs]);
        }
        for (name, expected) in [("a", 2), ("b", 3), ("c", 1), ("d", 1)] {
            assert_eq!(values[name], expected);
        }
    }
}
