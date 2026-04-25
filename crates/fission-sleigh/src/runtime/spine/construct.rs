use crate::compiler::{
    CompiledConstructTplKind, CompiledConstructorTemplate, CompiledOperandSpec, CompiledSpaceRef,
};

use super::RuntimeMatchTrace;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructState {
    pub construct_tpl_kind: CompiledConstructTplKind,
    pub constructor_template: CompiledConstructorTemplate,
    pub construct_nodes: Vec<RuntimeConstructNode>,
    pub handles: Vec<RuntimeHandle>,
    pub operands: Vec<BoundOperand>,
    pub condition_code: Option<u8>,
    pub length: usize,
    pub match_trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructNode {
    pub operand_index: Option<usize>,
    pub parent_index: Option<usize>,
    pub absolute_offset: usize,
    pub relative_length: usize,
    pub handle_index: Option<usize>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeHandle {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
    pub value: BoundOperand,
    pub fixed: RuntimeFixedHandle,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RuntimeFixedHandle {
    pub space: Option<CompiledSpaceRef>,
    pub size: u32,
    pub offset_space: Option<CompiledSpaceRef>,
    pub offset_offset: u64,
    pub offset_size: u32,
    pub temp_space: Option<CompiledSpaceRef>,
    pub temp_offset: u64,
    pub fixable: bool,
}

#[derive(Debug, Clone)]
pub enum BoundOperand {
    Register {
        index: u8,
        size: u32,
    },
    Memory {
        base: Option<u8>,
        index: Option<u8>,
        scale: u8,
        displacement: i64,
        rip_relative: bool,
        absolute: Option<u64>,
        size: u32,
    },
    Immediate {
        value: u64,
        encoded_size: u32,
        signed: bool,
    },
    Relative {
        target: u64,
    },
}
