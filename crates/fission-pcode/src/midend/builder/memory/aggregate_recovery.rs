use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn recover_aggregate_store_rhs_from_block(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        rhs: &Varnode,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let debug = std::env::var_os("FISSION_PREVIEW_DEBUG").is_some();
        if rhs.size < 16 {
            return Ok(None);
        }
        if debug {
            let line = format!(
                "[T1] STORE value block=0x{:x} op_idx={} space={} size={} offset=0x{:x}",
                block.start_address, op_idx, rhs.space_id, rhs.size, rhs.offset
            );
            eprintln!("{line}");
            append_preview_debug_trace(&line);
        }
        let mut current = rhs.clone();
        let mut current_block_idx = self
            .address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(0);
        let mut scan_end = op_idx;

        for _ in 0..8 {
            let current_block = &self.pcode.blocks[current_block_idx];
            if debug {
                eprintln!(
                    "[mlil-preview][agg] block=0x{:x} op_idx={} current=({},0x{:x},sz={}) scan_end={}",
                    current_block.start_address,
                    op_idx,
                    current.space_id,
                    current.offset,
                    current.size,
                    scan_end
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] block=0x{:x} op_idx={} current=({},0x{:x},sz={}) scan_end={}",
                    current_block.start_address,
                    op_idx,
                    current.space_id,
                    current.offset,
                    current.size,
                    scan_end
                ));
            }
            if current.space_id == REGISTER_SPACE_ID && current.size >= 16 {
                if let Some((source_block_idx, source, earliest_idx)) = self
                    .recover_wide_register_source_from_linear_chain(
                        current_block_idx,
                        scan_end,
                        &current,
                    )
                {
                    if debug {
                        eprintln!(
                            "[mlil-preview][agg] wide-reg source -> block=0x{:x} ({},0x{:x},sz={}) earliest_idx={}",
                            self.pcode.blocks[source_block_idx].start_address,
                            source.space_id,
                            source.offset,
                            source.size,
                            earliest_idx
                        );
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] wide-reg source -> block=0x{:x} ({},0x{:x},sz={}) earliest_idx={}",
                            self.pcode.blocks[source_block_idx].start_address,
                            source.space_id,
                            source.offset,
                            source.size,
                            earliest_idx
                        ));
                    }
                    current = source;
                    current_block_idx = source_block_idx;
                    scan_end = earliest_idx;
                    continue;
                }
                if let Some(slot_expr) = self.asm_guided_xmm_load_source_from_linear_chain(
                    current_block_idx,
                    scan_end,
                    &current,
                ) {
                    if debug {
                        let line =
                            format!("[T4] asm-guided xmm source -> {}", print_expr(&slot_expr));
                        eprintln!("{line}");
                        append_preview_debug_trace(&line);
                    }
                    return Ok(Some(slot_expr));
                }
                if debug {
                    eprintln!("[mlil-preview][agg] wide-reg source lookup failed");
                    append_preview_debug_trace("[mlil-preview][agg] wide-reg source lookup failed");
                }
                return Ok(None);
            }

            let Some((def_idx, def_op)) =
                find_prior_def_in_block(current_block, scan_end, &current)
            else {
                if debug {
                    let line = format!(
                        "[T2] NO DEF FOUND block=0x{:x} scan_end={} space={} size={} offset=0x{:x}",
                        current_block.start_address,
                        scan_end,
                        current.space_id,
                        current.size,
                        current.offset
                    );
                    eprintln!("{line}");
                    append_preview_debug_trace(&line);
                    eprintln!("[mlil-preview][agg] no prior def found");
                    append_preview_debug_trace("[mlil-preview][agg] no prior def found");
                }
                return Ok(None);
            };
            if debug {
                let header = format!(
                    "[T2] def idx={} seq=0x{:x} opcode={:?} out={}",
                    def_idx,
                    def_op.seq_num,
                    def_op.opcode,
                    format_varnode_opt(def_op.output.as_ref())
                );
                eprintln!("{header}");
                append_preview_debug_trace(&header);
                for (input_idx, input) in def_op.inputs.iter().enumerate() {
                    let line = format!("[T2]   input[{}]={}", input_idx, format_varnode(input));
                    eprintln!("{line}");
                    append_preview_debug_trace(&line);
                }
                eprintln!(
                    "[mlil-preview][agg] def idx={} seq=0x{:x} opcode={:?}",
                    def_idx, def_op.seq_num, def_op.opcode
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] def idx={} seq=0x{:x} opcode={:?}",
                    def_idx, def_op.seq_num, def_op.opcode
                ));
            }

            match def_op.opcode {
                PcodeOpcode::Load => {
                    if def_op.inputs.len() < 2 {
                        if debug {
                            eprintln!("[mlil-preview][agg] load malformed");
                            append_preview_debug_trace("[mlil-preview][agg] load malformed");
                        }
                        return Ok(None);
                    }
                    if let Some((slot_name, _)) = self.try_stack_slot_lvalue_for_memory_op(
                        def_op,
                        &def_op.inputs[1],
                        type_from_size(current.size, false),
                    ) {
                        if debug {
                            eprintln!("[mlil-preview][agg] resolved slot {}", slot_name);
                            append_preview_debug_trace(&format!(
                                "[mlil-preview][agg] resolved slot {}",
                                slot_name
                            ));
                        }
                        return Ok(Some(HirExpr::Var(slot_name)));
                    }
                    if debug {
                        eprintln!("[mlil-preview][agg] load did not resolve stack slot");
                        append_preview_debug_trace(
                            "[mlil-preview][agg] load did not resolve stack slot",
                        );
                    }
                    return Ok(None);
                }
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => {
                    let Some(next) = def_op.inputs.first() else {
                        if debug {
                            eprintln!("[mlil-preview][agg] copy/cast missing input");
                            append_preview_debug_trace(
                                "[mlil-preview][agg] copy/cast missing input",
                            );
                        }
                        return Ok(None);
                    };
                    if debug {
                        eprintln!(
                            "[mlil-preview][agg] stepping through {:?} -> ({},0x{:x},sz={})",
                            def_op.opcode, next.space_id, next.offset, next.size
                        );
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] stepping through {:?} -> ({},0x{:x},sz={})",
                            def_op.opcode, next.space_id, next.offset, next.size
                        ));
                    }
                    current = next.clone();
                    scan_end = def_idx;
                }
                _ => {
                    if debug {
                        eprintln!(
                            "[mlil-preview][agg] unsupported def opcode {:?}",
                            def_op.opcode
                        );
                        append_preview_debug_trace(&format!(
                            "[mlil-preview][agg] unsupported def opcode {:?}",
                            def_op.opcode
                        ));
                    }
                    return Ok(None);
                }
            }
        }

        if debug {
            eprintln!("[mlil-preview][agg] exceeded trace depth");
            append_preview_debug_trace("[mlil-preview][agg] exceeded trace depth");
        }
        Ok(None)
    }

    fn recover_wide_register_source_from_linear_chain(
        &self,
        start_block_idx: usize,
        scan_end: usize,
        reg_vn: &Varnode,
    ) -> Option<(usize, Varnode, usize)> {
        let debug = std::env::var_os("FISSION_PREVIEW_DEBUG").is_some();
        let mut block_idx = start_block_idx;
        let mut current_scan_end = scan_end;
        let dump_regs = std::env::var_os("FISSION_PREVIEW_DEBUG_REGDUMP").is_some();

        for depth in 0..4 {
            let block = &self.pcode.blocks[block_idx];
            if dump_regs {
                dump_block_register_defs(block, &format!("0x{:x}", block.start_address), reg_vn);
            }
            if let Some((source, earliest_idx)) =
                recover_wide_register_source_from_block(block, current_scan_end, reg_vn)
            {
                return Some((block_idx, source, earliest_idx));
            }

            let preds = self.predecessors.get(block_idx)?;
            if preds.len() != 1 {
                if debug {
                    let line = format!(
                        "[T4] linear predecessor stop block=0x{:x} preds={}",
                        block.start_address,
                        preds.len()
                    );
                    eprintln!("{line}");
                    append_preview_debug_trace(&line);
                }
                return None;
            }

            let pred_idx = preds[0];

            if debug {
                let line = format!(
                    "[T4] stepping predecessor depth={} from=0x{:x} to=0x{:x}",
                    depth, block.start_address, self.pcode.blocks[pred_idx].start_address
                );
                eprintln!("{line}");
                append_preview_debug_trace(&line);
            }

            block_idx = pred_idx;
            current_scan_end = self.pcode.blocks[block_idx].ops.len();
        }

        None
    }

    fn asm_guided_xmm_load_source_from_linear_chain(
        &mut self,
        start_block_idx: usize,
        scan_end: usize,
        reg_vn: &Varnode,
    ) -> Option<HirExpr> {
        let xmm_index = xmm_register_index(reg_vn)?;
        let mut block_idx = start_block_idx;
        let mut current_scan_end = scan_end;

        for _ in 0..4 {
            let block = &self.pcode.blocks[block_idx];
            for op in block.ops.iter().take(current_scan_end).rev() {
                let Some(asm) = op.asm_mnemonic.as_deref() else {
                    continue;
                };
                if !asm_loads_xmm_from_stack(asm, xmm_index) {
                    continue;
                }
                let Some((base, offset)) = self.resolve_stack_address_from_memory_op(op) else {
                    continue;
                };
                let (slot_name, _) = self.ensure_stack_slot_binding(
                    base,
                    offset,
                    NirType::Aggregate {
                        size: reg_vn.size,
                        fields: vec![],
                    },
                )?;
                return Some(HirExpr::Var(slot_name));
            }

            let preds = self.predecessors.get(block_idx)?;
            if preds.len() != 1 {
                return None;
            }
            block_idx = preds[0];
            current_scan_end = self.pcode.blocks[block_idx].ops.len();
        }

        None
    }
}

