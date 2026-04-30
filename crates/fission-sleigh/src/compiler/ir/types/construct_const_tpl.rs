
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledConstructTpl {
    pub constructor_hash: u64,
    pub num_labels: u32,
    #[serde(default)]
    pub result: Option<CompiledHandleTpl>,
    pub ops: Vec<CompiledOpTpl>,
}

impl CompiledConstructTpl {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledTemplateSource {
    SpecDerived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledOpTpl {
    /// Flat pcode opcode integer from SLA `ATTRIB_CODE` (Ghidra `PcodeOp` encoding).
    #[serde(default)]
    pub sla_raw_pcode_opcode: u32,
    pub opcode: CompiledOpTplOpcode,
    pub output: Option<CompiledVarnodeTpl>,
    pub inputs: Vec<CompiledVarnodeTpl>,
    pub label: Option<CompiledLabelRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledOpTplOpcode {
    Copy,
    Load,
    Store,
    IntAdd,
    IntSub,
    IntCarry,
    IntSCarry,
    IntSBorrow,
    IntAnd,
    IntOr,
    IntXor,
    IntMult,
    IntLeft,
    IntRight,
    IntSRight,
    IntEqual,
    IntNotEqual,
    IntLess,
    IntSLess,
    BoolNegate,
    BoolAnd,
    BoolOr,
    PopCount,
    IntZExt,
    IntSExt,
    Subpiece,
    Piece,
    Branch,
    BranchInd,
    CBranch,
    Call,
    CallInd,
    Return,
    CallOther,
    Build,
    /// Ghidra `PcodeEmit.appendCrossBuild`: `PTRSUB` placeholder in ConstructTpl.
    CrossBuild,
    /// Ghidra `PcodeEmit.delaySlot`: `INDIRECT` placeholder in ConstructTpl.
    DelaySlotIndirect,
    Label,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledVarnodeTpl {
    Varnode {
        space: CompiledSpaceTpl,
        offset: Box<CompiledConstTpl>,
        size: Box<CompiledConstTpl>,
    },
    HandleTpl(Box<CompiledHandleTpl>),
    Handle {
        operand_index: usize,
    },
    EffectiveAddress {
        operand_index: usize,
    },
    ConditionPredicate,
    Const(CompiledConstTpl),
    Space(CompiledSpaceRef),
    Temp {
        id: u32,
        size: u32,
    },
    Register {
        name: String,
        size: u32,
    },
    FixedRegister {
        reg: CompiledFixedRegister,
        size: u32,
    },
    Flag {
        bit: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledHandleTpl {
    pub space: Option<CompiledSpaceTpl>,
    pub size: Option<CompiledConstTpl>,
    pub ptr_space: Option<CompiledSpaceTpl>,
    pub ptr_offset: Option<CompiledConstTpl>,
    pub ptr_size: Option<CompiledConstTpl>,
    pub temp_space: Option<CompiledSpaceTpl>,
    pub temp_offset: Option<CompiledConstTpl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledSpaceTpl {
    SpaceRef(CompiledSpaceRef),
    Const(Box<CompiledConstTpl>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledConstTpl {
    Real {
        value: u64,
    },
    Handle {
        handle_index: i64,
        selector: CompiledHandleSelector,
        plus: Option<u64>,
    },
    Integer {
        value: i64,
        size: u32,
    },
    RelativeAddress,
    Relative {
        value: u64,
    },
    InstStart,
    InstNext,
    InstNext2,
    CurSpace,
    CurSpaceSize,
    SpaceId(CompiledSpaceRef),
    FlowRef,
    FlowRefSize,
    FlowDest,
    FlowDestSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSpaceRef {
    pub name: String,
    pub index: u64,
    /// Addressable unit size in bytes (ATTRIB_WORDSIZE in Ghidra).
    /// 1 for byte-addressed spaces (RAM, register). Defaults to 1.
    #[serde(default = "default_word_size")]
    pub word_size: u32,
    /// Pointer/address size in bytes (ATTRIB_SIZE in Ghidra).
    /// 4 for 32-bit address spaces, 8 for 64-bit. Defaults to 0 (unknown).
    #[serde(default)]
    pub addr_size: u32,
    /// Ghidra `ATTRIB_DELAY` on `space` (`-1` when absent). For `ELEM_SPACE`, `delay > 0`
    /// implies `AddressSpace.TYPE_RAM`; otherwise Ghidra treats it as register-class.
    #[serde(default = "default_sleigh_delay_attr")]
    pub sleigh_delay_slots: i32,
    /// `true` for `ELEM_SPACE` entries with `delay > 0` (Ghidra `TYPE_RAM`).
    #[serde(default)]
    pub sleigh_is_ram_class: bool,
    /// `true` for `space_unique` elements (Ghidra `TYPE_UNIQUE`).
    #[serde(default)]
    pub sleigh_is_unique_space: bool,
}

fn default_word_size() -> u32 {
    1
}

fn default_sleigh_delay_attr() -> i32 {
    -1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledLabelRef {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledHandleSelector {
    Space,
    Offset,
    Size,
    OffsetPlus,
}

