use super::*;
use crate::midend::cspec::RegisterNamer;

pub(super) fn collect_entry_register_param_aliases(
    pcode: &PcodeFunction,
    namer: &RegisterNamer,
) -> HashMap<u64, usize> {
    let mut aliases = HashMap::default();
    let Some(entry) = pcode.blocks.first() else {
        return aliases;
    };

    for op in &entry.ops {
        match op.opcode {
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Branch
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::Return => break,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                let Some(output) = &op.output else {
                    continue;
                };
                if !is_register_varnode(output) {
                    continue;
                }
                let Some((_, output_param_index)) =
                    namer.register_name_with_param_owned(output.offset, output.size)
                else {
                    continue;
                };
                if output_param_index.is_some() {
                    continue;
                }
                let Some(input) = op.inputs.first() else {
                    continue;
                };
                if !is_register_varnode(input) {
                    continue;
                }
                let alias_param_index = namer
                    .register_name_with_param_owned(input.offset, input.size)
                    .and_then(|(_, input_param_index)| input_param_index)
                    .or_else(|| aliases.get(&input.offset).copied());
                if let Some(param_index) = alias_param_index {
                    aliases.entry(output.offset).or_insert(param_index);
                }
            }
            _ => {}
        }
    }

    aliases
}

pub(super) fn infer_entry_stack_layout(
    pcode: &PcodeFunction,
    options: &MlilPreviewOptions,
) -> (i64, bool, i64) {
    let Some(entry) = pcode.blocks.first() else {
        return (0, false, 0);
    };

    let mut frame_size = 0_i64;
    let mut frame_pointer_established = false;
    let mut frame_pointer_bias = 0_i64;
    let mut seen_addrs = HashSet::default();
    let mut started = false;
    let register_namer = RegisterNamer::from_options(options);
    let pointer_size = i64::from(options.pointer_size);

    // Pure pcode-level tracking (independent of the asm-text-based
    // `frame_size`/`PUSH `/`SUB RSP,` scan below), used only to compute
    // `frame_pointer_bias`: `rsp_delta` is the cumulative byte offset of
    // `rsp` from its value on entry (negative once anything has pushed or
    // subtracted); `known_consts` remembers each register's last
    // compile-time-constant value (`mov eax, IMM`), needed so `sub rsp,
    // rax` immediately after `mov eax, IMM; call __chkstk`-or-
    // `___chkstk_ms` (the Windows/mingw large-frame stack-probe idiom --
    // the probe itself doesn't touch `rsp` on x64, see stack_slots.rs's
    // `resolve_constant_operand` doc comment for the same idiom, and
    // confirmed here too: a raw `Call` op has no output, so it doesn't
    // shadow `eax`'s def) still resolves to a concrete delta.
    let mut rsp_delta = 0_i64;
    let mut known_consts: HashMap<VarnodeKey, i64> = HashMap::default();
    // `lea rbp, [rsp+K]` for a nonzero compile-time-constant K: MSVC/mingw
    // sometimes position the frame pointer partway into a large frame
    // (rather than at its base, `mov rbp, rsp`'s implicit K=0) so both
    // locals and incoming-argument home slots stay within
    // signed-displacement reach. It lifts as two ops -- `IntAdd`/`PtrAdd`
    // into a unique-space temp, then a `Copy` of that temp into `rbp` --
    // not a direct register-to-register `Copy` the way `mov rbp, rsp`
    // does, so it needs its own two-step tracking below rather than fitting
    // the existing bare-`Copy` check.
    //
    // Without tracking this, every rbp-relative access resolves as if
    // rbp sat exactly at the boundary between locals and the caller's
    // stack (`resolve_stack_address_inner`'s bare-register shortcut always
    // treats `rbp` as "canonical frame base, offset 0") -- so on a frame
    // where rbp is instead established deep inside a large allocation,
    // locals sitting at a *positive* rbp-relative offset (still well
    // short of the true boundary) get misclassified as incoming
    // parameters by `classify_stack_slot_origin`'s positive-offset
    // heuristic. The bias needed is `rsp_delta + K + pointer_size` (not
    // just `K`): it has to account for everything already subtracted
    // from `rsp` *before* the `lea` runs too, not only the `lea`'s own
    // displacement -- the `+ pointer_size` term keeps this consistent
    // with the pre-existing hardcoded `bias = 0` for the textbook
    // `push rbp; mov rbp, rsp` case (`rsp_delta == -pointer_size` right
    // then, from that one push, so `bias` comes out to `0` either way).
    // Confirmed against a real `x86_64-w64-mingw32-gcc`-compiled 8KB-
    // local-array fixture using this exact idiom: without this, an access
    // deep in the buffer (`buf[8191]`) misclassified as an incoming
    // parameter; with it, only the real home-slot access past the true
    // boundary does.
    let mut pending_rsp_add: Option<(VarnodeKey, i64)> = None;
    for op in &entry.ops {
        if matches!(op.opcode, PcodeOpcode::Copy)
            && let (Some(output), Some(input)) = (op.output.as_ref(), op.inputs.first())
        {
            let output_key = VarnodeKey::from(output);
            if let Some(v) = const_offset(input) {
                known_consts.insert(output_key, v);
            } else {
                known_consts.remove(&output_key);
            }
        } else if let Some(output) = op.output.as_ref() {
            known_consts.remove(&VarnodeKey::from(output));
        }

        if matches!(op.opcode, PcodeOpcode::IntSub)
            && let Some(output) = op.output.as_ref()
            && op.inputs.len() == 2
        {
            let output_name = register_namer.hw_name_at(output.offset, output.size);
            let base_name = register_namer.hw_name_at(op.inputs[0].offset, op.inputs[0].size);
            if matches!(output_name.as_deref(), Some("esp") | Some("rsp"))
                && matches!(base_name.as_deref(), Some("esp") | Some("rsp"))
            {
                let delta = const_offset(&op.inputs[1])
                    .or_else(|| known_consts.get(&VarnodeKey::from(&op.inputs[1])).copied());
                if let Some(delta) = delta {
                    rsp_delta -= delta;
                }
            }
        }

        if matches!(op.opcode, PcodeOpcode::Copy)
            && let (Some(output), Some(input)) = (op.output.as_ref(), op.inputs.first())
            && is_register_varnode(output)
        {
            let output_name = register_namer.hw_name_at(output.offset, output.size);
            if matches!(output_name.as_deref(), Some("ebp") | Some("rbp")) {
                if is_register_varnode(input) {
                    let input_name = register_namer.hw_name_at(input.offset, input.size);
                    if matches!(
                        (output_name.as_deref(), input_name.as_deref()),
                        (Some("ebp"), Some("esp")) | (Some("rbp"), Some("rsp"))
                    ) {
                        // Zero-displacement `lea rbp, [rsp+0]` lifts as this
                        // same direct-register `Copy` (confirmed empirically
                        // -- SLEIGH collapses the +0 case, it doesn't go
                        // through an `IntAdd`-into-temp step the way a
                        // nonzero-displacement `lea` does), so this branch
                        // needs the identical `rsp_delta`-aware bias the
                        // `lea` branch below uses, not a hardcoded `0` --
                        // otherwise a large frame that zero-displacement-
                        // `lea`s (or plain `mov rbp,rsp`s) into rbp *after*
                        // substantial prior `push`/`sub rsp` still gets the
                        // same misclassification this whole fix targets.
                        frame_pointer_established = true;
                        frame_pointer_bias = rsp_delta + pointer_size;
                        started = true;
                    }
                } else if let Some((temp_key, k)) = &pending_rsp_add
                    && *temp_key == VarnodeKey::from(input)
                {
                    frame_pointer_established = true;
                    frame_pointer_bias = rsp_delta + *k + pointer_size;
                    started = true;
                }
            }
        }
        pending_rsp_add = None;
        if matches!(op.opcode, PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd)
            && let Some(output) = op.output.as_ref()
            && op.inputs.len() == 2
        {
            let (reg_input, const_input) = if is_register_varnode(&op.inputs[0]) {
                (&op.inputs[0], &op.inputs[1])
            } else {
                (&op.inputs[1], &op.inputs[0])
            };
            let reg_name = register_namer.hw_name_at(reg_input.offset, reg_input.size);
            if matches!(reg_name.as_deref(), Some("esp") | Some("rsp"))
                && let Some(k) = const_offset(const_input)
            {
                pending_rsp_add = Some((VarnodeKey::from(output), k));
            }
        }
        if !seen_addrs.insert(op.address) {
            continue;
        }
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            break;
        };
        let asm = asm.trim().to_ascii_uppercase();
        if asm.starts_with("PUSH ") {
            frame_size += pointer_size;
            started = true;
            continue;
        }
        let sub_rsp = if options.is_64bit {
            asm.strip_prefix("SUB RSP,")
        } else {
            asm.strip_prefix("SUB ESP,")
        };
        if let Some(imm) = sub_rsp.and_then(parse_signed_asm_immediate) {
            frame_size += imm;
            started = true;
            continue;
        }
        if asm.starts_with("MOV RBP,RSP") || asm.starts_with("MOV EBP,ESP") {
            frame_pointer_established = true;
            started = true;
            continue;
        }
        if started {
            break;
        }
    }
    (frame_size, frame_pointer_established, frame_pointer_bias)
}

