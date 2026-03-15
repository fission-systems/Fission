use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeOp};

mod bootstrap_x86;
mod normalize_arith;
mod normalize_bitstream;
mod normalize_slots;
mod structuring_conditionals;
mod structuring_linear;
mod structuring_loops;
mod structuring_misc;
mod structuring_switch;
mod type_hints_aggregates;
mod type_hints_aliases;
mod type_hints_imports;
mod type_hints_stack_slots;

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn uniq(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn cst(value: i64, size: u32) -> Varnode {
    Varnode::constant(value, size)
}

fn preview_options() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0x1400_0000,
        sections: vec![(0x1400_1000, 0x1400_2000)],
    }
}

fn preview_options_x86() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: false,
        pointer_size: 4,
        format: "PE".to_string(),
        image_base: 0x400000,
        sections: vec![(0x401000, 0x402000)],
    }
}
