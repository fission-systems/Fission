use super::*;
use crate::midend::cspec::RegisterNamer;

pub(super) fn collect_entry_register_param_aliases(
    pcode: &PcodeFunction,
    namer: &RegisterNamer,
) -> HashMap<u64, usize> {
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
) -> (i64, bool) {
    let Some(entry) = pcode.blocks.first() else {
        return (0, false);
    };

    let mut frame_size = 0_i64;
    let mut frame_pointer_established = false;
    let mut seen_addrs = HashSet::new();
    let mut started = false;
    let register_namer = RegisterNamer::from_options(options);
    for op in &entry.ops {
        if matches!(op.opcode, PcodeOpcode::Copy)
            && let (Some(output), Some(input)) = (op.output.as_ref(), op.inputs.first())
            && is_register_varnode(output)
            && is_register_varnode(input)
        {
            let output_name = register_namer.hw_name_at(output.offset, output.size);
            let input_name = register_namer.hw_name_at(input.offset, input.size);
            if matches!(
                (output_name.as_deref(), input_name.as_deref()),
                (Some("ebp"), Some("esp")) | (Some("rbp"), Some("rsp"))
            ) {
                frame_pointer_established = true;
                started = true;
            }
        }
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
            frame_pointer_established = true;
            started = true;
            continue;
        }
        if started {
            break;
        }
    }
    (frame_size, frame_pointer_established)
}

fn parse_signed_asm_immediate(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}
