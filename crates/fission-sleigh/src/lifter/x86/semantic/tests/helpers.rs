use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

pub(super) fn has_flag_write(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.output
            .as_ref()
            .map(|out| out.space_id == flag.space_id && out.offset == flag.offset)
            .unwrap_or(false)
    })
}

pub(super) fn has_flag_zero_copy(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.opcode == PcodeOpcode::Copy
            && op
                .output
                .as_ref()
                .map(|out| out.space_id == flag.space_id && out.offset == flag.offset)
                .unwrap_or(false)
            && op.inputs.len() == 1
            && op.inputs[0].is_constant
            && op.inputs[0].constant_val == 0
            && op.inputs[0].size == 1
    })
}

pub(super) fn has_flag_input(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.inputs
            .iter()
            .any(|inp| inp.space_id == flag.space_id && inp.offset == flag.offset)
    })
}

pub(super) fn has_pf_pipeline(ops: &[PcodeOp]) -> bool {
    let has_low8 = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_LOW8") && op.opcode == PcodeOpcode::IntAnd);
    let has_pop = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_POPCNT") && op.opcode == PcodeOpcode::PopCount);
    let has_lsb = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_LSB") && op.opcode == PcodeOpcode::IntAnd);
    let has_set = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SET_PF") && op.opcode == PcodeOpcode::IntEqual);
    has_low8 && has_pop && has_lsb && has_set
}
