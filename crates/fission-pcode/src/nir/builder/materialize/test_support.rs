use super::*;

pub(super) fn int(bits: u32) -> NirType {
    NirType::Int {
        bits,
        signed: false,
    }
}

pub(super) fn varnode(offset: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
}

pub(super) fn constant(value: i64) -> Varnode {
    Varnode::constant(value, 8)
}

pub(super) fn op(
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
        asm_mnemonic: None,
    }
}

pub(super) fn block(ops: Vec<PcodeOp>) -> crate::pcode::PcodeBasicBlock {
    crate::pcode::PcodeBasicBlock {
        index: 0,
        start_address: 0x1000,
        successors: Vec::new(),
        ops,
    }
}

pub(super) fn block_at(
    start_address: u64,
    index: u32,
    ops: Vec<PcodeOp>,
) -> crate::pcode::PcodeBasicBlock {
    crate::pcode::PcodeBasicBlock {
        index,
        start_address,
        successors: Vec::new(),
        ops,
    }
}

pub(super) fn pcode_function(
    blocks: Vec<crate::pcode::PcodeBasicBlock>,
) -> crate::pcode::PcodeFunction {
    crate::pcode::PcodeFunction { blocks }
}

pub(super) fn test_options() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0x1400_0000,
        sections: vec![(0x1400_1000, 0x1400_2000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        relocation_names: Default::default(),
        calling_convention: Default::default(),
    }
}
