impl CompiledOpTpl {
    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }

    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        if let Some(output) = &self.output {
            if let Some(reason) = output.ghidra_template_shape_error() {
                return Some(reason);
            }
        }
        self.inputs
            .iter()
            .find_map(CompiledVarnodeTpl::ghidra_template_shape_error)
    }
}

impl CompiledVarnodeTpl {
    pub fn is_ghidra_template_shape(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }

    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        match self {
            Self::Varnode { .. } => None,
            Self::HandleTpl(handle) => handle.ghidra_template_shape_error(),
            Self::Handle { .. } => Some("compatibility_handle_varnode"),
            Self::EffectiveAddress { .. } => Some("compatibility_effective_address_varnode"),
            Self::ConditionPredicate => Some("compatibility_condition_predicate_varnode"),
            Self::Const(_) => Some("compatibility_const_varnode"),
            Self::Space(_) => Some("compatibility_space_varnode"),
            Self::Temp { .. } => Some("compatibility_temp_varnode"),
            Self::Register { .. } => Some("compatibility_register_varnode"),
            Self::FixedRegister { .. } => Some("compatibility_fixed_register_varnode"),
            Self::Flag { .. } => Some("compatibility_flag_varnode"),
        }
    }
}

impl CompiledConstructorTemplate {
    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        if let Some(result) = &self.result {
            if let Some(reason) = result.ghidra_template_shape_error() {
                return Some(reason);
            }
        }
        self.ops
            .iter()
            .find_map(CompiledOpTpl::ghidra_template_shape_error)
    }

    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }
}

impl CompiledHandleTpl {
    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledArithmeticOpcode {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Mul,
    Shl,
    Shr,
    Sar,
    Inc,
    Dec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledFixedRegister {
    Accumulator,
    StackPointer,
    FramePointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledConstructTplKind {
    Unsupported,
    Generic,
    Nop,
    Ret,
    Call,
    Jmp,
    Jcc,
    Mov,
    AddressOf,
    StackStore,
    StackLoad,
    FrameTeardown,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Imul,
    Mul,
    Shl,
    Shr,
    Sar,
    Inc,
    Dec,
    Cmp,
    Test,
    Movzx,
    Movsx,
    Movsxd,
    Setcc,
    Cbw,
    Cwde,
    Cdqe,
}

impl CompiledConstructTplKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Nop => "nop",
            Self::Ret => "ret",
            Self::Call => "call",
            Self::Jmp => "jmp",
            Self::Jcc => "jcc",
            Self::Mov => "mov",
            Self::AddressOf => "lea",
            Self::StackStore => "push",
            Self::StackLoad => "pop",
            Self::FrameTeardown => "leave",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Imul => "imul",
            Self::Mul => "mul",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Sar => "sar",
            Self::Inc => "inc",
            Self::Dec => "dec",
            Self::Cmp => "cmp",
            Self::Test => "test",
            Self::Movzx => "movzx",
            Self::Movsx => "movsx",
            Self::Movsxd => "movsxd",
            Self::Setcc => "setcc",
            Self::Cbw => "cbw",
            Self::Cwde => "cwde",
            Self::Cdqe => "cdqe",
            Self::Generic => "generic",
        }
    }
}

impl CompiledDecisionProbe {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::InstructionBitSlice { .. } => "instruction_bit_slice",
            Self::ContextBitSlice { .. } => "context_bit_slice",
            Self::SlaInstructionBits { .. } => "sla_instruction_bits",
            Self::SlaContextBits { .. } => "sla_context_bits",
            Self::TokenFieldRef(CompiledTokenFieldRef::InstructionWidthProfile) => {
                "token_field_instruction_width"
            }
            Self::TokenFieldRef(CompiledTokenFieldRef::AddressingForm) => {
                "token_field_addressing_form"
            }
            Self::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
                "token_field_register_selector"
            }
            Self::ContextFieldRef(CompiledContextFieldRef::DefaultContext) => {
                "context_field_default"
            }
            Self::TerminalPatternCheck => "terminal_pattern_check",
        }
    }
}

impl CompiledTemplateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpecDerived => "spec_derived",
        }
    }
}

impl CompiledOpTplOpcode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Copy => "COPY",
            Self::Load => "LOAD",
            Self::Store => "STORE",
            Self::IntAdd => "INT_ADD",
            Self::IntSub => "INT_SUB",
            Self::IntCarry => "INT_CARRY",
            Self::IntSCarry => "INT_SCARRY",
            Self::IntSBorrow => "INT_SBORROW",
            Self::IntAnd => "INT_AND",
            Self::IntOr => "INT_OR",
            Self::IntXor => "INT_XOR",
            Self::IntMult => "INT_MULT",
            Self::IntLeft => "INT_LEFT",
            Self::IntRight => "INT_RIGHT",
            Self::IntSRight => "INT_SRIGHT",
            Self::IntEqual => "INT_EQUAL",
            Self::IntNotEqual => "INT_NOTEQUAL",
            Self::IntLess => "INT_LESS",
            Self::IntSLess => "INT_SLESS",
            Self::BoolNegate => "BOOL_NEGATE",
            Self::BoolAnd => "BOOL_AND",
            Self::BoolOr => "BOOL_OR",
            Self::PopCount => "POPCOUNT",
            Self::IntZExt => "INT_ZEXT",
            Self::IntSExt => "INT_SEXT",
            Self::Subpiece => "SUBPIECE",
            Self::Piece => "PIECE",
            Self::Branch => "BRANCH",
            Self::BranchInd => "BRANCHIND",
            Self::CBranch => "CBRANCH",
            Self::Call => "CALL",
            Self::CallInd => "CALLIND",
            Self::Return => "RETURN",
            Self::CallOther => "CALLOTHER",
            Self::Build => "BUILD",
            Self::CrossBuild => "CROSSBUILD",
            Self::DelaySlotIndirect => "DELAYSLOT_INDIRECT",
            Self::Label => "LABEL",
            Self::Unsupported => "UNSUPPORTED",
        }
    }
}

impl CompiledArithmeticOpcode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Mul => "mul",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Sar => "sar",
            Self::Inc => "inc",
            Self::Dec => "dec",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSemanticTemplate {
    pub status: String,
    pub action_hash: u64,
    pub op_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPcodeOp {
    pub name: String,
    pub defined_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPatternNode {
    pub node_id: String,
    pub source: String,
    pub mnemonic: String,
    pub with_depth: usize,
    pub control_flow: ControlFlowClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ControlFlowClass {
    None,
    Branch,
    ConditionalBranch,
    Call,
    Return,
    Mixed,
}

impl ControlFlowClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Branch => "branch",
            Self::ConditionalBranch => "conditional_branch",
            Self::Call => "call",
            Self::Return => "return",
            Self::Mixed => "mixed",
        }
    }
}