fn append_preview_debug_trace(line: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/fission_preview_agg_trace.log")
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()));
}

pub(in crate::midend::builder) fn find_prior_def_in_block<'a>(
    block: &'a crate::pcode::PcodeBasicBlock,
    scan_end: usize,
    target: &Varnode,
) -> Option<(usize, &'a PcodeOp)> {
    let key = VarnodeKey::from(target);
    block
        .ops
        .iter()
        .enumerate()
        .take(scan_end)
        .rev()
        .find(|(_, op)| {
            let Some(output) = op.output.as_ref() else {
                return false;
            };
            output.space_id == key.space_id
                && output.offset == key.offset
                && output.size == key.size
                && output.is_constant == key.is_constant
                && output.constant_val == key.constant_val
        })
}

pub(in crate::midend::builder) fn recover_wide_register_source_from_block(
    block: &crate::pcode::PcodeBasicBlock,
    scan_end: usize,
    reg_vn: &Varnode,
) -> Option<(Varnode, usize)> {
    let debug = std::env::var_os("FISSION_PREVIEW_DEBUG").is_some();
    if reg_vn.space_id != REGISTER_SPACE_ID || reg_vn.size < 16 || reg_vn.size % 4 != 0 {
        return None;
    }

    if let Some((def_idx, def_op)) = find_prior_def_in_block(block, scan_end, reg_vn) {
        if debug {
            eprintln!(
                "[mlil-preview][agg] wide-reg direct def idx={} seq=0x{:x} opcode={:?}",
                def_idx, def_op.seq_num, def_op.opcode
            );
            append_preview_debug_trace(&format!(
                "[mlil-preview][agg] wide-reg direct def idx={} seq=0x{:x} opcode={:?}",
                def_idx, def_op.seq_num, def_op.opcode
            ));
        }
        if matches!(
            def_op.opcode,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
        ) && let Some(input) = def_op.inputs.first()
            && input.size >= reg_vn.size
        {
            if debug {
                eprintln!(
                    "[mlil-preview][agg] wide-reg direct source -> ({},0x{:x},sz={})",
                    input.space_id, input.offset, input.size
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] wide-reg direct source -> ({},0x{:x},sz={})",
                    input.space_id, input.offset, input.size
                ));
            }
            return Some((input.clone(), def_idx));
        }
    }

    recover_common_subpiece_source_for_register(block, scan_end, reg_vn, debug)
}

