use super::*;

mod aggregate_recovery;
mod call_recovery;
mod entry_analysis;
mod lower_expr;
mod materialize;
mod stack_slots;
mod terminator;
mod type_hints;

pub(super) fn apply_preview_type_hints(func: &mut HirFunction, context: &PreviewTypeContext) {
    type_hints::apply_preview_type_hints(func, context);
}

#[cfg(test)]
pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    local_hints: &mut HashMap<String, String>,
) {
    type_hints::collect_local_surface_hints(body, pointer_hints, func, local_hints);
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn new(
        pcode: &'a PcodeFunction,
        options: &'a MlilPreviewOptions,
        type_context: Option<&'a PreviewTypeContext>,
    ) -> Self {
        let mut defs = HashMap::new();
        for (block_idx, block) in pcode.blocks.iter().enumerate() {
            for (op_idx, op) in block.ops.iter().enumerate() {
                if let Some(output) = &op.output {
                    defs.insert(
                        VarnodeKey::from(output),
                        DefSite {
                            block_idx,
                            op_idx,
                            op,
                        },
                    );
                }
            }
        }
        let address_to_index = pcode
            .blocks
            .iter()
            .enumerate()
            .map(|(idx, block)| (block.start_address, idx))
            .collect::<HashMap<_, _>>();
        let layout_fallthrough = build_layout_fallthrough_map(pcode);
        let successors = build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        let predecessors = build_predecessor_index_map(&successors);
        let register_param_aliases = entry_analysis::collect_entry_register_param_aliases(pcode);
        let stack_frame_size = entry_analysis::infer_entry_stack_frame_size(pcode, options);
        Self {
            pcode,
            options,
            type_context,
            defs,
            address_to_index,
            layout_fallthrough,
            successors,
            predecessors,
            params: BTreeMap::new(),
            locals: BTreeMap::new(),
            locals_next_id: 0,
            temps: BTreeMap::new(),
            temp_next_id: 0,
            materialized_vns: HashMap::new(),
            current_lowering_site: None,
            register_param_aliases,
            stack_frame_size,
            linear_exit_cache: HashMap::new(),
            linear_body_cache: HashMap::new(),
            jump_targets_cache: None,
        }
    }

    pub(super) fn build_hir(
        &mut self,
        name: &str,
        _address: u64,
    ) -> Result<HirFunction, MlilPreviewError> {
        if self.pcode.blocks.is_empty() {
            return Err(MlilPreviewError::UnsupportedPattern("empty pcode"));
        }

        let mut body = Vec::new();
        if self.pcode.blocks.len() == 1 {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir single_block_start: block=0x{:x} ops={}",
                    self.pcode.blocks[0].start_address,
                    self.pcode.blocks[0].ops.len()
                );
            }
            let block = &self.pcode.blocks[0];
            body.extend(self.lower_block_stmts(block)?);
            match self.lower_block_terminator(0)? {
                LoweredTerminator::Return(expr) => body.push(HirStmt::Return(expr)),
                LoweredTerminator::Fallthrough(None) => {}
                LoweredTerminator::Fallthrough(Some(target)) | LoweredTerminator::Goto(target) => {
                    body.push(HirStmt::Goto(block_label(target)))
                }
                LoweredTerminator::Cond {
                    cond,
                    true_target,
                    false_target,
                } => body.push(HirStmt::If {
                    cond,
                    then_body: vec![HirStmt::Goto(block_label(true_target))],
                    else_body: false_target
                        .map(block_label)
                        .map(HirStmt::Goto)
                        .into_iter()
                        .collect(),
                }),
                LoweredTerminator::Unsupported => {
                    return Err(MlilPreviewError::UnsupportedCfgIndirectCallRegion);
                }
            }
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir single_block_done: stmts={}", body.len());
            }
        } else {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] build_hir multiblock_start: blocks={} ops={}",
                    self.pcode.blocks.len(),
                    self.pcode
                        .blocks
                        .iter()
                        .map(|block| block.ops.len())
                        .sum::<usize>()
                );
            }
            body = self.build_multiblock_body()?;
            if preview_builder_diag_enabled() {
                eprintln!("[DIAG] build_hir multiblock_done: stmts={}", body.len());
            }
        }

        let return_type = body
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr_type(expr)),
                HirStmt::Return(None) => Some(NirType::Unknown),
                _ => None,
            })
            .unwrap_or(NirType::Unknown);

        Ok(HirFunction {
            name: name.to_string(),
            params: self.params.values().cloned().collect(),
            locals: self
                .locals
                .values()
                .map(|slot| NirBinding {
                    name: slot.name.clone(),
                    ty: slot.ty.clone(),
                    surface_type_name: None,
                    initializer: None,
                })
                .chain(self.temps.values().cloned())
                .collect(),
            return_type,
            body,
        })
    }

    fn with_lowering_site<T>(&mut self, site: LoweringSite, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.current_lowering_site;
        self.current_lowering_site = Some(site);
        let result = f(self);
        self.current_lowering_site = prev;
        result
    }

    pub(super) fn next_block_address(&self, idx: usize) -> Option<u64> {
        self.layout_fallthrough[idx].map(|next_idx| self.pcode.blocks[next_idx].start_address)
    }

    pub(super) fn ensure_temp_binding_for_output(
        &mut self,
        op: &PcodeOp,
        output: &Varnode,
    ) -> NirBinding {
        let key = MaterializedVarnodeKey::new(output, op);
        if let Some(name) = self.materialized_vns.get(&key)
            && let Some(binding) = self.temps.get(name)
        {
            return binding.clone();
        }

        let ty = type_from_size(output.size, false);
        let name = next_temp_name(&ty, &mut self.temp_next_id);
        let binding = NirBinding {
            name: name.clone(),
            ty,
            surface_type_name: None,
            initializer: None,
        };
        self.materialized_vns.insert(key, name.clone());
        self.temps.insert(name, binding.clone());
        binding
    }

    fn debug_lowering_error(
        &self,
        stage: &str,
        block_addr: u64,
        seq: u64,
        opcode: PcodeOpcode,
        err: &MlilPreviewError,
    ) {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage={} block=0x{:x} seq=0x{:x} opcode={:?} err={}",
                stage, block_addr, seq, opcode, err
            );
        }
    }
}

fn preview_builder_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}
