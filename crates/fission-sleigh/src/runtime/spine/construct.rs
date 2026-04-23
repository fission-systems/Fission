use crate::compiler::{CompiledConstructorTemplate, CompiledOperandSpec, CompiledSemanticKind};

use super::RuntimeMatchTrace;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeConstructState {
    pub semantic_kind: CompiledSemanticKind,
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

pub fn operand_size(operand: &BoundOperand) -> u32 {
    match operand {
        BoundOperand::Register { size, .. } | BoundOperand::Memory { size, .. } => *size,
        BoundOperand::Immediate { encoded_size, .. } => *encoded_size,
        BoundOperand::Relative { .. } => 8,
    }
}