fn recover_common_subpiece_source_for_register(
    block: &crate::pcode::PcodeBasicBlock,
    scan_end: usize,
    reg_vn: &Varnode,
    debug: bool,
) -> Option<(Varnode, usize)> {
    let reg_size = reg_vn.size as usize;
    let mut covered = vec![false; reg_size];
    let mut common_source: Option<Varnode> = None;
    let mut earliest_idx = scan_end;
    let mut lane_count = 0usize;
    let mut lane_sizes = Vec::new();
    let mut lane_traces = Vec::new();

    for (idx, op) in block.ops.iter().enumerate().take(scan_end).rev() {
        let Some(output) = &op.output else {
            continue;
        };
        if output.space_id != REGISTER_SPACE_ID {
            continue;
        }
        let within_reg = output.offset >= reg_vn.offset
            && output.offset + u64::from(output.size) <= reg_vn.offset + u64::from(reg_vn.size);
        if !within_reg {
            continue;
        }
        if !matches!(op.opcode, PcodeOpcode::SubPiece | PcodeOpcode::IntSub) || op.inputs.len() < 2
        {
            continue;
        }
        let Some(source) = op.inputs.first() else {
            continue;
        };
        if source.size < reg_vn.size {
            continue;
        }
        let Some(disp) = const_offset(&op.inputs[1]) else {
            continue;
        };
        if disp < 0 {
            continue;
        }
        let start = disp as usize;
        let end = start + output.size as usize;
        if end > reg_size {
            continue;
        }
        if covered[start..end].iter().all(|covered_byte| *covered_byte) {
            continue;
        }
        if covered[start..end].iter().any(|covered_byte| *covered_byte) {
            if debug {
                eprintln!(
                    "[mlil-preview][agg] lane overlap start={} end={}",
                    start, end
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] lane overlap start={} end={}",
                    start, end
                ));
            }
            return None;
        }
        match &common_source {
            Some(existing) if VarnodeKey::from(existing) != VarnodeKey::from(source) => {
                if debug {
                    eprintln!("[mlil-preview][agg] lane source mismatch");
                    append_preview_debug_trace("[mlil-preview][agg] lane source mismatch");
                }
                return None;
            }
            None => common_source = Some(source.clone()),
            _ => {}
        }
        covered[start..end].fill(true);
        earliest_idx = earliest_idx.min(idx);
        lane_count += 1;
        lane_sizes.push(output.size);
        lane_traces.push(format!(
            "idx={} opcode={:?} out={} disp={} src={}",
            idx,
            op.opcode,
            format_varnode(output),
            disp,
            format_varnode(source)
        ));
        if covered.iter().all(|covered_byte| *covered_byte) {
            if debug {
                let summary = format!("[T3] lane_defs count={} sizes={:?}", lane_count, lane_sizes);
                eprintln!("{summary}");
                append_preview_debug_trace(&summary);
                for trace in &lane_traces {
                    let line = format!("[T3]   {trace}");
                    eprintln!("{line}");
                    append_preview_debug_trace(&line);
                }
                let common = format!(
                    "[T4] common_source={}",
                    common_source
                        .as_ref()
                        .map(format_varnode)
                        .unwrap_or_else(|| "<none>".to_string())
                );
                eprintln!("{common}");
                append_preview_debug_trace(&common);
                eprintln!(
                    "[mlil-preview][agg] lane coverage complete count={} sizes={:?}",
                    lane_count, lane_sizes
                );
                append_preview_debug_trace(&format!(
                    "[mlil-preview][agg] lane coverage complete count={} sizes={:?}",
                    lane_count, lane_sizes
                ));
            }
            return common_source.map(|src| (src, earliest_idx));
        }
    }

    if debug {
        let summary = format!("[T3] lane_defs count={} sizes={:?}", lane_count, lane_sizes);
        eprintln!("{summary}");
        append_preview_debug_trace(&summary);
        for trace in &lane_traces {
            let line = format!("[T3]   {trace}");
            eprintln!("{line}");
            append_preview_debug_trace(&line);
        }
        let common = format!(
            "[T4] common_source={}",
            common_source
                .as_ref()
                .map(format_varnode)
                .unwrap_or_else(|| "<none>".to_string())
        );
        eprintln!("{common}");
        append_preview_debug_trace(&common);
        eprintln!(
            "[mlil-preview][agg] lane coverage incomplete count={} sizes={:?}",
            lane_count, lane_sizes
        );
        append_preview_debug_trace(&format!(
            "[mlil-preview][agg] lane coverage incomplete count={} sizes={:?}",
            lane_count, lane_sizes
        ));
    }
    None
}

