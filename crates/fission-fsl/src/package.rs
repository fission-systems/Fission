use crate::FslError;
use std::fmt;

pub const FSL_PACKAGE_VERSION: u16 = 1;
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
pub enum FirOp {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInstruction {
    pub name: String,
    pub mnemonic: String,
    pub opcode: u8,
    pub evidence: Vec<Evidence>,
    pub values: Vec<ValueDef>,
    pub ops: Vec<FirOp>,
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
        if self.version != FSL_PACKAGE_VERSION {
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
            if !names.insert(&instruction.name) || dispatch[instruction.opcode as usize].is_some() {
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
            instruction.validate()?;
            for value in &instruction.values {
                check_string(&value.name)?;
            }
            dispatch[instruction.opcode as usize] = Some(index as u16);
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
            writer.u8(instruction.opcode);
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
        if version != FSL_PACKAGE_VERSION {
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
            let opcode = reader.u8()?;
            if dispatch[opcode as usize].is_some() {
                return Err(FslError::at(
                    1,
                    1,
                    format!("duplicate opcode 0x{opcode:02x}"),
                ));
            }
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
            if op_count == 0 || op_count > MAX_OPS_PER_INSTRUCTION {
                return Err(FslError::at(
                    1,
                    1,
                    "invalid FIR operation count in FSL package",
                ));
            }
            let mut ops = Vec::with_capacity(op_count);
            for _ in 0..op_count {
                let op = match reader.u8()? {
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
            dispatch[opcode as usize] = Some(index as u16);
            instructions.push(CompiledInstruction {
                name,
                mnemonic,
                opcode,
                evidence,
                values,
                ops,
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
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
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