fn parse_signed_asm_immediate(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::cspec::test_maps::apply_preview_cspec;
    use crate::midend::support::CallingConvention;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn tmp(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn imm(value: i64, size: u32) -> Varnode {
        Varnode::constant(value, size)
    }

    fn op(
        seq_num: u32,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x1000 + u64::from(seq_num),
            output,
            inputs,
            // Never actually `None` from the real pipeline (confirmed:
            // falls back to the raw p-code opcode name, e.g. "COPY",
            // "INT_SUB", when there's no real disassembly text) --
            // `infer_entry_stack_layout` treats a `None` as "stop
            // scanning, out of information" and `break`s, which would
            // truncate these hand-built op sequences after the first op.
            asm_mnemonic: Some(format!("{opcode:?}").to_ascii_uppercase()),
        }
    }

    fn x64_options() -> MlilPreviewOptions {
        let mut options = MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0x1400_0000,
            sections: vec![(0x1400_1000, 0x1400_2000)],
            region_linearize_structuring: false,
            calling_convention: CallingConvention::WindowsX64,
            ..Default::default()
        };
        apply_preview_cspec(&mut options);
        options
    }

    fn pcode(ops: Vec<PcodeOp>) -> PcodeFunction {
        PcodeFunction {
            blocks: vec![crate::pcode::PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops,
            }],
        }
    }

    /// The exact shape a large (>1 page) Windows/mingw stack frame lifts
    /// as: `push rbp` (`IntSub rsp,rsp,8`), `mov eax, SIZE` (probe size,
    /// `Copy rax,const`), `call __chkstk`-or-`___chkstk_ms` (no output --
    /// the probe doesn't touch rsp on x64), `sub rsp, rax` (`IntSub
    /// rsp,rsp,rax` -- non-constant second operand), then
    /// `lea rbp, [rsp+K]` (`IntAdd`-into-temp, then `Copy rbp,temp`).
    /// Confirmed against a real `x86_64-w64-mingw32-gcc`-compiled 8KB-
    /// local-array fixture (`big_frame`/`x64_seh_guarded_test`-adjacent,
    /// not checked in) that this exact op sequence, and this exact
    /// expected bias, correctly reclassifies a local sitting at a small
    /// *positive* rbp-relative offset (previously misread as an incoming
    /// parameter) back to a local.
    #[test]
    fn lea_rbp_after_push_and_chkstk_style_sub_computes_correct_bias() {
        let rsp = reg(0x20, 8);
        let rbp = reg(0x28, 8);
        let rax = reg(0x0, 8);
        let probe_size = 0x1000_i64; // 4KB probe, scaled down from the real 8KB fixture
        let k = 0x40_i64; // lea rbp, [rsp+0x40]
        let lea_temp = tmp(0x9000, 8);

        let ops = vec![
            // push rbp
            op(
                0,
                PcodeOpcode::IntSub,
                Some(rsp.clone()),
                vec![rsp.clone(), imm(8, 8)],
            ),
            // mov eax, probe_size
            op(
                1,
                PcodeOpcode::Copy,
                Some(rax.clone()),
                vec![imm(probe_size, 8)],
            ),
            // call ___chkstk_ms -- no output, doesn't touch rax or rsp
            op(
                2,
                PcodeOpcode::Call,
                None,
                vec![Varnode {
                    space_id: 3,
                    offset: 0x1400025a0,
                    size: 8,
                    is_constant: true,
                    constant_val: 0x1400025a0,
                }],
            ),
            // sub rsp, rax
            op(
                3,
                PcodeOpcode::IntSub,
                Some(rsp.clone()),
                vec![rsp.clone(), rax.clone()],
            ),
            // lea rbp, [rsp+K]
            op(
                4,
                PcodeOpcode::IntAdd,
                Some(lea_temp.clone()),
                vec![rsp.clone(), imm(k, 8)],
            ),
            op(5, PcodeOpcode::Copy, Some(rbp), vec![lea_temp]),
        ];

        let (_, established, bias) = infer_entry_stack_layout(&pcode(ops), &x64_options());
        assert!(established);
        let rsp_delta = -(8 + probe_size);
        assert_eq!(bias, rsp_delta + k + 8);
    }

    /// The textbook `push rbp; mov rbp, rsp` prologue -- confirms the bias
    /// still comes out to `0` (matching every already-validated fixture
    /// using this shape throughout the rest of the test suite), even
    /// though `rsp_delta` is `-8` (nonzero) at the point of the `mov`.
    #[test]
    fn plain_mov_rbp_rsp_after_one_push_computes_zero_bias() {
        let rsp = reg(0x20, 8);
        let rbp = reg(0x28, 8);
        let ops = vec![
            op(
                0,
                PcodeOpcode::IntSub,
                Some(rsp.clone()),
                vec![rsp.clone(), imm(8, 8)],
            ),
            op(1, PcodeOpcode::Copy, Some(rbp), vec![rsp]),
        ];
        let (_, established, bias) = infer_entry_stack_layout(&pcode(ops), &x64_options());
        assert!(established);
        assert_eq!(bias, 0);
    }

    /// `mov rbp, rsp` with *no* preceding push (an unusual but legal
    /// prologue shape): `rsp_delta` is `0` at that point, so bias should
    /// be `pointer_size`, not `0` -- this is what
    /// `x86_32_callind_staged_args_prefer_stack_param_not_live_eax` (in
    /// `expr/lower_expr_tests.rs`) exercises indirectly via a *realistic*
    /// (push-then-mov) synthetic prologue; this test covers the
    /// degenerate no-push case directly instead.
    #[test]
    fn mov_rbp_rsp_without_preceding_push_computes_pointer_size_bias() {
        let rsp = reg(0x20, 8);
        let rbp = reg(0x28, 8);
        let ops = vec![op(0, PcodeOpcode::Copy, Some(rbp), vec![rsp])];
        let (_, established, bias) = infer_entry_stack_layout(&pcode(ops), &x64_options());
        assert!(established);
        assert_eq!(bias, 8);
    }
}