fn format_varnode(vn: &Varnode) -> String {
    format!(
        "space={} size={} offset=0x{:x} const={}",
        vn.space_id, vn.size, vn.offset, vn.is_constant
    )
}

fn format_varnode_opt(vn: Option<&Varnode>) -> String {
    vn.map(format_varnode)
        .unwrap_or_else(|| "<none>".to_string())
}

fn xmm_register_index(vn: &Varnode) -> Option<u64> {
    if vn.space_id != REGISTER_SPACE_ID || vn.size < 16 || vn.offset < 0x1200 {
        return None;
    }
    let delta = vn.offset - 0x1200;
    if delta % 0x10 == 0 {
        Some(delta / 0x10)
    } else {
        None
    }
}

fn asm_loads_xmm_from_stack(asm: &str, xmm_index: u64) -> bool {
    let asm = asm.trim().to_ascii_uppercase();
    let body = ["MOVUPS ", "MOVDQU ", "MOVAPS ", "MOVDQA "]
        .iter()
        .find_map(|prefix| asm.strip_prefix(prefix));
    let Some(body) = body else {
        return false;
    };
    let Some((dst, src)) = body.split_once(',') else {
        return false;
    };
    let expected_dst = format!("XMM{xmm_index}");
    if dst.trim() != expected_dst {
        return false;
    }
    src.contains('[')
        && (src.contains("RSP")
            || src.contains("RBP")
            || src.contains("ESP")
            || src.contains("EBP"))
}

