use super::*;

pub(super) fn collect_entry_register_param_aliases(pcode: &PcodeFunction) -> HashMap<u64, usize> {
    let mut aliases = HashMap::new();
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
                if output.space_id != REGISTER_SPACE_ID {
                    continue;
                }
                let Some((_, output_param_index)) =
                    register_name_with_param(output.offset, output.size)
                else {
                    continue;
                };
                if output_param_index.is_some() {
                    continue;
                }
                let Some(input) = op.inputs.first() else {
                    continue;
                };
                if input.space_id != REGISTER_SPACE_ID {
                    continue;
                }
                let alias_param_index = register_name_with_param(input.offset, input.size)
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

pub(super) fn infer_entry_stack_frame_size(
    pcode: &PcodeFunction,
    options: &MlilPreviewOptions,
) -> i64 {
    let Some(entry) = pcode.blocks.first() else {
        return 0;
    };

    let mut frame_size = 0_i64;
    let mut seen_addrs = HashSet::new();
    let mut started = false;
    for op in &entry.ops {
        if !seen_addrs.insert(op.address) {
            continue;
        }
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            break;
        };
        let asm = asm.trim().to_ascii_uppercase();
        let pointer_size = i64::from(options.pointer_size);
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
            started = true;
            continue;
        }
        if started {
            break;
        }
    }
    frame_size
}

fn parse_signed_asm_immediate(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}
