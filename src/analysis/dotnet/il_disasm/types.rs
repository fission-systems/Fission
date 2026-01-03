//! IL Types
//!
//! Core types for IL instruction representation

/// A single IL instruction with decoded opcode and operand text.
#[derive(Debug, Clone)]
pub struct ILInstruction {
    pub offset: u32,
    pub opcode: String,
    pub operand: Option<String>,
    pub size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OperandType {
    InlineNone,
    InlineBrTarget,
    InlineField,
    InlineI,
    InlineI8,
    InlineMethod,
    InlineR,
    InlineSig,
    InlineString,
    InlineSwitch,
    InlineTok,
    InlineType,
    InlineVar,
    ShortInlineBrTarget,
    ShortInlineI,
    ShortInlineR,
    ShortInlineVar,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OpCodeDef {
    pub code: u16,
    pub name: &'static str,
    pub operand: OperandType,
    pub size: u8,
}
