use crate::{BitField, Encoding, FslError};
use std::fmt;

pub const FSL_PACKAGE_VERSION: u16 = 7;
const STATE_PACKAGE_VERSION: u16 = 3;
const CARRY_IN_PACKAGE_VERSION: u16 = 4;
const MAGIC: &[u8; 8] = b"FSLCPKG\0";
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STRING_BYTES: usize = 65_535;
const MAX_INSTRUCTIONS: usize = 256;
const MAX_VALUES_PER_INSTRUCTION: usize = u16::MAX as usize;
const MAX_OPS_PER_INSTRUCTION: usize = u16::MAX as usize;
const MAX_EVIDENCE_PER_INSTRUCTION: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressUnit {
    Byte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerSign {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueType {
    pub bits: u16,
    pub sign: IntegerSign,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            if self.sign == IntegerSign::Signed {
                'i'
            } else {
                'u'
            },
            self.bits
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u16);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueDef {
    pub id: ValueId,
    pub name: String,
    pub ty: ValueType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub source_id: String,
    pub url: String,
    pub revision: String,
    pub claim: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntPredicate {
    Equal,
    UnsignedLess,
    SignedLess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirEdge {
    pub target: u16,
    pub arguments: Vec<ValueId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirTerminator {
    Return,
    Branch(FirEdge),
    CondBranch {
        condition: ValueId,
        on_true: FirEdge,
        on_false: FirEdge,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirBlock {
    pub name: String,
    pub parameters: Vec<ValueId>,
    /// This block owns this contiguous range in the instruction's sole op table.
    pub start: u16,
    pub end: u16,
    pub terminator: FirTerminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntConversion {
    ZeroExtend,
    SignExtend,
    Truncate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirOp {
    IntConvert {
        output: ValueId,
        input: ValueId,
        kind: IntConversion,
    },
    IntConstant {
        output: ValueId,
        value: u64,
    },
    IntCompare {
        output: ValueId,
        left: ValueId,
        right: ValueId,
        predicate: IntPredicate,
    },
    /// Semantic support is absent; this is never an executable no-op.
    Unsupported,
    /// Snapshot of the lane activation mask; extent is explicit and <=64.
    LaneMaskRead {
        output: ValueId,
        lanes: u16,
    },
    /// Per-lane read. Raw selector minus bias indexes the lane register bank.
    LaneRead {
        output: ValueId,
        field: u16,
        bias: u64,
        mask: ValueId,
    },
    /// Ordered write under the specified mask snapshot. Uniform values broadcast.
    LaneWrite {
        field: u16,
        value: ValueId,
        mask: ValueId,
    },
    RegisterRead {
        output: ValueId,
        field: u16,
    },
    FlagRead {
        output: ValueId,
        slot: u16,
    },
    RegisterWrite {
        field: u16,
        value: ValueId,
    },
    FlagWrite {
        slot: u16,
        value: ValueId,
    },
    IntAddCarry {
        output: ValueId,
        left: ValueId,
        right: ValueId,
    },
    IntAddCarryIn {
        output: ValueId,
        left: ValueId,
        right: ValueId,
        carry: ValueId,
    },
    IntAddWrapCarry {
        output: ValueId,
        left: ValueId,
        right: ValueId,
        carry: ValueId,
    },
    VmStackPop {
        output: ValueId,
    },
    IntAddWrap {
        output: ValueId,
        left: ValueId,
        right: ValueId,
    },
    VmStackPush {
        value: ValueId,
    },
}

impl FirOp {
    pub(crate) fn requires_control_version(&self) -> bool {
        matches!(
            self,
            Self::IntConstant { .. } | Self::IntCompare { .. } | Self::IntConvert { .. }
        )
    }
    pub(crate) fn requires_lane_version(&self) -> bool {
        matches!(
            self,
            Self::LaneMaskRead { .. } | Self::LaneRead { .. } | Self::LaneWrite { .. }
        )
    }
    pub(crate) fn requires_state_version(&self) -> bool {
        self.requires_lane_version()
            || matches!(
                self,
                Self::RegisterRead { .. }
                    | Self::FlagRead { .. }
                    | Self::RegisterWrite { .. }
                    | Self::FlagWrite { .. }
                    | Self::IntAddCarry { .. }
                    | Self::IntAddCarryIn { .. }
                    | Self::IntAddWrapCarry { .. }
            )
    }

    pub(crate) fn requires_carry_in_version(&self) -> bool {
        matches!(
            self,
            Self::FlagRead { .. } | Self::IntAddCarryIn { .. } | Self::IntAddWrapCarry { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInstruction {
    pub name: String,
    pub mnemonic: String,
    pub encoding: Encoding,
    pub evidence: Vec<Evidence>,
    pub values: Vec<ValueDef>,
    pub ops: Vec<FirOp>,
    /// Empty for legacy linear bodies; consumers treat those as entry + return.
    pub blocks: Vec<FirBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FslcPackage {
    pub version: u16,
    pub language: String,
    pub byte_order: ByteOrder,
    pub address_unit: AddressUnit,
    pub instructions: Vec<CompiledInstruction>,
    pub(crate) dispatch: [Option<u16>; 256],
}

impl FslcPackage {
    /// Validate the package and canonical FIR at every consumer boundary.
    pub fn validate(&self) -> Result<(), FslError> {
        if !matches!(self.version, 1..=FSL_PACKAGE_VERSION) {
            return Err(FslError::at(1, 1, "unsupported FSL package version"));
        }
        if self.instructions.is_empty() || self.instructions.len() > MAX_INSTRUCTIONS {
            return Err(FslError::at(1, 1, "invalid instruction count"));
        }
        let check_string = |value: &str| -> Result<(), FslError> {
            if value.is_empty() || value.len() > MAX_STRING_BYTES {
                return Err(FslError::at(
                    1,
                    1,
                    "package metadata must be nonempty and fit the string format",
                ));
            }
            Ok(())
        };
        check_string(&self.language)?;
        let mut dispatch = [None; 256];
        let mut names = std::collections::HashSet::new();
        for (index, instruction) in self.instructions.iter().enumerate() {
            check_string(&instruction.name)?;
            check_string(&instruction.mnemonic)?;
            if !names.insert(&instruction.name) {
                return Err(FslError::at(1, 1, "duplicate instruction name or opcode"));
            }
            if instruction.evidence.is_empty()
                || instruction.evidence.len() > MAX_EVIDENCE_PER_INSTRUCTION
            {
                return Err(FslError::at(1, 1, "invalid evidence count"));
            }
            for evidence in &instruction.evidence {
                for field in [
                    &evidence.source_id,
                    &evidence.url,
                    &evidence.revision,
                    &evidence.claim,
                ] {
                    check_string(field)?;
                }
            }
            instruction.encoding.validate()?;
            if self.version < 7
                && instruction
                    .ops
                    .iter()
                    .any(|op| matches!(op, FirOp::IntConvert { .. }))
            {
                return Err(FslError::at(
                    1,
                    1,
                    "integer conversion requires package version 7",
                ));
            }
            if self.version < 6
                && (!instruction.blocks.is_empty()
                    || instruction.ops.iter().any(FirOp::requires_control_version))
            {
                return Err(FslError::at(
                    1,
                    1,
                    "block/constant/comparison FIR requires package version 6",
                ));
            }
            if instruction.encoding.bits != self.instructions[0].encoding.bits {
                return Err(FslError::at(
                    1,
                    1,
                    "fixed-width profiles cannot mix encoding widths",
                ));
            }
            if self.version == 1
                && (instruction.encoding.opcode().is_none()
                    || instruction.ops.contains(&FirOp::Unsupported))
            {
                return Err(FslError::at(
                    1,
                    1,
                    "version 1 cannot represent encoding plans or unsupported semantics",
                ));
            }
            if self.version < STATE_PACKAGE_VERSION
                && instruction.ops.iter().any(|op| op.requires_state_version())
            {
                return Err(FslError::at(1, 1, "state FIR requires package version 3"));
            }
            if self.version < 5 && instruction.ops.iter().any(FirOp::requires_lane_version) {
                return Err(FslError::at(1, 1, "lane FIR requires package version 5"));
            }
            if self.version < CARRY_IN_PACKAGE_VERSION
                && instruction
                    .ops
                    .iter()
                    .any(|op| op.requires_carry_in_version())
            {
                return Err(FslError::at(
                    1,
                    1,
                    "carry-input FIR requires package version 4",
                ));
            }
            for previous in &self.instructions[..index] {
                let shared = previous.encoding.mask & instruction.encoding.mask;
                if (previous.encoding.value ^ instruction.encoding.value) & shared == 0 {
                    return Err(FslError::at(
                        1,
                        1,
                        "encoding patterns overlap; explicit disjoint masks are required",
                    ));
                }
            }
            instruction.validate()?;
            for value in &instruction.values {
                check_string(&value.name)?;
            }
            if let Some(opcode) = instruction.encoding.opcode() {
                dispatch[opcode as usize] = Some(index as u16);
            }
        }
        if self.dispatch != dispatch {
            return Err(FslError::at(
                1,
                1,
                "FSL dispatch does not match instruction definitions",
            ));
        }
        Ok(())
    }

    pub fn instruction_for_opcode(&self, opcode: u8) -> Option<&CompiledInstruction> {
        self.dispatch[opcode as usize].and_then(|index| self.instructions.get(index as usize))
    }

    pub fn encode_binary(&self) -> Result<Vec<u8>, FslError> {
        self.validate()?;
        let mut writer = Writer::default();
        writer.bytes.extend_from_slice(MAGIC);
        writer.u16(self.version);
        writer.string(&self.language)?;
        writer.u8(match self.byte_order {
            ByteOrder::Little => 0,
            ByteOrder::Big => 1,
        });
        writer.u8(match self.address_unit {
            AddressUnit::Byte => 1,
        });
        writer.u16(u16::try_from(self.instructions.len()).map_err(|_| {
            FslError::at(
                1,
                1,
                "too many instructions for the current FSL package format",
            )
        })?);
        for instruction in &self.instructions {
            writer.string(&instruction.name)?;
            writer.string(&instruction.mnemonic)?;
            if self.version == 1 {
                writer.u8(instruction
                    .encoding
                    .opcode()
                    .ok_or_else(|| FslError::at(1, 1, "invalid version 1 encoding"))?);
            } else {
                writer.u16(instruction.encoding.bits);
                writer.u128(instruction.encoding.mask);
                writer.u128(instruction.encoding.value);
                writer.u16(count_u16(
                    instruction.encoding.fields.len(),
                    "encoding fields",
                )?);
                for field in &instruction.encoding.fields {
                    writer.string(&field.name)?;
                    writer.u16(field.offset);
                    writer.u16(field.bits);
                    writer.u16(count_u16(field.excluded.len(), "excluded field values")?);
                    for value in &field.excluded {
                        writer.u64(*value);
                    }
                }
            }
            writer.u16(count_u16(instruction.evidence.len(), "evidence entries")?);
            for evidence in &instruction.evidence {
                writer.string(&evidence.source_id)?;
                writer.string(&evidence.url)?;
                writer.string(&evidence.revision)?;
                writer.string(&evidence.claim)?;
            }
            writer.u16(count_u16(instruction.values.len(), "FIR values")?);
            for value in &instruction.values {
                writer.u16(value.id.0);
                writer.string(&value.name)?;
                writer.u8(match value.ty.sign {
                    IntegerSign::Signed => 0,
                    IntegerSign::Unsigned => 1,
                });
                writer.u16(value.ty.bits);
            }
            writer.u16(count_u16(instruction.ops.len(), "FIR operations")?);
            for op in &instruction.ops {
                match op {
                    FirOp::IntConvert {
                        output,
                        input,
                        kind,
                    } => {
                        writer.u8(16);
                        writer.u16(output.0);
                        writer.u16(input.0);
                        writer.u8(match kind {
                            IntConversion::ZeroExtend => 0,
                            IntConversion::SignExtend => 1,
                            IntConversion::Truncate => 2,
                        });
                    }
                    FirOp::IntConstant { output, value } => {
                        writer.u8(14);
                        writer.u16(output.0);
                        writer.u64(*value);
                    }
                    FirOp::IntCompare {
                        output,
                        left,
                        right,
                        predicate,
                    } => {
                        writer.u8(15);
                        writer.u16(output.0);
                        writer.u16(left.0);
                        writer.u16(right.0);
                        writer.u8(match predicate {
                            IntPredicate::Equal => 0,
                            IntPredicate::UnsignedLess => 1,
                            IntPredicate::SignedLess => 2,
                        });
                    }
                    FirOp::LaneMaskRead { output, lanes } => {
                        writer.u8(11);
                        writer.u16(output.0);
                        writer.u16(*lanes);
                    }
                    FirOp::LaneRead {
                        output,
                        field,
                        bias,
                        mask,
                    } => {
                        writer.u8(12);
                        writer.u16(output.0);
                        writer.u16(*field);
                        writer.u64(*bias);
                        writer.u16(mask.0);
                    }
                    FirOp::LaneWrite { field, value, mask } => {
                        writer.u8(13);
                        writer.u16(*field);
                        writer.u16(value.0);
                        writer.u16(mask.0);
                    }
                    FirOp::RegisterRead { output, field } => {
                        writer.u8(4);
                        writer.u16(output.0);
                        writer.u16(*field);
                    }
                    FirOp::FlagRead { output, slot } => {
                        writer.u8(8);
                        writer.u16(output.0);
                        writer.u16(*slot);
                    }
                    FirOp::RegisterWrite { field, value } => {
                        writer.u8(5);
                        writer.u16(*field);
                        writer.u16(value.0);
                    }
                    FirOp::FlagWrite { slot, value } => {
                        writer.u8(6);
                        writer.u16(*slot);
                        writer.u16(value.0);
                    }
                    FirOp::IntAddCarry {
                        output,
                        left,
                        right,
                    } => {
                        writer.u8(7);
                        writer.u16(output.0);
                        writer.u16(left.0);
                        writer.u16(right.0);
                    }
                    FirOp::IntAddCarryIn {
                        output,
                        left,
                        right,
                        carry,
                    } => {
                        writer.u8(9);
                        writer.u16(output.0);
                        writer.u16(left.0);
                        writer.u16(right.0);
                        writer.u16(carry.0);
                    }
                    FirOp::IntAddWrapCarry {
                        output,
                        left,
                        right,
                        carry,
                    } => {
                        writer.u8(10);
                        writer.u16(output.0);
                        writer.u16(left.0);
                        writer.u16(right.0);
                        writer.u16(carry.0);
                    }
                    FirOp::Unsupported => writer.u8(3),
                    FirOp::VmStackPop { output } => {
                        writer.u8(0);
                        writer.u16(output.0);
                    }
                    FirOp::IntAddWrap {
                        output,
                        left,
                        right,
                    } => {
                        writer.u8(1);
                        writer.u16(output.0);
                        writer.u16(left.0);
                        writer.u16(right.0);
                    }
                    FirOp::VmStackPush { value } => {
                        writer.u8(2);
                        writer.u16(value.0);
                    }
                }
            }
            if self.version >= 6 {
                writer.u16(count_u16(instruction.blocks.len(), "FIR blocks")?);
                for block in &instruction.blocks {
                    writer.string(&block.name)?;
                    writer.u16(count_u16(block.parameters.len(), "block parameters")?);
                    for p in &block.parameters {
                        writer.u16(p.0);
                    }
                    writer.u16(block.start);
                    writer.u16(block.end);
                    match &block.terminator {
                        FirTerminator::Return => writer.u8(0),
                        FirTerminator::Branch(edge) => {
                            writer.u8(1);
                            writer.edge(edge)?;
                        }
                        FirTerminator::CondBranch {
                            condition,
                            on_true,
                            on_false,
                        } => {
                            writer.u8(2);
                            writer.u16(condition.0);
                            writer.edge(on_true)?;
                            writer.edge(on_false)?;
                        }
                    }
                }
            }
        }
        if writer.bytes.len() > MAX_PACKAGE_BYTES {
            return Err(FslError::at(1, 1, "compiled FSL package exceeds 64 MiB"));
        }
        Ok(writer.bytes)
    }

    pub fn decode_binary(bytes: &[u8]) -> Result<Self, FslError> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(FslError::at(1, 1, "compiled FSL package exceeds 64 MiB"));
        }
        let mut reader = Reader { bytes, cursor: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(FslError::at(1, 1, "invalid FSL package magic"));
        }
        let version = reader.u16()?;
        if !matches!(version, 1..=FSL_PACKAGE_VERSION) {
            return Err(FslError::at(
                1,
                1,
                format!("unsupported FSL package version {version}"),
            ));
        }
        let language = reader.string()?;
        let byte_order = match reader.u8()? {
            0 => ByteOrder::Little,
            1 => ByteOrder::Big,
            value => {
                return Err(FslError::at(
                    1,
                    1,
                    format!("invalid byte-order tag {value}"),
                ))
            }
        };
        let address_unit = match reader.u8()? {
            1 => AddressUnit::Byte,
            value => {
                return Err(FslError::at(
                    1,
                    1,
                    format!("invalid address-unit tag {value}"),
                ))
            }
        };
        let instruction_count = reader.u16()? as usize;
        if instruction_count == 0 || instruction_count > MAX_INSTRUCTIONS {
            return Err(FslError::at(
                1,
                1,
                "invalid instruction count in FSL package",
            ));
        }
        let mut instructions = Vec::with_capacity(instruction_count);
        let mut dispatch = [None; 256];
        for index in 0..instruction_count {
            let name = reader.string()?;
            let mnemonic = reader.string()?;
            let encoding = if version == 1 {
                Encoding::byte_opcode(reader.u8()?)
            } else {
                let bits = reader.u16()?;
                let mask = reader.u128()?;
                let value = reader.u128()?;
                let field_count = usize::from(reader.u16()?);
                if field_count > 256 {
                    return Err(FslError::at(1, 1, "too many encoding fields"));
                }
                let mut fields = Vec::with_capacity(field_count);
                for _ in 0..field_count {
                    let name = reader.string()?;
                    let offset = reader.u16()?;
                    let bits = reader.u16()?;
                    let count = usize::from(reader.u16()?);
                    if count > 256 {
                        return Err(FslError::at(1, 1, "too many excluded field values"));
                    }
                    let mut excluded = Vec::with_capacity(count);
                    for _ in 0..count {
                        excluded.push(reader.u64()?);
                    }
                    fields.push(BitField {
                        name,
                        offset,
                        bits,
                        excluded,
                    });
                }
                Encoding {
                    bits,
                    mask,
                    value,
                    fields,
                }
            };
            encoding.validate()?;
            let evidence_count = reader.u16()? as usize;
            if evidence_count == 0 || evidence_count > MAX_EVIDENCE_PER_INSTRUCTION {
                return Err(FslError::at(1, 1, "invalid evidence count in FSL package"));
            }
            let mut evidence = Vec::with_capacity(evidence_count);
            for _ in 0..evidence_count {
                evidence.push(Evidence {
                    source_id: reader.string()?,
                    url: reader.string()?,
                    revision: reader.string()?,
                    claim: reader.string()?,
                });
            }
            let value_count = reader.u16()? as usize;
            if value_count > MAX_VALUES_PER_INSTRUCTION {
                return Err(FslError::at(1, 1, "invalid FIR value count in FSL package"));
            }
            let mut values = Vec::with_capacity(value_count);
            for expected_id in 0..value_count {
                let id = reader.u16()?;
                if id as usize != expected_id {
                    return Err(FslError::at(1, 1, "FIR values are not densely indexed"));
                }
                let name = reader.string()?;
                let sign = match reader.u8()? {
                    0 => IntegerSign::Signed,
                    1 => IntegerSign::Unsigned,
                    value => return Err(FslError::at(1, 1, format!("invalid sign tag {value}"))),
                };
                let bits = reader.u16()?;
                if bits == 0 || bits > 4096 {
                    return Err(FslError::at(1, 1, format!("invalid integer width {bits}")));
                }
                values.push(ValueDef {
                    id: ValueId(id),
                    name,
                    ty: ValueType { bits, sign },
                });
            }
            let op_count = reader.u16()? as usize;
            if (op_count == 0 && version < 6) || op_count > MAX_OPS_PER_INSTRUCTION {
                return Err(FslError::at(
                    1,
                    1,
                    "invalid FIR operation count in FSL package",
                ));
            }
            let mut ops = Vec::with_capacity(op_count);
            for _ in 0..op_count {
                let op = match reader.u8()? {
                    16 if version >= 7 => FirOp::IntConvert {
                        output: reader.value_id(value_count)?,
                        input: reader.value_id(value_count)?,
                        kind: match reader.u8()? {
                            0 => IntConversion::ZeroExtend,
                            1 => IntConversion::SignExtend,
                            2 => IntConversion::Truncate,
                            _ => return Err(FslError::at(1, 1, "unknown FIR conversion kind")),
                        },
                    },
                    14 if version >= 6 => FirOp::IntConstant {
                        output: reader.value_id(value_count)?,
                        value: reader.u64()?,
                    },
                    15 if version >= 6 => FirOp::IntCompare {
                        output: reader.value_id(value_count)?,
                        left: reader.value_id(value_count)?,
                        right: reader.value_id(value_count)?,
                        predicate: match reader.u8()? {
                            0 => IntPredicate::Equal,
                            1 => IntPredicate::UnsignedLess,
                            2 => IntPredicate::SignedLess,
                            _ => {
                                return Err(FslError::at(1, 1, "unknown FIR comparison predicate"))
                            }
                        },
                    },
                    11 if version >= 5 => FirOp::LaneMaskRead {
                        output: reader.value_id(value_count)?,
                        lanes: reader.u16()?,
                    },
                    12 if version >= 5 => FirOp::LaneRead {
                        output: reader.value_id(value_count)?,
                        field: reader.u16()?,
                        bias: reader.u64()?,
                        mask: reader.value_id(value_count)?,
                    },
                    13 if version >= 5 => FirOp::LaneWrite {
                        field: reader.u16()?,
                        value: reader.value_id(value_count)?,
                        mask: reader.value_id(value_count)?,
                    },
                    4 if version >= 3 => FirOp::RegisterRead {
                        output: reader.value_id(value_count)?,
                        field: reader.u16()?,
                    },
                    8 if version >= 4 => FirOp::FlagRead {
                        output: reader.value_id(value_count)?,
                        slot: reader.u16()?,
                    },
                    5 if version >= 3 => FirOp::RegisterWrite {
                        field: reader.u16()?,
                        value: reader.value_id(value_count)?,
                    },
                    6 if version >= 3 => FirOp::FlagWrite {
                        slot: reader.u16()?,
                        value: reader.value_id(value_count)?,
                    },
                    7 if version >= 3 => FirOp::IntAddCarry {
                        output: reader.value_id(value_count)?,
                        left: reader.value_id(value_count)?,
                        right: reader.value_id(value_count)?,
                    },
                    9 if version >= 4 => FirOp::IntAddCarryIn {
                        output: reader.value_id(value_count)?,
                        left: reader.value_id(value_count)?,
                        right: reader.value_id(value_count)?,
                        carry: reader.value_id(value_count)?,
                    },
                    10 if version >= 4 => FirOp::IntAddWrapCarry {
                        output: reader.value_id(value_count)?,
                        left: reader.value_id(value_count)?,
                        right: reader.value_id(value_count)?,
                        carry: reader.value_id(value_count)?,
                    },
                    3 if version >= 2 => FirOp::Unsupported,
                    0 => FirOp::VmStackPop {
                        output: reader.value_id(value_count)?,
                    },
                    1 => FirOp::IntAddWrap {
                        output: reader.value_id(value_count)?,
                        left: reader.value_id(value_count)?,
                        right: reader.value_id(value_count)?,
                    },
                    2 => FirOp::VmStackPush {
                        value: reader.value_id(value_count)?,
                    },
                    value => return Err(FslError::at(1, 1, format!("invalid FIR op tag {value}"))),
                };
                ops.push(op);
            }
            let mut blocks = Vec::new();
            if version >= 6 {
                let count = usize::from(reader.u16()?);
                if count > 256 {
                    return Err(FslError::at(1, 1, "too many FIR blocks"));
                }
                for _ in 0..count {
                    let name = reader.string()?;
                    let parameters_count = usize::from(reader.u16()?);
                    if parameters_count > value_count {
                        return Err(FslError::at(1, 1, "invalid block parameter count"));
                    }
                    let mut parameters = Vec::with_capacity(parameters_count);
                    for _ in 0..parameters_count {
                        parameters.push(reader.value_id(value_count)?);
                    }
                    let start = reader.u16()?;
                    let end = reader.u16()?;
                    let terminator = match reader.u8()? {
                        0 => FirTerminator::Return,
                        1 => FirTerminator::Branch(reader.edge(value_count)?),
                        2 => FirTerminator::CondBranch {
                            condition: reader.value_id(value_count)?,
                            on_true: reader.edge(value_count)?,
                            on_false: reader.edge(value_count)?,
                        },
                        _ => return Err(FslError::at(1, 1, "unknown FIR terminator")),
                    };
                    blocks.push(FirBlock {
                        name,
                        parameters,
                        start,
                        end,
                        terminator,
                    });
                }
            }
            if let Some(opcode) = encoding.opcode() {
                dispatch[opcode as usize] = Some(index as u16);
            }
            instructions.push(CompiledInstruction {
                name,
                mnemonic,
                encoding,
                evidence,
                values,
                ops,
                blocks,
            });
        }
        if reader.cursor != bytes.len() {
            return Err(FslError::at(1, 1, "trailing bytes after FSL package"));
        }
        let package = Self {
            version,
            language,
            byte_order,
            address_unit,
            instructions,
            dispatch,
        };
        package.validate()?;
        Ok(package)
    }
}

fn count_u16(value: usize, description: &str) -> Result<u16, FslError> {
    u16::try_from(value)
        .map_err(|_| FslError::at(1, 1, format!("too many {description} for FSL package")))
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn edge(&mut self, edge: &FirEdge) -> Result<(), FslError> {
        self.u16(edge.target);
        self.u16(count_u16(edge.arguments.len(), "branch arguments")?);
        for arg in &edge.arguments {
            self.u16(arg.0);
        }
        Ok(())
    }
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u128(&mut self, value: u128) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: &str) -> Result<(), FslError> {
        if value.len() > MAX_STRING_BYTES {
            return Err(FslError::at(
                1,
                1,
                "string exceeds 65535 bytes in FSL package",
            ));
        }
        self.u16(value.len() as u16);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn edge(&mut self, value_count: usize) -> Result<FirEdge, FslError> {
        let target = self.u16()?;
        let count = usize::from(self.u16()?);
        if count > value_count {
            return Err(FslError::at(1, 1, "invalid branch argument count"));
        }
        let mut arguments = Vec::with_capacity(count);
        for _ in 0..count {
            arguments.push(self.value_id(value_count)?);
        }
        Ok(FirEdge { target, arguments })
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], FslError> {
        let end = self
            .cursor
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| FslError::at(1, 1, "truncated FSL package"))?;
        let result = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8, FslError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, FslError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u64(&mut self) -> Result<u64, FslError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn u128(&mut self) -> Result<u128, FslError> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, FslError> {
        let length = self.u16()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| FslError::at(1, 1, "invalid UTF-8 in FSL package"))
    }

    fn value_id(&mut self, value_count: usize) -> Result<ValueId, FslError> {
        let id = self.u16()?;
        if id as usize >= value_count {
            return Err(FslError::at(
                1,
                1,
                format!("FIR value id {id} is out of range"),
            ));
        }
        Ok(ValueId(id))
    }
}