fn dump_block_register_defs(block: &crate::pcode::PcodeBasicBlock, label: &str, target: &Varnode) {
    let header = format!("=== BLOCK DUMP: {} ===", label);
    eprintln!("{header}");
    append_preview_debug_trace(&header);
    for (idx, op) in block.ops.iter().enumerate() {
        let Some(out) = &op.output else {
            continue;
        };
        if out.space_id != REGISTER_SPACE_ID {
            continue;
        }
        let line = format!(
            "  [{}] {:?} -> reg(offset=0x{:x}, size={})",
            idx, op.opcode, out.offset, out.size
        );
        eprintln!("{line}");
        append_preview_debug_trace(&line);
        for (input_idx, inp) in op.inputs.iter().enumerate() {
            let input_line = format!(
                "         input[{}]: space={} offset=0x{:x} size={}",
                input_idx, inp.space_id, inp.offset, inp.size
            );
            eprintln!("{input_line}");
            append_preview_debug_trace(&input_line);
        }
    }

    let overlap_header = format!(
        "  --- Overlap check: covers 0x{:x}..0x{:x} ---",
        target.offset,
        target.offset + u64::from(target.size)
    );
    eprintln!("{overlap_header}");
    append_preview_debug_trace(&overlap_header);
    let target_lo = target.offset;
    let target_hi = target.offset + u64::from(target.size);
    for (idx, op) in block.ops.iter().enumerate() {
        let Some(out) = &op.output else {
            continue;
        };
        if out.space_id != REGISTER_SPACE_ID {
            continue;
        }
        let lo = out.offset;
        let hi = out.offset + u64::from(out.size);
        if lo < target_hi && hi > target_lo {
            let line = format!(
                "  [{}] OVERLAP {:?} -> reg(0x{:x}..0x{:x})",
                idx, op.opcode, lo, hi
            );
            eprintln!("{line}");
            append_preview_debug_trace(&line);
        }
    }
}
